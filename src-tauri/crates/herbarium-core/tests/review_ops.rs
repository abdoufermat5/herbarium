// Behavior of the review operations, driven through `Host::call`.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use herbarium_core::time::{DAY_MS, now_ms};
use herbarium_core::{Caller, Host};
use serde_json::{Value, json};

const DAY: i64 = 1440;

fn utc_midnight(ms: i64) -> i64 {
    ms.div_euclid(DAY_MS) * DAY_MS
}

fn temp_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "herbarium-review-{tag}-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn open(vault: &Path) -> Host {
    let mut host = Host::new();
    host.open_vault(vault.to_str().unwrap()).unwrap();
    host
}

fn call(host: &Host, op: &str, args: Value) -> Value {
    host.call(Caller::Ui, op, args).unwrap()
}

fn create(host: &Host, title: &str) -> String {
    let html = format!(
        "<!doctype html><html><head><title>{title}</title></head><body><p>{title}</p></body></html>"
    );
    let page = call(host, "pages.create", json!({ "html": html }));
    page["id"].as_str().unwrap().to_string()
}

/// Make a root-folder page due at `next_review` by editing its sidecar on disk
/// (pushing its mtime forward so the rescan notices), then rescan.
fn set_due(host: &Host, vault: &Path, ids: &[&str], next_review: i64) {
    for id in ids {
        let path = vault.join(format!("{id}.json"));
        let mut meta: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        meta["nextReview"] = json!(next_review);
        meta["intervalMinutes"] = json!(DAY);
        std::fs::write(&path, serde_json::to_string(&meta).unwrap()).unwrap();
        let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_modified(SystemTime::now() + Duration::from_secs(10))
            .unwrap();
    }
    call(host, "vault.rescan", json!({}));
}

#[test]
fn schedule_does_not_set_last_review_and_clear_keeps_it() {
    let vault = temp_vault("schedule");
    let host = open(&vault);
    let id = create(&host, "Schedule");

    let page = call(
        &host,
        "review.schedule",
        json!({ "id": id, "intervalMinutes": 60 }),
    );
    assert_eq!(page["intervalMinutes"], 60);
    assert!(page["nextReview"].is_i64());
    assert!(page.get("lastReview").is_none_or(Value::is_null), "{page}");

    let done = call(&host, "review.complete", json!({ "id": id }));
    let last = done["lastReview"].as_i64().unwrap();

    let cleared = call(&host, "review.clear", json!({ "id": id }));
    assert!(cleared.get("nextReview").is_none_or(Value::is_null));
    assert!(cleared.get("intervalMinutes").is_none_or(Value::is_null));
    assert_eq!(
        cleared["lastReview"], last,
        "clear keeps the last review date"
    );
    assert_eq!(
        cleared["ext"]["review"]["count"], 1,
        "clear keeps the history"
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn complete_good_climbs_and_again_resets_with_history() {
    let vault = temp_vault("complete");
    let host = open(&vault);
    let id = create(&host, "Complete");
    call(&host, "review.configure", json!({ "strategy": "ladder" }));

    call(
        &host,
        "review.schedule",
        json!({ "id": id, "intervalMinutes": 3 * DAY }),
    );
    let good = call(
        &host,
        "review.complete",
        json!({ "id": id, "grade": "good" }),
    );
    assert_eq!(good["intervalMinutes"], 7 * DAY, "ladder steps up");
    assert!(good["lastReview"].is_i64());

    let again = call(
        &host,
        "review.complete",
        json!({ "id": id, "grade": "again" }),
    );
    assert_eq!(
        again["intervalMinutes"], DAY,
        "again resets to the first preset"
    );

    let default = call(&host, "review.complete", json!({ "id": id }));
    assert_eq!(
        default["intervalMinutes"],
        3 * DAY,
        "grade defaults to good"
    );

    let history = &default["ext"]["review"];
    assert_eq!(history["count"], 3);
    let log = history["log"].as_array().unwrap();
    let grades: Vec<&str> = log.iter().map(|e| e["grade"].as_str().unwrap()).collect();
    assert_eq!(grades, ["good", "again", "good"]);
    let intervals: Vec<i64> = log
        .iter()
        .map(|e| e["intervalMinutes"].as_i64().unwrap())
        .collect();
    assert_eq!(intervals, [7 * DAY, DAY, 3 * DAY]);
    assert_eq!(log[2]["at"], default["lastReview"]);

    // The history survives a reload from the sidecar.
    let reopened = open(&vault);
    let reread = call(&reopened, "pages.get", json!({ "id": id }));
    assert_eq!(reread["meta"]["ext"]["review"]["count"], 3);

    let err = host
        .call(
            Caller::Ui,
            "review.complete",
            json!({ "id": id, "grade": "nope" }),
        )
        .unwrap_err();
    assert!(!err.is_empty());

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn history_log_keeps_the_last_hundred_entries() {
    let vault = temp_vault("cap");
    let host = open(&vault);
    let id = create(&host, "Cap");

    let mut page = Value::Null;
    for _ in 0..105 {
        page = call(&host, "review.complete", json!({ "id": id }));
    }
    assert_eq!(page["ext"]["review"]["count"], 105);
    assert_eq!(page["ext"]["review"]["log"].as_array().unwrap().len(), 100);

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn stats_count_every_due_page_beyond_the_queue_limit() {
    let vault = temp_vault("stats");
    let host = open(&vault);
    let ids: Vec<String> = (0..3).map(|i| create(&host, &format!("Due {i}"))).collect();
    let later = create(&host, "Later");
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();

    let now = now_ms();
    let today = utc_midnight(now);

    call(&host, "review.configure", json!({ "queueLimit": 1 }));
    // Explicit boundaries:
    // refs[..2]: due today (at UTC midnight today), so next <= now (due), next >= today (not overdue)
    set_due(&host, &vault, &refs[..2], today);
    // refs[2..]: due 3 days before today, so next < today (overdue) and next <= now (due)
    set_due(&host, &vault, &refs[2..], today - 3 * DAY_MS);
    // later: scheduled for 2 days from today (at noon on day 2), so upcoming[2] is exact and impervious to midnight rollover
    set_due(
        &host,
        &vault,
        &[&later],
        today + 2 * DAY_MS + 12 * 3600 * 1000,
    );

    let due = call(&host, "review.due", json!({ "now": now }));
    assert_eq!(due.as_array().unwrap().len(), 1, "the queue is capped");

    let stats = call(&host, "review.stats", json!({ "now": now }));
    assert_eq!(stats["dueTotal"], 3, "{stats}");
    assert_eq!(
        stats["overdue"], 1,
        "only the page due before today is overdue"
    );
    assert_eq!(
        stats["reviewedToday"], 0,
        "rescans and schedules are not reviews"
    );
    assert_eq!(stats["totalReviews"], 0);
    let upcoming = stats["upcoming"].as_array().unwrap();
    assert_eq!(upcoming.len(), 14);
    assert_eq!(
        upcoming
            .iter()
            .map(|d| d["count"].as_u64().unwrap())
            .sum::<u64>(),
        1
    );
    assert_eq!(upcoming[2]["count"], 1, "{upcoming:?}");
    assert_eq!(upcoming[0]["day"].as_str().unwrap().len(), 10);

    // Calling stats without args also works
    let default_stats = call(&host, "review.stats", json!({}));
    assert_eq!(default_stats["dueTotal"], 3);

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn stats_with_fixed_clock_helper_and_explicit_boundaries() {
    let vault = temp_vault("fixed-clock");
    let host = open(&vault);
    let p_overdue = create(&host, "Overdue");
    let p_due_today = create(&host, "DueToday");
    let p_due_later_today = create(&host, "DueLaterToday");
    let p_day2 = create(&host, "Day2");
    let p_day13 = create(&host, "Day13");
    let p_day14_beyond = create(&host, "Day14Beyond");

    // Fixed reference time: 2023-11-14 22:13:20 UTC
    let fixed_now = 1_700_000_000_000_i64;
    let fixed_today = utc_midnight(fixed_now);

    set_due(&host, &vault, &[&p_overdue], fixed_today - 1000); // 1 sec before today => overdue
    set_due(&host, &vault, &[&p_due_today], fixed_today + 1000); // 1 sec after today's midnight => due today, not overdue
    set_due(&host, &vault, &[&p_due_later_today], fixed_now + 1000); // 1 sec after fixed_now => upcoming today (slot 0)
    set_due(&host, &vault, &[&p_day2], fixed_today + 2 * DAY_MS + 1000); // 2 days from today => upcoming[2]
    set_due(&host, &vault, &[&p_day13], fixed_today + 13 * DAY_MS + 1000); // 13 days => upcoming[13]
    set_due(
        &host,
        &vault,
        &[&p_day14_beyond],
        fixed_today + 14 * DAY_MS + 1000,
    ); // 14 days => out of 14-day window

    let stats = call(&host, "review.stats", json!({ "now": fixed_now }));
    assert_eq!(stats["dueTotal"], 2, "overdue + due today");
    assert_eq!(stats["overdue"], 1, "only 1 page is overdue");
    let upcoming = stats["upcoming"].as_array().unwrap();
    assert_eq!(upcoming.len(), 14);
    assert_eq!(upcoming[0]["count"], 1, "due later today is upcoming[0]");
    assert_eq!(upcoming[2]["count"], 1, "day 2 is upcoming[2]");
    assert_eq!(upcoming[13]["count"], 1, "day 13 is upcoming[13]");
    assert_eq!(
        upcoming
            .iter()
            .map(|d| d["count"].as_u64().unwrap())
            .sum::<u64>(),
        3
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn reviewed_today_counts_completes_only() {
    let vault = temp_vault("today");
    let host = open(&vault);
    let a = create(&host, "A");
    let b = create(&host, "B");
    let past = create(&host, "Past");

    call(
        &host,
        "review.schedule",
        json!({ "id": a, "intervalMinutes": 60 }),
    );
    call(
        &host,
        "review.schedule",
        json!({ "id": b, "intervalMinutes": 60 }),
    );
    call(&host, "review.clear", json!({ "id": b }));
    assert_eq!(call(&host, "review.stats", json!({}))["reviewedToday"], 0);

    call(&host, "review.complete", json!({ "id": a }));
    call(
        &host,
        "review.complete",
        json!({ "id": a, "grade": "again" }),
    );
    call(&host, "review.complete", json!({ "id": b }));

    // Simulate an old review completed yesterday on `past`
    let now = now_ms();
    let today = utc_midnight(now);
    let past_path = vault.join(format!("{past}.json"));
    let mut past_meta: Value =
        serde_json::from_str(&std::fs::read_to_string(&past_path).unwrap()).unwrap();
    past_meta["lastReview"] = json!(today - DAY_MS);
    past_meta["ext"] = json!({
        "review": {
            "count": 2,
            "log": [
                { "at": today - 2 * DAY_MS, "grade": "good", "intervalMinutes": 1440 },
                { "at": today - DAY_MS, "grade": "good", "intervalMinutes": 3 * 1440 }
            ]
        }
    });
    std::fs::write(&past_path, serde_json::to_string(&past_meta).unwrap()).unwrap();
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&past_path)
        .unwrap();
    file.set_modified(SystemTime::now() + Duration::from_secs(10))
        .unwrap();
    call(&host, "vault.rescan", json!({}));

    let stats = call(&host, "review.stats", json!({ "now": now }));
    assert_eq!(
        stats["reviewedToday"], 3,
        "only the 3 reviews completed today are counted"
    );
    assert_eq!(
        stats["totalReviews"], 5,
        "all 5 reviews across history are counted in totalReviews"
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn corrupt_settings_are_reported_and_can_be_replaced() {
    let vault = temp_vault("corrupt");
    let host = open(&vault);
    let id = create(&host, "Corrupt");

    let file = vault.join(".herbarium").join("review.json");
    std::fs::write(&file, "{ not json").unwrap();

    let err = host
        .call(Caller::Ui, "review.settings", json!({}))
        .unwrap_err();
    assert!(err.contains("invalid"), "{err}");
    let err = host
        .call(Caller::Ui, "review.complete", json!({ "id": id }))
        .unwrap_err();
    assert!(err.contains("invalid"), "{err}");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "{ not json",
        "the file is left alone"
    );

    // Invalid configure payload must not overwrite or corrupt configuration
    let bad_err = host
        .call(Caller::Ui, "review.configure", json!({ "presets": [] }))
        .unwrap_err();
    assert!(!bad_err.is_empty());
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "{ not json",
        "invalid configure did not overwrite"
    );

    // Valid configure replaces the corrupt settings
    let saved = call(&host, "review.configure", json!({ "presets": [30, 60] }));
    assert_eq!(saved["presets"], json!([30, 60]));
    assert_eq!(
        call(&host, "review.settings", json!({}))["presets"],
        json!([30, 60])
    );

    // Now with valid configuration, invalid configure payload must NOT change it
    let bad_mult = host
        .call(Caller::Ui, "review.configure", json!({ "multiplier": 0.5 }))
        .unwrap_err();
    assert!(!bad_mult.is_empty());
    assert_eq!(
        call(&host, "review.settings", json!({}))["presets"],
        json!([30, 60]),
        "valid config was preserved"
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn missing_settings_mean_defaults() {
    let vault = temp_vault("missing");
    let host = open(&vault);
    let settings = call(&host, "review.settings", json!({}));
    assert_eq!(
        settings["presets"],
        json!([DAY, 3 * DAY, 7 * DAY, 30 * DAY])
    );
    assert_eq!(
        settings["strategy"], "fsrs",
        "adaptive review is the default"
    );
    assert_eq!(settings["desiredRetention"].as_f64().unwrap(), 0.9);
    let _ = std::fs::remove_dir_all(&vault);
}

/// Rewrite a page's sidecar on disk (pushing its mtime forward) and rescan, so
/// the change is picked up. Used to simulate sidecars written by other versions.
fn write_sidecar(host: &Host, vault: &Path, id: &str, meta: &Value) {
    let path = vault.join(format!("{id}.json"));
    std::fs::write(&path, serde_json::to_string(meta).unwrap()).unwrap();
    let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_modified(SystemTime::now() + Duration::from_secs(10))
        .unwrap();
    call(host, "vault.rescan", json!({}));
}

fn preview(host: &Host, id: &str) -> Value {
    call(host, "review.preview", json!({ "id": id }))
}

#[test]
fn fsrs_default_preview_orders_grades_and_matches_complete() {
    let vault = temp_vault("fsrs");
    let host = open(&vault);
    let id = create(&host, "Adaptive");

    let p = preview(&host, &id);
    let again = p["again"].as_i64().unwrap();
    let hard = p["hard"].as_i64().unwrap();
    let good = p["good"].as_i64().unwrap();
    let easy = p["easy"].as_i64().unwrap();
    assert!(again < hard, "again < hard: {p}");
    assert!(hard <= good && good <= easy, "hard <= good <= easy: {p}");
    assert_eq!(again, DAY, "again is the first preset (relearning step)");

    let done = call(
        &host,
        "review.complete",
        json!({ "id": id, "grade": "good" }),
    );
    assert_eq!(done["intervalMinutes"], good, "complete matches preview");
    let h = &done["ext"]["review"];
    let stability = h["stability"].as_f64().unwrap();
    assert!(stability > 0.0, "{h}");
    let difficulty = h["difficulty"].as_f64().unwrap();
    assert!((1.0..=10.0).contains(&difficulty), "{h}");

    // A same-day follow-up still matches the fresh preview and does not shrink.
    let p2 = preview(&host, &id);
    let done2 = call(
        &host,
        "review.complete",
        json!({ "id": id, "grade": "easy" }),
    );
    assert_eq!(
        done2["intervalMinutes"], p2["easy"],
        "matches preview after a review"
    );
    let h2 = &done2["ext"]["review"];
    assert!(h2["stability"].as_f64().unwrap() >= stability, "{h2}");

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn higher_desired_retention_shortens_the_interval() {
    let vault = temp_vault("retention-order");
    let host = open(&vault);
    let id = create(&host, "Retention");

    call(
        &host,
        "review.configure",
        json!({ "desiredRetention": 0.90 }),
    );
    let relaxed = preview(&host, &id)["good"].as_i64().unwrap();
    call(
        &host,
        "review.configure",
        json!({ "desiredRetention": 0.97 }),
    );
    let strict = preview(&host, &id)["good"].as_i64().unwrap();
    assert!(strict < relaxed, "0.97 ({strict}) < 0.90 ({relaxed})");

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn desired_retention_is_validated() {
    let vault = temp_vault("retention-bounds");
    let host = open(&vault);
    for bad in [0.69, 0.98, 0.0, -1.0] {
        let err = host
            .call(
                Caller::Ui,
                "review.configure",
                json!({ "desiredRetention": bad }),
            )
            .unwrap_err();
        assert!(!err.is_empty(), "accepted {bad}");
    }
    for ok in [0.70, 0.90, 0.97] {
        let saved = call(&host, "review.configure", json!({ "desiredRetention": ok }));
        assert_eq!(saved["desiredRetention"].as_f64().unwrap(), ok);
    }
    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn legacy_sidecar_without_memory_loads_and_completes() {
    let vault = temp_vault("legacy-fsrs");
    let host = open(&vault);
    let id = create(&host, "Legacy");

    // A 0.1 sidecar: history entries with no stability/difficulty.
    let path = vault.join(format!("{id}.json"));
    let mut meta: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    meta["nextReview"] = json!(now_ms() + DAY_MS);
    meta["intervalMinutes"] = json!(DAY);
    meta["ext"] = json!({
        "review": {
            "count": 2,
            "log": [
                { "at": 1000, "grade": "good", "intervalMinutes": 1440 },
                { "at": 2000, "grade": "again", "intervalMinutes": 1440 }
            ]
        }
    });
    write_sidecar(&host, &vault, &id, &meta);

    let p = preview(&host, &id);
    assert!(p["good"].as_i64().unwrap() > 0, "{p}");
    let done = call(
        &host,
        "review.complete",
        json!({ "id": id, "grade": "good" }),
    );
    assert_eq!(
        done["intervalMinutes"], p["good"],
        "initialized from the grade"
    );
    let h = &done["ext"]["review"];
    assert_eq!(h["count"], 3, "the old history is kept");
    assert!(h["stability"].as_f64().unwrap() > 0.0, "{h}");
    assert!(h["difficulty"].as_f64().unwrap() >= 1.0, "{h}");

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn non_fsrs_hard_and_easy_interval_rules() {
    let vault = temp_vault("non-fsrs-grades");
    let host = open(&vault);
    let id = create(&host, "Ladder");
    call(&host, "review.configure", json!({ "strategy": "ladder" }));
    call(
        &host,
        "review.schedule",
        json!({ "id": id, "intervalMinutes": 3 * DAY }),
    );

    // Hard repeats the current interval and records no FSRS state.
    let hard = call(
        &host,
        "review.complete",
        json!({ "id": id, "grade": "hard" }),
    );
    assert_eq!(hard["intervalMinutes"], 3 * DAY);
    assert!(
        hard["ext"]["review"]
            .get("stability")
            .is_none_or(Value::is_null),
        "ladder scheduling leaves the memory state alone: {hard}"
    );

    // Easy advances twice: 3 days -> 7 days -> 30 days.
    let easy = call(
        &host,
        "review.complete",
        json!({ "id": id, "grade": "easy" }),
    );
    assert_eq!(easy["intervalMinutes"], 30 * DAY);

    // Again restarts at the first preset; Good advances once.
    let again = call(
        &host,
        "review.complete",
        json!({ "id": id, "grade": "again" }),
    );
    assert_eq!(again["intervalMinutes"], DAY);
    let good = call(
        &host,
        "review.complete",
        json!({ "id": id, "grade": "good" }),
    );
    assert_eq!(good["intervalMinutes"], 3 * DAY);

    let _ = std::fs::remove_dir_all(&vault);
}
