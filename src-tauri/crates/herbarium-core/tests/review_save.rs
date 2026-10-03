// Review statistics past the bounded history, and optimistic saves that chain
// on the metadata returned by the previous save.

use std::fs::{self, File};
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

use herbarium_core::time::{DAY_MS, now_ms};
use herbarium_core::{Caller, Host};
use serde_json::json;

fn doc(body: &str) -> String {
    format!("<!doctype html><html><head><title>T</title></head><body><p>{body}</p></body></html>")
}

fn utc_midnight(ms: i64) -> i64 {
    ms.div_euclid(DAY_MS) * DAY_MS
}

fn open(tag: &str) -> (Host, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "herbarium-review-save-{tag}-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let _ = fs::remove_dir_all(&dir);
    let mut host = Host::new();
    host.open_vault(dir.to_str().unwrap()).unwrap();
    (host, dir)
}

fn create(host: &Host) -> String {
    host.call(Caller::Agent, "pages.create", json!({ "html": doc("one") }))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn reviewed_today_stays_exact_beyond_the_history_limit() {
    let (host, _dir) = open("stats");
    let id = create(&host);

    // `review.complete` reads the wall clock, so capture both ends of the run:
    // all 130 grades land inside [start, end]. Compare against the UTC days the
    // run actually touched rather than assuming it fits inside one day, which
    // would flake when the run crosses midnight.
    let start = now_ms();
    for _ in 0..130 {
        host.call(Caller::Agent, "review.complete", json!({ "id": id }))
            .unwrap();
    }
    let end = now_ms();

    let start_day = utc_midnight(start);
    let end_day = utc_midnight(end);
    let at_start = host
        .call(Caller::Agent, "review.stats", json!({ "now": start }))
        .unwrap();
    assert_eq!(
        at_start["totalReviews"], 130,
        "the history count is unbounded"
    );
    if start_day == end_day {
        assert_eq!(
            at_start["reviewedToday"], 130,
            "all 130 grades are counted for their UTC day, not capped at 100"
        );
    } else {
        // Midnight fell inside the run: the two UTC days together hold all 130.
        let at_end = host
            .call(Caller::Agent, "review.stats", json!({ "now": end }))
            .unwrap();
        assert_eq!(
            at_start["reviewedToday"].as_u64().unwrap() + at_end["reviewedToday"].as_u64().unwrap(),
            130
        );
    }
}

#[test]
fn consecutive_optimistic_saves_do_not_conflict_but_external_edits_do() {
    let (host, dir) = open("optimistic");
    let id = create(&host);
    let mut base = host
        .call(Caller::Agent, "pages.get", json!({ "id": id }))
        .unwrap()["meta"]["updatedAt"]
        .as_i64()
        .unwrap();

    for i in 0..25 {
        let saved = host
            .call(
                Caller::Agent,
                "pages.set_html",
                json!({ "id": id, "html": doc(&format!("rev {i}")), "baseUpdatedAt": base }),
            )
            .unwrap();
        base = saved["updatedAt"].as_i64().unwrap();
        let saved = host
            .call(
                Caller::Agent,
                "pages.update",
                json!({ "id": id, "note": format!("n{i}"), "baseUpdatedAt": base }),
            )
            .unwrap();
        base = saved["updatedAt"].as_i64().unwrap();
    }

    let html = dir.join(format!("{id}.html"));
    fs::write(&html, doc("edited elsewhere")).unwrap();
    File::options()
        .write(true)
        .open(&html)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_millis(base as u64 + 60_000))
        .unwrap();

    let err = host
        .call(
            Caller::Agent,
            "pages.set_html",
            json!({ "id": id, "html": doc("mine"), "baseUpdatedAt": base }),
        )
        .unwrap_err();
    assert!(err.starts_with("conflict:"), "{err}");
}
