// The rescan fast path, folder renames and preview staleness, driven through
// `Host::call` against a real vault directory.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use herbarium_core::time::now_ms;
use herbarium_core::{Caller, Host};
use serde_json::{Value, json};

fn doc(title: &str, body: &str) -> String {
    format!(
        "<!doctype html><html><head><title>{title}</title></head><body><p>{body}</p></body></html>"
    )
}

fn temp_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "herbarium-rescan-{tag}-{}-{}",
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

fn create(host: &Host, title: &str, body: &str, folder: Option<&str>) -> String {
    let page = host
        .call(
            Caller::Agent,
            "pages.create",
            json!({ "html": doc(title, body), "folder": folder }),
        )
        .unwrap();
    page["id"].as_str().unwrap().to_string()
}

/// Modification time of every file under `dir` except the internal `.herbarium`.
fn mtimes(dir: &Path, out: &mut BTreeMap<PathBuf, SystemTime>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if entry.file_name() == ".herbarium" {
            continue;
        }
        if path.is_dir() {
            mtimes(&path, out);
        } else {
            out.insert(path.clone(), entry.metadata().unwrap().modified().unwrap());
        }
    }
}

fn rescan(host: &Host) -> Value {
    host.call(Caller::Ui, "vault.rescan", json!({})).unwrap()
}

fn search_ids(host: &Host, query: &str) -> Vec<String> {
    let hits = host
        .call(Caller::Agent, "pages.search", json!({ "query": query }))
        .unwrap();
    hits.as_array()
        .unwrap()
        .iter()
        .map(|h| h["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn rescan_of_an_unchanged_vault_writes_nothing() {
    let vault = temp_vault("unchanged");
    let host = open(&vault);
    create(&host, "Root page", "alpha", None);
    create(&host, "Nested page", "beta", Some("notes/deep"));
    // A bare HTML file dropped into the vault: the first rescan adopts it and
    // writes its sidecar, the second must leave everything alone.
    std::fs::write(vault.join("dropped.html"), doc("Dropped", "gamma")).unwrap();

    let first = rescan(&host);
    assert_eq!(first["indexed"], 1, "only the dropped file is new: {first}");
    assert!(vault.join("dropped.json").is_file());

    let mut before = BTreeMap::new();
    mtimes(&vault, &mut before);
    std::thread::sleep(Duration::from_millis(50));

    let second = rescan(&host);
    assert_eq!(second["indexed"], 0, "nothing changed: {second}");
    assert_eq!(second["removed"], 0);
    let mut after = BTreeMap::new();
    mtimes(&vault, &mut after);
    assert_eq!(before, after, "an unchanged rescan must not touch any file");
}

#[test]
fn rescan_picks_up_an_edited_page_and_keeps_its_metadata() {
    let vault = temp_vault("edited");
    let host = open(&vault);
    let id = create(&host, "Original", "first body", None);
    host.call(
        Caller::Agent,
        "pages.update",
        json!({ "id": id, "tags": ["keep"] }),
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(
        vault.join(format!("{id}.html")),
        doc("Original", "second body zebra"),
    )
    .unwrap();

    let report = rescan(&host);
    assert_eq!(report["indexed"], 1, "{report}");
    assert_eq!(search_ids(&host, "zebra"), vec![id.clone()]);
    let page = host
        .call(Caller::Agent, "pages.get", json!({ "id": id }))
        .unwrap();
    assert_eq!(page["meta"]["tags"], json!(["keep"]));
}

#[test]
fn folder_rename_updates_the_index_and_search_without_a_rescan() {
    let vault = temp_vault("rename");
    let host = open(&vault);
    let top = create(&host, "Top", "zebra stripes", Some("animals"));
    let nested = create(&host, "Nested", "zebra herd", Some("animals/africa"));
    let other = create(&host, "Other", "zebra elsewhere", Some("animals-extra"));

    let renamed = host
        .call(
            Caller::Ui,
            "folders.rename",
            json!({ "from": "animals", "to": "fauna" }),
        )
        .unwrap();
    assert_eq!(renamed["folder"], "fauna");

    let folder_of = |id: &str| -> Value {
        host.call(Caller::Agent, "pages.get", json!({ "id": id }))
            .unwrap()["meta"]["folder"]
            .clone()
    };
    assert_eq!(folder_of(&top), "fauna");
    assert_eq!(folder_of(&nested), "fauna/africa");
    assert_eq!(
        folder_of(&other),
        "animals-extra",
        "a sibling prefix is untouched"
    );

    // Search still finds every page, and the FTS folder column follows.
    let mut ids = search_ids(&host, "zebra");
    ids.sort();
    let mut expected = vec![top.clone(), nested.clone(), other.clone()];
    expected.sort();
    assert_eq!(ids, expected);
    assert_eq!(search_ids(&host, "fauna").len(), 2);
    assert!(search_ids(&host, "animals").iter().all(|id| *id == other));

    // The rows already match the files: a rescan has nothing to do.
    let report = rescan(&host);
    assert_eq!(report["indexed"], 0, "{report}");
    assert_eq!(report["removed"], 0, "{report}");
}

fn digest() -> Value {
    json!({ "w": 800.0, "h": 600.0, "bg": "rgb(255, 255, 255)", "blocks": [] })
}

#[test]
fn previews_follow_the_html_not_the_metadata() {
    let vault = temp_vault("previews");
    let host = open(&vault);
    let id = create(&host, "Preview me", "body", None);

    let listed = host.call(Caller::Ui, "previews.list", json!({})).unwrap();
    assert_eq!(listed["missing"], json!([id]));

    host.call(
        Caller::Ui,
        "previews.save",
        json!({ "id": id, "digest": digest() }),
    )
    .unwrap();
    let listed = host.call(Caller::Ui, "previews.list", json!({})).unwrap();
    assert!(listed["previews"][&id].is_object(), "{listed}");
    assert_eq!(listed["missing"], json!([]));

    // Metadata, review and note edits bump `updatedAt` but not the HTML.
    host.call(
        Caller::Ui,
        "pages.update",
        json!({ "id": id, "tags": ["t"], "note": "a note" }),
    )
    .unwrap();
    host.call(
        Caller::Ui,
        "review.schedule",
        json!({ "id": id, "intervalMinutes": 60 }),
    )
    .unwrap();
    let listed = host.call(Caller::Ui, "previews.list", json!({})).unwrap();
    assert!(listed["previews"][&id].is_object(), "{listed}");
    assert_eq!(listed["missing"], json!([]));

    // A new HTML body invalidates it.
    std::thread::sleep(Duration::from_millis(20));
    host.call(
        Caller::Ui,
        "pages.set_html",
        json!({ "id": id, "html": doc("Preview me", "different body") }),
    )
    .unwrap();
    let listed = host.call(Caller::Ui, "previews.list", json!({})).unwrap();
    assert_eq!(listed["missing"], json!([id]), "{listed}");
    assert!(listed["previews"].get(&id).is_none());
}
