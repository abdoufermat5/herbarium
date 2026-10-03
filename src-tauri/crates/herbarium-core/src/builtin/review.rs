// Spaced review: schedule a page to come back after N days.

use serde::Deserialize;
use serde_json::json;

use super::{id_prop, object};
use crate::extension::{Ctx, Extension, OpResult, Operation, Registry};
use crate::models::PageMeta;
use crate::time::{now_ms, DAY_MS};

pub(crate) struct Review;

/// Set the next review `days` from now (at least one day).
pub(crate) fn apply_schedule(meta: &mut PageMeta, days: i64) {
    let days = days.max(1);
    let now = now_ms();
    meta.interval_days = Some(days);
    meta.next_review = Some(now + days * DAY_MS);
    meta.last_review = Some(now);
    meta.updated_at = now;
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleArgs {
    id: String,
    interval_days: i64,
}

#[derive(Deserialize)]
struct IdArgs {
    id: String,
}

#[derive(Deserialize)]
struct NoArgs {}

impl Extension for Review {
    fn id(&self) -> &str {
        "core.review"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.add(Operation::new(
            "review.schedule",
            "Mark a page for review: it shows up in the review queue after `intervalDays` days (the app offers 1, 3, 7 or 30).",
            object(
                json!({
                    "id": id_prop(),
                    "intervalDays": { "type": "integer", "minimum": 1, "description": "Days until the page is due." }
                }),
                &["id", "intervalDays"],
            ),
            |ctx: &mut Ctx, a: ScheduleArgs| {
                let mut meta = ctx.page(&a.id)?;
                apply_schedule(&mut meta, a.interval_days);
                ctx.save(&meta)?;
                Ok(meta)
            },
        ))?;

        r.add(Operation::new(
            "review.clear",
            "Remove a page from the review schedule.",
            object(json!({ "id": id_prop() }), &["id"]),
            |ctx: &mut Ctx, a: IdArgs| {
                let mut meta = ctx.page(&a.id)?;
                meta.interval_days = None;
                meta.next_review = None;
                meta.last_review = None;
                meta.updated_at = now_ms();
                ctx.save(&meta)?;
                Ok(meta)
            },
        ))?;

        r.add(Operation::new(
            "review.due",
            "Pages due for review now, earliest first.",
            object(json!({}), &[]),
            |ctx: &mut Ctx, _: NoArgs| ctx.store.due(now_ms()).map_err(|e| e.to_string()),
        ))
    }
}
