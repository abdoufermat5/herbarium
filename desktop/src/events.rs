// Pushes vault changes made by this process (every UI operation) to the
// window, so the frontend patches its lists instead of reloading them. An
// extension subscribes to core's event bus and re-emits each event as the
// Tauri event `vault-event`: `{ kind, id?, meta?, report?, htmlChanged? }`.

use herbarium_core::{Event, Extension, Host, OpResult, Registry, events};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::AppState;

/// Tauri event the frontend listens to.
pub const VAULT_EVENT: &str = "vault-event";

const FORWARDED: [&str; 6] = [
    events::PAGE_CREATED,
    events::PAGE_UPDATED,
    events::PAGE_DELETED,
    events::VAULT_INDEXED,
    events::PROPOSAL_CREATED,
    events::PROPOSAL_RESOLVED,
];

struct Bridge {
    app: AppHandle,
}

impl Extension for Bridge {
    fn id(&self) -> &str {
        "vault-events"
    }

    fn register(&self, registry: &mut Registry) -> OpResult<()> {
        for name in FORWARDED {
            let app = self.app.clone();
            registry.on(name, move |_store, event| {
                let _ = app.emit_to("main", VAULT_EVENT, payload(event));
            });
        }
        Ok(())
    }
}

/// The window's view of a core event: its kind, the page (or proposal) it is
/// about, the page's metadata after the change (page events) and the
/// re-index outcome (`vault.indexed`).
fn payload(event: &Event) -> Value {
    let page = event.payload.get("page");
    let id = page
        .and_then(|p| p.get("id"))
        .or_else(|| event.payload.get("id"))
        .or_else(|| event.payload.get("proposal").and_then(|p| p.get("id")));
    let report = (event.name == events::VAULT_INDEXED).then_some(&event.payload);
    let html_changed = event.payload.get("htmlChanged");
    json!({ "kind": event.name, "id": id, "meta": page, "report": report, "htmlChanged": html_changed })
}

/// Rebuild the app's host with the bridge registered. Runs in `setup`, before
/// the frontend opens a vault, so nothing is lost by replacing the host.
pub fn install(app: &tauri::App) -> OpResult<()> {
    let host = Host::with_extensions(vec![Box::new(Bridge {
        app: app.handle().clone(),
    })])?;
    let state = app.state::<AppState>();
    *state.host.lock().map_err(|e| e.to_string())? = host;
    Ok(())
}
