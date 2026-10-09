// Page version history: list, read and restore the snapshots kept under
// `.herbarium/history/<id>/`. A snapshot is taken automatically just before
// any in-app or agent overwrite of a page's HTML, so these operations are the
// page's undo history.

use serde::Deserialize;
use serde_json::json;

use super::proposals::overwrite_or_propose;
use super::{id_prop, object};
use crate::extension::{Ctx, Extension, OpResult, Operation, Registry};
use crate::vault;

pub(crate) struct History;

#[derive(Deserialize)]
struct ListArgs {
    id: String,
}

#[derive(Deserialize)]
struct GetArgs {
    id: String,
    at: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestoreArgs {
    id: String,
    at: i64,
    expected_updated_at: Option<i64>,
}

impl Extension for History {
    fn id(&self) -> &str {
        "core.history"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.add(Operation::new(
            "history.list",
            "List the saved versions of a page's HTML, newest first. Each entry is `{ at (unix ms), caller (\"ui\" or \"agent\"), bytes }`; pass its `at` to `history.get` or `history.restore`. A version is saved automatically just before any in-app or agent overwrite of the page, so this is the page's undo history.",
            object(json!({ "id": id_prop() }), &["id"]),
            |ctx: &mut Ctx, a: ListArgs| vault::list_history(&ctx.store.vault, &a.id),
        ))?;

        r.add(Operation::new(
            "history.get",
            "Read one saved version of a page's HTML (`at` from `history.list`).",
            object(
                json!({
                    "id": id_prop(),
                    "at": { "type": "integer", "description": "Version timestamp from `history.list`." }
                }),
                &["id", "at"],
            ),
            |ctx: &mut Ctx, a: GetArgs| {
                let html = vault::read_history(&ctx.store.vault, &a.id, a.at)?;
                Ok(json!({ "html": html }))
            },
        ))?;

        r.add(Operation::new(
            "history.restore",
            "Restore a page to an earlier version (`at` from `history.list`), undoing a later `pages.set_html` or `history.restore`. The current HTML is snapshotted first, so the restore is itself undoable. If the user reviews agent edits, the restore waits for their approval like `pages_set_html`. Pass `expectedUpdatedAt` (the page's `updatedAt` when you read it) to fail with a `conflict:` error instead of overwriting a newer edit.",
            object(
                json!({
                    "id": id_prop(),
                    "at": { "type": "integer", "description": "Version timestamp from `history.list`." },
                    "expectedUpdatedAt": { "type": "integer", "description": "The page's `updatedAt` when you read it; the restore is rejected if it changed since." }
                }),
                &["id", "at"],
            ),
            |ctx: &mut Ctx, a: RestoreArgs| {
                let html = vault::read_history(&ctx.store.vault, &a.id, a.at)?;
                overwrite_or_propose(ctx, &a.id, &html, a.expected_updated_at)
            },
        ))?;

        Ok(())
    }
}
