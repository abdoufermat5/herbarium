// IPC commands exposed to the frontend. Vault selection is app-level; every
// other feature goes through `invoke_op`, which runs a registered operation.

use std::path::PathBuf;
use std::sync::Mutex;

use herbarium_core::{Caller, Host};
use serde_json::Value;
use tauri::State;

use crate::config::{self, Config};

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
