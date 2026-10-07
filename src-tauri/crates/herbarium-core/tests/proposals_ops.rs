// Agent edits that wait for approval: proposals created by agent rewrites
// when the vault asks for review, then accepted or rejected from the UI.

use std::path::{Path, PathBuf};

use herbarium_core::time::now_ms;
use herbarium_core::{Caller, Host};
use serde_json::{Value, json};

fn doc(title: &str) -> String {
    format!(
        "<!doctype html><html><head><title>{title}</title></head><body><p>{title}</p></body></html>"
    )
}

fn temp_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "herbarium-proposals-{tag}-{}-{}",
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

fn html_of(host: &Host, id: &str) -> String {
    host.call(
        Caller::Ui,
        "pages.get",
        json!({ "id": id, "format": "html" }),
    )
    .unwrap()["html"]
        .as_str()
        .unwrap()
        .to_string()
}

fn review_edits(host: &Host, on: bool) {
    host.call(Caller::Ui, "agents.configure", json!({ "reviewEdits": on }))
        .unwrap();
}

/// A page created by the UI, and its id.
fn page(host: &Host, title: &str) -> (String, Value) {
    let meta = host
        .call(Caller::Ui, "pages.create", json!({ "html": doc(title) }))
        .unwrap();
    (meta["id"].as_str().unwrap().to_string(), meta)
}

#[test]
fn agent_edits_apply_directly_until_review_is_turned_on() {
    let vault = temp_vault("off");
    let host = open(&vault);
    let (id, _) = page(&host, "One");

    let settings = host
        .call(Caller::Agent, "agents.settings", json!({}))
        .unwrap();
    assert_eq!(settings["reviewEdits"], false);
    let out = host
        .call(
            Caller::Agent,
            "pages.set_html",
            json!({ "id": id, "html": doc("Two") }),
        )
        .unwrap();
    assert_eq!(out["title"], "Two", "without review the edit lands");
    assert!(html_of(&host, &id).contains("Two"));

    let err = host
        .call(
            Caller::Agent,
            "agents.configure",
            json!({ "reviewEdits": false }),
        )
        .unwrap_err();
    assert!(
        err.contains("unknown operation"),
        "agents cannot change the setting"
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn agent_edit_waits_and_is_accepted() {
    let vault = temp_vault("accept");
    let host = open(&vault);
    let (id, _) = page(&host, "Original");
    review_edits(&host, true);

    let out = host
        .call(
            Caller::Agent,
            "pages.set_html",
            json!({ "id": id, "html": doc("Proposed") }),
        )
        .unwrap();
    assert_eq!(out["pendingApproval"], true);
    assert_eq!(out["proposal"]["title"], "Proposed");
    assert!(
        html_of(&host, &id).contains("Original"),
        "the page is unchanged"
    );

    // A UI edit is never held back.
    let list = host
        .call(Caller::Agent, "proposals.list", json!({}))
        .unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], id.as_str());
    assert_eq!(list[0]["pageTitle"], "Original");
    assert_eq!(list[0]["stale"], false);

    let got = host
        .call(Caller::Ui, "proposals.get", json!({ "id": id }))
        .unwrap();
    assert!(got["html"].as_str().unwrap().contains("Proposed"));

    let err = host
        .call(Caller::Agent, "proposals.accept", json!({ "id": id }))
        .unwrap_err();
    assert!(
        err.contains("unknown operation"),
        "agents cannot approve their own edits"
    );

    let meta = host
        .call(Caller::Ui, "proposals.accept", json!({ "id": id }))
        .unwrap();
    assert_eq!(meta["title"], "Proposed");
    assert!(html_of(&host, &id).contains("Proposed"));
    let history = host
        .call(Caller::Ui, "history.list", json!({ "id": id }))
        .unwrap();
    assert_eq!(
        history[0]["caller"], "agent",
        "the replaced HTML is kept as an agent change"
    );
    let list = host.call(Caller::Ui, "proposals.list", json!({})).unwrap();
    assert!(
        list.as_array().unwrap().is_empty(),
        "accepting resolves the proposal"
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn stale_proposal_conflicts_unless_forced_and_can_be_rejected() {
    let vault = temp_vault("stale");
    let host = open(&vault);
    let (id, _) = page(&host, "Base");
    review_edits(&host, true);

    host.call(
        Caller::Agent,
        "pages.set_html",
        json!({ "id": id, "html": doc("Agent v1") }),
    )
    .unwrap();
    // A newer proposal replaces the older one.
    host.call(
        Caller::Agent,
        "pages.set_html",
        json!({ "id": id, "html": doc("Agent v2") }),
    )
    .unwrap();
    let list = host.call(Caller::Ui, "proposals.list", json!({})).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["title"], "Agent v2");

    // The user edits the page meanwhile: the proposal is now stale.
    std::thread::sleep(std::time::Duration::from_millis(5));
    host.call(
        Caller::Ui,
        "pages.set_html",
        json!({ "id": id, "html": doc("User edit") }),
    )
    .unwrap();
    let list = host.call(Caller::Ui, "proposals.list", json!({})).unwrap();
    assert_eq!(list[0]["stale"], true);
    let err = host
        .call(Caller::Ui, "proposals.accept", json!({ "id": id }))
        .unwrap_err();
    assert!(err.starts_with("conflict"), "{err}");
    assert!(html_of(&host, &id).contains("User edit"));

    let forced = host
        .call(
            Caller::Ui,
            "proposals.accept",
            json!({ "id": id, "force": true }),
        )
        .unwrap();
    assert_eq!(forced["title"], "Agent v2");

    // Reject leaves the page alone.
    host.call(
        Caller::Agent,
        "pages.set_html",
        json!({ "id": id, "html": doc("Agent v3") }),
    )
    .unwrap();
    host.call(Caller::Ui, "proposals.reject", json!({ "id": id }))
        .unwrap();
    assert!(html_of(&host, &id).contains("Agent v2"));
    let err = host
        .call(Caller::Ui, "proposals.reject", json!({ "id": id }))
        .unwrap_err();
    assert!(err.contains("no proposal"));

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn agent_history_restore_also_waits_and_deleting_the_page_drops_the_proposal() {
    let vault = temp_vault("restore");
    let host = open(&vault);
    let (id, _) = page(&host, "First");
    host.call(
        Caller::Ui,
        "pages.set_html",
        json!({ "id": id, "html": doc("Second") }),
    )
    .unwrap();
    let at = host
        .call(Caller::Ui, "history.list", json!({ "id": id }))
        .unwrap()[0]["at"]
        .clone();
    review_edits(&host, true);

    let out = host
        .call(
            Caller::Agent,
            "history.restore",
            json!({ "id": id, "at": at }),
        )
        .unwrap();
    assert_eq!(out["pendingApproval"], true);
    assert!(html_of(&host, &id).contains("Second"));

    // A bad edit is still rejected straight away.
    let err = host
        .call(
            Caller::Agent,
            "pages.set_html",
            json!({ "id": id, "html": "plain text" }),
        )
        .unwrap_err();
    assert!(err.contains("HTML"), "{err}");
    let err = host
        .call(
            Caller::Agent,
            "pages.set_html",
            json!({ "id": id, "html": doc("x"), "baseUpdatedAt": 1 }),
        )
        .unwrap_err();
    assert!(err.starts_with("conflict"), "{err}");

    host.call(Caller::Ui, "pages.delete", json!({ "id": id }))
        .unwrap();
    assert!(
        !vault
            .join(".herbarium/proposals")
            .join(format!("{id}.json"))
            .exists(),
        "a trashed page's proposal is dropped"
    );
    let err = host
        .call(Caller::Agent, "proposals.get", json!({ "id": "../x" }))
        .unwrap_err();
    assert!(!err.is_empty());

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn the_ui_can_leave_a_proposal_for_a_remix() {
    let vault = temp_vault("create");
    let host = open(&vault);
    let (id, meta) = page(&host, "Plain");

    let err = host
        .call(
            Caller::Agent,
            "proposals.create",
            json!({ "id": id, "html": doc("Agent") }),
        )
        .unwrap_err();
    assert!(!err.is_empty());
    assert!(
        host.call(
            Caller::Ui,
            "proposals.create",
            json!({ "id": id, "html": "just words" }),
        )
        .is_err()
    );

    let proposal = host
        .call(
            Caller::Ui,
            "proposals.create",
            json!({ "id": id, "html": doc("Remixed"), "baseUpdatedAt": meta["updatedAt"] }),
        )
        .unwrap();
    assert_eq!(proposal["title"], "Remixed");
    assert_eq!(
        html_of(&host, &id),
        doc("Plain"),
        "the page waits for approval"
    );
    let list = host.call(Caller::Ui, "proposals.list", json!({})).unwrap();
    assert_eq!(list[0]["id"], id.as_str());
    assert_eq!(list[0]["stale"], false);

    host.call(Caller::Ui, "proposals.accept", json!({ "id": id }))
        .unwrap();
    assert_eq!(html_of(&host, &id), doc("Remixed"));
    let _ = std::fs::remove_dir_all(&vault);
}
