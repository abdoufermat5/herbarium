// Vault-wide views and edits: tags, folders, overview, and re-indexing from disk.

use serde::Deserialize;
use serde_json::json;

use super::object;
use crate::extension::{Ctx, Extension, OpResult, Operation, Registry, events};
use crate::host::reindex;
use crate::time::now_ms;
use crate::vault;

pub(crate) struct Library;

#[derive(Deserialize)]
struct NoArgs {}

#[derive(Deserialize)]
struct CreateFolderArgs {
    path: String,
}

#[derive(Deserialize)]
struct RenameArgs {
    from: String,
    to: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteFolderArgs {
    path: String,
    #[serde(default)]
    with_pages: bool,
}

#[derive(Deserialize)]
struct DeleteTagArgs {
    tag: String,
}

fn tag_name(raw: &str) -> OpResult<String> {
    let tag = raw.trim();
    if tag.is_empty() {
        return Err("tag name is empty".into());
    }
    Ok(tag.to_string())
}

/// Apply `edit` to the tag list of every page carrying `tag`, saving the pages
/// whose tags changed. Returns how many were saved.
fn edit_tagged(
    ctx: &mut Ctx,
    tag: &str,
    edit: impl Fn(&[String]) -> Vec<String>,
) -> OpResult<usize> {
    let pages = ctx.store.all().map_err(|e| e.to_string())?;
    let mut updated = 0;
    for mut meta in pages
        .into_iter()
        .filter(|m| m.tags.iter().any(|t| t == tag))
    {
        let tags = edit(&meta.tags);
        if tags == meta.tags {
            continue;
        }
        meta.tags = tags;
        meta.updated_at = now_ms();
        ctx.save(&meta)?;
        updated += 1;
    }
    Ok(updated)
}

impl Extension for Library {
    fn id(&self) -> &str {
        "core.library"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.add(Operation::new(
            "tags.list",
            "All tags with the number of pages carrying each.",
            object(json!({}), &[]),
            |ctx: &mut Ctx, _: NoArgs| ctx.store.tag_counts().map_err(|e| e.to_string()),
        ))?;

        r.add(Operation::new(
            "folders.list",
            "All folders in the vault, including empty ones, as `a/b/c` paths.",
            object(json!({}), &[]),
            |ctx: &mut Ctx, _: NoArgs| vault::list_folders(&ctx.store.vault),
        ))?;

        r.add(Operation::new(
            "folders.create",
            "Create a folder (and missing parents), e.g. `rust/cargo`. Returns the normalized path.",
            object(json!({ "path": { "type": "string" } }), &["path"]),
            |ctx: &mut Ctx, a: CreateFolderArgs| vault::create_folder(&ctx.store.vault, &a.path),
        ))?;

        r.add(Operation::new(
            "folders.rename",
            "Rename or move a folder with everything in it, e.g. `rust/cargo` to `tools/cargo`. Page ids are kept. Fails if the destination exists. Returns the new path.",
            object(
                json!({ "from": { "type": "string" }, "to": { "type": "string" } }),
                &["from", "to"],
            ),
            |ctx: &mut Ctx, a: RenameArgs| {
                let folder = vault::rename_folder(&ctx.store.vault, &a.from, &a.to)?;
                let report = reindex(ctx.store)?;
                ctx.emit(events::VAULT_INDEXED, &report);
                Ok(json!({ "folder": folder }))
            },
        ))?;

        r.add(Operation::new(
            "folders.delete",
            "Delete a folder and its subfolders. Fails while it still holds pages unless `withPages` is true, which first moves those pages to the trash. Returns how many pages were trashed.",
            object(
                json!({
                    "path": { "type": "string" },
                    "withPages": { "type": "boolean", "description": "Move the pages inside to the trash first." }
                }),
                &["path"],
            ),
            |ctx: &mut Ctx, a: DeleteFolderArgs| {
                let mut trashed = 0;
                if a.with_pages {
                    for meta in vault::folder_pages(ctx.store, &a.path)? {
                        vault::trash_page(&ctx.store.vault, &meta)?;
                        ctx.store.delete(&meta.id).map_err(|e| e.to_string())?;
                        ctx.emit(events::PAGE_DELETED, json!({ "page": meta }));
                        trashed += 1;
                    }
                }
                vault::delete_folder(&ctx.store.vault, &a.path)?;
                Ok(json!({ "trashed": trashed }))
            },
        ))?;

        r.add(Operation::new(
            "tags.rename",
            "Rename a tag on every page carrying it. Renaming onto an existing tag merges the two. Returns how many pages changed.",
            object(
                json!({ "from": { "type": "string" }, "to": { "type": "string" } }),
                &["from", "to"],
            ),
            |ctx: &mut Ctx, a: RenameArgs| {
                let from = tag_name(&a.from)?;
                let to = tag_name(&a.to)?;
                let updated = edit_tagged(ctx, &from, |tags| {
                    let mut out: Vec<String> = Vec::with_capacity(tags.len());
                    for tag in tags {
                        let tag = if *tag == from { &to } else { tag };
                        if !out.contains(tag) {
                            out.push(tag.clone());
                        }
                    }
                    out
                })?;
                Ok(json!({ "updated": updated }))
            },
        ))?;

        r.add(Operation::new(
            "tags.delete",
            "Remove a tag from every page carrying it. Pages are kept. Returns how many pages changed.",
            object(json!({ "tag": { "type": "string" } }), &["tag"]),
            |ctx: &mut Ctx, a: DeleteTagArgs| {
                let tag = tag_name(&a.tag)?;
                let updated = edit_tagged(ctx, &tag, |tags| tags.iter().filter(|t| **t != tag).cloned().collect())?;
                Ok(json!({ "updated": updated }))
            },
        ))?;

        r.add(Operation::new(
            "vault.info",
            "Overview of the open vault: location, page count, pages due for review, folders and tags.",
            object(json!({}), &[]),
            |ctx: &mut Ctx, _: NoArgs| {
                let s = ctx.store;
                Ok(json!({
                    "path": s.vault,
                    "pages": s.count().map_err(|e| e.to_string())?,
                    "dueForReview": s.due(now_ms()).map_err(|e| e.to_string())?.len(),
                    "folders": vault::list_folders(&s.vault)?,
                    "tags": s.tag_counts().map_err(|e| e.to_string())?,
                    "lastIndexedAt": s.synced_at(),
                }))
            },
        ))?;

        r.add(Operation::new(
            "vault.rescan",
            "Re-index the vault from the files on disk (picks up pages added, edited or removed outside Herbarium).",
            object(json!({}), &[]),
            |ctx: &mut Ctx, _: NoArgs| {
                let report = reindex(ctx.store)?;
                ctx.emit(events::VAULT_INDEXED, &report);
                Ok(report)
            },
        ))
    }
}
