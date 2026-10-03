// Vault-wide views: tags, folders, overview, and re-indexing from disk.

use serde::Deserialize;
use serde_json::json;

use super::object;
use crate::extension::{events, Ctx, Extension, OpResult, Operation, Registry};
use crate::host::reindex;
use crate::time::now_ms;

pub(crate) struct Library;

#[derive(Deserialize)]
struct NoArgs {}

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
            "All folders that contain pages, as `a/b/c` paths.",
            object(json!({}), &[]),
            |ctx: &mut Ctx, _: NoArgs| ctx.store.folders().map_err(|e| e.to_string()),
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
                    "folders": s.folders().map_err(|e| e.to_string())?,
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
