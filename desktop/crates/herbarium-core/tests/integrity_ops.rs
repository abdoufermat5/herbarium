// Two hosts on one vault stand in for two processes (desktop app + native
// host / CLI): creates never collide, a rescan never drops the other's page,
// and opening a fresh index concurrently succeeds for both.

use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;

use herbarium_core::time::now_ms;
use herbarium_core::{Caller, Host};
use serde_json::{Value, json};

fn temp_vault(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "herbarium-integrity-{tag}-{}-{}",
        std::process::id(),
        now_ms()
    ))
}

fn open(vault: &std::path::Path) -> Host {
    let mut host = Host::new();
    host.open_vault(vault.to_str().unwrap()).unwrap();
    host
}

fn create(host: &Host, title: &str) -> Value {
    host.call(
        Caller::Agent,
        "pages.create",
        json!({ "html": format!("<title>{title}</title><p>{title}</p>"), "title": title }),
    )
    .unwrap()
}

#[test]
fn concurrent_creates_with_one_title_yield_distinct_pages() {
    let vault = temp_vault("creates");
    let hosts: Vec<_> = (0..2).map(|_| open(&vault)).collect();
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = hosts
        .into_iter()
        .map(|host| {
            let barrier = barrier.clone();
            thread::spawn(move || {
                let mut ids = Vec::new();
                for i in 0..10 {
                    barrier.wait();
                    let page = create(&host, "Same Title");
                    // The page's own body proves it was not overwritten.
                    let id = page["id"].as_str().unwrap().to_string();
                    let got = host
                        .call(Caller::Ui, "pages.get", json!({ "id": id }))
                        .unwrap();
                    assert_eq!(got["meta"]["id"], page["id"], "round {i}");
                    ids.push(id);
                }
                ids
            })
        })
        .collect();
    let mut all: Vec<String> = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 20, "every create got its own id");
    let files = std::fs::read_dir(&vault)
        .unwrap()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "html"))
        .count();
    assert_eq!(files, 20, "no page file was overwritten");
    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn a_rescan_in_one_host_keeps_a_page_the_other_created() {
    let vault = temp_vault("rescan");
    let a = open(&vault);
    let b = open(&vault);
    let page = create(&b, "Made by B");
    let report = a.call(Caller::Ui, "vault.rescan", json!({})).unwrap();
    assert_eq!(report["removed"], 0);
    let id = page["id"].as_str().unwrap();
    a.call(Caller::Ui, "pages.get", json!({ "id": id }))
        .unwrap();
    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn a_save_sees_the_sidecar_another_host_wrote() {
    let vault = temp_vault("stale");
    let a = open(&vault);
    let b = open(&vault);
    let page = create(&a, "Shared");
    let id = page["id"].as_str().unwrap();
    // A's index copy of the page is now stale: B retags it.
    b.call(
        Caller::Ui,
        "pages.update",
        json!({ "id": id, "tags": ["from-b"] }),
    )
    .unwrap();
    // A edits the title; B's tags must survive.
    a.call(
        Caller::Ui,
        "pages.update",
        json!({ "id": id, "title": "Renamed" }),
    )
    .unwrap();
    let got = a
        .call(Caller::Ui, "pages.get", json!({ "id": id }))
        .unwrap();
    assert_eq!(got["meta"]["title"], "Renamed");
    assert_eq!(got["meta"]["tags"], json!(["from-b"]));
    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn opening_a_fresh_index_from_two_threads_succeeds() {
    for round in 0..5 {
        let vault = temp_vault(&format!("migrate{round}"));
        std::fs::create_dir_all(&vault).unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let (vault, barrier) = (vault.clone(), barrier.clone());
                thread::spawn(move || {
                    barrier.wait();
                    let mut host = Host::new();
                    host.open_vault(vault.to_str().unwrap()).map(|_| ())
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap().unwrap();
        }
        let _ = std::fs::remove_dir_all(&vault);
    }
}
