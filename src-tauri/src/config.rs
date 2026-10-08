// App settings shared by the desktop window and `herbarium mcp`.
// Stored in `<config dir>/io.herbarium.desktop/config.json`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Must match `identifier` in `tauri.conf.json`.
const IDENTIFIER: &str = "io.herbarium.desktop";

fn default_close_to_tray() -> bool {
    true
}

/// Saves the clipboard's HTML as a page from anywhere.
pub const DEFAULT_CAPTURE_SHORTCUT: &str = "CommandOrControl+Alt+H";

fn default_capture_shortcut() -> Option<String> {
    Some(DEFAULT_CAPTURE_SHORTCUT.into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub vault_path: Option<String>,
    #[serde(default)]
    pub recent_vaults: Vec<String>,
    #[serde(default = "default_close_to_tray")]
    pub close_to_tray: bool,
    /// Global shortcut that saves the clipboard as a page; None turns it off.
    #[serde(default = "default_capture_shortcut")]
    pub capture_shortcut: Option<String>,
    /// Offer to save HTML files that appear in the Downloads folder.
    #[serde(default = "default_close_to_tray")]
    pub watch_downloads: bool,
    /// Offer to save when a whole HTML page is copied.
    #[serde(default)]
    pub watch_clipboard: bool,
    /// Who remixes pages: `anthropic` (the API, with a key) or `claude-code`
    /// (the `claude` command line).
    #[serde(default = "default_ai_provider")]
    pub ai_provider: String,
    #[serde(default = "default_ai_model")]
    pub ai_model: String,
    /// The API address for a custom OpenAI-compatible service (or to reach
    /// Ollama on another machine).
    #[serde(default)]
    pub ai_base_url: Option<String>,
    /// The GitHub account the stored token belongs to.
    #[serde(default)]
    pub github_login: Option<String>,
    /// Repository of the GitHub Pages site pages are published to.
    #[serde(default = "default_publish_repo")]
    pub publish_repo: String,
}

fn default_publish_repo() -> String {
    "herbarium-pages".into()
}

fn default_ai_provider() -> String {
    "anthropic".into()
}

/// Empty: the service's recommended model, looked up when needed, so the
/// newest model is used without anyone typing a name.
fn default_ai_model() -> String {
    String::new()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vault_path: None,
            recent_vaults: Vec::new(),
            close_to_tray: true,
            capture_shortcut: default_capture_shortcut(),
            watch_downloads: true,
            watch_clipboard: false,
            ai_provider: default_ai_provider(),
            ai_model: default_ai_model(),
            ai_base_url: None,
            github_login: None,
            publish_repo: default_publish_repo(),
        }
    }
}

/// The app's configuration folder, created if needed.
pub fn dir() -> Result<PathBuf, String> {
    let dir = dirs::config_dir()
        .ok_or("no configuration directory on this system")?
        .join(IDENTIFIER);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn path() -> Result<PathBuf, String> {
    Ok(dir()?.join("config.json"))
}

pub fn load() -> Result<Config, String> {
    let config_path = path()?;
    if !config_path.exists() {
        return Ok(Config::default());
    }
    let raw = match std::fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "herbarium: failed to read config file {}: {}",
                config_path.display(),
                e
            );
            return Ok(Config::default());
        }
    };
    match serde_json::from_str::<Config>(&raw) {
        Ok(cfg) => Ok(cfg),
        Err(e) => {
            eprintln!(
                "herbarium: corrupt config file {}: {}",
                config_path.display(),
                e
            );
            let bak_path = config_path.with_file_name("config.json.bak");
            if let Err(err) = std::fs::rename(&config_path, &bak_path) {
                eprintln!(
                    "herbarium: failed to rename corrupt config file to {}: {}",
                    bak_path.display(),
                    err
                );
            }
            Ok(Config::default())
        }
    }
}

/// Serialises changes to the config: commands run concurrently, and two that
/// each load, change and save it would otherwise lose one of the changes.
static UPDATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Load the config, apply `change` and save it, as one step. `change` must
/// not wait on anything slow (the network): do that before calling.
pub fn update<T>(
    change: impl FnOnce(&mut Config) -> Result<T, String>,
) -> Result<(Config, T), String> {
    let _guard = UPDATE.lock().unwrap_or_else(|e| e.into_inner());
    let mut cfg = load().unwrap_or_default();
    let out = change(&mut cfg)?;
    save(&cfg)?;
    Ok((cfg, out))
}

pub fn save(cfg: &Config) -> Result<(), String> {
    let config_path = path()?;
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    herbarium_core::vault::write_atomic(&config_path, json.as_bytes()).map_err(|e| e.to_string())
}

pub fn add_recent(cfg: &mut Config, path: &str) {
    cfg.recent_vaults.retain(|p| p != path);
    cfg.recent_vaults.insert(0, path.to_string());
    if cfg.recent_vaults.len() > 8 {
        cfg.recent_vaults.truncate(8);
    }
}

pub fn remove_recent(cfg: &mut Config, path: &str) {
    cfg.recent_vaults.retain(|p| p != path);
}
