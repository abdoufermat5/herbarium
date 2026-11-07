// Today: what to do with the library right now. Pages due for review, the
// next page of each reading path, recent saves, a page to rediscover and
// pages saved on this day in earlier months. Reading is tracked in
// `.herbarium/reads.json` (last time and count per page), never in the page's
// files, so opening a page does not touch what sync tools see.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::paths;
use super::review::{current_streak, load_settings};
use super::{id_prop, object};
use crate::extension::{Ctx, Extension, OpResult, Operation, Registry, events};
use crate::models::PageMeta;
use crate::time::{DAY_MS, now_ms};

pub(crate) struct Today;

/// How many entries each list of the summary holds.
const LIST: usize = 6;
/// A page not opened for this long can be rediscovered.
const REDISCOVER_AFTER: i64 = 30 * DAY_MS;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Read {
    /// Last time the page was opened, unix ms.
    pub last: i64,
    pub count: u64,
}

fn reads_path(vault: &Path) -> PathBuf {
    vault.join(".herbarium").join("reads.json")
}

/// What identifies a version of `reads.json` on disk.
type Stamp = Option<(std::time::SystemTime, u64)>;

/// The parsed `reads.json` per vault, with the stamp of the file it came from.
/// Opening a page reloads it only when another process changed the file.
static READS_CACHE: Mutex<BTreeMap<PathBuf, CachedReads>> = Mutex::new(BTreeMap::new());

type CachedReads = (Stamp, Arc<BTreeMap<String, Read>>);

fn stamp_of(path: &Path) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

pub(crate) fn load_reads(vault: &Path) -> Arc<BTreeMap<String, Read>> {
    let path = reads_path(vault);
    let stamp = stamp_of(&path);
    let mut cache = READS_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((cached, reads)) = cache.get(&path)
        && *cached == stamp
    {
        return Arc::clone(reads);
    }
    let reads: BTreeMap<String, Read> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    let reads = Arc::new(reads);
    cache.insert(path, (stamp, Arc::clone(&reads)));
    reads
}

/// Replace `reads.json` (temp file + rename, no fsync: the file only holds
/// when and how often pages were opened, so losing the last open to a crash
/// costs nothing, while an fsync per opened page would be felt).
fn save_reads(vault: &Path, reads: BTreeMap<String, Read>) -> OpResult<()> {
    let path = reads_path(vault);
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create settings folder: {e}"))?;
    let bytes = serde_json::to_vec(&reads).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!(".reads.json.tmp-{}", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, &bytes)
        .and_then(|()| std::fs::rename(&tmp, &path))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("cannot write {}: {e}", path.display())
        })?;
    let stamp = stamp_of(&path);
    READS_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(path, (stamp, Arc::new(reads)));
    Ok(())
}

/// Month and day of `ms` (UTC), for "on this day".
fn month_day(ms: i64) -> (u32, u32) {
    use chrono::{Datelike, TimeZone, Utc};
    Utc.timestamp_millis_opt(ms)
        .single()
        .map(|d| (d.month(), d.day()))
        .unwrap_or((0, 0))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContinuePath {
    path_id: String,
    path_name: String,
    /// 0-based position of `page` in the path.
    position: usize,
    total: usize,
    page: PageMeta,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    due_total: usize,
    due: Vec<PageMeta>,
    #[serde(rename = "continue")]
    continue_paths: Vec<ContinuePath>,
    recent: Vec<PageMeta>,
    rediscover: Option<PageMeta>,
    on_this_day: Vec<PageMeta>,
    streak: usize,
    total_pages: usize,
}

fn summary(ctx: &Ctx, now: i64) -> OpResult<Summary> {
    let store = ctx.store;
    let vault = &store.vault;
    let pages = store.all().map_err(|e| e.to_string())?;
    let by_id: BTreeMap<&str, &PageMeta> = pages.iter().map(|m| (m.id.as_str(), m)).collect();
    let reads = load_reads(vault);
    let settings = load_settings(vault);

    let mut due: Vec<PageMeta> = store
        .due(now)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|m| !settings.excludes(m))
        .collect();
    let due_total = due.len();
    due.truncate(LIST);
    let due: Vec<PageMeta> = due.iter().map(PageMeta::review_summary).collect();

    // The first page of each path not opened since it was added; else the
    // page after the most recently opened one.
    let mut continue_paths = Vec::new();
    for path in paths::load(vault) {
        let present: Vec<&PageMeta> = path
            .pages
            .iter()
            .filter_map(|id| by_id.get(id.as_str()).copied())
            .collect();
        if present.is_empty() {
            continue;
        }
        let last_read = present
            .iter()
            .enumerate()
            .filter_map(|(i, m)| reads.get(&m.id).map(|r| (r.last, i)))
            .max();
        let next = match last_read {
            None => Some(0),
            Some((_, i)) => present
                .iter()
                .enumerate()
                .skip(i + 1)
                .find(|(_, m)| !reads.contains_key(&m.id))
                .map(|(j, _)| j)
                .or_else(|| (i + 1 < present.len()).then_some(i + 1)),
        };
        if let Some(i) = next {
            continue_paths.push(ContinuePath {
                path_id: path.id.clone(),
                path_name: path.name.clone(),
                position: i,
                total: present.len(),
                page: present[i].summary(),
            });
        }
    }

    let mut newest: Vec<&PageMeta> = pages.iter().collect();
    newest.sort_by_key(|m| std::cmp::Reverse(m.created_at));
    let recent: Vec<PageMeta> = newest.iter().take(LIST).map(|m| m.summary()).collect();

    // Not opened for a month (or never) and older than a month; the pick is
    // stable through the day, so it does not change on every refresh.
    let candidates: Vec<&PageMeta> = pages
        .iter()
        .filter(|m| m.created_at < now - REDISCOVER_AFTER)
        .filter(|m| {
            reads
                .get(&m.id)
                .is_none_or(|r| r.last < now - REDISCOVER_AFTER)
        })
        .collect();
    let rediscover = (!candidates.is_empty()).then(|| {
        let day = (now / DAY_MS) as usize;
        candidates[day.wrapping_mul(2_654_435_761) % candidates.len()].summary()
    });

    let today = month_day(now);
    let start_of_today = now.div_euclid(DAY_MS) * DAY_MS;
    let mut on_this_day: Vec<&PageMeta> = pages
        .iter()
        .filter(|m| m.created_at < start_of_today && month_day(m.created_at).1 == today.1)
        .filter(|m| month_day(m.created_at) == today || m.created_at < now - 27 * DAY_MS)
        .collect();
    on_this_day.sort_by_key(|m| std::cmp::Reverse(m.created_at));
    let on_this_day: Vec<PageMeta> = on_this_day.iter().take(LIST).map(|m| m.summary()).collect();

    Ok(Summary {
        due_total,
        due,
        continue_paths,
        recent,
        rediscover,
        on_this_day,
        streak: current_streak(&settings, &pages, now),
        total_pages: pages.len(),
    })
}

#[derive(Deserialize)]
struct IdArgs {
    id: String,
}

#[derive(Deserialize, Default)]
struct SummaryArgs {
    #[serde(default)]
    now: Option<i64>,
}

impl Extension for Today {
    fn id(&self) -> &str {
        "core.today"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.on(events::PAGE_DELETED, |store, event| {
            if let Some(id) = event.payload["page"]["id"].as_str() {
                let reads = load_reads(&store.vault);
                if reads.contains_key(id) {
                    let mut reads = (*reads).clone();
                    reads.remove(id);
                    let _ = save_reads(&store.vault, reads);
                }
            }
        });

        r.add(Operation::new(
            "today.summary",
            "What to do with the library today: `due` (pages due for review, up to 6, and `dueTotal`), `continue` (the next page of each reading path), `recent` (newest pages), `rediscover` (a page not opened for a month), `onThisDay` (pages saved on this day of an earlier month or year), the review `streak` and `totalPages`.",
            object(
                json!({ "now": { "type": ["integer", "null"], "description": "Reference timestamp in unix ms; defaults to now." } }),
                &[],
            ),
            |ctx: &mut Ctx, a: SummaryArgs| summary(ctx, a.now.unwrap_or_else(now_ms)),
        ))?;

        r.add(
            Operation::new(
                "reads.mark",
                "Record that the user opened a page now.",
                object(json!({ "id": id_prop() }), &["id"]),
                |ctx: &mut Ctx, a: IdArgs| {
                    ctx.page(&a.id)?;
                    let vault = &ctx.store.vault;
                    let mut reads = (*load_reads(vault)).clone();
                    let entry = reads.entry(a.id).or_default();
                    entry.last = now_ms();
                    entry.count += 1;
                    let read = *entry;
                    save_reads(vault, reads)?;
                    Ok(read)
                },
            )
            .ui_only(),
        )
    }
}
