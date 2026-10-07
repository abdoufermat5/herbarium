// Credentials the app keeps for the user: the Anthropic API key used to
// remix pages and the GitHub token used to publish them. Stored apart from
// the settings, in `<config dir>/io.herbarium.desktop/secrets.json`, readable
// only by the user, and never sent to the window: the UI only learns whether
// one is set.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Secrets {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anthropic_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_token: Option<String>,
}

fn path() -> Result<PathBuf, String> {
    Ok(crate::config::dir()?.join("secrets.json"))
}

pub fn load() -> Secrets {
    path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(secrets: &Secrets) -> Result<(), String> {
    let path = path()?;
    let json = serde_json::to_vec_pretty(secrets).map_err(|e| e.to_string())?;
    herbarium_core::vault::write_atomic(&path, &json)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("cannot protect {}: {e}", path.display()))?;
    }
    Ok(())
}

/// Store or clear (`None` or blank) one secret.
pub fn set(
    update: impl FnOnce(&mut Secrets, Option<String>),
    value: Option<String>,
) -> Result<(), String> {
    let mut secrets = load();
    update(
        &mut secrets,
        value
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
    );
    save(&secrets)
}
