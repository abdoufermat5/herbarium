// Page version history: `history.list` / `history.get` / `history.restore`,
// the snapshots `pages.set_html` takes, retention, and lifecycle (purge,
// reused ids), driven through `Host::call`.

use std::path::{Path, PathBuf};

use herbarium_core::time::now_ms;
use herbarium_core::{Caller, Host};
use serde_json::{Value, json};

fn doc(body: &str) -> String {
    format!(
        "<!doctype html><html><head><title>History</title></head><body><p>{body}</p></body></html>"
    )
}

fn temp_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "herbarium-history-{tag}-{}-{}",
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

fn create(host: &Host, title: &str) -> String {
    let page = host
        .call(
            Caller::Agent,
            "pages.create",
            json!({ "html": doc(title), "title": title }),
        )
        .unwrap();
    page["id"].as_str().unwrap().to_string()
}

fn html_of(host: &Host, id: &str) -> String {
    host.call(
        Caller::Agent,
        "pages.get",
        json!({ "id": id, "format": "html" }),
    )
    .unwrap()["html"]
        .as_str()
        .unwrap()
        .to_string()
}

fn updated_at(host: &Host, id: &str) -> i64 {
    host.call(Caller::Agent, "pages.get", json!({ "id": id }))
        .unwrap()["meta"]["updatedAt"]
        .as_i64()
        .unwrap()
}

fn versions(host: &Host, id: &str) -> Vec<Value> {
    host.call(Caller::Agent, "history.list", json!({ "id": id }))
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

fn set_html(host: &Host, caller: Caller, id: &str, html: &str) -> Value {
    host.call(caller, "pages.set_html", json!({ "id": id, "html": html }))
        .unwrap()
}

#[test]
fn set_html_snapshots_the_previous_html() {
    let vault = temp_vault("snapshot");
    let host = open(&vault);
    let id = create(&host, "Snapshot Page");
    let original = html_of(&host, &id);

    set_html(&host, Caller::Agent, &id, &doc("revision 1"));

    let list = versions(&host, &id);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["caller"], "agent");
    assert_eq!(list[0]["bytes"].as_u64().unwrap() as usize, original.len());

    let at = list[0]["at"].as_i64().unwrap();
    let got = host
        .call(Caller::Agent, "history.get", json!({ "id": id, "at": at }))
        .unwrap();
    assert_eq!(got["html"], original);

    // The UI caller is recorded distinctly.
    set_html(&host, Caller::Ui, &id, &doc("revision 2"));
    let list = versions(&host, &id);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["caller"], "ui");
    assert_eq!(
        list[0]["bytes"].as_u64().unwrap() as usize,
        doc("revision 1").len()
    );
}

#[test]
fn identical_html_creates_no_snapshot() {
    let vault = temp_vault("identical");
    let host = open(&vault);
    let id = create(&host, "Same Page");
    let current = html_of(&host, &id);

    set_html(&host, Caller::Agent, &id, &current);
    assert!(versions(&host, &id).is_empty());

    // A real change does snapshot the identical-write's content exactly once.
    set_html(&host, Caller::Agent, &id, &doc("changed"));
    let list = versions(&host, &id);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["bytes"].as_u64().unwrap() as usize, current.len());
}

#[test]
fn retention_keeps_the_newest_fifty() {
    let vault = temp_vault("retention");
    let host = open(&vault);
    let id = create(&host, "Many Versions");

    for i in 1..=55 {
        set_html(&host, Caller::Agent, &id, &doc(&format!("version {i}")));
    }

    let list = versions(&host, &id);
    assert_eq!(list.len(), 50);
    // Snapshot contents run from the original page (v0) up to v54; the oldest
    // five were pruned, so the newest is v54 and the oldest kept is v5.
    let newest = list[0]["at"].as_i64().unwrap();
    let oldest = list[49]["at"].as_i64().unwrap();
    let get = |at: i64| {
        host.call(Caller::Agent, "history.get", json!({ "id": id, "at": at }))
            .unwrap()["html"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(get(newest), doc("version 54"));
    assert_eq!(get(oldest), doc("version 5"));
}

#[test]
fn restore_round_trips_and_snapshots_itself() {
    let vault = temp_vault("restore");
    let host = open(&vault);
    let id = create(&host, "Restore Page");
    let original = html_of(&host, &id);

    set_html(&host, Caller::Agent, &id, &doc("regressed"));
    let list = versions(&host, &id);
    assert_eq!(list.len(), 1);
    let at = list[0]["at"].as_i64().unwrap();

    let restored = host
        .call(
            Caller::Agent,
            "history.restore",
            json!({ "id": id, "at": at, "expectedUpdatedAt": updated_at(&host, &id) }),
        )
        .unwrap();
    assert_eq!(restored["id"], id);
    assert_eq!(html_of(&host, &id), original);

    // The overwrite the restore replaced is now itself recoverable, and the
    // restored original version is still listed.
    let list = versions(&host, &id);
    assert_eq!(list.len(), 2);
    let newest = list[0]["at"].as_i64().unwrap();
    let got = host
        .call(
            Caller::Agent,
            "history.get",
            json!({ "id": id, "at": newest }),
        )
        .unwrap();
    assert_eq!(got["html"], doc("regressed"));
}

#[test]
fn stale_expected_updated_at_is_rejected() {
    let vault = temp_vault("conflict");
    let host = open(&vault);
    let id = create(&host, "Conflict Page");
    set_html(&host, Caller::Agent, &id, &doc("first"));
    let at = versions(&host, &id)[0]["at"].as_i64().unwrap();
    let stale = updated_at(&host, &id);

    // Someone else saves between the read and the restore.
    set_html(&host, Caller::Agent, &id, &doc("second"));

    let err = host
        .call(
            Caller::Agent,
            "history.restore",
            json!({ "id": id, "at": at, "expectedUpdatedAt": stale }),
        )
        .unwrap_err();
    assert!(err.starts_with("conflict:"), "unexpected error: {err}");
}

#[test]
fn purging_a_trashed_page_removes_its_history() {
    let vault = temp_vault("purge");
    let host = open(&vault);
    let id = create(&host, "Purge Page");
    set_html(&host, Caller::Agent, &id, &doc("second"));
    let history_dir = vault.join(".herbarium/history").join(&id);
    assert!(history_dir.is_dir());

    host.call(Caller::Agent, "pages.delete", json!({ "id": id }))
        .unwrap();
    // History survives a trash/restore round trip.
    assert!(history_dir.is_dir());
    assert!(!versions(&host, &id).is_empty());

    host.call(Caller::Ui, "pages.purge", json!({ "id": id }))
        .unwrap();
    assert!(!history_dir.exists());
}

#[test]
fn new_page_with_reused_id_starts_with_empty_history() {
    let vault = temp_vault("reused");
    let host = open(&vault);

    // A leftover history directory with no page and no trash entry.
    let stale = vault.join(".herbarium/history/reused-page");
    std::fs::create_dir_all(&stale).unwrap();
    std::fs::write(stale.join("1.ui.html"), doc("ghost")).unwrap();

    let id = create(&host, "Reused Page");
    assert_eq!(id, "reused-page");
    assert!(!stale.exists());
    assert!(versions(&host, &id).is_empty());
}

#[test]
fn history_survives_move_and_restore() {
    let vault = temp_vault("move");
    let host = open(&vault);
    let page = host
        .call(
            Caller::Agent,
            "pages.create",
            json!({ "html": doc("moved"), "title": "Moved Page", "folder": "one" }),
        )
        .unwrap();
    let id = page["id"].as_str().unwrap().to_string();
    set_html(&host, Caller::Agent, &id, &doc("moved again"));
    assert_eq!(versions(&host, &id).len(), 1);

    host.call(
        Caller::Agent,
        "pages.bulk_update",
        json!({ "ids": [id], "folder": "two/three" }),
    )
    .unwrap();
    assert_eq!(versions(&host, &id).len(), 1);

    // Trash and restore keep the id and its history; a purge only removes the
    // history, which is covered separately.
    host.call(Caller::Agent, "pages.delete", json!({ "id": id }))
        .unwrap();
    host.call(Caller::Agent, "pages.restore", json!({ "id": id }))
        .unwrap();
    assert_eq!(versions(&host, &id).len(), 1);
}

#[test]
fn invalid_id_and_missing_version_are_rejected() {
    let vault = temp_vault("invalid");
    let host = open(&vault);
    let id = create(&host, "Valid Page");

    let err = host
        .call(Caller::Agent, "history.list", json!({ "id": "../escape" }))
        .unwrap_err();
    assert!(err.contains("invalid page id"), "unexpected error: {err}");

    let err = host
        .call(
            Caller::Agent,
            "history.get",
            json!({ "id": id, "at": 12345 }),
        )
        .unwrap_err();
    assert!(err.contains("version not found"), "unexpected error: {err}");

    let err = host
        .call(
            Caller::Agent,
            "history.restore",
            json!({ "id": id, "at": 12345 }),
        )
        .unwrap_err();
    assert!(err.contains("version not found"), "unexpected error: {err}");
}
