// Spaced review: schedule a page to come back after N minutes, with per-vault
// settings (preset intervals, how a completed review picks the next one,
// automatic scheduling of imports, queue size). Every duration is in minutes,
// so "30 minutes", "2 hours" and "3 days" are all the same kind of value.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use chrono::{NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{fsrs, id_prop, object};
use crate::extension::{Ctx, Extension, OpResult, Operation, Registry};
use crate::models::PageMeta;
use crate::time::{DAY_MS, MINUTE_MS, now_ms};
use crate::vault;

pub(crate) struct Review;

/// Longest interval any setting may use: ten years, in minutes.
const MAX_MINUTES: i64 = 3650 * 24 * 60;
const MAX_PRESETS: usize = 12;
/// Minutes in a day, for turning FSRS's day intervals into stored intervals.
const MINUTES_PER_DAY: f64 = 1440.0;
/// Desired-retention bounds for the FSRS strategy.
const MIN_RETENTION: f64 = 0.70;
const MAX_RETENTION: f64 = 0.97;
/// Stored next to the index, inside the vault so it travels with it.
const SETTINGS_FILE: &str = "review.json";
/// Key of the review history in `PageMeta::ext`.
const HISTORY_KEY: &str = "review";
/// Most recent history entries kept per page.
const MAX_LOG: usize = 100;
/// Most recent days kept in the per-day review tally.
const MAX_DAYS: usize = 400;
/// Days covered by `review.stats` `upcoming`.
const UPCOMING_DAYS: i64 = 14;

/// How completing a review (`review.complete`) picks the next interval.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Strategy {
    /// Repeat the interval the page already has.
    Same,
    /// Step up to the next preset interval, staying on the last one.
    Ladder,
    /// Multiply the current interval by `multiplier`.
    Multiply,
    /// Adaptive FSRS-6 scheduling from the page's memory state.
    #[default]
    Fsrs,
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
    /// Target recall probability for the `fsrs` strategy (0.70..=0.97).
    pub desired_retention: f64,
    /// Schedule imported pages for review this many minutes out; none = don't.
    pub import_review_minutes: Option<i64>,
    /// Show at most this many pages in the review queue; none = all due.
    pub queue_limit: Option<usize>,
}

impl Default for ReviewSettings {
    fn default() -> Self {
        Self {
            presets: vec![1440, 3 * 1440, 7 * 1440, 30 * 1440],
            strategy: Strategy::Fsrs,
            multiplier: 2.0,
            max_interval_minutes: 365 * 1440,
            desired_retention: 0.9,
            import_review_minutes: None,
            queue_limit: None,
        }
    }
}

impl ReviewSettings {
    /// Reject out-of-range values; normalise the preset list (sorted, unique).
    fn validate(mut self) -> OpResult<Self> {
        if self.presets.is_empty() || self.presets.len() > MAX_PRESETS {
            return Err(format!(
                "presets: provide between 1 and {MAX_PRESETS} values"
            ));
        }
        if self.presets.iter().any(|m| !(1..=MAX_MINUTES).contains(m)) {
            return Err(format!(
                "presets: each value must be between 1 and {MAX_MINUTES} minutes"
            ));
        }
        self.presets.sort_unstable();
        self.presets.dedup();
        if !(1.1..=10.0).contains(&self.multiplier) {
            return Err("multiplier: must be between 1.1 and 10".into());
        }
        if !(MIN_RETENTION..=MAX_RETENTION).contains(&self.desired_retention) {
            return Err(format!(
                "desiredRetention: must be between {MIN_RETENTION} and {MAX_RETENTION}"
            ));
        }
        if !(1..=MAX_MINUTES).contains(&self.max_interval_minutes) {
            return Err(format!(
                "maxIntervalMinutes: must be between 1 and {MAX_MINUTES}"
            ));
        }
        if self
            .import_review_minutes
            .is_some_and(|m| !(1..=MAX_MINUTES).contains(&m))
        {
            return Err(format!(
                "importReviewMinutes: must be between 1 and {MAX_MINUTES}"
            ));
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
                // `complete` uses the memory model for FSRS; this coarse helper
                // (used by the other strategies) falls back to the ladder.
                Strategy::Ladder | Strategy::Fsrs => {
                    self.presets.iter().copied().find(|&m| m > c).unwrap_or(c)
                }
                Strategy::Multiply => ((c as f64 * self.multiplier).ceil() as i64).max(c + 1),
            },
        };
        next.clamp(1, self.max_interval_minutes)
    }

    /// The next interval for a grade under a non-FSRS strategy: Again restarts
    /// at the first preset, Hard repeats the current interval, Easy advances
    /// twice, Good advances once.
    fn non_fsrs_interval(&self, current: Option<i64>, grade: Grade) -> i64 {
        match grade {
            Grade::Again => self.next_interval(None),
            Grade::Good => self.next_interval(current),
            Grade::Hard => current
                .unwrap_or(self.presets[0])
                .clamp(1, self.max_interval_minutes),
            Grade::Easy => {
                let once = self.next_interval(current);
                self.next_interval(Some(once))
            }
        }
    }
}

/// The vault's review settings. A missing file means defaults; an unreadable
/// or invalid file is an error so it is never silently replaced.
pub(crate) fn read_settings(vault: &Path) -> OpResult<ReviewSettings> {
    let path = vault.join(".herbarium").join(SETTINGS_FILE);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(ReviewSettings::default()),
        Err(e) => {
            return Err(format!(
                "cannot read review settings (.herbarium/{SETTINGS_FILE}): {e}"
            ));
        }
    };
    serde_json::from_str::<ReviewSettings>(&raw)
        .map_err(|e| e.to_string())
        .and_then(ReviewSettings::validate)
        .map_err(|e| {
            format!("review settings file .herbarium/{SETTINGS_FILE} is invalid ({e}); save new settings to replace it")
        })
}

/// Settings for callers that must keep working (imports, the queue): an
/// invalid file is logged and defaults are used.
pub(crate) fn load_settings(vault: &Path) -> ReviewSettings {
    read_settings(vault).unwrap_or_else(|e| {
        eprintln!("herbarium: {e}");
        ReviewSettings::default()
    })
}

fn save_settings(vault: &Path, settings: &ReviewSettings) -> OpResult<()> {
    let dir = vault.join(".herbarium");
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create settings folder: {e}"))?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    vault::write_atomic(&dir.join(SETTINGS_FILE), json.as_bytes())
        .map_err(|e| format!("cannot write review settings: {e}"))
}

/// Set the next review `minutes` from now (at least one minute). Does not
/// touch `last_review`: scheduling is not reviewing.
pub(crate) fn apply_schedule(meta: &mut PageMeta, minutes: i64) {
    let minutes = minutes.clamp(1, MAX_MINUTES);
    let now = now_ms();
    meta.interval_minutes = Some(minutes);
    meta.next_review = Some(now + minutes * MINUTE_MS);
    meta.updated_at = now;
}

/// How a review went.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Grade {
    /// Forgotten: start over at the first preset.
    Again,
    /// Recalled with difficulty.
    Hard,
    /// Remembered: the next interval follows the vault's strategy.
    #[default]
    Good,
    /// Recalled easily.
    Easy,
}

impl Grade {
    /// The grade as an FSRS rating: 1 again, 2 hard, 3 good, 4 easy.
    fn rating(self) -> fsrs::Rating {
        match self {
            Grade::Again => 1,
            Grade::Hard => 2,
            Grade::Good => 3,
            Grade::Easy => 4,
        }
    }
}

/// One completed review, stored in `ext["review"].log`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct LogEntry {
    at: i64,
    grade: Grade,
    interval_minutes: i64,
}

/// Review history kept in `ext["review"]`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
struct History {
    count: u64,
    log: Vec<LogEntry>,
    /// Reviews completed per UTC day (key: that day's midnight in unix ms).
    /// Unlike `log` this is not cut at `MAX_LOG`, so daily stats stay exact.
    days: BTreeMap<i64, u64>,
    /// FSRS memory state in days (`stability`) and on the 1..10 scale
    /// (`difficulty`). Absent on pages last reviewed before adaptive scheduling
    /// or under another strategy, in which case the next review initializes it.
    stability: Option<f64>,
    difficulty: Option<f64>,
}

impl History {
    /// The page's FSRS memory state, when one has been recorded.
    fn memory(&self) -> Option<fsrs::Memory> {
        match (self.stability, self.difficulty) {
            (Some(stability), Some(difficulty)) if stability > 0.0 && difficulty > 0.0 => {
                Some(fsrs::Memory {
                    stability,
                    difficulty,
                })
            }
            _ => None,
        }
    }

    /// Reviews completed in the UTC day starting at `day`. Histories written
    /// before `days` existed fall back to counting the log.
    fn on_day(&self, day: i64) -> u64 {
        let from_log = self
            .log
            .iter()
            .filter(|e| utc_midnight(e.at) == day)
            .count() as u64;
        self.days.get(&day).copied().unwrap_or(0).max(from_log)
    }
}

fn history(meta: &PageMeta) -> History {
    meta.ext
        .get(HISTORY_KEY)
        .and_then(|v| serde_json::from_value::<History>(v.clone()).ok())
        .map(|mut h| {
            if h.count < h.log.len() as u64 {
                h.count = h.log.len() as u64;
            }
            h
        })
        .unwrap_or_default()
}

/// The interval, in minutes, and the new FSRS memory state (if any) a review
/// with `grade` would produce. Shared by `complete` and `review.preview`, so
/// the previewed intervals are exactly what completing would schedule.
fn schedule_after_review(
    meta: &PageMeta,
    settings: &ReviewSettings,
    grade: Grade,
    prior: &History,
    now: i64,
) -> (i64, Option<fsrs::Memory>) {
    if settings.strategy != Strategy::Fsrs {
        return (
            settings.non_fsrs_interval(meta.interval_minutes, grade),
            None,
        );
    }
    // Whole days since the last review; 0 (including same-day reviews) uses the
    // short-term stability update inside `fsrs::next_memory`.
    let elapsed_days = meta
        .last_review
        .map(|t| ((now - t).max(0) as f64 / DAY_MS as f64).floor())
        .unwrap_or(0.0);
    let memory = fsrs::next_memory(prior.memory(), elapsed_days, grade.rating());
    // Again is a relearning step: back to the first preset. The other grades
    // use the interval that reaches the desired retention.
    let minutes = if grade == Grade::Again {
        settings.presets[0]
    } else {
        (fsrs::interval_days(memory.stability, settings.desired_retention) * MINUTES_PER_DAY)
            .round() as i64
    };
    (
        minutes.clamp(1, settings.max_interval_minutes),
        Some(memory),
    )
}

/// Record a review now: next interval from the grade, `last_review = now`,
/// and a history entry (last `MAX_LOG` kept). Under FSRS the page's memory
/// state is updated too.
fn complete(meta: &mut PageMeta, settings: &ReviewSettings, grade: Grade) {
    let now = now_ms();
    let mut h = history(meta);
    let (minutes, memory) = schedule_after_review(meta, settings, grade, &h, now);
    apply_schedule(meta, minutes);
    let now = meta.updated_at;
    meta.last_review = Some(now);
    if let Some(m) = memory {
        h.stability = Some(m.stability);
        h.difficulty = Some(m.difficulty);
    }
    let day = utc_midnight(now);
    // Seed from the log so a pre-`days` history keeps its existing count.
    let seeded = h.on_day(day);
    h.days.insert(day, seeded + 1);
    while h.days.len() > MAX_DAYS {
        h.days.pop_first();
    }
    h.count += 1;
    h.log.push(LogEntry {
        at: now,
        grade,
        interval_minutes: minutes.clamp(1, MAX_MINUTES),
    });
    if h.log.len() > MAX_LOG {
        let excess = h.log.len() - MAX_LOG;
        h.log.drain(..excess);
    }
    meta.ext.insert(
        HISTORY_KEY.into(),
        serde_json::to_value(h).unwrap_or(Value::Null),
    );
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Preview {
    again: i64,
    hard: i64,
    good: i64,
    easy: i64,
}

/// Interval each grade would schedule if the page were reviewed now; no writes.
fn preview(meta: &PageMeta, settings: &ReviewSettings, now: i64) -> Preview {
    let h = history(meta);
    let interval = |grade| schedule_after_review(meta, settings, grade, &h, now).0;
    Preview {
        again: interval(Grade::Again),
        hard: interval(Grade::Hard),
        good: interval(Grade::Good),
        easy: interval(Grade::Easy),
    }
}

/// Start of the UTC day containing `ms`.
fn utc_midnight(ms: i64) -> i64 {
    ms.div_euclid(DAY_MS) * DAY_MS
}

fn day_label(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms)
        .single()
        .map(|d| d.date_naive())
        .unwrap_or(NaiveDate::MIN)
        .format("%Y-%m-%d")
        .to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DayCount {
    day: String,
    count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Stats {
    due_total: usize,
    overdue: usize,
    reviewed_today: usize,
    upcoming: Vec<DayCount>,
    total_reviews: u64,
}

fn stats(ctx: &Ctx, fixed_now: Option<i64>) -> OpResult<Stats> {
    let now = fixed_now.unwrap_or_else(now_ms);
    let today = utc_midnight(now);
    let due_total = ctx.store.due_count(now).map_err(|e| e.to_string())?;
    let mut upcoming: Vec<DayCount> = (0..UPCOMING_DAYS)
        .map(|d| DayCount {
            day: day_label(today + d * DAY_MS),
            count: 0,
        })
        .collect();
    let (mut overdue, mut reviewed_today, mut total_reviews) = (0, 0, 0);
    for meta in ctx.store.all().map_err(|e| e.to_string())? {
        if let Some(next) = meta.next_review {
            if next < today {
                overdue += 1;
            } else if next > now {
                let day = (next - today) / DAY_MS;
                if let Some(slot) = upcoming.get_mut(day as usize) {
                    slot.count += 1;
                }
            }
        }
        let h = history(&meta);
        total_reviews += h.count;
        reviewed_today += h.on_day(today) as usize;
    }
    Ok(Stats {
        due_total,
        overdue,
        reviewed_today,
        upcoming,
        total_reviews,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompleteArgs {
    id: String,
    #[serde(default)]
    grade: Grade,
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

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct DueArgs {
    #[serde(default)]
    now: Option<i64>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct StatsArgs {
    #[serde(default)]
    now: Option<i64>,
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
            "Record that a page was reviewed now and schedule the next review. `grade` is `again`, `hard`, `good` (default) or `easy`. Under the adaptive (FSRS) strategy the vault's `desiredRetention` and the page's memory state set the interval; `again` goes back to the first preset. Under the other strategies `again` restarts the ladder, `hard` repeats the current interval, `good` advances once and `easy` advances twice. A page that was not scheduled starts at the first preset. Each review is logged in the page's review history; `review.preview` shows the intervals first.",
            object(
                json!({
                    "id": id_prop(),
                    "grade": { "type": "string", "enum": ["again", "hard", "good", "easy"], "description": "How the review went; default `good`." }
                }),
                &["id"],
            ),
            |ctx: &mut Ctx, a: CompleteArgs| {
                let settings = read_settings(&ctx.store.vault)?;
                let mut meta = ctx.page(&a.id)?;
                complete(&mut meta, &settings, a.grade);
                ctx.save(&meta)?;
                Ok(meta)
            },
        ))?;

        r.add(Operation::new(
            "review.preview",
            "The interval in minutes each grade (`again`, `hard`, `good`, `easy`) would schedule if this page were reviewed right now, using the vault's current strategy and settings. Reads only; `review.complete` applies one of them.",
            object(json!({ "id": id_prop() }), &["id"]),
            |ctx: &mut Ctx, a: IdArgs| {
                let settings = read_settings(&ctx.store.vault)?;
                let meta = ctx.page(&a.id)?;
                Ok(preview(&meta, &settings, now_ms()))
            },
        ))?;

        r.add(Operation::new(
            "review.clear",
            "Remove a page from the review schedule. The date of its last review and its history are kept.",
            object(json!({ "id": id_prop() }), &["id"]),
            |ctx: &mut Ctx, a: IdArgs| {
                let mut meta = ctx.page(&a.id)?;
                meta.interval_minutes = None;
                meta.next_review = None;
                meta.updated_at = now_ms();
                ctx.save(&meta)?;
                Ok(meta)
            },
        ))?;

        r.add(Operation::new(
            "review.due",
            "Pages due for review now, earliest first (at most the vault's queue limit, if one is set).",
            object(
                json!({
                    "now": { "type": ["integer", "null"], "description": "Reference timestamp in unix ms; defaults to now." }
                }),
                &[],
            ),
            |ctx: &mut Ctx, a: DueArgs| {
                let now = a.now.unwrap_or_else(now_ms);
                let mut due = ctx.store.due(now).map_err(|e| e.to_string())?;
                if let Some(limit) = load_settings(&ctx.store.vault).queue_limit {
                    due.truncate(limit);
                }
                Ok(due)
            },
        ))?;

        r.add(Operation::new(
            "review.stats",
            "Review overview: `dueTotal` (every page due now, ignoring the queue limit), `overdue` (due before today, UTC), `reviewedToday` (reviews completed since UTC midnight), `upcoming` (pages coming due on each of the next 14 UTC days) and `totalReviews` (all completed reviews).",
            object(
                json!({
                    "now": { "type": ["integer", "null"], "description": "Reference timestamp in unix ms; defaults to now." }
                }),
                &[],
            ),
            |ctx: &mut Ctx, a: StatsArgs| stats(ctx, a.now),
        ))?;
        r.add(Operation::new(
            "review.settings",
            "The vault's review settings: preset intervals in minutes, the strategy used by `review.complete` (adaptive FSRS with `desiredRetention` by default), and queue options.",
            object(json!({}), &[]),
            |ctx: &mut Ctx, _: NoArgs| read_settings(&ctx.store.vault),
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
                        "strategy": { "type": "string", "enum": ["same", "ladder", "multiply", "fsrs"], "description": "How `review.complete` picks the next interval; `fsrs` is the adaptive default." },
                        "multiplier": { "type": "number", "minimum": 1.1, "maximum": 10 },
                        "maxIntervalMinutes": { "type": "integer", "minimum": 1, "maximum": MAX_MINUTES },
                        "desiredRetention": { "type": "number", "minimum": MIN_RETENTION, "maximum": MAX_RETENTION, "description": "Target recall probability for the `fsrs` strategy." },
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
        ReviewSettings {
            strategy,
            ..Default::default()
        }
    }

    #[test]
    fn ladder_climbs_the_presets_and_stays_on_the_last() {
        let s = with(Strategy::Ladder);
        assert_eq!(s.next_interval(None), DAY);
        assert_eq!(s.next_interval(Some(DAY)), 3 * DAY);
        assert_eq!(
            s.next_interval(Some(5 * DAY)),
            7 * DAY,
            "a custom interval steps to the next preset above it"
        );
        assert_eq!(s.next_interval(Some(30 * DAY)), 30 * DAY);
        assert_eq!(
            s.next_interval(Some(45 * DAY)),
            45 * DAY,
            "never shrinks past the top preset"
        );
    }

    #[test]
    fn ladder_mixes_minutes_hours_and_days() {
        let s = ReviewSettings {
            presets: vec![30, 120, DAY],
            ..with(Strategy::Ladder)
        };
        assert_eq!(s.next_interval(None), 30);
        assert_eq!(s.next_interval(Some(30)), 120);
        assert_eq!(s.next_interval(Some(120)), DAY);
    }

    #[test]
    fn multiply_grows_at_least_one_minute_and_respects_the_cap() {
        let s = ReviewSettings {
            multiplier: 1.1,
            max_interval_minutes: 20,
            ..with(Strategy::Multiply)
        };
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
    fn non_fsrs_hard_repeats_and_easy_advances_twice() {
        let ladder = with(Strategy::Ladder);
        assert_eq!(
            ladder.non_fsrs_interval(Some(3 * DAY), Grade::Hard),
            3 * DAY
        );
        assert_eq!(ladder.non_fsrs_interval(None, Grade::Hard), DAY);
        assert_eq!(
            ladder.non_fsrs_interval(Some(3 * DAY), Grade::Easy),
            30 * DAY
        );
        assert_eq!(
            ladder.non_fsrs_interval(Some(3 * DAY), Grade::Good),
            7 * DAY
        );
        assert_eq!(ladder.non_fsrs_interval(Some(3 * DAY), Grade::Again), DAY);

        let multiply = ReviewSettings {
            strategy: Strategy::Multiply,
            multiplier: 2.0,
            ..Default::default()
        };
        assert_eq!(multiply.non_fsrs_interval(Some(1440), Grade::Hard), 1440);
        assert_eq!(multiply.non_fsrs_interval(Some(1440), Grade::Easy), 5760);
    }

    #[test]
    fn validation_normalises_presets_and_rejects_bad_values() {
        let ok = ReviewSettings {
            presets: vec![30, 3, 3, 1],
            desired_retention: MIN_RETENTION,
            ..Default::default()
        }
        .validate()
        .unwrap();
        assert_eq!(ok.presets, vec![1, 3, 30]);
        assert_eq!(ReviewSettings::default().desired_retention, 0.9);

        for bad in [
            ReviewSettings {
                presets: vec![],
                ..Default::default()
            },
            ReviewSettings {
                presets: vec![0],
                ..Default::default()
            },
            ReviewSettings {
                presets: vec![MAX_MINUTES + 1],
                ..Default::default()
            },
            ReviewSettings {
                presets: (1..=13).collect(),
                ..Default::default()
            },
            ReviewSettings {
                multiplier: 1.0,
                ..Default::default()
            },
            ReviewSettings {
                multiplier: f64::NAN,
                ..Default::default()
            },
            ReviewSettings {
                max_interval_minutes: 0,
                ..Default::default()
            },
            ReviewSettings {
                desired_retention: MIN_RETENTION - 0.01,
                ..Default::default()
            },
            ReviewSettings {
                desired_retention: MAX_RETENTION + 0.01,
                ..Default::default()
            },
            ReviewSettings {
                desired_retention: f64::NAN,
                ..Default::default()
            },
            ReviewSettings {
                import_review_minutes: Some(0),
                ..Default::default()
            },
            ReviewSettings {
                queue_limit: Some(0),
                ..Default::default()
            },
        ] {
            assert!(bad.clone().validate().is_err(), "{bad:?}");
        }
    }
}

#[cfg(test)]
mod history_tests {
    use super::*;

    #[test]
    fn complete_logs_grades_and_keeps_the_last_hundred() {
        let s = ReviewSettings {
            strategy: Strategy::Ladder,
            ..Default::default()
        };
        let mut meta = PageMeta::new("p".into());
        apply_schedule(&mut meta, 7 * 1440);
        assert_eq!(meta.last_review, None, "scheduling is not a review");

        complete(&mut meta, &s, Grade::Good);
        assert_eq!(meta.interval_minutes, Some(30 * 1440));
        assert_eq!(meta.last_review, Some(meta.updated_at));
        complete(&mut meta, &s, Grade::Again);
        assert_eq!(
            meta.interval_minutes,
            Some(1440),
            "again goes back to the first preset"
        );

        let h = history(&meta);
        assert_eq!(h.count, 2);
        assert_eq!(
            h.log.iter().map(|e| e.grade).collect::<Vec<_>>(),
            vec![Grade::Good, Grade::Again]
        );

        for _ in 0..150 {
            complete(&mut meta, &s, Grade::Good);
        }
        let h = history(&meta);
        assert_eq!(h.count, 152);
        assert_eq!(h.log.len(), MAX_LOG);
        assert_eq!(
            h.days.values().sum::<u64>(),
            152,
            "the daily tally is not cut with the log"
        );
    }

    #[test]
    fn day_tally_survives_log_truncation_and_legacy_histories() {
        let day = 40 * DAY_MS;
        let mut legacy = PageMeta::new("p".into());
        legacy.ext.insert(
            HISTORY_KEY.into(),
            json!({ "count": 2, "log": [
                { "at": day + 1, "grade": "good", "intervalMinutes": 1 },
                { "at": day + 2, "grade": "good", "intervalMinutes": 1 }
            ] }),
        );
        let h = history(&legacy);
        assert_eq!(h.on_day(day), 2, "legacy log still counted");

        let s = ReviewSettings::default();
        let mut meta = PageMeta::new("q".into());
        for _ in 0..(MAX_LOG + 25) {
            complete(&mut meta, &s, Grade::Good);
        }
        let h = history(&meta);
        assert_eq!(h.log.len(), MAX_LOG);
        let total: u64 = h.days.values().sum();
        assert_eq!(total, (MAX_LOG + 25) as u64);
    }

    #[test]
    fn utc_days_are_labelled() {
        assert_eq!(utc_midnight(DAY_MS + 5), DAY_MS);
        assert_eq!(day_label(DAY_MS), "1970-01-02");
    }

    #[test]
    fn utc_midnight_handles_boundaries_and_negative_epochs() {
        assert_eq!(utc_midnight(0), 0);
        assert_eq!(utc_midnight(DAY_MS - 1), 0);
        assert_eq!(utc_midnight(DAY_MS), DAY_MS);
        assert_eq!(utc_midnight(-1), -DAY_MS);
        assert_eq!(utc_midnight(-DAY_MS), -DAY_MS);
        assert_eq!(utc_midnight(-DAY_MS - 1), -2 * DAY_MS);
    }

    #[test]
    fn history_tolerates_missing_count_and_bounds_count() {
        let mut meta = PageMeta::new("p".into());
        meta.ext.insert(
            HISTORY_KEY.into(),
            json!({
                "log": [
                    { "at": 1000, "grade": "good", "intervalMinutes": 1440 },
                    { "at": 2000, "grade": "again", "intervalMinutes": 1440 }
                ]
            }),
        );
        let h = history(&meta);
        assert_eq!(h.count, 2, "count defaulted to at least log.len()");
        assert_eq!(h.log.len(), 2);
    }
}
