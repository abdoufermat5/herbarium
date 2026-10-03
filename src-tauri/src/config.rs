// App settings shared by the desktop window and `herbarium mcp`: currently
// just the last opened vault, in `<config dir>/io.herbarium.desktop/config.json`
// (the same directory Tauri reports as `app_config_dir`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Must match `identifier` in `tauri.conf.json`.
const IDENTIFIER: &str = "io.herbarium.desktop";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub vault_path: Option<String>,
}

fn path() -> Result<PathBuf, String> {
    let dir = dirs::config_dir()
        .ok_or("no configuration directory on this system")?
        .join(IDENTIFIER);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("config.json"))
}

pub fn load() -> Result<Config, String> {
    let raw = std::fs::read_to_string(path()?).ok();
    Ok(raw.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default())
}

pub fn save(cfg: &Config) -> Result<(), String> {
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(path()?, json).map_err(|e| e.to_string())
}
