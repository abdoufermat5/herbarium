// Page operations: create, import, read, list, search, edit, rewrite, duplicate,
// bulk edits, and delete through the trash, driven through `Host::call`.

use std::path::{Path, PathBuf};

use herbarium_core::time::now_ms;
use herbarium_core::{Caller, Host};
use serde_json::{Value, json};

fn doc(title: &str) -> String {
    format!(
        "<!doctype html><html><head><title>{title}</title></head><body><h1>{title}</h1><p>{title} content</p></body></html>"
    )
}

fn temp_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "herbarium-pages-{tag}-{}-{}",
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

fn create(host: &Host, title: &str, folder: Option<&str>, tags: &[&str]) -> Value {
    host.call(
        Caller::Agent,
        "pages.create",
        json!({ "html": doc(title), "folder": folder, "tags": tags }),
    )
    .unwrap()
}

fn page_meta(host: &Host, id: &str) -> Value {
    host.call(Caller::Agent, "pages.get", json!({ "id": id }))
        .unwrap()["meta"]
        .clone()
}

#[test]
fn delete_trash_restore_purge_lifecycle() {
    let vault = temp_vault("lifecycle");
    let host = open(&vault);

    let page = create(&host, "Trashable Page", Some("drafts"), &["temp"]);
    let id = page["id"].as_str().unwrap();

    // Initially trash is empty.
    let trash = host.call(Caller::Agent, "pages.trash", json!({})).unwrap();
    assert!(trash.as_array().unwrap().is_empty());

    // Delete moves page to trash.
    let del = host
        .call(Caller::Agent, "pages.delete", json!({ "id": id }))
        .unwrap();
    assert_eq!(del["deleted"], id);

    // Removed from active store.
    assert!(
        host.call(Caller::Agent, "pages.get", json!({ "id": id }))
            .is_err()
    );
    assert!(!vault.join(format!("drafts/{id}.html")).exists());
    assert!(!vault.join(format!("drafts/{id}.json")).exists());

    // Trash contains the entry.
    let trash_dir = vault.join(".herbarium/trash").join(id);
    assert!(trash_dir.join(format!("{id}.html")).is_file());
    assert!(trash_dir.join(format!("{id}.json")).is_file());
    assert!(trash_dir.join("trashed.json").is_file());

    let trash_list = host.call(Caller::Agent, "pages.trash", json!({})).unwrap();
    let entries = trash_list.as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["id"], id);
    assert!(entries[0]["deletedAt"].as_i64().unwrap() > 0);

    // Restore brings page back to drafts.
    let restored = host
        .call(Caller::Agent, "pages.restore", json!({ "id": id }))
        .unwrap();
    assert_eq!(restored["id"], id);
    assert_eq!(restored["folder"], "drafts");
    assert!(vault.join(format!("drafts/{id}.html")).is_file());
    assert!(vault.join(format!("drafts/{id}.json")).is_file());
    assert_eq!(page_meta(&host, id)["title"], "Trashable Page");

    let trash_list = host.call(Caller::Agent, "pages.trash", json!({})).unwrap();
    assert!(trash_list.as_array().unwrap().is_empty());

    // Delete again and purge single item (UI only).
    host.call(Caller::Agent, "pages.delete", json!({ "id": id }))
        .unwrap();
    let purged = host
        .call(Caller::Ui, "pages.purge", json!({ "id": id }))
        .unwrap();
    assert_eq!(purged["removed"], 1);
    assert!(!trash_dir.exists());

    // Test purge all.
    let p1 = create(&host, "P1", None, &[]);
    let p2 = create(&host, "P2", None, &[]);
    host.call(Caller::Agent, "pages.delete", json!({ "id": p1["id"] }))
        .unwrap();
    host.call(Caller::Agent, "pages.delete", json!({ "id": p2["id"] }))
        .unwrap();
    assert_eq!(
        host.call(Caller::Agent, "pages.trash", json!({}))
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let purged_all = host.call(Caller::Ui, "pages.purge", json!({})).unwrap();
    assert_eq!(purged_all["removed"], 2);
    assert_eq!(
        host.call(Caller::Agent, "pages.trash", json!({}))
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        0
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn conflict_error_on_stale_base_updated_at() {
    let vault = temp_vault("conflict-stale");
    let host = open(&vault);

    let page = create(&host, "Original", None, &[]);
    let id = page["id"].as_str().unwrap();
    let base_updated = page["updatedAt"].as_i64().unwrap();

    // Advance updatedAt with an update.
    let updated = host
        .call(
            Caller::Agent,
            "pages.update",
            json!({ "id": id, "note": "new note" }),
        )
        .unwrap();
    let new_updated = updated["updatedAt"].as_i64().unwrap();
    assert!(new_updated >= base_updated);

    // Stale baseUpdatedAt on pages.update fails with conflict.
    let err = host
        .call(
            Caller::Agent,
            "pages.update",
            json!({ "id": id, "title": "Conflict", "baseUpdatedAt": base_updated - 1 }),
        )
        .unwrap_err();
    assert!(
        err.starts_with("conflict:"),
        "expected conflict error, got {err}"
    );

    // Stale baseUpdatedAt on pages.set_html fails with conflict.
    let err = host
        .call(
            Caller::Agent,
            "pages.set_html",
            json!({ "id": id, "html": doc("New HTML"), "baseUpdatedAt": base_updated - 1 }),
        )
        .unwrap_err();
    assert!(
        err.starts_with("conflict:"),
        "expected conflict error, got {err}"
    );

    // Valid baseUpdatedAt succeeds.
    let ok = host
        .call(
            Caller::Agent,
            "pages.set_html",
            json!({ "id": id, "html": doc("Valid HTML"), "baseUpdatedAt": new_updated }),
        )
        .unwrap();
    assert_eq!(ok["sourceTitle"], "Valid HTML");

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn conflict_error_on_external_disk_change() {
    let vault = temp_vault("conflict-disk");
    let host = open(&vault);

    let page = create(&host, "Disk Page", None, &[]);
    let id = page["id"].as_str().unwrap();
    let base_updated = page["updatedAt"].as_i64().unwrap();

    // External edit to the HTML file on disk without notifying SQLite.
    let html_path = vault.join(format!("{id}.html"));
    std::fs::write(&html_path, doc("Externally Edited HTML")).unwrap();
    // Advance file modification time into the future to ensure mtime detection.
    let future = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(&html_path)
        .unwrap();
    f.set_modified(future).unwrap();

    // Attempt update with the stale baseUpdatedAt (which matches SQLite, but disk changed).
    let err = host
        .call(
            Caller::Agent,
            "pages.update",
            json!({ "id": id, "title": "Try Update", "baseUpdatedAt": base_updated }),
        )
        .unwrap_err();
    assert!(
        err.starts_with("conflict:"),
        "expected conflict error on disk change, got: {err}"
    );

    // Attempt set_html with stale baseUpdatedAt also rejected.
    let err2 = host
        .call(
            Caller::Agent,
            "pages.set_html",
            json!({ "id": id, "html": doc("Try SetHtml"), "baseUpdatedAt": base_updated }),
        )
        .unwrap_err();
    assert!(
        err2.starts_with("conflict:"),
        "expected conflict error on disk change for set_html, got: {err2}"
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn read_after_external_edit_yields_a_usable_overwrite_baseline() {
    let vault = temp_vault("read-after-external");
    let host = open(&vault);

    let page = create(&host, "Overwrite Me", None, &[]);
    let id = page["id"].as_str().unwrap();

    // Another tool rewrites the HTML without touching the sidecar or index.
    let html_path = vault.join(format!("{id}.html"));
    std::fs::write(&html_path, doc("Written By Another Tool")).unwrap();
    let future = std::time::SystemTime::now() + std::time::Duration::from_secs(30);
    std::fs::OpenOptions::new()
        .write(true)
        .open(&html_path)
        .unwrap()
        .set_modified(future)
        .unwrap();
    let disk_ms = future
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    // Reading the page must hand back the disk-consistent baseline, so the
    // reader's explicit overwrite is not rejected as stale forever.
    let fresh = host
        .call(
            Caller::Ui,
            "pages.get",
            json!({ "id": id, "format": "html" }),
        )
        .unwrap();
    assert_eq!(fresh["meta"]["updatedAt"].as_i64(), Some(disk_ms));
    assert_eq!(
        fresh["html"].as_str(),
        Some(doc("Written By Another Tool").as_str())
    );

    // That baseline is accepted by the next save, and the draft wins.
    let saved = host
        .call(
            Caller::Ui,
            "pages.set_html",
            json!({ "id": id, "html": doc("Reader Draft Wins"), "baseUpdatedAt": disk_ms }),
        )
        .unwrap();
    assert_eq!(saved["title"].as_str(), Some("Reader Draft Wins"));
    assert_eq!(
        std::fs::read_to_string(&html_path).unwrap(),
        doc("Reader Draft Wins")
    );

    // A page read after that save round-trips: read, then save with the value.
    let after = host
        .call(Caller::Ui, "pages.get", json!({ "id": id }))
        .unwrap();
    let base = after["meta"]["updatedAt"].as_i64().unwrap();
    host.call(
        Caller::Ui,
        "pages.set_html",
        json!({ "id": id, "html": doc("Second Save"), "baseUpdatedAt": base }),
    )
    .unwrap();

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn conflict_error_on_external_sidecar_edit() {
    let vault = temp_vault("conflict-sidecar");
    let host = open(&vault);

    let page = create(&host, "Sidecar Page", None, &[]);
    let id = page["id"].as_str().unwrap();
    let base_updated = page["updatedAt"].as_i64().unwrap();

    // Externally rewrite sidecar JSON with a newer updatedAt.
    let sidecar_path = vault.join(format!("{id}.json"));
    let mut sidecar: Value =
        serde_json::from_str(&std::fs::read_to_string(&sidecar_path).unwrap()).unwrap();
    sidecar["updatedAt"] = json!(base_updated + 10_000);
    std::fs::write(&sidecar_path, serde_json::to_string(&sidecar).unwrap()).unwrap();

    let err = host
        .call(
            Caller::Agent,
            "pages.update",
            json!({ "id": id, "title": "Try Update", "baseUpdatedAt": base_updated }),
        )
        .unwrap_err();
    assert!(
        err.starts_with("conflict:"),
        "expected conflict when sidecar was edited on disk, got: {err}"
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn duplicate_page() {
    let vault = temp_vault("duplicate");
    let host = open(&vault);

    let page = host
        .call(
            Caller::Agent,
            "pages.create",
            json!({
                "html": doc("Architecture"),
                "folder": "docs",
                "tags": ["arch", "v1"],
                "note": "Initial architecture",
                "reviewInMinutes": 1440,
                "allowCdn": true
            }),
        )
        .unwrap();
    let orig_id = page["id"].as_str().unwrap();

    let dup = host
        .call(Caller::Agent, "pages.duplicate", json!({ "id": orig_id }))
        .unwrap();
    let dup_id = dup["id"].as_str().unwrap();

    assert_ne!(orig_id, dup_id);
    assert_eq!(dup["title"], "Architecture (copy)");
    assert_eq!(dup["folder"], "docs");
    assert_eq!(dup["tags"], json!(["arch", "v1"]));
    assert_eq!(dup["note"], "Initial architecture");
    assert_eq!(dup["allowCdn"], true);
    assert!(
        dup["intervalMinutes"].is_null(),
        "review schedule is cleared on duplicate"
    );
    assert!(dup["nextReview"].is_null());
    assert!(dup["lastReview"].is_null());

    // Both files exist on disk in the same folder.
    assert!(vault.join(format!("docs/{dup_id}.html")).is_file());
    assert!(vault.join(format!("docs/{dup_id}.json")).is_file());
    assert!(vault.join(format!("docs/{orig_id}.html")).is_file());

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn bulk_update_moves_and_updates_tags() {
    let vault = temp_vault("bulk-update");
    let host = open(&vault);

    let p1 = create(&host, "Page One", Some("drafts"), &["wip", "shared"]);
    let p2 = create(&host, "Page Two", Some("drafts"), &["wip", "other"]);
    let p3 = create(&host, "Page Three", Some("drafts"), &["wip"]);
    let id1 = p1["id"].as_str().unwrap();
    let id2 = p2["id"].as_str().unwrap();
    let id3 = p3["id"].as_str().unwrap();

    let res = host
        .call(
            Caller::Agent,
            "pages.bulk_update",
            json!({
                "ids": [id1, id2],
                "folder": "published",
                "addTags": ["live", "shared"],
                "removeTags": ["wip"]
            }),
        )
        .unwrap();

    let updated = res["updated"].as_array().unwrap();
    assert_eq!(updated.len(), 2);
    assert!(res["errors"].as_array().unwrap().is_empty());

    let meta1 = page_meta(&host, id1);
    assert_eq!(meta1["folder"], "published");
    assert_eq!(meta1["tags"], json!(["shared", "live"]));
    assert!(vault.join(format!("published/{id1}.html")).is_file());

    let meta2 = page_meta(&host, id2);
    assert_eq!(meta2["folder"], "published");
    assert_eq!(meta2["tags"], json!(["other", "live", "shared"]));

    // p3 untouched in drafts.
    let meta3 = page_meta(&host, id3);
    assert_eq!(meta3["folder"], "drafts");
    assert_eq!(meta3["tags"], json!(["wip"]));

    // Moving to root via null folder.
    let root_res = host
        .call(
            Caller::Agent,
            "pages.bulk_update",
            json!({ "ids": [id1], "folder": null }),
        )
        .unwrap();
    assert_eq!(root_res["updated"][0]["folder"], Value::Null);
    assert!(vault.join(format!("{id1}.html")).is_file());

    // Partial error handling.
    let err_res = host
        .call(
            Caller::Agent,
            "pages.bulk_update",
            json!({ "ids": [id2, "missing-page-id"], "addTags": ["tested"] }),
        )
        .unwrap();
    assert_eq!(err_res["updated"].as_array().unwrap().len(), 1);
    let errors = err_res["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["id"], "missing-page-id");

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn bulk_delete_trashes_multiple_pages() {
    let vault = temp_vault("bulk-delete");
    let host = open(&vault);

    let p1 = create(&host, "Delete 1", None, &[]);
    let p2 = create(&host, "Delete 2", None, &[]);
    let p3 = create(&host, "Keep 3", None, &[]);
    let id1 = p1["id"].as_str().unwrap();
    let id2 = p2["id"].as_str().unwrap();
    let id3 = p3["id"].as_str().unwrap();

    let res = host
        .call(
            Caller::Agent,
            "pages.bulk_delete",
            json!({ "ids": [id1, id2, "not-real"] }),
        )
        .unwrap();

    let deleted = res["deleted"].as_array().unwrap();
    let expected = vec![Value::from(id1), Value::from(id2)];
    assert_eq!(deleted, &expected);
    let errors = res["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["id"], "not-real");

    assert!(
        host.call(Caller::Agent, "pages.get", json!({ "id": id1 }))
            .is_err()
    );
    assert!(
        host.call(Caller::Agent, "pages.get", json!({ "id": id2 }))
            .is_err()
    );
    assert!(
        host.call(Caller::Agent, "pages.get", json!({ "id": id3 }))
            .is_ok()
    );

    let trash = host.call(Caller::Agent, "pages.trash", json!({})).unwrap();
    assert_eq!(trash.as_array().unwrap().len(), 2);

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn readable_slug_ids_generation() {
    let vault = temp_vault("slugs");
    let host = open(&vault);

    let p1 = create(&host, "Getting Started with Rust 2024!", None, &[]);
    let id1 = p1["id"].as_str().unwrap();
    assert_eq!(id1, "getting-started-with-rust-2024");

    // Duplicate title gets deduplicated suffix.
    let p2 = create(&host, "Getting Started with Rust 2024!", None, &[]);
    let id2 = p2["id"].as_str().unwrap();
    assert_eq!(id2, "getting-started-with-rust-2024-2");

    let p3 = create(&host, "Getting Started with Rust 2024!", None, &[]);
    let id3 = p3["id"].as_str().unwrap();
    assert_eq!(id3, "getting-started-with-rust-2024-3");

    // UI import slug from file stem.
    let files = json!([
        { "name": "guide.spec.html", "content": doc("Imported Guide") },
    ]);
    let imported = host
        .call(Caller::Ui, "pages.import", json!({ "files": files }))
        .unwrap();
    assert_eq!(imported["imported"], 1);

    let list = host.call(Caller::Agent, "pages.list", json!({})).unwrap();
    assert!(
        list.as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"].as_str().unwrap().starts_with("guide-spec")),
        "imported page id should be slug of file stem"
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn source_title_tracking_and_html_title_updates() {
    let vault = temp_vault("source-title");
    let host = open(&vault);

    // Initial page without explicit title takes HTML title as both title and sourceTitle.
    let page = host
        .call(
            Caller::Agent,
            "pages.create",
            json!({ "html": "<!doctype html><html><head><title>Initial Title</title></head><body><p>x</p></body></html>" }),
        )
        .unwrap();
    let id = page["id"].as_str().unwrap();
    assert_eq!(page["title"], "Initial Title");
    assert_eq!(page["sourceTitle"], "Initial Title");

    // set_html updates HTML title: since user never renamed, title follows new sourceTitle.
    let updated = host
        .call(
            Caller::Agent,
            "pages.set_html",
            json!({
                "id": id,
                "html": "<!doctype html><html><head><title>Followed Title</title></head><body><p>x</p></body></html>"
            }),
        )
        .unwrap();
    assert_eq!(updated["title"], "Followed Title");
    assert_eq!(updated["sourceTitle"], "Followed Title");

    // User explicitly renames the page via pages.update.
    let renamed = host
        .call(
            Caller::Agent,
            "pages.update",
            json!({ "id": id, "title": "User Renamed Title" }),
        )
        .unwrap();
    assert_eq!(renamed["title"], "User Renamed Title");
    assert_eq!(renamed["sourceTitle"], "Followed Title");

    // Subsequent set_html refreshes sourceTitle, but user's title is preserved.
    let updated2 = host
        .call(
            Caller::Agent,
            "pages.set_html",
            json!({
                "id": id,
                "html": "<!doctype html><html><head><title>New Doc Title</title></head><body><p>x</p></body></html>"
            }),
        )
        .unwrap();
    assert_eq!(updated2["title"], "User Renamed Title");
    assert_eq!(updated2["sourceTitle"], "New Doc Title");

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn agent_default_limit_and_search_hit() {
    let vault = temp_vault("limits");
    let host = open(&vault);

    // Create 55 pages.
    for i in 0..55 {
        host.call(
            Caller::Agent,
            "pages.create",
            json!({ "html": format!("<!doctype html><html><head><title>Page {i:02}</title></head><body><p>needle in haystack {i}</p></body></html>") }),
        )
        .unwrap();
    }

    // Agent list defaults to 50.
    let agent_list = host.call(Caller::Agent, "pages.list", json!({})).unwrap();
    assert_eq!(agent_list.as_array().unwrap().len(), 50);

    // Agent list with explicit limit.
    let agent_limited = host
        .call(Caller::Agent, "pages.list", json!({ "limit": 10 }))
        .unwrap();
    assert_eq!(agent_limited.as_array().unwrap().len(), 10);

    // UI list is uncapped.
    let ui_list = host.call(Caller::Ui, "pages.list", json!({})).unwrap();
    assert_eq!(ui_list.as_array().unwrap().len(), 55);

    // Agent search defaults to 50 hits and returns SearchHit shape (flattened meta + snippet).
    let search_hits = host
        .call(Caller::Agent, "pages.search", json!({ "query": "needle" }))
        .unwrap();
    let hits = search_hits.as_array().unwrap();
    assert_eq!(hits.len(), 50);
    // Flattened meta fields accessible directly on hit.
    assert!(hits[0]["id"].is_string());
    assert!(hits[0]["title"].is_string());
    // Snippet present for body text match.
    assert!(hits[0].get("snippet").is_some());

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn network_settings_vault_default() {
    let vault = temp_vault("network");
    let host = open(&vault);

    // Default network setting is false when network.json does not exist.
    let p1 = host
        .call(Caller::Agent, "pages.create", json!({ "html": doc("Off") }))
        .unwrap();
    assert_eq!(p1["allowCdn"], false);

    // Write network.json with defaultAllowCdn = true.
    std::fs::create_dir_all(vault.join(".herbarium")).unwrap();
    std::fs::write(
        vault.join(".herbarium/network.json"),
        r#"{"defaultAllowCdn": true}"#,
    )
    .unwrap();

    let p2 = host
        .call(Caller::Agent, "pages.create", json!({ "html": doc("On") }))
        .unwrap();
    assert_eq!(
        p2["allowCdn"], true,
        "page should inherit vault default true"
    );

    // Explicit allowCdn: false overrides the vault default.
    let p3 = host
        .call(
            Caller::Agent,
            "pages.create",
            json!({ "html": doc("Override"), "allowCdn": false }),
        )
        .unwrap();
    assert_eq!(p3["allowCdn"], false);

    // Import also inherits vault default.
    let files = json!([{ "name": "imp.html", "content": doc("Imported") }]);
    host.call(Caller::Ui, "pages.import", json!({ "files": files }))
        .unwrap();
    let imported_page = host
        .call(Caller::Agent, "pages.get", json!({ "id": "imp" }))
        .unwrap();
    assert_eq!(imported_page["meta"]["allowCdn"], true);

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn ui_only_restrictions() {
    let vault = temp_vault("ui-only");
    let host = open(&vault);

    // pages.import is ui_only
    let err_import = host
        .call(
            Caller::Agent,
            "pages.import",
            json!({ "files": [{ "name": "x.html", "content": doc("X") }] }),
        )
        .unwrap_err();
    assert!(err_import.contains("unknown operation") || err_import.contains("pages.import"));

    // pages.purge is ui_only
    let err_purge = host
        .call(Caller::Agent, "pages.purge", json!({}))
        .unwrap_err();
    assert!(err_purge.contains("unknown operation") || err_purge.contains("pages.purge"));

    // Both succeed when invoked as UI
    let ok_import = host
        .call(
            Caller::Ui,
            "pages.import",
            json!({ "files": [{ "name": "x.html", "content": doc("X") }] }),
        )
        .unwrap();
    assert_eq!(ok_import["imported"], 1);

    let ok_purge = host.call(Caller::Ui, "pages.purge", json!({})).unwrap();
    assert_eq!(ok_purge["removed"], 0);

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn source_is_recorded_cleaned_searchable_and_editable() {
    let vault = temp_vault("source");
    let host = open(&vault);

    let page = host
        .call(
            Caller::Agent,
            "pages.create",
            json!({
                "html": doc("Cargo"),
                "source": { "url": " https://example.com/cargo ", "tool": "Claude Code", "prompt": "explain workspaces", }
            }),
        )
        .unwrap();
    let id = page["id"].as_str().unwrap().to_string();
    assert_eq!(page["ext"]["source"]["url"], "https://example.com/cargo");
    assert_eq!(page["ext"]["source"]["tool"], "Claude Code");

    // The prompt is searchable even though it is not in the page text.
    let hits = host
        .call(Caller::Ui, "pages.search", json!({ "query": "workspaces" }))
        .unwrap();
    assert_eq!(hits[0]["id"], id.as_str());

    // The sidecar carries it, so it survives a rebuilt index.
    let sidecar: Value =
        serde_json::from_str(&std::fs::read_to_string(vault.join(format!("{id}.json"))).unwrap())
            .unwrap();
    assert_eq!(sidecar["ext"]["source"]["prompt"], "explain workspaces");

    // A duplicate keeps where the original came from.
    let copy = host
        .call(Caller::Ui, "pages.duplicate", json!({ "id": id }))
        .unwrap();
    assert_eq!(copy["ext"]["source"]["tool"], "Claude Code");

    // Blank fields are dropped; an all-blank source is no source.
    let updated = host
        .call(
            Caller::Ui,
            "pages.update",
            json!({ "id": id, "source": { "tool": "ChatGPT", "url": "  " } }),
        )
        .unwrap();
    assert_eq!(updated["ext"]["source"], json!({ "tool": "ChatGPT" }));
    let cleared = host
        .call(
            Caller::Ui,
            "pages.update",
            json!({ "id": id, "source": null }),
        )
        .unwrap();
    assert!(cleared["ext"].get("source").is_none());
    let blank = host
        .call(
            Caller::Ui,
            "pages.update",
            json!({ "id": id, "source": { "tool": " " } }),
        )
        .unwrap();
    assert!(blank["ext"].get("source").is_none());

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn source_rejects_non_web_urls_before_writing() {
    let vault = temp_vault("source-invalid");
    let host = open(&vault);

    let err = host
        .call(
            Caller::Agent,
            "pages.create",
            json!({ "html": doc("Bad"), "source": { "url": "javascript:alert(1)" } }),
        )
        .unwrap_err();
    assert!(err.contains("http"), "{err}");
    let pages = host.call(Caller::Ui, "pages.list", json!({})).unwrap();
    assert_eq!(pages.as_array().unwrap().len(), 0, "nothing was written");

    let page = create(&host, "Good", None, &[]);
    let err = host
        .call(
            Caller::Agent,
            "pages.update",
            json!({ "id": page["id"], "source": { "url": "file:///etc/passwd" }, "folder": "moved" }),
        )
        .unwrap_err();
    assert!(err.contains("http"), "{err}");
    let after = host
        .call(Caller::Ui, "pages.get", json!({ "id": page["id"] }))
        .unwrap();
    assert!(
        after["meta"]["folder"].is_null(),
        "a rejected update moves nothing"
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn search_filters_narrow_results_before_the_limit() {
    let vault = temp_vault("filters");
    let host = open(&vault);
    for i in 0..5 {
        create(&host, &format!("Rust {i}"), Some("lang/rust"), &["rust"]);
    }
    create(
        &host,
        "Rust in python folder",
        Some("lang/python"),
        &["python"],
    );
    let due = create(&host, "Due page", None, &["rust"]);
    host.call(
        Caller::Ui,
        "review.schedule",
        json!({ "id": due["id"], "intervalMinutes": 1 }),
    )
    .unwrap();

    let ids = |q: &str, limit: Option<usize>| -> Vec<String> {
        let mut args = json!({ "query": q });
        if let Some(l) = limit {
            args["limit"] = json!(l);
        }
        host.call(Caller::Ui, "pages.search", args)
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h["id"].as_str().unwrap().to_string())
            .collect()
    };

    assert_eq!(ids("rust folder:lang/python", None).len(), 1);
    assert_eq!(ids("tag:rust", None).len(), 6);
    assert_eq!(
        ids("tag:rust -folder:lang", None),
        vec![due["id"].as_str().unwrap()]
    );
    // The limit applies after filtering, so a filtered match is never cut off.
    assert_eq!(ids("folder:lang/python", Some(1)).len(), 1);
    assert_eq!(
        ids("is:scheduled due:1d", None),
        vec![due["id"].as_str().unwrap()]
    );
    assert_eq!(ids("is:unscheduled", None).len(), 6);

    let err = host
        .call(
            Caller::Ui,
            "pages.search",
            json!({ "query": "is:whatever" }),
        )
        .unwrap_err();
    assert!(err.contains("is:whatever"), "{err}");

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn saved_searches_round_trip() {
    let vault = temp_vault("saved");
    let host = open(&vault);
    let list = host
        .call(
            Caller::Agent,
            "searches.save",
            json!({ "name": "Rust due", "query": "tag:rust is:due" }),
        )
        .unwrap();
    assert_eq!(
        list,
        json!([{ "name": "Rust due", "query": "tag:rust is:due" }])
    );
    // Same name, any case: replaced in place.
    host.call(
        Caller::Ui,
        "searches.save",
        json!({ "name": "rust due", "query": "tag:rust" }),
    )
    .unwrap();
    host.call(
        Caller::Ui,
        "searches.save",
        json!({ "name": "Notes", "query": "has:note" }),
    )
    .unwrap();
    let list = host.call(Caller::Ui, "searches.list", json!({})).unwrap();
    assert_eq!(list[0], json!({ "name": "rust due", "query": "tag:rust" }));
    assert_eq!(list[1]["name"], "Notes");

    assert!(
        host.call(
            Caller::Ui,
            "searches.save",
            json!({ "name": "Bad", "query": "is:nope" })
        )
        .is_err()
    );
    assert!(
        host.call(
            Caller::Ui,
            "searches.save",
            json!({ "name": " ", "query": "x" })
        )
        .is_err()
    );

    let list = host
        .call(Caller::Ui, "searches.delete", json!({ "name": "RUST DUE" }))
        .unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert!(vault.join(".herbarium/searches.json").exists());

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn links_and_backlinks_follow_the_files() {
    let vault = temp_vault("links");
    let host = open(&vault);
    let target = create(&host, "Target", None, &[]);
    let tid = target["id"].as_str().unwrap().to_string();
    let linker = host
        .call(
            Caller::Ui,
            "pages.create",
            json!({ "html": format!(
                "<title>Linker</title><a href=\"herbarium-app://open/{tid}#part\">t</a>\
                 <a href=\"HERBARIUM-APP://open/{tid}\">again</a>\
                 <a href=\"herbarium-app://open/ghost\">g</a><a href=\"https://x.y/\">web</a>\
                 <p>herbarium-app://open/not-a-link</p>"
            ) }),
        )
        .unwrap();
    let lid = linker["id"].as_str().unwrap().to_string();

    let links = host
        .call(Caller::Agent, "pages.links", json!({ "id": lid }))
        .unwrap();
    assert_eq!(links["links"].as_array().unwrap().len(), 1);
    assert_eq!(links["links"][0]["id"], tid.as_str());
    assert_eq!(links["broken"], json!(["ghost"]));
    let back = host
        .call(Caller::Agent, "pages.links", json!({ "id": tid }))
        .unwrap();
    assert_eq!(back["backlinks"][0]["id"], lid.as_str());
    let graph = host.call(Caller::Agent, "pages.graph", json!({})).unwrap();
    assert_eq!(
        graph["edges"],
        json!([[lid.clone(), tid.clone()]]),
        "only links to existing pages"
    );
    assert_eq!(graph["nodes"].as_array().unwrap().len(), 2);
    let everything = host
        .call(Caller::Ui, "pages.graph", json!({ "all": true }))
        .unwrap();
    assert!(everything["nodes"].as_array().unwrap().len() >= 2);

    // Rewriting the page drops the link; the index notices the change.
    std::thread::sleep(std::time::Duration::from_millis(10));
    host.call(
        Caller::Ui,
        "pages.set_html",
        json!({ "id": lid, "html": "<title>Linker</title><p>no links</p>" }),
    )
    .unwrap();
    let back = host
        .call(Caller::Ui, "pages.links", json!({ "id": tid }))
        .unwrap();
    assert!(back["backlinks"].as_array().unwrap().is_empty());

    // A trashed linker no longer counts.
    host.call(
        Caller::Ui,
        "pages.set_html",
        json!({ "id": lid, "html": format!("<title>Linker</title><a href=\"herbarium-app://open/{tid}\">t</a>") }),
    )
    .unwrap();
    assert_eq!(
        host.call(Caller::Ui, "pages.links", json!({ "id": tid }))
            .unwrap()["backlinks"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    host.call(Caller::Ui, "pages.delete", json!({ "id": lid }))
        .unwrap();
    let back = host
        .call(Caller::Ui, "pages.links", json!({ "id": tid }))
        .unwrap();
    assert!(back["backlinks"].as_array().unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn created_at_import_key_and_find_by_url() {
    let vault = temp_vault("import-key");
    let host = open(&vault);
    let page = host
        .call(
            Caller::Ui,
            "pages.create",
            json!({
                "html": doc("Old"),
                "createdAt": 1_600_000_000_000i64,
                "importKey": "claude:c1:solar",
                "source": { "url": "https://Claude.AI/chat/c1#frag" }
            }),
        )
        .unwrap();
    assert_eq!(page["createdAt"], 1_600_000_000_000i64);
    assert_eq!(page["ext"]["import"]["key"], "claude:c1:solar");

    // A future date is clamped to now.
    let future = host
        .call(
            Caller::Ui,
            "pages.create",
            json!({ "html": doc("Future"), "createdAt": i64::MAX / 2 }),
        )
        .unwrap();
    assert!(future["createdAt"].as_i64().unwrap() <= now_ms());

    let keys = herbarium_core::importer::imported_keys(host.store().unwrap()).unwrap();
    assert!(keys.contains("claude:c1:solar") && keys.len() == 1);

    for url in [
        "https://claude.ai/chat/c1",
        "https://claude.ai/chat/c1/",
        "HTTPS://CLAUDE.AI/chat/c1#other",
    ] {
        let found = host
            .call(Caller::Agent, "pages.find_by_url", json!({ "url": url }))
            .unwrap();
        assert_eq!(found.as_array().unwrap().len(), 1, "{url}");
    }
    let none = host
        .call(
            Caller::Agent,
            "pages.find_by_url",
            json!({ "url": "https://claude.ai/chat/C1" }),
        )
        .unwrap();
    assert!(
        none.as_array().unwrap().is_empty(),
        "paths stay case-sensitive"
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn highlights_with_notes_are_kept_and_searchable() {
    let vault = temp_vault("highlights");
    let host = open(&vault);
    let page = create(&host, "Ferns", None, &[]);
    let id = page["id"].as_str().unwrap();

    let added = host
        .call(Caller::Ui, "pages.create", json!({ "html": doc("Other") }))
        .unwrap();
    assert!(added["id"].is_string());

    let out = host
        .call(
            Caller::Ui,
            "highlights.add",
            json!({ "page": id, "quote": " Ferns content ", "prefix": "x".repeat(200), "suffix": "", "note": "spores, not seeds" }),
        )
        .unwrap();
    let hid = out["highlight"]["id"].as_str().unwrap().to_string();
    assert_eq!(out["highlight"]["quote"], "Ferns content");
    assert_eq!(out["highlight"]["color"], "yellow");
    assert_eq!(
        out["highlight"]["prefix"].as_str().unwrap().len(),
        64,
        "context is clipped"
    );
    assert_eq!(out["page"]["ext"]["highlights"][0]["id"], hid.as_str());

    let hits = host
        .call(Caller::Agent, "pages.search", json!({ "query": "spores" }))
        .unwrap();
    assert_eq!(hits[0]["id"], id, "notes are searchable");

    let updated = host
        .call(
            Caller::Ui,
            "highlights.update",
            json!({ "page": id, "id": hid, "color": "green", "note": "  sporangia  " }),
        )
        .unwrap();
    assert_eq!(updated["highlights"][0]["color"], "green");
    assert_eq!(updated["highlights"][0]["note"], "sporangia");
    assert!(
        host.call(
            Caller::Ui,
            "highlights.update",
            json!({ "page": id, "id": hid, "color": "red" })
        )
        .is_err()
    );

    let listed = host
        .call(Caller::Agent, "highlights.list", json!({ "page": id }))
        .unwrap();
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert!(
        host.call(
            Caller::Agent,
            "highlights.add",
            json!({ "page": id, "quote": "x" })
        )
        .is_err(),
        "only the user highlights"
    );
    assert!(
        host.call(
            Caller::Ui,
            "highlights.add",
            json!({ "page": id, "quote": "   " })
        )
        .is_err()
    );

    let removed = host
        .call(
            Caller::Ui,
            "highlights.remove",
            json!({ "page": id, "id": hid }),
        )
        .unwrap();
    assert!(removed["page"]["ext"].get("highlights").is_none());
    let _ = std::fs::remove_dir_all(&vault);
}
