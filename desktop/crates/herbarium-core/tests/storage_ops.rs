// Per-page persistent state: round trips per namespace, whole-batch limits,
// `updatedAt` stability, and survival across moves, trash and re-index.

use std::path::{Path, PathBuf};

use herbarium_core::time::now_ms;
use herbarium_core::{Caller, Host};
use serde_json::{Value, json};

const MAX_STATE_BYTES: usize = 1024 * 1024;

fn doc(title: &str) -> String {
    format!(
        "<!doctype html><html><head><title>{title}</title></head><body><h1>{title}</h1><p>{title} content</p></body></html>"
    )
}

fn temp_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "herbarium-storage-{tag}-{}-{}",
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

fn create(host: &Host, title: &str, folder: Option<&str>) -> Value {
    host.call(
        Caller::Agent,
        "pages.create",
        json!({ "html": doc(title), "folder": folder }),
    )
    .unwrap()
}

fn uid(page: &Value) -> String {
    page["id"].as_str().unwrap().to_string()
}

fn write(host: &Host, id: &str, changes: Value) -> Result<Value, String> {
    host.call(
        Caller::Ui,
        "storage.write",
        json!({ "id": id, "changes": changes }),
    )
}

fn get(host: &Host, id: &str) -> Value {
    host.call(Caller::Ui, "storage.get", json!({ "id": id }))
        .unwrap()
}

#[test]
fn write_get_roundtrip_per_area_and_delete() {
    let vault = temp_vault("roundtrip");
    let host = open(&vault);
    let id = uid(&create(&host, "Stateful", None));

    let stored = write(
        &host,
        &id,
        json!([
            { "area": "local", "key": "theme", "value": "dark" },
            { "area": "personal", "key": "notes", "value": "hello" },
            { "area": "shared", "key": "count", "value": "3" }
        ]),
    )
    .unwrap();
    assert_eq!(stored["local"]["theme"], "dark");
    assert_eq!(stored["personal"]["notes"], "hello");
    assert_eq!(stored["shared"]["count"], "3");

    // A page that never wrote reads as three empty maps.
    let other = uid(&create(&host, "Blank", None));
    assert_eq!(
        get(&host, &other),
        json!({ "local": {}, "personal": {}, "shared": {} })
    );

    // Overwrite one key and delete another in a single batch.
    let stored = write(
        &host,
        &id,
        json!([
            { "area": "local", "key": "theme", "value": "light" },
            { "area": "shared", "key": "count", "value": null },
            { "area": "local", "key": "missing", "value": null }
        ]),
    )
    .unwrap();
    assert_eq!(stored["local"]["theme"], "light");
    assert!(stored["shared"].as_object().unwrap().is_empty());
    assert_eq!(get(&host, &id)["personal"]["notes"], "hello");

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn limits_reject_the_whole_batch_unchanged() {
    let vault = temp_vault("limits");
    let host = open(&vault);
    let id = uid(&create(&host, "Limited", None));
    write(
        &host,
        &id,
        json!([{ "area": "local", "key": "keep", "value": "1" }]),
    )
    .unwrap();

    // One valid change plus one bad key: neither is applied.
    let err = write(
        &host,
        &id,
        json!([
            { "area": "local", "key": "added", "value": "2" },
            { "area": "local", "key": "k".repeat(201), "value": "x" }
        ]),
    )
    .unwrap_err();
    assert!(err.contains("1 to 200"), "{err}");
    assert_eq!(get(&host, &id)["local"], json!({ "keep": "1" }));

    // Empty key.
    let err = write(
        &host,
        &id,
        json!([{ "area": "local", "key": "", "value": "x" }]),
    )
    .unwrap_err();
    assert!(err.contains("1 to 200"), "{err}");

    // Unknown area.
    let err = write(
        &host,
        &id,
        json!([{ "area": "session", "key": "k", "value": "x" }]),
    )
    .unwrap_err();
    assert!(err.contains("unknown area"), "{err}");

    // A value that pushes the serialized state past 1 MiB.
    let err = write(
        &host,
        &id,
        json!([{ "area": "local", "key": "big", "value": "x".repeat(MAX_STATE_BYTES) }]),
    )
    .unwrap_err();
    assert!(err.contains("exceed"), "{err}");
    assert_eq!(get(&host, &id)["local"], json!({ "keep": "1" }));

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn writes_do_not_change_updated_at_or_break_save_conflicts() {
    let vault = temp_vault("updated");
    let host = open(&vault);
    let id = uid(&create(&host, "Stable", None));

    let before = host
        .call(Caller::Agent, "pages.get", json!({ "id": id }))
        .unwrap()["meta"]["updatedAt"]
        .as_i64()
        .unwrap();

    write(
        &host,
        &id,
        json!([{ "area": "personal", "key": "k", "value": "v" }]),
    )
    .unwrap();

    let after = host
        .call(Caller::Agent, "pages.get", json!({ "id": id }))
        .unwrap()["meta"]["updatedAt"]
        .as_i64()
        .unwrap();
    assert_eq!(before, after, "storage writes are not page revisions");

    // The `updatedAt` the page handed out still passes the optimistic check.
    let new_html = doc("Stable").replace("content", "edited");
    host.call(
        Caller::Agent,
        "pages.set_html",
        json!({ "id": id, "html": new_html, "baseUpdatedAt": after }),
    )
    .unwrap();
    // The edited HTML keeps the state, because `set_html` rewrites the sidecar.
    assert_eq!(get(&host, &id)["personal"]["k"], "v");

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn state_survives_move_trash_restore_and_reopen() {
    let vault = temp_vault("lifecycle");
    let id = {
        let host = open(&vault);
        let id = uid(&create(&host, "Traveler", Some("drafts")));
        write(
            &host,
            &id,
            json!([
                { "area": "local", "key": "a", "value": "1" },
                { "area": "personal", "key": "b", "value": "2" },
                { "area": "shared", "key": "c", "value": "3" }
            ]),
        )
        .unwrap();

        // Moving the page carries the sidecar.
        host.call(
            Caller::Agent,
            "pages.update",
            json!({ "id": id, "folder": "archive/deep" }),
        )
        .unwrap();
        assert_eq!(get(&host, &id)["local"]["a"], "1");

        // Trash, restore (back into its folder), still intact.
        host.call(Caller::Agent, "pages.delete", json!({ "id": id }))
            .unwrap();
        assert!(
            host.call(Caller::Ui, "storage.get", json!({ "id": id }))
                .is_err()
        );
        let restored = host
            .call(Caller::Agent, "pages.restore", json!({ "id": id }))
            .unwrap();
        assert_eq!(restored["folder"], "archive/deep");
        assert_eq!(get(&host, &id)["shared"]["c"], "3");
        id
    };

    // Re-opening the vault (as on app restart) re-reads the sidecars.
    let host = open(&vault);
    let state = get(&host, &id);
    assert_eq!(state["local"]["a"], "1");
    assert_eq!(state["personal"]["b"], "2");
    assert_eq!(state["shared"]["c"], "3");

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn storage_is_not_indexed_by_search_and_survives_a_rescan() {
    let vault = temp_vault("indexing");
    let host = open(&vault);
    let id = uid(&create(&host, "Quiet", None));
    write(
        &host,
        &id,
        json!([{ "area": "local", "key": "secret", "value": "zzzuniquestoragevalue" }]),
    )
    .unwrap();

    let hits = host
        .call(
            Caller::Agent,
            "pages.search",
            json!({ "query": "zzzuniquestoragevalue" }),
        )
        .unwrap();
    assert!(
        hits.as_array().unwrap().is_empty(),
        "page state never enters the search index"
    );

    host.call(Caller::Agent, "vault.rescan", json!({})).unwrap();
    assert_eq!(get(&host, &id)["local"]["secret"], "zzzuniquestoragevalue");

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn storage_operations_are_ui_only() {
    let vault = temp_vault("ui-only");
    let host = open(&vault);
    let id = uid(&create(&host, "Hidden", None));

    for (name, args) in [
        ("storage.get", json!({ "id": id })),
        (
            "storage.write",
            json!({ "id": id, "changes": [{ "area": "local", "key": "k", "value": "v" }] }),
        ),
        ("storage.clear", json!({ "id": id })),
    ] {
        let err = host.call(Caller::Agent, name, args).unwrap_err();
        assert!(err.contains("unknown operation"), "{name}: {err}");
        assert!(
            host.operations(Caller::Agent).all(|op| op.name != name),
            "{name} must not be listed for agents"
        );
    }
    for name in ["storage.get", "storage.write", "storage.clear"] {
        assert!(
            host.operations(Caller::Ui).any(|op| op.name == name),
            "{name} must be available to the UI"
        );
    }

    // `clear` empties every namespace and drops the ext entry entirely.
    write(
        &host,
        &id,
        json!([
            { "area": "local", "key": "a", "value": "1" },
            { "area": "shared", "key": "b", "value": "2" }
        ]),
    )
    .unwrap();
    host.call(Caller::Ui, "storage.clear", json!({ "id": id }))
        .unwrap();
    assert_eq!(
        get(&host, &id),
        json!({ "local": {}, "personal": {}, "shared": {} })
    );
    let raw = std::fs::read_to_string(vault.join(format!("{id}.json"))).unwrap();
    assert!(
        !raw.contains("\"storage\""),
        "an empty state is not written back"
    );

    let _ = std::fs::remove_dir_all(&vault);
}

fn state_file(vault: &Path, id: &str) -> PathBuf {
    vault
        .join(".herbarium")
        .join("storage")
        .join(format!("{id}.json"))
}

#[test]
fn state_lives_in_its_own_file_and_never_in_the_sidecar_or_lists() {
    let vault = temp_vault("own-file");
    let host = open(&vault);
    let id = uid(&create(&host, "Roomy", None));
    write(
        &host,
        &id,
        json!([{ "area": "local", "key": "k", "value": "zzzbulkyvalue" }]),
    )
    .unwrap();

    assert!(state_file(&vault, &id).exists());
    let sidecar = std::fs::read_to_string(vault.join(format!("{id}.json"))).unwrap();
    assert!(!sidecar.contains("zzzbulkyvalue"), "sidecar untouched");

    for (name, args) in [
        ("pages.list", json!({})),
        ("pages.search", json!({ "query": "" })),
        ("pages.get", json!({ "id": id })),
        ("today.summary", json!({})),
    ] {
        let out = host.call(Caller::Ui, name, args).unwrap().to_string();
        assert!(!out.contains("zzzbulkyvalue"), "{name} carries page state");
    }

    // Clearing removes the file.
    host.call(Caller::Ui, "storage.clear", json!({ "id": id }))
        .unwrap();
    assert!(!state_file(&vault, &id).exists());
    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn legacy_sidecar_state_migrates_to_the_file_on_open_and_on_use() {
    let vault = temp_vault("legacy");
    let (a, b) = {
        let host = open(&vault);
        (
            uid(&create(&host, "Old A", None)),
            uid(&create(&host, "Old B", None)),
        )
    };
    // Hand-write the pre-migration layout into both sidecars.
    for id in [&a, &b] {
        let path = vault.join(format!("{id}.json"));
        let mut meta: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        meta["ext"] = json!({
            "storage": { "local": { "legacy": "yes" } },
            "look": { "icon": "🌿" }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&meta).unwrap()).unwrap();
    }

    // Reopening indexes and migrates every page.
    let host = open(&vault);
    for id in [&a, &b] {
        assert!(state_file(&vault, id).exists(), "{id} migrated at index");
        let sidecar = std::fs::read_to_string(vault.join(format!("{id}.json"))).unwrap();
        assert!(!sidecar.contains("\"storage\""), "{id} stripped");
        assert!(sidecar.contains("🌿"), "other ext keys are kept");
        assert_eq!(get(&host, id)["local"]["legacy"], "yes");
    }
    let meta = host
        .call(Caller::Agent, "pages.get", json!({ "id": a }))
        .unwrap();
    assert!(meta["meta"]["ext"].get("storage").is_none());

    // A legacy sidecar that appears later is migrated by the first storage op.
    let path = vault.join(format!("{a}.json"));
    let mut sidecar: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    sidecar["ext"]["storage"] = json!({ "shared": { "late": "1" } });
    std::fs::write(&path, serde_json::to_string_pretty(&sidecar).unwrap()).unwrap();
    host.call(Caller::Agent, "vault.rescan", json!({})).unwrap();
    let state = get(&host, &a);
    assert_eq!(state["shared"]["late"], "1");
    assert_eq!(state["local"]["legacy"], "yes", "file entries are kept");
    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn duplicate_copies_state_and_purge_removes_it() {
    let vault = temp_vault("dup-purge");
    let host = open(&vault);
    let id = uid(&create(&host, "Original", None));
    write(
        &host,
        &id,
        json!([{ "area": "personal", "key": "p", "value": "v" }]),
    )
    .unwrap();

    let copy = uid(&host
        .call(Caller::Agent, "pages.duplicate", json!({ "id": id }))
        .unwrap());
    assert_eq!(get(&host, &copy)["personal"]["p"], "v");
    write(
        &host,
        &copy,
        json!([{ "area": "personal", "key": "p", "value": "changed" }]),
    )
    .unwrap();
    assert_eq!(
        get(&host, &id)["personal"]["p"],
        "v",
        "copies are independent"
    );

    host.call(Caller::Agent, "pages.delete", json!({ "id": copy }))
        .unwrap();
    assert!(state_file(&vault, &copy).exists(), "trash keeps the state");
    host.call(Caller::Ui, "pages.purge", json!({ "id": copy }))
        .unwrap();
    assert!(!state_file(&vault, &copy).exists());
    assert!(
        state_file(&vault, &id).exists(),
        "other pages are untouched"
    );

    host.call(Caller::Agent, "pages.delete", json!({ "id": id }))
        .unwrap();
    host.call(Caller::Ui, "pages.purge", json!({})).unwrap();
    assert!(!state_file(&vault, &id).exists());
    let _ = std::fs::remove_dir_all(&vault);
}
