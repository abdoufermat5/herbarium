// Pages: create, import, read, list, search, edit, rewrite and delete.

use serde::Deserialize;
use serde_json::json;

use super::review::{apply_schedule, load_settings};
use super::{id_prop, object};
use crate::content::{extract_text, extract_title, looks_like_html};
use crate::extension::{events, Ctx, Extension, OpResult, Operation, Registry};
use crate::models::{ImportFile, ImportResult, MetaPatch, Page, PageMeta};
use crate::time::now_ms;
use crate::vault;

pub(crate) struct Pages;

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
}

#[derive(Deserialize)]
struct ImportArgs {
    files: Vec<ImportFile>,
    /// Destination folder for every file; omitted or blank is the vault root.
    folder: Option<String>,
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
struct UpdateArgs {
    id: String,
    #[serde(flatten)]
    patch: MetaPatch,
}

#[derive(Deserialize)]
struct SetHtmlArgs {
    id: String,
    html: String,
}

#[derive(Deserialize)]
struct IdArgs {
    id: String,
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
    if looks_like_html(html) { Ok(()) } else { Err("not an HTML document".into()) }
}

/// Write the page files, index them, and emit `page.created`.
/// `fallback_title` is used when neither `title` nor the HTML provides one.
fn create(ctx: &mut Ctx, args: CreateArgs, fallback_title: Option<&str>) -> OpResult<PageMeta> {
    ensure_html(&args.html)?;
    let mut meta = PageMeta::new(uuid::Uuid::new_v4().to_string());
    meta.folder = vault::clean_folder(args.folder.as_deref())?;
    meta.title = args
        .title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .or_else(|| Some(extract_title(&args.html)).filter(|t| !t.is_empty()))
        .or_else(|| fallback_title.map(str::to_string))
        .unwrap_or_else(|| meta.id.clone());
    meta.tags = clean_tags(args.tags.unwrap_or_default());
    meta.note = args.note.unwrap_or_default();
    meta.allow_cdn = args.allow_cdn.unwrap_or(true);
    if let Some(minutes) = args.review_in_minutes {
        apply_schedule(&mut meta, minutes);
    }

    let store = ctx.store;
    vault::write_page(&store.vault, &meta, &args.html)?;
    let path = vault::html_path(&store.vault, &meta.id, meta.folder.as_deref());
    let mtime = vault::file_mtime(&path).unwrap_or_else(crate::time::now_secs);
    if let Err(e) = store.upsert(&meta, &extract_text(&args.html), mtime) {
        let _ = vault::delete_page_files(&store.vault, &meta);
        return Err(e.to_string());
    }
    ctx.emit(events::PAGE_CREATED, json!({ "page": meta }));
    Ok(meta)
}

impl Extension for Pages {
    fn id(&self) -> &str {
        "core.pages"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.add(Operation::new(
            "pages.create",
            "Save a new HTML page to the vault and return its metadata. The page must be one self-contained HTML document; its <title> (or first <h1>) becomes the page title unless `title` is given.",
            object(
                json!({
                    "html": { "type": "string", "description": "Complete HTML document." },
                    "title": { "type": "string", "description": "Overrides the title taken from the HTML." },
                    "folder": { "type": "string", "description": "Folder path such as `rust/cargo`; created if missing. Omit for the vault root." },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "note": { "type": "string", "description": "Personal note shown next to the page." },
                    "reviewInMinutes": { "type": "integer", "minimum": 1, "description": "Schedule a review this many minutes from now (1 day = 1440)." },
                    "allowCdn": { "type": "boolean", "description": "Allow known CDNs (default true). False blocks all network access." }
                }),
                &["html"],
            ),
            |ctx: &mut Ctx, a: CreateArgs| create(ctx, a, None),
        ))?;

        r.add(
            Operation::new(
                "pages.import",
                "Import several HTML files at once into one folder; invalid files are reported, not fatal. Pages are scheduled for review if the vault's review settings say so.",
                object(
                    json!({
                        "files": {
                            "type": "array",
                            "items": object(json!({
                                "name": { "type": ["string", "null"] },
                                "content": { "type": "string" }
                            }), &["content"])
                        },
                        "folder": { "type": ["string", "null"], "description": "Destination folder such as `rust/cargo`; created if missing. Omit for the vault root." }
                    }),
                    &["files"],
                ),
                |ctx: &mut Ctx, a: ImportArgs| {
                    let folder = vault::clean_folder(a.folder.as_deref())?;
                    let review_in_minutes = load_settings(&ctx.store.vault).import_review_minutes;
                    let mut result = ImportResult { imported: 0, errors: Vec::new() };
                    for f in a.files {
                        let label = f.name.clone().unwrap_or_else(|| "pasted content".into());
                        let stem = f.name.as_deref().map(|n| n.rsplit_once('.').map_or(n, |(s, _)| s).to_string());
                        let args = CreateArgs { html: f.content, folder: folder.clone(), review_in_minutes, ..Default::default() };
                        match create(ctx, args, stem.as_deref()) {
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
            "Read one page. `format`: `meta` (metadata only, default), `text` (metadata + visible text) or `html` (metadata + full source).",
            object(
                json!({
                    "id": id_prop(),
                    "format": { "type": "string", "enum": ["meta", "text", "html"] }
                }),
                &["id"],
            ),
            |ctx: &mut Ctx, a: GetArgs| {
                let meta = ctx.page(&a.id)?;
                let store = ctx.store;
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
            "List pages sorted by title, optionally limited to a folder (including its subfolders) or a tag.",
            object(
                json!({
                    "folder": { "type": "string" },
                    "tag": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1 }
                }),
                &[],
            ),
            |ctx: &mut Ctx, a: ListArgs| {
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
                    .take(a.limit.unwrap_or(usize::MAX))
                    .collect::<Vec<_>>())
            },
        ))?;

        r.add(Operation::new(
            "pages.search",
            "Full-text search over titles, tags, folders and page text; best matches first. An empty query lists every page.",
            object(
                json!({
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1 }
                }),
                &["query"],
            ),
            |ctx: &mut Ctx, a: SearchArgs| {
                let mut hits = ctx.store.search(&a.query).map_err(|e| e.to_string())?;
                hits.truncate(a.limit.unwrap_or(usize::MAX));
                Ok(hits)
            },
        ))?;

        r.add(Operation::new(
            "pages.update",
            "Edit a page's metadata. Only the given fields change; `folder: null` or `\"\"` moves the page to the vault root.",
            object(
                json!({
                    "id": id_prop(),
                    "title": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Replaces all tags." },
                    "folder": { "type": ["string", "null"] },
                    "note": { "type": "string" }
                }),
                &["id"],
            ),
            |ctx: &mut Ctx, a: UpdateArgs| {
                let mut meta = ctx.page(&a.id)?;
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
            "Replace a page's HTML (e.g. to revise generated documentation). Metadata, title and review state are kept.",
            object(
                json!({ "id": id_prop(), "html": { "type": "string", "description": "Complete HTML document." } }),
                &["id", "html"],
            ),
            |ctx: &mut Ctx, a: SetHtmlArgs| {
                ensure_html(&a.html)?;
                let mut meta = ctx.page(&a.id)?;
                meta.updated_at = now_ms();
                let store = ctx.store;
                vault::write_page(&store.vault, &meta, &a.html)?;
                let path = vault::html_path(&store.vault, &meta.id, meta.folder.as_deref());
                let mtime = vault::file_mtime(&path).unwrap_or_else(crate::time::now_secs);
                store.upsert(&meta, &extract_text(&a.html), mtime).map_err(|e| e.to_string())?;
                ctx.emit(events::PAGE_UPDATED, json!({ "page": meta }));
                Ok(meta)
            },
        ))?;

        r.add(Operation::new(
            "pages.delete",
            "Delete a page and its files from the vault. This cannot be undone.",
            object(json!({ "id": id_prop() }), &["id"]),
            |ctx: &mut Ctx, a: IdArgs| {
                let meta = ctx.page(&a.id)?;
                ctx.store.delete(&a.id).map_err(|e| e.to_string())?;
                vault::delete_page_files(&ctx.store.vault, &meta)?;
                ctx.emit(events::PAGE_DELETED, json!({ "page": meta }));
                Ok(json!({ "deleted": a.id }))
            },
        ))
    }
}
