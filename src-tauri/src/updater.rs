// In-app updates: signed GitHub release artifacts, verified by tauri-plugin-updater.
//
// The minisign public key is baked in at compile time from
// HERBARIUM_UPDATER_PUBLIC_KEY. Builds without it (local dev, forks) keep every
// other feature but refuse to update, with an explicit error and before any
// network request. There is no unsigned or default-key path.

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Runtime, Url};
use tauri_plugin_updater::{Config, UpdaterExt};

const ENDPOINT: &str =
    "https://github.com/abdoufermat5/herbarium/releases/latest/download/latest.json";
const PUBLIC_KEY: Option<&str> = option_env!("HERBARIUM_UPDATER_PUBLIC_KEY");

const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
// Covers the whole download as well as the re-check, so it is generous.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(15 * 60);

const MISSING_KEY: &str = "updates are unavailable: this build has no updater public key \
(HERBARIUM_UPDATER_PUBLIC_KEY was not set at compile time). Install a release build from GitHub.";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
}

fn configured_key(raw: Option<&'static str>) -> Option<&'static str> {
    raw.map(str::trim).filter(|k| !k.is_empty())
}

fn require_key(raw: Option<&'static str>) -> Result<&'static str, String> {
    configured_key(raw).ok_or_else(|| MISSING_KEY.to_string())
}

fn same_version(a: &str, b: &str) -> bool {
    a.trim_start_matches('v') == b.trim_start_matches('v')
}

pub fn plugin(
    context: &mut tauri::Context<tauri::Wry>,
) -> Option<tauri::plugin::TauriPlugin<tauri::Wry, Config>> {
    let key = configured_key(PUBLIC_KEY)?;
    // The plugin deserializes its config before applying Builder overrides.
    context.config_mut().plugins.0.insert(
        "updater".into(),
        serde_json::json!({ "pubkey": key, "endpoints": [ENDPOINT] }),
    );
    Some(tauri_plugin_updater::Builder::new().pubkey(key).build())
}

fn updater<R: Runtime>(
    app: &AppHandle<R>,
    timeout: Duration,
) -> Result<tauri_plugin_updater::Updater, String> {
    require_key(PUBLIC_KEY)?;
    let endpoint = Url::parse(ENDPOINT).map_err(|e| e.to_string())?;
    app.updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|e| e.to_string())?
        .timeout(timeout)
        .build()
        .map_err(|e| e.to_string())
}

/// Look for a newer signed release. `None` when this build is current.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let updater = updater(&app, CHECK_TIMEOUT)?;
    let update = updater.check().await.map_err(|e| e.to_string())?;
    Ok(update.map(|u| UpdateInfo {
        version: u.version,
        current_version: u.current_version,
        notes: u.body,
    }))
}

/// Install `version`, the one the user was shown. The latest release is
/// fetched again and the install is refused if it moved on in the meantime.
/// The updater verifies the artifact's signature against the embedded key
/// before installing; the app restarts afterwards.
#[tauri::command]
pub async fn install_update(app: AppHandle, version: String) -> Result<(), String> {
    let updater = updater(&app, INSTALL_TIMEOUT)?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("no update is available")?;
    if !same_version(&update.version, &version) {
        return Err(format!(
            "the latest release changed from {version} to {}; check for updates again",
            update.version
        ));
    }
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_blank_key_is_an_explicit_error() {
        assert!(
            require_key(None)
                .unwrap_err()
                .contains("HERBARIUM_UPDATER_PUBLIC_KEY")
        );
        assert!(require_key(Some("  \n")).is_err());
        assert_eq!(require_key(Some(" abc\n")).unwrap(), "abc");
    }

    #[test]
    fn version_match_ignores_leading_v() {
        assert!(same_version("v1.2.3", "1.2.3"));
        assert!(!same_version("1.2.4", "1.2.3"));
    }
}
