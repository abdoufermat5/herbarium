// IPC commands exposed to the frontend. Vault selection is app-level; every
// other feature goes through `invoke_op`, which runs a registered operation.

use std::path::PathBuf;
use std::sync::Mutex;

use herbarium_core::{Caller, Host};
use serde_json::{json, Value};
use tauri::State;

use crate::config::{self, Config};
use crate::editors::{self, Choice, EditorInfo};

pub struct AppState {
    pub host: Mutex<Host>,
}

type CmdResult<T> = Result<T, String>;

/// Open a vault (create dir if needed), index it, persist it in the config.
fn open_vault(state: &State<'_, AppState>, path: &str) -> CmdResult<Config> {
    let mut host = state.host.lock().map_err(|e| e.to_string())?;
    let report = host.open_vault(path)?;
    let canonical = host.vault_path().map(|p| p.to_string_lossy().into_owned());
    eprintln!(
        "herbarium: vault opened at {} (indexed {}, removed {}, total {})",
        canonical.as_deref().unwrap_or_default(),
        report.indexed,
        report.removed,
        report.total
    );
    let cfg = Config { vault_path: canonical };
    config::save(&cfg)?;
    Ok(cfg)
}

#[tauri::command]
pub async fn get_config() -> CmdResult<Config> {
    config::load()
}

#[tauri::command]
pub async fn set_vault(state: State<'_, AppState>, path: String) -> CmdResult<Config> {
    open_vault(&state, &path)
}

#[tauri::command]
pub async fn create_vault(state: State<'_, AppState>, parent_dir: String, name: String) -> CmdResult<Config> {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
        return Err("invalid folder name".into());
    }
    let target = PathBuf::from(&parent_dir).join(name);
    open_vault(&state, &target.to_string_lossy())
}

#[tauri::command]
pub async fn invoke_op(state: State<'_, AppState>, name: String, args: Value) -> CmdResult<Value> {
    let host = state.host.lock().map_err(|e| e.to_string())?;
    host.call(Caller::Ui, &name, args)
}

/// Text editors installed on this system, best candidates first.
#[tauri::command]
pub async fn list_editors() -> Vec<EditorInfo> {
    editors::list()
}

/// Open a page's HTML file in an external editor and return the editor's name.
/// `custom` (a command template) wins over `editor` (a detected id); with
/// neither, the first detected editor is used.
#[tauri::command]
pub async fn open_in_editor(
    state: State<'_, AppState>,
    page_id: String,
    editor: Option<String>,
    custom: Option<String>,
) -> CmdResult<String> {
    let path = {
        let host = state.host.lock().map_err(|e| e.to_string())?;
        let vault = host.vault_path().ok_or("no vault open")?.to_path_buf();
        let page = host.call(Caller::Ui, "pages.get", json!({ "id": page_id }))?;
        let folder = page["meta"]["folder"].as_str().map(str::to_owned);
        herbarium_core::vault::html_path(&vault, &page_id, folder.as_deref())
    };
    if !path.is_file() {
        return Err(format!("page file not found: {}", path.display()));
    }
    editors::open(&Choice { editor, custom }, &path)
}

/// Open a web link in the system browser. The UI calls this only after the
/// user confirms a link a page tried to follow (see `navigation`).
#[tauri::command]
pub async fn open_external(app: tauri::AppHandle, url: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let parsed = tauri::Url::parse(&url).map_err(|e| format!("invalid URL: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(format!("only web links can be opened: {url}"));
    }
    app.opener().open_url(parsed.as_str(), None::<&str>).map_err(|e| e.to_string())
}
