// Spaced review: schedule a page to come back after N minutes, with per-vault
// settings (preset intervals, how a completed review picks the next one,
// automatic scheduling of imports, queue size). Every duration is in minutes,
// so "30 minutes", "2 hours" and "3 days" are all the same kind of value.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{id_prop, object};
use crate::extension::{Ctx, Extension, OpResult, Operation, Registry};
use crate::models::PageMeta;
use crate::time::{now_ms, MINUTE_MS};

pub(crate) struct Review;

/// Longest interval any setting may use: ten years, in minutes.
const MAX_MINUTES: i64 = 3650 * 24 * 60;
const MAX_PRESETS: usize = 12;
/// Stored next to the index, inside the vault so it travels with it.
const SETTINGS_FILE: &str = "review.json";

/// How completing a review (`review.complete`) picks the next interval.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Strategy {
    /// Repeat the interval the page already has.
    Same,
    /// Step up to the next preset interval, staying on the last one.
    #[default]
    Ladder,
    /// Multiply the current interval by `multiplier`.
    Multiply,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ReviewSettings {
    /// Preset intervals in minutes, offered as quick buttons and used as the
    /// ladder steps. Sorted ascending, no duplicates.
    pub presets: Vec<i64>,
    pub strategy: Strategy,
    /// Growth factor for the `multiply` strategy.
    pub multiplier: f64,
    /// Cap for intervals computed by `review.complete`, in minutes.
    pub max_interval_minutes: i64,
    /// Schedule imported pages for review this many minutes out; none = don't.
    pub import_review_minutes: Option<i64>,
    /// Show at most this many pages in the review queue; none = all due.
    pub queue_limit: Option<usize>,
}

impl Default for ReviewSettings {
    fn default() -> Self {
        Self {
            presets: vec![1440, 3 * 1440, 7 * 1440, 30 * 1440],
            strategy: Strategy::Ladder,
            multiplier: 2.0,
            max_interval_minutes: 365 * 1440,
            import_review_minutes: None,
            queue_limit: None,
        }
    }
}

impl ReviewSettings {
    /// Reject out-of-range values; normalise the preset list (sorted, unique).
    fn validate(mut self) -> OpResult<Self> {
        if self.presets.is_empty() || self.presets.len() > MAX_PRESETS {
            return Err(format!("presets: provide between 1 and {MAX_PRESETS} values"));
        }
        if self.presets.iter().any(|m| !(1..=MAX_MINUTES).contains(m)) {
            return Err(format!("presets: each value must be between 1 and {MAX_MINUTES} minutes"));
        }
        self.presets.sort_unstable();
        self.presets.dedup();
        if !(1.1..=10.0).contains(&self.multiplier) {
            return Err("multiplier: must be between 1.1 and 10".into());
        }
        if !(1..=MAX_MINUTES).contains(&self.max_interval_minutes) {
            return Err(format!("maxIntervalMinutes: must be between 1 and {MAX_MINUTES}"));
        }
        if self.import_review_minutes.is_some_and(|m| !(1..=MAX_MINUTES).contains(&m)) {
            return Err(format!("importReviewMinutes: must be between 1 and {MAX_MINUTES}"));
        }
        if self.queue_limit == Some(0) {
            return Err("queueLimit: must be at least 1 (or null for no limit)".into());
        }
        Ok(self)
    }

    /// Interval, in minutes, that follows a completed review of a page currently
    /// on `current` minutes (`None`: not scheduled yet, so start at the first preset).
    pub fn next_interval(&self, current: Option<i64>) -> i64 {
        let next = match current {
            None => self.presets[0],
            Some(c) => match self.strategy {
                Strategy::Same => c,
                Strategy::Ladder => self.presets.iter().copied().find(|&m| m > c).unwrap_or(c),
                Strategy::Multiply => ((c as f64 * self.multiplier).ceil() as i64).max(c + 1),
            },
        };
        next.clamp(1, self.max_interval_minutes)
    }
}

/// The vault's review settings; defaults when the file is missing or unusable.
pub(crate) fn load_settings(vault: &Path) -> ReviewSettings {
    fs::read_to_string(vault.join(".herbarium").join(SETTINGS_FILE))
        .ok()
        .and_then(|raw| serde_json::from_str::<ReviewSettings>(&raw).ok())
        .and_then(|s| s.validate().ok())
        .unwrap_or_default()
}

fn save_settings(vault: &Path, settings: &ReviewSettings) -> OpResult<()> {
    let dir = vault.join(".herbarium");
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create settings folder: {e}"))?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(dir.join(SETTINGS_FILE), json).map_err(|e| format!("cannot write review settings: {e}"))
}

/// Set the next review `minutes` from now (at least one minute).
pub(crate) fn apply_schedule(meta: &mut PageMeta, minutes: i64) {
    let minutes = minutes.clamp(1, MAX_MINUTES);
    let now = now_ms();
    meta.interval_minutes = Some(minutes);
    meta.next_review = Some(now + minutes * MINUTE_MS);
    meta.last_review = Some(now);
    meta.updated_at = now;
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleArgs {
    id: String,
    interval_minutes: i64,
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
            "Mark a page for review: it shows up in the review queue after `intervalMinutes` minutes (1 hour = 60, 1 day = 1440). The user's preset intervals are listed by `review.settings`.",
            object(
                json!({
                    "id": id_prop(),
                    "intervalMinutes": { "type": "integer", "minimum": 1, "maximum": MAX_MINUTES, "description": "Minutes until the page is due." }
                }),
                &["id", "intervalMinutes"],
            ),
            |ctx: &mut Ctx, a: ScheduleArgs| {
                let mut meta = ctx.page(&a.id)?;
                apply_schedule(&mut meta, a.interval_minutes);
                ctx.save(&meta)?;
                Ok(meta)
            },
        ))?;

        r.add(Operation::new(
            "review.complete",
            "Record that a page was reviewed and schedule the next review using the vault's strategy (repeat the same interval, step up the preset ladder, or multiply). A page that was not scheduled starts at the first preset.",
            object(json!({ "id": id_prop() }), &["id"]),
            |ctx: &mut Ctx, a: IdArgs| {
                let settings = load_settings(&ctx.store.vault);
                let mut meta = ctx.page(&a.id)?;
                let minutes = settings.next_interval(meta.interval_minutes);
                apply_schedule(&mut meta, minutes);
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
                meta.interval_minutes = None;
                meta.next_review = None;
                meta.last_review = None;
                meta.updated_at = now_ms();
                ctx.save(&meta)?;
                Ok(meta)
            },
        ))?;

        r.add(Operation::new(
            "review.due",
            "Pages due for review now, earliest first (at most the vault's queue limit, if one is set).",
            object(json!({}), &[]),
            |ctx: &mut Ctx, _: NoArgs| {
                let mut due = ctx.store.due(now_ms()).map_err(|e| e.to_string())?;
                if let Some(limit) = load_settings(&ctx.store.vault).queue_limit {
                    due.truncate(limit);
                }
                Ok(due)
            },
        ))?;

        r.add(Operation::new(
            "review.settings",
            "The vault's review settings: preset intervals in minutes, the strategy used by `review.complete`, and queue options.",
            object(json!({}), &[]),
            |ctx: &mut Ctx, _: NoArgs| Ok(load_settings(&ctx.store.vault)),
        ))?;

        r.add(
            Operation::new(
                "review.configure",
                "Replace the vault's review settings. Omitted fields go back to their defaults. Durations are in minutes.",
                object(
                    json!({
                        "presets": {
                            "type": "array",
                            "items": { "type": "integer", "minimum": 1, "maximum": MAX_MINUTES },
                            "minItems": 1,
                            "maxItems": MAX_PRESETS,
                            "description": "Preset intervals in minutes."
                        },
                        "strategy": { "type": "string", "enum": ["same", "ladder", "multiply"] },
                        "multiplier": { "type": "number", "minimum": 1.1, "maximum": 10 },
                        "maxIntervalMinutes": { "type": "integer", "minimum": 1, "maximum": MAX_MINUTES },
                        "importReviewMinutes": { "type": ["integer", "null"], "minimum": 1, "maximum": MAX_MINUTES },
                        "queueLimit": { "type": ["integer", "null"], "minimum": 1 }
                    }),
                    &[],
                ),
                |ctx: &mut Ctx, a: ReviewSettings| {
                    let settings = a.validate()?;
                    save_settings(&ctx.store.vault, &settings)?;
                    Ok(settings)
                },
            )
            .ui_only(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 1440;

    fn with(strategy: Strategy) -> ReviewSettings {
        ReviewSettings { strategy, ..Default::default() }
    }

    #[test]
    fn ladder_climbs_the_presets_and_stays_on_the_last() {
        let s = with(Strategy::Ladder);
        assert_eq!(s.next_interval(None), DAY);
        assert_eq!(s.next_interval(Some(DAY)), 3 * DAY);
        assert_eq!(s.next_interval(Some(5 * DAY)), 7 * DAY, "a custom interval steps to the next preset above it");
        assert_eq!(s.next_interval(Some(30 * DAY)), 30 * DAY);
        assert_eq!(s.next_interval(Some(45 * DAY)), 45 * DAY, "never shrinks past the top preset");
    }

    #[test]
    fn ladder_mixes_minutes_hours_and_days() {
        let s = ReviewSettings { presets: vec![30, 120, DAY], ..with(Strategy::Ladder) };
        assert_eq!(s.next_interval(None), 30);
        assert_eq!(s.next_interval(Some(30)), 120);
        assert_eq!(s.next_interval(Some(120)), DAY);
    }

    #[test]
    fn multiply_grows_at_least_one_minute_and_respects_the_cap() {
        let s = ReviewSettings { multiplier: 1.1, max_interval_minutes: 20, ..with(Strategy::Multiply) };
        assert_eq!(s.next_interval(Some(1)), 2, "ceil(1.1) = 2");
        assert_eq!(s.next_interval(Some(3)), 4);
        assert_eq!(s.next_interval(Some(19)), 20);
        assert_eq!(s.next_interval(Some(20)), 20, "capped");
    }

    #[test]
    fn same_repeats_the_current_interval() {
        assert_eq!(with(Strategy::Same).next_interval(Some(45)), 45);
    }

    #[test]
    fn validation_normalises_presets_and_rejects_bad_values() {
        let ok = ReviewSettings { presets: vec![30, 3, 3, 1], ..Default::default() }.validate().unwrap();
        assert_eq!(ok.presets, vec![1, 3, 30]);

        for bad in [
            ReviewSettings { presets: vec![], ..Default::default() },
            ReviewSettings { presets: vec![0], ..Default::default() },
            ReviewSettings { presets: vec![MAX_MINUTES + 1], ..Default::default() },
            ReviewSettings { presets: (1..=13).collect(), ..Default::default() },
            ReviewSettings { multiplier: 1.0, ..Default::default() },
            ReviewSettings { multiplier: f64::NAN, ..Default::default() },
            ReviewSettings { max_interval_minutes: 0, ..Default::default() },
            ReviewSettings { import_review_minutes: Some(0), ..Default::default() },
            ReviewSettings { queue_limit: Some(0), ..Default::default() },
        ] {
            assert!(bad.clone().validate().is_err(), "{bad:?}");
        }
    }
}
