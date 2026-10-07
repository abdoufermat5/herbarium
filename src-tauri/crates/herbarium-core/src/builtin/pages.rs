// Pages: create, import, read, list, search, edit, rewrite, duplicate, bulk
// edits, and delete through the trash.

use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;

use super::network::default_allow_cdn;
use super::review::{apply_schedule, load_settings};
use super::{id_prop, object};
use crate::content::{extract_text, extract_title, looks_like_html};
use crate::extension::{Caller, Ctx, Extension, OpResult, Operation, Registry, events};
use crate::models::{ImportFile, ImportResult, MetaPatch, Page, PageMeta, PageSource};
use crate::store::Store;
use crate::time::now_ms;
use crate::vault;

pub(crate) struct Pages;

/// Result cap for `pages.list` and `pages.search` when an agent gives no
/// `limit`, so a large vault does not flood its context. The UI is uncapped.
const AGENT_DEFAULT_LIMIT: usize = 50;

const CONFLICT: &str = "conflict: page changed since it was loaded";

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CreateArgs {
    html: String,
    title: Option<String>,
    folder: Option<String>,
    tags: Option<Vec<String>>,
    note: Option<String>,
    review_in_minutes: Option<i64>,
    allow_cdn: Option<bool>,
    source: Option<PageSource>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportArgs {
    files: Vec<ImportFile>,
    /// Destination folder for every file; omitted or blank is the vault root.
    folder: Option<String>,
    /// Network access for every imported page; omitted follows the vault setting.
    allow_cdn: Option<bool>,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Format {
    #[default]
    Meta,
    Text,
    Html,
}

#[derive(Deserialize)]
struct GetArgs {
    id: String,
    #[serde(default)]
    format: Format,
}

#[derive(Deserialize)]
struct ListArgs {
    folder: Option<String>,
    tag: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct SearchArgs {
    query: String,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateArgs {
    id: String,
    base_updated_at: Option<i64>,
    #[serde(flatten)]
    patch: MetaPatch,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetHtmlArgs {
    id: String,
    html: String,
    base_updated_at: Option<i64>,
}

#[derive(Deserialize)]
struct IdArgs {
    id: String,
}

#[derive(Deserialize)]
struct PurgeArgs {
    id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BulkUpdateArgs {
    ids: Vec<String>,
    /// Absent: pages stay put; `null` or `""`: move to the vault root.
    #[serde(default, deserialize_with = "present")]
    folder: Option<Option<String>>,
    #[serde(default)]
    add_tags: Vec<String>,
    #[serde(default)]
    remove_tags: Vec<String>,
}

#[derive(Deserialize)]
struct IdsArgs {
    ids: Vec<String>,
}

#[derive(Serialize)]
struct ItemError {
    id: String,
    error: String,
}

#[derive(Serialize)]
struct BulkUpdateResult {
    updated: Vec<PageMeta>,
    errors: Vec<ItemError>,
}

#[derive(Serialize)]
struct BulkDeleteResult {
    deleted: Vec<String>,
    errors: Vec<ItemError>,
}

/// Distinguishes an explicit `null` (Some(None)) from an absent field (None).
fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}

/// Trim, drop blanks and duplicates, keep first-seen order.
fn clean_tags(tags: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(tags.len());
    for tag in tags {
        let tag = tag.trim();
        if !tag.is_empty() && !out.iter().any(|t| t == tag) {
            out.push(tag.to_string());
        }
    }
    out
}

fn ensure_html(html: &str) -> OpResult<()> {
    if looks_like_html(html) {
        Ok(())
    } else {
        Err("not an HTML document".into())
    }
}

/// Reject an edit made against a stale copy of the page.
fn check_base(vault: &Path, meta: &PageMeta, base_updated_at: Option<i64>) -> OpResult<()> {
    if let Some(base) = base_updated_at {
        if base != meta.updated_at {
            return Err(CONFLICT.into());
        }
        let current = vault::disk_updated_at(vault, meta).unwrap_or(meta.updated_at);
        if current != base {
            return Err(CONFLICT.into());
        }
    }
    Ok(())
}

/// Raise `updatedAt` to the HTML file's mtime (in ms) when the file is newer,
/// rewriting the sidecar with it. The optimistic check compares
/// `baseUpdatedAt` with the HTML mtime, so every value we hand back must be at
/// least the mtime of the bytes just written; otherwise the next save that
/// passes that value back would look like an external edit.
fn refresh_updated_at(vault: &Path, meta: &mut PageMeta) -> OpResult<()> {
    let html_ms = vault::file_mtime_ms(&vault::html_path(vault, &meta.id, meta.folder.as_deref()));
    if let Some(m) = html_ms.filter(|m| *m > meta.updated_at) {
        meta.updated_at = m;
        vault::write_meta(vault, meta)?;
    }
    Ok(())
}

/// Bring `meta` in step with the files on disk when another tool wrote the
/// page since the last rescan: adopt the disk's `updatedAt` (and the extracted
/// title/`source_title`) and refresh the index. Without this, `pages.get`
/// hands out an `updatedAt` that the next `pages.set_html`/`pages.update`
/// rejects as stale, so the reader's explicit overwrite could never succeed
/// after an external edit. Returns the metadata to hand back.
fn reconcile_with_disk(store: &Store, mut meta: PageMeta) -> OpResult<PageMeta> {
    let Some(disk) = vault::disk_updated_at(&store.vault, &meta) else {
        return Ok(meta);
    };
    if disk <= meta.updated_at {
        return Ok(meta);
    }
    let html = vault::read_html(&store.vault, &meta.id, meta.folder.as_deref())?;
    meta.updated_at = disk;
    let extracted = extract_title(&html);
    let user_never_renamed = meta.source_title.as_deref() == Some(meta.title.as_str());
    if user_never_renamed || meta.title.is_empty() {
        meta.title = if extracted.is_empty() {
            meta.id.clone()
        } else {
            extracted.clone()
        };
    }
    meta.source_title = Some(extracted);
    vault::write_meta(&store.vault, &meta)?;
    store
        .upsert(
            &meta,
            &extract_text(&html),
            vault::page_mtime(&store.vault, &meta),
        )
        .map_err(|e| e.to_string())?;
    Ok(meta)
}

fn source_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "description": "Where the page came from, shown with the page and searchable.",
        "properties": {
            "url": { "type": "string", "description": "http(s) address the page was saved from." },
            "tool": { "type": "string", "description": "Tool or model that generated the page, e.g. `Claude Code`." },
            "prompt": { "type": "string", "description": "The request that produced the page." }
        },
        "additionalProperties": false
    })
}

/// The explicit `limit`, else 50 for agents and unlimited for the UI.
fn effective_limit(caller: Caller, limit: Option<usize>) -> usize {
    limit.unwrap_or(match caller {
        Caller::Agent => AGENT_DEFAULT_LIMIT,
        Caller::Ui => usize::MAX,
    })
}

/// Write a new page's files, index them, and emit `page.created`. The files
/// are removed again if indexing fails, so disk and index stay in step.
fn insert(ctx: &mut Ctx, meta: &mut PageMeta, html: &str) -> OpResult<()> {
    let store = ctx.store;
    // `new_page_id` never reuses a trashed id, so an id with snapshots but no
    // trash entry was left behind by a page that is gone for good: a brand-new
    // page must not inherit its history.
    if !vault::trash_entry_exists(&store.vault, &meta.id) {
        vault::delete_history(&store.vault, &meta.id)?;
    }
    vault::write_page(&store.vault, meta, html)?;
    // The bytes are on disk now; keep the returned `updatedAt` at least their
    // mtime so a caller can save again with it as `baseUpdatedAt`.
    if let Err(e) = refresh_updated_at(&store.vault, meta) {
        let _ = vault::delete_page_files(&store.vault, meta);
        return Err(e);
    }
    if let Err(e) = store.upsert(
        meta,
        &extract_text(html),
        vault::page_mtime(&store.vault, meta),
    ) {
        let _ = vault::delete_page_files(&store.vault, meta);
        return Err(e.to_string());
    }
    ctx.emit(events::PAGE_CREATED, json!({ "page": meta }));
    Ok(())
}

/// Build, write and index a new page.
/// `fallback_title` is used when neither `title` nor the HTML provides one;
/// `id_source` (e.g. an imported file's stem) is slugged into the id instead
/// of the title.
fn create(
    ctx: &mut Ctx,
    args: CreateArgs,
    fallback_title: Option<&str>,
    id_source: Option<&str>,
) -> OpResult<PageMeta> {
    ensure_html(&args.html)?;
    let folder = vault::clean_folder(args.folder.as_deref())?;
    let source_title = extract_title(&args.html);
    let title = args
        .title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .or_else(|| Some(source_title.clone()).filter(|t| !t.is_empty()))
        .or_else(|| {
            fallback_title
                .map(str::to_string)
                .filter(|t| !t.trim().is_empty())
        });

    let store = ctx.store;
    let slug_from = id_source
        .filter(|s| !s.trim().is_empty())
        .or(title.as_deref())
        .unwrap_or("");
    let mut meta = PageMeta::new(vault::new_page_id(store, slug_from));
    meta.folder = folder;
    meta.title = title.unwrap_or_else(|| meta.id.clone());
    meta.source_title = Some(source_title);
    meta.tags = clean_tags(args.tags.unwrap_or_default());
    meta.note = args.note.unwrap_or_default();
    meta.allow_cdn = args
        .allow_cdn
        .unwrap_or_else(|| default_allow_cdn(&store.vault));
    PageSource::set(
        &mut meta,
        args.source.map(PageSource::clean).transpose()?.flatten(),
    );
    if let Some(minutes) = args.review_in_minutes {
        apply_schedule(&mut meta, minutes);
    }

    insert(ctx, &mut meta, &args.html)?;
    Ok(meta)
}

/// Move a page to the vault trash, drop it from the index, and emit `page.deleted`.
fn trash(ctx: &mut Ctx, id: &str) -> OpResult<PageMeta> {
    let meta = ctx.page(id)?;
    vault::trash_page(&ctx.store.vault, &meta)?;
    ctx.store.delete(id).map_err(|e| e.to_string())?;
    ctx.emit(events::PAGE_DELETED, json!({ "page": meta }));
    Ok(meta)
}

/// Apply a bulk move and tag edit to one page and save it.
fn bulk_edit(
    ctx: &mut Ctx,
    id: &str,
    folder: Option<Option<&str>>,
    add: &[String],
    remove: &[String],
) -> OpResult<PageMeta> {
    let mut meta = ctx.page(id)?;
    if let Some(folder) = folder {
        vault::move_page(&ctx.store.vault, &mut meta, folder)?;
    }
    let mut tags = std::mem::take(&mut meta.tags);
    tags.extend(add.iter().cloned());
    tags.retain(|t| !remove.contains(t));
    meta.tags = clean_tags(tags);
    meta.updated_at = now_ms();
    ctx.save(&meta)?;
    Ok(meta)
}

/// Replace a page's HTML: the one code path shared by `pages.set_html` and
/// `history.restore`. Snapshots the current on-disk HTML first (unless the new
/// bytes are identical), enforces the same `baseUpdatedAt` conflict rule, keeps
/// the title following the document unless the user renamed the page, and
/// emits `page.updated`.
pub(crate) fn overwrite_html(
    ctx: &mut Ctx,
    id: &str,
    html: &str,
    base_updated_at: Option<i64>,
) -> OpResult<PageMeta> {
    ensure_html(html)?;
    let mut meta = ctx.page(id)?;
    check_base(&ctx.store.vault, &meta, base_updated_at)?;
    let new_source = extract_title(html);
    if !new_source.is_empty()
        && (meta.source_title.as_deref() == Some(meta.title.as_str()) || meta.title.is_empty())
    {
        meta.title = new_source.clone();
    }
    meta.source_title = Some(new_source);
    meta.updated_at = now_ms();
    let store = ctx.store;
    // Keep the bytes being replaced recoverable before they are overwritten.
    vault::snapshot_history(
        &store.vault,
        &meta.id,
        meta.folder.as_deref(),
        html,
        ctx.caller.tag(),
    )?;
    vault::write_page(&store.vault, &meta, html)?;
    // The optimistic check compares `updatedAt` with the HTML's mtime, so the
    // value we hand back must not be older than the file just written.
    refresh_updated_at(&store.vault, &mut meta)?;
    store
        .upsert(
            &meta,
            &extract_text(html),
            vault::page_mtime(&store.vault, &meta),
        )
        .map_err(|e| e.to_string())?;
    ctx.emit(events::PAGE_UPDATED, json!({ "page": meta }));
    Ok(meta)
}

impl Extension for Pages {
    fn id(&self) -> &str {
        "core.pages"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.add(Operation::new(
            "pages.create",
            "Save a new HTML page to the vault and return its metadata (including the new `id`, a readable slug of the title). Pass `source` with the tool you are (`tool`) and the user's request (`prompt`) so the user can tell later where the page came from. The page must be one self-contained HTML document; its <title> (or first <h1>) becomes the page title unless `title` is given. Network access follows the vault's default (off unless the user enabled it): pass `allowCdn: true` if the page loads scripts or styles from a CDN; only allowlisted CDNs work.",
            object(
                json!({
                    "html": { "type": "string", "description": "Complete HTML document." },
                    "title": { "type": "string", "description": "Overrides the title taken from the HTML." },
                    "folder": { "type": "string", "description": "Folder path such as `rust/cargo`; created if missing. Omit for the vault root." },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "note": { "type": "string", "description": "Personal note shown next to the page." },
                    "reviewInMinutes": { "type": "integer", "minimum": 1, "description": "Schedule a review this many minutes from now (1 day = 1440)." },
                    "allowCdn": { "type": "boolean", "description": "Allow loading from allowlisted CDNs. Defaults to the vault setting (normally false, which blocks all network access)." },
                    "source": source_schema()
                }),
                &["html"],
            ),
            |ctx: &mut Ctx, a: CreateArgs| create(ctx, a, None, None),
        ))?;

        r.add(
            Operation::new(
                "pages.import",
                "Import several HTML files at once into one folder; invalid files are reported, not fatal. Ids are slugs of the file names. Pages are scheduled for review if the vault's review settings say so.",
                object(
                    json!({
                        "files": {
                            "type": "array",
                            "items": object(json!({
                                "name": { "type": ["string", "null"] },
                                "content": { "type": "string" }
                            }), &["content"])
                        },
                        "folder": { "type": ["string", "null"], "description": "Destination folder such as `rust/cargo`; created if missing. Omit for the vault root." },
                        "allowCdn": { "type": ["boolean", "null"], "description": "Network access for every imported page; omitted follows the vault setting." }
                    }),
                    &["files"],
                ),
                |ctx: &mut Ctx, a: ImportArgs| {
                    let folder = vault::clean_folder(a.folder.as_deref())?;
                    let review_in_minutes = load_settings(&ctx.store.vault).import_review_minutes;
                    let allow_cdn = Some(a.allow_cdn.unwrap_or_else(|| default_allow_cdn(&ctx.store.vault)));
                    let mut result = ImportResult { imported: 0, errors: Vec::new() };
                    for f in a.files {
                        let label = f.name.clone().unwrap_or_else(|| "pasted content".into());
                        let stem = f.name.as_deref().map(|n| n.rsplit_once('.').map_or(n, |(s, _)| s).to_string());
                        let args = CreateArgs {
                            html: f.content,
                            folder: folder.clone(),
                            review_in_minutes,
                            allow_cdn,
                            ..Default::default()
                        };
                        match create(ctx, args, stem.as_deref(), stem.as_deref()) {
                            Ok(_) => result.imported += 1,
                            Err(e) => result.errors.push(format!("{label}: {e}")),
                        }
                    }
                    Ok(result)
                },
            )
            .ui_only(),
        )?;

        r.add(Operation::new(
            "pages.get",
            "Read one page. `format`: `meta` (metadata only, default), `text` (metadata + visible text) or `html` (metadata + full source). Keep the returned `updatedAt` and pass it as `baseUpdatedAt` to `pages.update` / `pages.set_html` to avoid overwriting newer edits.",
            object(
                json!({
                    "id": id_prop(),
                    "format": { "type": "string", "enum": ["meta", "text", "html"] }
                }),
                &["id"],
            ),
            |ctx: &mut Ctx, a: GetArgs| {
                let store = ctx.store;
                let meta = reconcile_with_disk(store, ctx.page(&a.id)?)?;
                let (html, text) = match a.format {
                    Format::Meta => (None, None),
                    Format::Text => (None, Some(store.text_for(&a.id).map_err(|e| e.to_string())?.unwrap_or_default())),
                    Format::Html => (Some(vault::read_html(&store.vault, &a.id, meta.folder.as_deref())?), None),
                };
                Ok(Page { meta, html, text })
            },
        ))?;

        r.add(Operation::new(
            "pages.list",
            "List page metadata sorted by title, optionally limited to a folder (including its subfolders) or a tag. Returns at most `limit` pages (default 50).",
            object(
                json!({
                    "folder": { "type": "string", "description": "Folder path such as `rust`; subfolders are included." },
                    "tag": { "type": "string", "description": "Only pages carrying this exact tag." },
                    "limit": { "type": "integer", "minimum": 1, "description": "Maximum number of pages (default 50)." }
                }),
                &[],
            ),
            |ctx: &mut Ctx, a: ListArgs| {
                let limit = effective_limit(ctx.caller, a.limit);
                let folder = vault::clean_folder(a.folder.as_deref())?;
                let pages = ctx.store.all().map_err(|e| e.to_string())?;
                let in_folder = |m: &PageMeta| match (&folder, &m.folder) {
                    (None, _) => true,
                    (Some(want), Some(have)) => {
                        have == want || have.strip_prefix(want.as_str()).is_some_and(|rest| rest.starts_with('/'))
                    }
                    (Some(_), None) => false,
                };
                Ok(pages
                    .into_iter()
                    .filter(|m| in_folder(m))
                    .filter(|m| a.tag.as_ref().is_none_or(|t| m.tags.contains(t)))
                    .take(limit)
                    .collect::<Vec<_>>())
            },
        ))?;

        r.add(Operation::new(
            "pages.search",
            "Full-text search over titles, tags, folders, notes and page text; best matches first (title matches rank highest). Each hit is the page metadata plus an optional `snippet` of matching text with the match in [brackets]. An empty query lists every page. Returns at most `limit` hits (default 50).",
            object(
                json!({
                    "query": { "type": "string", "description": "Words to find; all must match." },
                    "limit": { "type": "integer", "minimum": 1, "description": "Maximum number of hits (default 50)." }
                }),
                &["query"],
            ),
            |ctx: &mut Ctx, a: SearchArgs| {
                let limit = effective_limit(ctx.caller, a.limit);
                ctx.store.search(&a.query, limit).map_err(|e| e.to_string())
            },
        ))?;

        r.add(Operation::new(
            "pages.update",
            "Edit a page's metadata. Only the given fields change; `folder: null` or `\"\"` moves the page to the vault root. Pass `baseUpdatedAt` (the `updatedAt` you last read) to fail with a `conflict:` error instead of overwriting a newer edit.",
            object(
                json!({
                    "id": id_prop(),
                    "title": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Replaces all tags." },
                    "folder": { "type": ["string", "null"] },
                    "note": { "type": "string" },
                    "source": { "anyOf": [source_schema(), { "type": "null" }], "description": "Replaces the page's source; `null` removes it." },
                    "baseUpdatedAt": { "type": "integer", "description": "The page's `updatedAt` when you read it; the edit is rejected if it changed since." }
                }),
                &["id"],
            ),
            |ctx: &mut Ctx, a: UpdateArgs| {
                let mut meta = ctx.page(&a.id)?;
                check_base(&ctx.store.vault, &meta, a.base_updated_at)?;
                let p = a.patch;
                if let Some(title) = p.title {
                    let title = title.trim();
                    meta.title = if title.is_empty() { meta.id.clone() } else { title.to_string() };
                }
                if let Some(tags) = p.tags {
                    meta.tags = clean_tags(tags);
                }
                if let Some(note) = p.note {
                    meta.note = note;
                }
                if let Some(source) = p.source {
                    PageSource::set(&mut meta, source.map(PageSource::clean).transpose()?.flatten());
                }
                if let Some(folder) = p.folder {
                    vault::move_page(&ctx.store.vault, &mut meta, folder.as_deref())?;
                }
                meta.updated_at = now_ms();
                ctx.save(&meta)?;
                Ok(meta)
            },
        ))?;

        r.add(Operation::new(
            "pages.set_html",
            "Replace a page's HTML (e.g. to revise generated documentation). Tags, folder, note and review state are kept; the title follows the new <title> unless the user renamed the page. Pass `baseUpdatedAt` (the `updatedAt` you last read) to fail with a `conflict:` error instead of overwriting a newer edit.",
            object(
                json!({
                    "id": id_prop(),
                    "html": { "type": "string", "description": "Complete HTML document." },
                    "baseUpdatedAt": { "type": "integer", "description": "The page's `updatedAt` when you read it; the edit is rejected if it changed since." }
                }),
                &["id", "html"],
            ),
            |ctx: &mut Ctx, a: SetHtmlArgs| overwrite_html(ctx, &a.id, &a.html, a.base_updated_at),
        ))?;

        r.add(Operation::new(
            "pages.duplicate",
            "Copy a page (HTML and metadata) into the same folder as \"<title> (copy)\" with a new id and no review schedule. Returns the new page's metadata.",
            object(json!({ "id": id_prop() }), &["id"]),
            |ctx: &mut Ctx, a: IdArgs| {
                let original = ctx.page(&a.id)?;
                let store = ctx.store;
                let html = vault::read_html(&store.vault, &original.id, original.folder.as_deref())?;
                let title = format!("{} (copy)", original.title);
                let mut meta = PageMeta::new(vault::new_page_id(store, &title));
                meta.title = title;
                meta.source_title = Some(extract_title(&html));
                meta.folder = original.folder;
                meta.tags = original.tags;
                meta.note = original.note;
                meta.allow_cdn = original.allow_cdn;
                meta.ext = original.ext;
                meta.ext.remove("review");
                insert(ctx, &mut meta, &html)?;
                Ok(meta)
            },
        ))?;

        r.add(Operation::new(
            "pages.bulk_update",
            "Move and/or retag several pages at once. `folder` moves every page (`null` or `\"\"` is the vault root; omit to leave pages in place); `addTags` are added, then `removeTags` removed. Pages that fail are listed in `errors`; the rest are still updated.",
            object(
                json!({
                    "ids": { "type": "array", "items": { "type": "string" }, "description": "Page ids." },
                    "folder": { "type": ["string", "null"], "description": "Destination folder such as `rust/cargo`; created if missing." },
                    "addTags": { "type": "array", "items": { "type": "string" } },
                    "removeTags": { "type": "array", "items": { "type": "string" } }
                }),
                &["ids"],
            ),
            |ctx: &mut Ctx, a: BulkUpdateArgs| {
                let add = clean_tags(a.add_tags);
                let remove = clean_tags(a.remove_tags);
                let folder = a.folder.as_ref().map(|f| f.as_deref());
                let mut result = BulkUpdateResult { updated: Vec::new(), errors: Vec::new() };
                for id in a.ids {
                    match bulk_edit(ctx, &id, folder, &add, &remove) {
                        Ok(meta) => result.updated.push(meta),
                        Err(error) => result.errors.push(ItemError { id, error }),
                    }
                }
                Ok(result)
            },
        ))?;

        r.add(Operation::new(
            "pages.delete",
            "Move a page to the vault trash. It disappears from lists and search; the user can restore it with `pages.restore`.",
            object(json!({ "id": id_prop() }), &["id"]),
            |ctx: &mut Ctx, a: IdArgs| {
                trash(ctx, &a.id)?;
                Ok(json!({ "deleted": a.id }))
            },
        ))?;

        r.add(Operation::new(
            "pages.bulk_delete",
            "Move several pages to the vault trash. Returns the trashed ids in `deleted`; pages that fail are listed in `errors`.",
            object(
                json!({ "ids": { "type": "array", "items": { "type": "string" }, "description": "Page ids." } }),
                &["ids"],
            ),
            |ctx: &mut Ctx, a: IdsArgs| {
                let mut result = BulkDeleteResult { deleted: Vec::new(), errors: Vec::new() };
                for id in a.ids {
                    match trash(ctx, &id) {
                        Ok(_) => result.deleted.push(id),
                        Err(error) => result.errors.push(ItemError { id, error }),
                    }
                }
                Ok(result)
            },
        ))?;

        r.add(Operation::new(
            "pages.trash",
            "List pages in the vault trash, most recently deleted first. Each entry is the page metadata plus `deletedAt` (unix ms).",
            object(json!({}), &[]),
            |ctx: &mut Ctx, _: serde_json::Value| vault::list_trash(&ctx.store.vault),
        ))?;

        r.add(Operation::new(
            "pages.restore",
            "Bring a page back from the trash into its original folder (or the vault root if that folder is no longer valid) and return its metadata.",
            object(json!({ "id": id_prop() }), &["id"]),
            |ctx: &mut Ctx, a: IdArgs| {
                let store = ctx.store;
                let meta = vault::restore_page(&store.vault, &a.id)?;
                let html = vault::read_html(&store.vault, &meta.id, meta.folder.as_deref())?;
                store
                    .upsert(&meta, &extract_text(&html), vault::page_mtime(&store.vault, &meta))
                    .map_err(|e| e.to_string())?;
                ctx.emit(events::PAGE_CREATED, json!({ "page": meta }));
                Ok(meta)
            },
        ))?;

        r.add(
            Operation::new(
                "pages.purge",
                "Permanently delete one trashed page, or empty the whole trash when `id` is omitted.",
                object(json!({ "id": { "type": ["string", "null"], "description": "Trashed page id; omit to empty the trash." } }), &[]),
                |ctx: &mut Ctx, a: PurgeArgs| {
                    let removed = vault::purge_trash(&ctx.store.vault, a.id.as_deref())?;
                    Ok(json!({ "removed": removed }))
                },
            )
            .ui_only(),
        )
    }
}
