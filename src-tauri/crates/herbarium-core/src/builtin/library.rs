// Vault-wide views: tags, folders, overview, and re-indexing from disk.

use serde::Deserialize;
use serde_json::json;

use super::object;
use crate::extension::{events, Ctx, Extension, OpResult, Operation, Registry};
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
