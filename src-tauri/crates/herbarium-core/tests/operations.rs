// Behavior of the built-in operations and of the extension contract, driven
// through `Host::call` the way every front end uses them.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use herbarium_core::{Caller, Ctx, Extension, Host, OpResult, Operation, Registry, events};
use serde::Deserialize;
use serde_json::{Value, json};

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

    let page = create(
        &host,
        json!({ "html": DOC, "folder": "rust/cargo", "tags": [" rust ", "rust", ""], "reviewInMinutes": 4320 }),
    );
    let id = page["id"].as_str().unwrap();
    assert_eq!(page["title"], "Cargo features");
    assert_eq!(page["tags"], json!(["rust"]));
    assert_eq!(page["intervalMinutes"], 4320);
    assert!(vault.join(format!("rust/cargo/{id}.html")).is_file());
    assert!(vault.join(format!("rust/cargo/{id}.json")).is_file());

    let hits = host
        .call(
            Caller::Agent,
            "pages.search",
            json!({ "query": "additive" }),
        )
        .unwrap();
    assert_eq!(hits[0]["id"], id);
    let listed = host
        .call(Caller::Agent, "pages.list", json!({ "folder": "rust" }))
        .unwrap();
    assert_eq!(
        listed.as_array().unwrap().len(),
        1,
        "folder filter includes subfolders"
    );
    let text = host
        .call(
            Caller::Agent,
            "pages.get",
            json!({ "id": id, "format": "text" }),
        )
        .unwrap();
    assert!(
        text["text"]
            .as_str()
            .unwrap()
            .contains("Additive semver flags")
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn create_rejects_non_html_and_folders_outside_the_vault() {
    let vault = temp_vault("reject");
    let mut host = Host::new();
    open(&mut host, &vault);

    let err = host
        .call(
            Caller::Agent,
            "pages.create",
            json!({ "html": "just notes" }),
        )
        .unwrap_err();
    assert!(err.contains("not an HTML document"), "{err}");
    let err = host
        .call(
            Caller::Agent,
            "pages.create",
            json!({ "html": DOC, "folder": "../escape" }),
        )
        .unwrap_err();
    assert!(err.contains("invalid folder"), "{err}");
    assert_eq!(
        host.call(Caller::Agent, "vault.info", json!({})).unwrap()["pages"],
        0
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn update_changes_only_given_fields_and_null_folder_moves_to_root() {
    let vault = temp_vault("update");
    let mut host = Host::new();
    open(&mut host, &vault);
    let page = create(
        &host,
        json!({ "html": DOC, "folder": "drafts", "tags": ["a"], "note": "keep" }),
    );
    let id = page["id"].as_str().unwrap();

    let updated = host
        .call(
            Caller::Agent,
            "pages.update",
            json!({ "id": id, "title": "Renamed" }),
        )
        .unwrap();
    assert_eq!(updated["tags"], json!(["a"]));
    assert_eq!(updated["note"], "keep");
    assert_eq!(updated["folder"], "drafts");

    let moved = host
        .call(
            Caller::Agent,
            "pages.update",
            json!({ "id": id, "folder": null }),
        )
        .unwrap();
    assert_eq!(moved["folder"], Value::Null);
    assert!(vault.join(format!("{id}.html")).is_file());
    assert!(
        vault.join("drafts").exists(),
        "emptied folder is not pruned"
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn set_html_replaces_content_and_keeps_metadata() {
    let vault = temp_vault("sethtml");
    let mut host = Host::new();
    open(&mut host, &vault);
    let page = create(&host, json!({ "html": DOC, "tags": ["t"] }));
    let id = page["id"].as_str().unwrap();

    let new_html =
        "<html><head><title>Other</title></head><body><p>workspace inheritance</p></body></html>";
    let out = host
        .call(
            Caller::Agent,
            "pages.set_html",
            json!({ "id": id, "html": new_html }),
        )
        .unwrap();
    assert_eq!(out["tags"], json!(["t"]));
    let hits = host
        .call(
            Caller::Agent,
            "pages.search",
            json!({ "query": "inheritance" }),
        )
        .unwrap();
    assert_eq!(hits[0]["id"], id);
    assert!(
        host.call(
            Caller::Agent,
            "pages.search",
            json!({ "query": "additive" })
        )
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty()
    );

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
        r.add(Operation::new(
            "stars.add",
            "Star a page.",
            json!({ "type": "object" }),
            |ctx: &mut Ctx, a: StarArgs| {
                let mut meta = ctx.page(&a.id)?;
                meta.ext.insert("stars".into(), json!({ "starred": true }));
                ctx.save(&meta)?;
                Ok(meta)
            },
        ))?;
        r.add(
            Operation::new(
                "stars.secret",
                "UI only.",
                json!({ "type": "object" }),
                |_: &mut Ctx, _: Value| Ok(1),
            )
            .ui_only(),
        )?;
        let created = self.created.clone();
        r.on(events::PAGE_CREATED, move |_, e| {
            created
                .lock()
                .unwrap()
                .push(e.payload["page"]["id"].as_str().unwrap().to_string());
        });
        Ok(())
    }
}

#[test]
fn extension_data_survives_core_edits_and_reindexing() {
    let vault = temp_vault("ext");
    let created = Arc::new(Mutex::new(Vec::new()));
    let mut host = Host::with_extensions(vec![Box::new(Stars {
        created: created.clone(),
    })])
    .unwrap();
    open(&mut host, &vault);

    let page = create(&host, json!({ "html": DOC }));
    let id = page["id"].as_str().unwrap().to_string();
    assert_eq!(
        *created.lock().unwrap(),
        vec![id.clone()],
        "subscriber saw page.created"
    );

    host.call(Caller::Agent, "stars.add", json!({ "id": id }))
        .unwrap();
    for (op, args) in [
        (
            "pages.update",
            json!({ "id": id, "note": "n", "folder": "x" }),
        ),
        (
            "review.schedule",
            json!({ "id": id, "intervalMinutes": 10080 }),
        ),
        ("network.set", json!({ "id": id, "allowCdn": false })),
        ("pages.set_html", json!({ "id": id, "html": DOC })),
    ] {
        let meta = host.call(Caller::Agent, op, args).unwrap();
        let meta = if meta.get("meta").is_some() {
            meta["meta"].clone()
        } else {
            meta
        };
        assert_eq!(meta["ext"]["stars"]["starred"], true, "{op} kept ext data");
    }

    // The sidecar is the source of truth: a fresh index rebuilt from disk keeps it.
    std::fs::remove_dir_all(vault.join(".herbarium")).unwrap();
    let mut fresh = Host::new();
    open(&mut fresh, &vault);
    let got = fresh
        .call(Caller::Agent, "pages.get", json!({ "id": id }))
        .unwrap();
    assert_eq!(got["meta"]["ext"]["stars"]["starred"], true);
    assert_eq!(got["meta"]["schemaVersion"], 2);

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn ui_only_operations_are_hidden_from_agents_and_duplicates_rejected() {
    let host = Host::with_extensions(vec![Box::new(Stars {
        created: Default::default(),
    })])
    .unwrap();
    assert!(
        host.operations(Caller::Ui)
            .any(|op| op.name == "stars.secret")
    );
    assert!(
        !host
            .operations(Caller::Agent)
            .any(|op| op.name == "stars.secret" || op.name == "pages.import")
    );
    let err = host
        .call(Caller::Agent, "stars.secret", json!({}))
        .unwrap_err();
    assert!(err.contains("unknown operation"), "{err}");

    let dup = Host::with_extensions(vec![
        Box::new(Stars {
            created: Default::default(),
        }),
        Box::new(Stars {
            created: Default::default(),
        }),
    ]);
    assert!(dup.is_err());
}

#[test]
fn empty_folders_are_created_listed_and_confined_to_the_vault() {
    let vault = temp_vault("folders");
    let mut host = Host::new();
    open(&mut host, &vault);

    let made = host
        .call(
            Caller::Agent,
            "folders.create",
            json!({ "path": " rust//cargo/ " }),
        )
        .unwrap();
    assert_eq!(made, "rust/cargo");
    let listed = host.call(Caller::Agent, "folders.list", json!({})).unwrap();
    assert_eq!(
        listed,
        json!(["rust", "rust/cargo"]),
        "empty folders and their parents are listed, .herbarium is not"
    );

    for bad in ["../x", ".hidden", "a/.git", "   "] {
        assert!(
            host.call(Caller::Agent, "folders.create", json!({ "path": bad }))
                .is_err(),
            "{bad}"
        );
    }
    assert!(!vault.parent().unwrap().join("x").exists());

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn import_lands_in_the_chosen_folder_and_applies_import_scheduling() {
    let vault = temp_vault("import");
    let mut host = Host::new();
    open(&mut host, &vault);
    host.call(Caller::Ui, "folders.create", json!({ "path": "rust" }))
        .unwrap();

    let files = json!([{ "name": "a.html", "content": DOC }, { "name": "bad.html", "content": "plain text" }]);
    let res = host
        .call(
            Caller::Ui,
            "pages.import",
            json!({ "files": files, "folder": "rust" }),
        )
        .unwrap();
    assert_eq!(res["imported"], 1);
    assert_eq!(res["errors"].as_array().unwrap().len(), 1);
    let listed = host.call(Caller::Ui, "pages.list", json!({})).unwrap();
    assert_eq!(listed[0]["folder"], "rust");
    assert_eq!(
        listed[0]["intervalMinutes"],
        Value::Null,
        "imports are not scheduled by default"
    );

    let root = host
        .call(
            Caller::Ui,
            "pages.import",
            json!({ "files": [{ "content": DOC }], "folder": null }),
        )
        .unwrap();
    assert_eq!(root["imported"], 1);
    let all = host.call(Caller::Ui, "pages.list", json!({})).unwrap();
    assert_eq!(
        all.as_array()
            .unwrap()
            .iter()
            .filter(|p| p["folder"].is_null())
            .count(),
        1
    );

    assert!(
        host.call(
            Caller::Ui,
            "pages.import",
            json!({ "files": [], "folder": "../x" })
        )
        .is_err()
    );

    host.call(
        Caller::Ui,
        "review.configure",
        json!({ "importReviewMinutes": 30 }),
    )
    .unwrap();
    host.call(
        Caller::Ui,
        "pages.import",
        json!({ "files": [{ "content": DOC }], "folder": "rust" }),
    )
    .unwrap();
    let scheduled = host
        .call(Caller::Ui, "pages.list", json!({ "folder": "rust" }))
        .unwrap();
    assert!(
        scheduled
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["intervalMinutes"] == 30)
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn review_settings_persist_validate_and_drive_completion_and_the_queue() {
    let vault = temp_vault("review");
    let mut host = Host::new();
    open(&mut host, &vault);

    let defaults = host
        .call(Caller::Agent, "review.settings", json!({}))
        .unwrap();
    assert_eq!(defaults["presets"], json!([1440, 4320, 10080, 43200]));
    assert_eq!(defaults["strategy"], "fsrs");
    assert_eq!(defaults["desiredRetention"].as_f64().unwrap(), 0.9);
    assert!(
        !host
            .operations(Caller::Agent)
            .any(|op| op.name == "review.configure"),
        "agents read, the user writes"
    );

    assert!(
        host.call(Caller::Ui, "review.configure", json!({ "presets": [] }))
            .is_err()
    );
    assert!(
        host.call(Caller::Ui, "review.configure", json!({ "queueLimit": 0 }))
            .is_err()
    );
    let unchanged = host
        .call(Caller::Agent, "review.settings", json!({}))
        .unwrap();
    assert_eq!(unchanged, defaults, "a rejected update changes nothing");

    host.call(
        Caller::Ui,
        "review.configure",
        json!({ "presets": [5, 2, 2, 10], "queueLimit": 1, "strategy": "ladder" }),
    )
    .unwrap();
    let page = create(&host, json!({ "html": DOC }));
    let id = page["id"].as_str().unwrap().to_string();
    let steps: Vec<Value> = (0..4)
        .map(|_| {
            host.call(Caller::Agent, "review.complete", json!({ "id": id }))
                .unwrap()["intervalMinutes"]
                .clone()
        })
        .collect();
    assert_eq!(
        steps,
        vec![json!(2), json!(5), json!(10), json!(10)],
        "ladder over the sorted presets, then stays on top"
    );
    assert!(
        host.call(Caller::Agent, "review.complete", json!({ "id": "missing" }))
            .is_err()
    );

    // Settings live in the vault, so a freshly opened host sees them.
    let mut fresh = Host::new();
    open(&mut fresh, &vault);
    let saved = fresh
        .call(Caller::Agent, "review.settings", json!({}))
        .unwrap();
    assert_eq!(saved["presets"], json!([2, 5, 10]));
    assert_eq!(saved["queueLimit"], 1);

    // The queue is capped, and `review.due` only lists pages that are due.
    let other = create(&host, json!({ "html": DOC }));
    for p in [&page, &other] {
        let mut meta = host
            .call(Caller::Agent, "pages.get", json!({ "id": p["id"] }))
            .unwrap()["meta"]
            .clone();
        meta["nextReview"] = json!(1);
        let store = host.store().unwrap();
        let parsed: herbarium_core::models::PageMeta = serde_json::from_value(meta).unwrap();
        store.upsert(&parsed, "x", 0).unwrap();
    }
    let due = host.call(Caller::Agent, "review.due", json!({})).unwrap();
    assert_eq!(
        due.as_array().unwrap().len(),
        1,
        "queueLimit caps the queue"
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn reviews_can_be_scheduled_in_minutes() {
    let vault = temp_vault("minutes");
    let mut host = Host::new();
    open(&mut host, &vault);

    let page = create(&host, json!({ "html": DOC }));
    let meta = host
        .call(
            Caller::Agent,
            "review.schedule",
            json!({ "id": page["id"], "intervalMinutes": 30 }),
        )
        .unwrap();
    assert_eq!(meta["intervalMinutes"], 30);
    let due = host.call(Caller::Agent, "review.due", json!({})).unwrap();
    assert!(due.as_array().unwrap().is_empty(), "not due yet");

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn sidecars_and_indexes_written_with_whole_days_are_upgraded() {
    let vault = temp_vault("legacy");
    let mut host = Host::new();
    open(&mut host, &vault);
    let page = create(&host, json!({ "html": DOC, "reviewInMinutes": 1440 }));
    let id = page["id"].as_str().unwrap().to_string();

    // Rewrite the sidecar the way a schema-1 build stored it.
    let sidecar = vault.join(format!("{id}.json"));
    let mut raw: Value = serde_json::from_str(&std::fs::read_to_string(&sidecar).unwrap()).unwrap();
    let obj = raw.as_object_mut().unwrap();
    obj.remove("intervalMinutes");
    obj.insert("intervalDays".into(), json!(3));
    obj.insert("schemaVersion".into(), json!(1));
    std::fs::write(&sidecar, raw.to_string()).unwrap();

    std::fs::remove_dir_all(vault.join(".herbarium")).unwrap();
    let mut fresh = Host::new();
    open(&mut fresh, &vault);
    let got = fresh
        .call(Caller::Agent, "pages.get", json!({ "id": id }))
        .unwrap();
    assert_eq!(got["meta"]["intervalMinutes"], 3 * 1440);
    assert_eq!(got["meta"]["schemaVersion"], 2);
    assert!(got["meta"].get("intervalDays").is_none());

    let _ = std::fs::remove_dir_all(&vault);
}
