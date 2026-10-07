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
    /// Keys for the other AI services, by provider id (see ai.rs).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub ai_keys: std::collections::BTreeMap<String, String>,
}

impl Secrets {
    /// The stored key for an AI provider.
    pub fn ai_key(&self, provider: &str) -> Option<String> {
        if provider == "anthropic" {
            self.anthropic_key.clone()
        } else {
            self.ai_keys.get(provider).cloned()
        }
    }

    fn set_ai_key(&mut self, provider: &str, key: Option<String>) {
        if provider == "anthropic" {
            self.anthropic_key = key;
        } else if let Some(key) = key {
            self.ai_keys.insert(provider.to_string(), key);
        } else {
            self.ai_keys.remove(provider);
        }
    }
}

/// Store or clear (blank) the key for an AI provider.
pub fn set_ai_key(provider: &str, key: Option<String>) -> Result<(), String> {
    set(|s, v| s.set_ai_key(provider, v), key)
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
