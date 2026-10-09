// Reading paths: ordered page lists kept in `.herbarium/paths.json`.

use std::path::{Path, PathBuf};

use herbarium_core::time::now_ms;
use herbarium_core::{Caller, Host};
use serde_json::{Value, json};

fn temp_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "herbarium-paths-{tag}-{}-{}",
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
    host.call(Caller::Agent, op, args).unwrap()
}

fn page(host: &Host, title: &str) -> String {
    call(
        host,
        "pages.create",
        json!({ "html": format!("<title>{title}</title><p>{title}</p>") }),
    )["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn ids(path: &Value) -> Vec<String> {
    path["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            p.as_str()
                .map(str::to_string)
                .unwrap_or_else(|| p["id"].as_str().unwrap().to_string())
        })
        .collect()
}

#[test]
fn paths_keep_order_and_survive_trash() {
    let vault = temp_vault("order");
    let host = open(&vault);
    let (a, b, c) = (page(&host, "A"), page(&host, "B"), page(&host, "C"));

    let path = call(
        &host,
        "paths.create",
        json!({ "name": "Rust: basics!", "pages": [a, b, a] }),
    );
    assert_eq!(path["id"], "rust-basics");
    assert_eq!(ids(&path), [a.clone(), b.clone()], "duplicates are dropped");
    let second = call(&host, "paths.create", json!({ "name": "Rust basics" }));
    assert_eq!(second["id"], "rust-basics-2");

    let path = call(
        &host,
        "paths.add_page",
        json!({ "id": "rust-basics", "page": c, "position": 0 }),
    );
    assert_eq!(ids(&path), [c.clone(), a.clone(), b.clone()]);
    let path = call(
        &host,
        "paths.add_page",
        json!({ "id": "rust-basics", "page": a }),
    );
    assert_eq!(
        ids(&path),
        [c.clone(), b.clone(), a.clone()],
        "an existing page moves"
    );

    // Trashing a page keeps it in the path, reported as missing.
    call(&host, "pages.delete", json!({ "id": b }));
    let got = call(&host, "paths.get", json!({ "id": "rust-basics" }));
    assert_eq!(got["pages"][1], json!({ "id": b, "missing": true }));
    assert_eq!(got["pages"][0]["title"], "C");
    // Reordering may pass the missing id back; it is kept.
    let path = call(
        &host,
        "paths.update",
        json!({ "id": "rust-basics", "pages": [b, a, c] }),
    );
    assert_eq!(
        ids(&path),
        [b.clone(), a.clone(), c.clone()],
        "a missing page keeps its place"
    );
    call(&host, "pages.restore", json!({ "id": b }));
    let got = call(&host, "paths.get", json!({ "id": "rust-basics" }));
    assert_eq!(got["pages"][0]["title"], "B");

    let path = call(
        &host,
        "paths.remove_page",
        json!({ "id": "rust-basics", "page": c }),
    );
    assert_eq!(ids(&path), [b.clone(), a.clone()]);
    let path = call(
        &host,
        "paths.update",
        json!({ "id": "rust-basics", "name": " Renamed ", "description": "d" }),
    );
    assert_eq!(path["name"], "Renamed");
    assert_eq!(path["id"], "rust-basics", "renaming keeps the id");

    call(&host, "paths.delete", json!({ "id": "rust-basics" }));
    let list = call(&host, "paths.list", json!({}));
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert!(vault.join(".herbarium/paths.json").exists());

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn paths_reject_unknown_pages_and_bad_names() {
    let vault = temp_vault("invalid");
    let host = open(&vault);
    let a = page(&host, "A");
    let err = host
        .call(
            Caller::Agent,
            "paths.create",
            json!({ "name": "x", "pages": [a, "ghost"] }),
        )
        .unwrap_err();
    assert!(err.contains("ghost"), "{err}");
    assert!(
        host.call(Caller::Agent, "paths.create", json!({ "name": "  " }))
            .is_err()
    );
    call(&host, "paths.create", json!({ "name": "P" }));
    assert!(
        host.call(
            Caller::Agent,
            "paths.add_page",
            json!({ "id": "p", "page": "ghost" })
        )
        .is_err()
    );
    assert!(
        host.call(Caller::Agent, "paths.get", json!({ "id": "nope" }))
            .unwrap_err()
            .contains("not found")
    );
    assert!(
        call(&host, "paths.list", json!({}))[0]["pages"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let _ = std::fs::remove_dir_all(&vault);
}
