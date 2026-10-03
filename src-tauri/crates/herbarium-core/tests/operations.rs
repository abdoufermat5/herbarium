// Behavior of the built-in operations and of the extension contract, driven
// through `Host::call` the way every front end uses them.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use herbarium_core::{events, Caller, Ctx, Extension, Host, OpResult, Operation, Registry};
use serde::Deserialize;
use serde_json::{json, Value};

const DOC: &str = "<!doctype html><html><head><title>Cargo features</title></head><body><h1>Features</h1><p>Additive semver flags</p></body></html>";

fn temp_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "herbarium-ops-{tag}-{}-{}",
        std::process::id(),
        herbarium_core::time::now_ms()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn open(host: &mut Host, vault: &Path) {
    host.open_vault(vault.to_str().unwrap()).unwrap();
}

fn create(host: &Host, args: Value) -> Value {
    host.call(Caller::Agent, "pages.create", args).unwrap()
}

#[test]
fn create_stores_files_indexes_text_and_schedules_review() {
    let vault = temp_vault("create");
    let mut host = Host::new();
    open(&mut host, &vault);

    let page = create(&host, json!({ "html": DOC, "folder": "rust/cargo", "tags": [" rust ", "rust", ""], "reviewInDays": 3 }));
    let id = page["id"].as_str().unwrap();
    assert_eq!(page["title"], "Cargo features");
    assert_eq!(page["tags"], json!(["rust"]));
    assert_eq!(page["intervalDays"], 3);
    assert!(vault.join(format!("rust/cargo/{id}.html")).is_file());
    assert!(vault.join(format!("rust/cargo/{id}.json")).is_file());

    let hits = host.call(Caller::Agent, "pages.search", json!({ "query": "additive" })).unwrap();
    assert_eq!(hits[0]["id"], id);
    let listed = host.call(Caller::Agent, "pages.list", json!({ "folder": "rust" })).unwrap();
    assert_eq!(listed.as_array().unwrap().len(), 1, "folder filter includes subfolders");
    let text = host.call(Caller::Agent, "pages.get", json!({ "id": id, "format": "text" })).unwrap();
    assert!(text["text"].as_str().unwrap().contains("Additive semver flags"));

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn create_rejects_non_html_and_folders_outside_the_vault() {
    let vault = temp_vault("reject");
    let mut host = Host::new();
    open(&mut host, &vault);

    let err = host.call(Caller::Agent, "pages.create", json!({ "html": "just notes" })).unwrap_err();
    assert!(err.contains("not an HTML document"), "{err}");
    let err = host.call(Caller::Agent, "pages.create", json!({ "html": DOC, "folder": "../escape" })).unwrap_err();
    assert!(err.contains("invalid folder"), "{err}");
    assert_eq!(host.call(Caller::Agent, "vault.info", json!({})).unwrap()["pages"], 0);

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn update_changes_only_given_fields_and_null_folder_moves_to_root() {
    let vault = temp_vault("update");
    let mut host = Host::new();
    open(&mut host, &vault);
    let page = create(&host, json!({ "html": DOC, "folder": "drafts", "tags": ["a"], "note": "keep" }));
    let id = page["id"].as_str().unwrap();

    let updated = host.call(Caller::Agent, "pages.update", json!({ "id": id, "title": "Renamed" })).unwrap();
    assert_eq!(updated["tags"], json!(["a"]));
    assert_eq!(updated["note"], "keep");
    assert_eq!(updated["folder"], "drafts");

    let moved = host.call(Caller::Agent, "pages.update", json!({ "id": id, "folder": null })).unwrap();
    assert_eq!(moved["folder"], Value::Null);
    assert!(vault.join(format!("{id}.html")).is_file());
    assert!(!vault.join("drafts").exists(), "emptied folder is pruned");

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn set_html_replaces_content_and_keeps_metadata() {
    let vault = temp_vault("sethtml");
    let mut host = Host::new();
    open(&mut host, &vault);
    let page = create(&host, json!({ "html": DOC, "tags": ["t"] }));
    let id = page["id"].as_str().unwrap();

    let new_html = "<html><head><title>Other</title></head><body><p>workspace inheritance</p></body></html>";
    let out = host.call(Caller::Agent, "pages.set_html", json!({ "id": id, "html": new_html })).unwrap();
    assert_eq!(out["title"], "Cargo features");
    assert_eq!(out["tags"], json!(["t"]));
    let hits = host.call(Caller::Agent, "pages.search", json!({ "query": "inheritance" })).unwrap();
    assert_eq!(hits[0]["id"], id);
    assert!(host.call(Caller::Agent, "pages.search", json!({ "query": "additive" })).unwrap().as_array().unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&vault);
}

/// A third-party style extension: an operation writing its own `ext` data and
/// a subscriber counting created pages.
struct Stars {
    created: Arc<Mutex<Vec<String>>>,
}

#[derive(Deserialize)]
struct StarArgs {
    id: String,
}

impl Extension for Stars {
    fn id(&self) -> &str {
        "stars"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.add(Operation::new("stars.add", "Star a page.", json!({ "type": "object" }), |ctx: &mut Ctx, a: StarArgs| {
            let mut meta = ctx.page(&a.id)?;
            meta.ext.insert("stars".into(), json!({ "starred": true }));
            ctx.save(&meta)?;
            Ok(meta)
        }))?;
        r.add(Operation::new("stars.secret", "UI only.", json!({ "type": "object" }), |_: &mut Ctx, _: Value| Ok(1)).ui_only())?;
        let created = self.created.clone();
        r.on(events::PAGE_CREATED, move |_, e| {
            created.lock().unwrap().push(e.payload["page"]["id"].as_str().unwrap().to_string());
        });
        Ok(())
    }
}

#[test]
fn extension_data_survives_core_edits_and_reindexing() {
    let vault = temp_vault("ext");
    let created = Arc::new(Mutex::new(Vec::new()));
    let mut host = Host::with_extensions(vec![Box::new(Stars { created: created.clone() })]).unwrap();
    open(&mut host, &vault);

    let page = create(&host, json!({ "html": DOC }));
    let id = page["id"].as_str().unwrap().to_string();
    assert_eq!(*created.lock().unwrap(), vec![id.clone()], "subscriber saw page.created");

    host.call(Caller::Agent, "stars.add", json!({ "id": id })).unwrap();
    for (op, args) in [
        ("pages.update", json!({ "id": id, "note": "n", "folder": "x" })),
        ("review.schedule", json!({ "id": id, "intervalDays": 7 })),
        ("network.set", json!({ "id": id, "allowCdn": false })),
        ("pages.set_html", json!({ "id": id, "html": DOC })),
    ] {
        let meta = host.call(Caller::Agent, op, args).unwrap();
        let meta = if meta.get("meta").is_some() { meta["meta"].clone() } else { meta };
        assert_eq!(meta["ext"]["stars"]["starred"], true, "{op} kept ext data");
    }

    // The sidecar is the source of truth: a fresh index rebuilt from disk keeps it.
    std::fs::remove_dir_all(vault.join(".herbarium")).unwrap();
    let mut fresh = Host::new();
    open(&mut fresh, &vault);
    let got = fresh.call(Caller::Agent, "pages.get", json!({ "id": id })).unwrap();
    assert_eq!(got["meta"]["ext"]["stars"]["starred"], true);
    assert_eq!(got["meta"]["schemaVersion"], 1);

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn ui_only_operations_are_hidden_from_agents_and_duplicates_rejected() {
    let host = Host::with_extensions(vec![Box::new(Stars { created: Default::default() })]).unwrap();
    assert!(host.operations(Caller::Ui).any(|op| op.name == "stars.secret"));
    assert!(!host.operations(Caller::Agent).any(|op| op.name == "stars.secret" || op.name == "pages.import"));
    let err = host.call(Caller::Agent, "stars.secret", json!({})).unwrap_err();
    assert!(err.contains("unknown operation"), "{err}");

    let dup = Host::with_extensions(vec![
        Box::new(Stars { created: Default::default() }),
        Box::new(Stars { created: Default::default() }),
    ]);
    assert!(dup.is_err());
}
