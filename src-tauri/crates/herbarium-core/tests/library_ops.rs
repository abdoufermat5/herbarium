// Folder, tag and network-settings operations, driven through `Host::call`.

use std::path::{Path, PathBuf};

use herbarium_core::{Caller, Host};
use serde_json::{Value, json};

fn doc(title: &str) -> String {
    format!(
        "<!doctype html><html><head><title>{title}</title></head><body><p>{title} body</p></body></html>"
    )
}

fn temp_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "herbarium-lib-{tag}-{}-{}",
        std::process::id(),
        herbarium_core::time::now_ms()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn open(vault: &Path) -> Host {
    let mut host = Host::new();
    host.open_vault(vault.to_str().unwrap()).unwrap();
    host
}

fn create(host: &Host, title: &str, folder: Option<&str>, tags: &[&str]) -> String {
    let page = host
        .call(
            Caller::Agent,
            "pages.create",
            json!({ "html": doc(title), "folder": folder, "tags": tags }),
        )
        .unwrap();
    page["id"].as_str().unwrap().to_string()
}

fn meta(host: &Host, id: &str) -> Value {
    host.call(Caller::Agent, "pages.get", json!({ "id": id }))
        .unwrap()["meta"]
        .clone()
}

fn meta_exists(host: &Host, id: &str) -> bool {
    host.call(Caller::Agent, "pages.get", json!({ "id": id }))
        .is_ok()
}

#[test]
fn folder_rename_moves_nested_pages_and_keeps_their_ids() {
    let vault = temp_vault("rename");
    let host = open(&vault);
    let top = create(&host, "Ownership", Some("rust"), &[]);
    let nested = create(&host, "Cargo features", Some("rust/cargo"), &[]);

    let res = host
        .call(
            Caller::Agent,
            "folders.rename",
            json!({ "from": "rust", "to": " lang/rust/ " }),
        )
        .unwrap();
    assert_eq!(res, json!({ "folder": "lang/rust" }));

    assert_eq!(meta(&host, &top)["folder"], "lang/rust");
    assert_eq!(meta(&host, &nested)["folder"], "lang/rust/cargo");
    assert!(vault.join(format!("lang/rust/{top}.html")).is_file());
    assert!(
        vault
            .join(format!("lang/rust/cargo/{nested}.html"))
            .is_file()
    );
    assert!(!vault.join("rust").exists());

    let sidecar: Value = serde_json::from_str(
        &std::fs::read_to_string(vault.join(format!("lang/rust/cargo/{nested}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(
        sidecar["folder"], "lang/rust/cargo",
        "nested sidecars are rewritten"
    );

    // A rescan from disk agrees with the index.
    host.call(Caller::Agent, "vault.rescan", json!({})).unwrap();
    assert_eq!(meta(&host, &nested)["folder"], "lang/rust/cargo");
    assert_eq!(
        host.call(Caller::Agent, "vault.info", json!({})).unwrap()["pages"],
        2
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn folder_rename_refuses_existing_destination_and_moving_into_itself() {
    let vault = temp_vault("rename-refuse");
    let host = open(&vault);
    let id = create(&host, "Ownership", Some("rust"), &[]);
    host.call(Caller::Agent, "folders.create", json!({ "path": "go" }))
        .unwrap();

    assert!(
        host.call(
            Caller::Agent,
            "folders.rename",
            json!({ "from": "rust", "to": "go" })
        )
        .is_err()
    );
    assert!(
        host.call(
            Caller::Agent,
            "folders.rename",
            json!({ "from": "rust", "to": "rust/inner" })
        )
        .is_err()
    );
    assert!(
        host.call(
            Caller::Agent,
            "folders.rename",
            json!({ "from": "missing", "to": "elsewhere" })
        )
        .is_err()
    );
    assert!(
        host.call(
            Caller::Agent,
            "folders.rename",
            json!({ "from": "rust", "to": "../out" })
        )
        .is_err()
    );

    assert_eq!(meta(&host, &id)["folder"], "rust");
    assert!(vault.join(format!("rust/{id}.html")).is_file());

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn deleting_an_empty_folder_removes_it() {
    let vault = temp_vault("delete-empty");
    let host = open(&vault);
    host.call(
        Caller::Agent,
        "folders.create",
        json!({ "path": "drafts/old" }),
    )
    .unwrap();

    let res = host
        .call(Caller::Agent, "folders.delete", json!({ "path": "drafts" }))
        .unwrap();
    assert_eq!(res, json!({ "trashed": 0 }));
    assert!(!vault.join("drafts").exists());
    assert_eq!(
        host.call(Caller::Agent, "folders.list", json!({})).unwrap(),
        json!([])
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn deleting_a_folder_with_pages_needs_with_pages_which_trashes_them() {
    let vault = temp_vault("delete-pages");
    let host = open(&vault);
    let kept = create(&host, "Elsewhere", None, &[]);
    let a = create(&host, "Ownership", Some("rust"), &[]);
    let b = create(&host, "Cargo features", Some("rust/cargo"), &[]);

    let err = host
        .call(Caller::Agent, "folders.delete", json!({ "path": "rust" }))
        .unwrap_err();
    assert!(err.contains("not empty"), "{err}");
    assert!(vault.join(format!("rust/cargo/{b}.html")).is_file());
    assert_eq!(
        host.call(Caller::Agent, "vault.info", json!({})).unwrap()["pages"],
        3
    );

    let res = host
        .call(
            Caller::Agent,
            "folders.delete",
            json!({ "path": "rust", "withPages": true }),
        )
        .unwrap();
    assert_eq!(res, json!({ "trashed": 2 }));
    assert!(!vault.join("rust").exists());
    assert_eq!(
        host.call(Caller::Agent, "vault.info", json!({})).unwrap()["pages"],
        1
    );
    assert!(meta_exists(&host, &kept));
    assert!(!meta_exists(&host, &a));

    let trash = host.call(Caller::Agent, "pages.trash", json!({})).unwrap();
    let mut trashed: Vec<&str> = trash
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    trashed.sort();
    let mut expected = vec![a.as_str(), b.as_str()];
    expected.sort();
    assert_eq!(trashed, expected);
    assert!(
        vault
            .join(format!(".herbarium/trash/{b}/{b}.html"))
            .is_file()
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn renaming_a_tag_onto_an_existing_one_merges_without_duplicates() {
    let vault = temp_vault("tag-rename");
    let host = open(&vault);
    let both = create(&host, "Both", None, &["rs", "rust", "lang"]);
    let old = create(&host, "Old only", None, &["rs"]);
    let other = create(&host, "Other", None, &["go"]);

    let res = host
        .call(
            Caller::Agent,
            "tags.rename",
            json!({ "from": "rs", "to": " rust " }),
        )
        .unwrap();
    assert_eq!(res, json!({ "updated": 2 }));
    assert_eq!(meta(&host, &both)["tags"], json!(["rust", "lang"]));
    assert_eq!(meta(&host, &old)["tags"], json!(["rust"]));
    assert_eq!(meta(&host, &other)["tags"], json!(["go"]));

    let counts = host.call(Caller::Agent, "tags.list", json!({})).unwrap();
    let names: Vec<&str> = counts
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["tag"].as_str().unwrap())
        .collect();
    assert!(!names.contains(&"rs"), "{counts}");

    // Sidecars carry the change, so a rescan keeps it.
    host.call(Caller::Agent, "vault.rescan", json!({})).unwrap();
    assert_eq!(meta(&host, &both)["tags"], json!(["rust", "lang"]));

    assert!(
        host.call(
            Caller::Agent,
            "tags.rename",
            json!({ "from": "rust", "to": "  " })
        )
        .is_err()
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn deleting_a_tag_removes_it_from_pages_and_keeps_the_pages() {
    let vault = temp_vault("tag-delete");
    let host = open(&vault);
    let a = create(&host, "A", None, &["draft", "rust"]);
    let b = create(&host, "B", None, &["draft"]);
    let c = create(&host, "C", None, &["rust"]);

    let res = host
        .call(Caller::Agent, "tags.delete", json!({ "tag": "draft" }))
        .unwrap();
    assert_eq!(res, json!({ "updated": 2 }));
    assert_eq!(meta(&host, &a)["tags"], json!(["rust"]));
    assert_eq!(meta(&host, &b)["tags"], json!([]));
    assert_eq!(meta(&host, &c)["tags"], json!(["rust"]));
    assert_eq!(
        host.call(Caller::Agent, "vault.info", json!({})).unwrap()["pages"],
        3
    );

    let res = host
        .call(Caller::Agent, "tags.delete", json!({ "tag": "missing" }))
        .unwrap();
    assert_eq!(res, json!({ "updated": 0 }));

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn network_default_is_off_and_configurable_from_the_ui_only() {
    let vault = temp_vault("net-settings");
    let host = open(&vault);

    let settings = host
        .call(Caller::Agent, "network.settings", json!({}))
        .unwrap();
    assert_eq!(settings, json!({ "defaultAllowCdn": false }));
    let before = create(&host, "Before", None, &[]);
    assert_eq!(meta(&host, &before)["allowCdn"], false);

    let err = host
        .call(
            Caller::Agent,
            "network.configure",
            json!({ "defaultAllowCdn": true }),
        )
        .unwrap_err();
    assert!(err.contains("unknown operation"), "{err}");

    host.call(
        Caller::Ui,
        "network.configure",
        json!({ "defaultAllowCdn": true }),
    )
    .unwrap();
    assert_eq!(
        host.call(Caller::Agent, "network.settings", json!({}))
            .unwrap(),
        json!({ "defaultAllowCdn": true })
    );
    assert!(vault.join(".herbarium/network.json").is_file());
    let after = create(&host, "After", None, &[]);
    assert_eq!(
        meta(&host, &after)["allowCdn"],
        true,
        "new pages follow the vault default"
    );
    assert_eq!(
        meta(&host, &before)["allowCdn"],
        false,
        "existing pages are untouched"
    );

    // The setting persists across reopening the vault.
    let reopened = open(&vault);
    assert_eq!(
        reopened
            .call(Caller::Ui, "network.settings", json!({}))
            .unwrap(),
        json!({ "defaultAllowCdn": true })
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn agents_may_turn_network_off_but_not_on() {
    let vault = temp_vault("net-agent");
    let host = open(&vault);
    let id = create(&host, "Chart", None, &[]);

    let err = host
        .call(
            Caller::Agent,
            "network.set",
            json!({ "id": id, "allowCdn": true }),
        )
        .unwrap_err();
    assert!(
        err.contains("agents may only turn network access off"),
        "{err}"
    );
    assert_eq!(meta(&host, &id)["allowCdn"], false);

    let on = host
        .call(
            Caller::Ui,
            "network.set",
            json!({ "id": id, "allowCdn": true }),
        )
        .unwrap();
    assert_eq!(on["allowCdn"], true);

    let off = host
        .call(
            Caller::Agent,
            "network.set",
            json!({ "id": id, "allowCdn": false }),
        )
        .unwrap();
    assert_eq!(off["allowCdn"], false);
    assert_eq!(meta(&host, &id)["allowCdn"], false);

    let _ = std::fs::remove_dir_all(&vault);
}
