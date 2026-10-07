// Today: due pages, reading paths to continue, recent saves, rediscovery
// and "on this day", with reads tracked outside the page files.

use std::path::{Path, PathBuf};

use herbarium_core::time::{DAY_MS, now_ms};
use herbarium_core::{Caller, Host};
use serde_json::{Value, json};

fn temp_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "herbarium-today-{tag}-{}-{}",
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

fn page(host: &Host, title: &str, created_at: i64) -> String {
    call(
        host,
        "pages.create",
        json!({ "html": format!("<title>{title}</title><p>{title}</p>"), "createdAt": created_at }),
    )["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn ids(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn summary_lists_due_paths_recent_rediscover_and_on_this_day() {
    let vault = temp_vault("summary");
    let host = open(&vault);
    let now = now_ms();
    let year_ago = now - 365 * DAY_MS;
    let old = page(&host, "Old", year_ago);
    let month_ago = page(&host, "Month", now - 40 * DAY_MS);
    let fresh = page(&host, "Fresh", now);
    let (a, b, c) = (
        page(&host, "A", now - 3 * DAY_MS),
        page(&host, "B", now - 2 * DAY_MS),
        page(&host, "C", now - DAY_MS),
    );
    call(
        &host,
        "paths.create",
        json!({ "name": "Course", "pages": [a, b, c] }),
    );
    call(
        &host,
        "review.schedule",
        json!({ "id": fresh, "intervalMinutes": 1 }),
    );

    let later = now + 2 * 60_000;
    let s = call(&host, "today.summary", json!({ "now": later }));
    assert_eq!(s["dueTotal"], 1);
    assert_eq!(ids(&s["due"]), std::slice::from_ref(&fresh));
    assert_eq!(
        s["continue"][0]["page"]["id"],
        a.as_str(),
        "a new path starts at its first page"
    );
    assert_eq!(s["continue"][0]["total"], 3);
    assert_eq!(ids(&s["recent"])[0], fresh, "newest first");
    let rediscover = s["rediscover"]["id"].as_str().unwrap();
    assert!(rediscover == old || rediscover == month_ago, "{rediscover}");
    assert!(
        ids(&s["onThisDay"]).contains(&old),
        "saved on this day a year ago"
    );
    assert_eq!(s["totalPages"], 6);

    // Reading A moves the path on to B; reading B too moves it to C.
    call(&host, "reads.mark", json!({ "id": a }));
    let s = call(&host, "today.summary", json!({}));
    assert_eq!(s["continue"][0]["page"]["id"], b.as_str());
    assert_eq!(s["continue"][0]["position"], 1);
    call(&host, "reads.mark", json!({ "id": b }));
    let s = call(&host, "today.summary", json!({}));
    assert_eq!(s["continue"][0]["page"]["id"], c.as_str());

    // A page read recently is not offered for rediscovery.
    call(&host, "reads.mark", json!({ "id": old }));
    call(&host, "reads.mark", json!({ "id": month_ago }));
    let s = call(&host, "today.summary", json!({}));
    assert!(s["rediscover"].is_null());

    // Reads live outside the page files and are forgotten with the page.
    let reads: Value = serde_json::from_str(
        &std::fs::read_to_string(vault.join(".herbarium/reads.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(reads[&a]["count"], 1);
    call(&host, "pages.delete", json!({ "id": a }));
    let reads: Value = serde_json::from_str(
        &std::fs::read_to_string(vault.join(".herbarium/reads.json")).unwrap(),
    )
    .unwrap();
    assert!(reads.get(&a).is_none());
    assert!(
        host.call(Caller::Agent, "reads.mark", json!({ "id": b }))
            .is_err(),
        "UI only"
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn samples_make_a_linked_getting_started_path_once() {
    let vault = temp_vault("samples");
    let host = open(&vault);
    let first = call(&host, "vault.add_samples", json!({}));
    assert_eq!(first["added"], 3);
    assert_eq!(
        first["pages"],
        json!([
            "herbarium-welcome",
            "herbarium-spaced-repetition",
            "herbarium-flexbox-playground"
        ])
    );
    let again = call(&host, "vault.add_samples", json!({}));
    assert_eq!(again["added"], 0, "samples are added once");
    assert_eq!(again["pages"], first["pages"]);

    let path = call(&host, "paths.get", json!({ "id": first["path"] }));
    assert_eq!(path["pages"].as_array().unwrap().len(), 3);
    assert_eq!(path["pages"][0]["folder"], "Getting started");
    let links = call(&host, "pages.links", json!({ "id": "herbarium-welcome" }));
    assert!(links["broken"].as_array().unwrap().is_empty());
    assert_eq!(links["backlinks"].as_array().unwrap().len(), 2);

    let s = call(
        &host,
        "today.summary",
        json!({ "now": now_ms() + 2 * 60_000 }),
    );
    assert_eq!(
        s["due"][0]["id"], "herbarium-welcome",
        "something to review on day one"
    );
    assert!(
        host.call(Caller::Agent, "vault.add_samples", json!({}))
            .is_err(),
        "UI only"
    );
    let _ = std::fs::remove_dir_all(&vault);
}
