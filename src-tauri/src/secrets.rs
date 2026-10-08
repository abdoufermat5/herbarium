// Credentials the app keeps for the user: the AI service keys used to
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
    try_load().unwrap_or_default()
}

/// The stored secrets; an error when the file exists but cannot be read or
/// parsed, so a write never replaces keys it could not see.
fn try_load() -> Result<Secrets, String> {
    let path = path()?;
    match std::fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw)
            .map_err(|e| format!("{} is damaged ({e}); fix or delete it", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Secrets::default()),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

pub fn save(secrets: &Secrets) -> Result<(), String> {
    let path = path()?;
    let json = serde_json::to_vec_pretty(secrets).map_err(|e| e.to_string())?;
    write_private(&path, &json)
}

/// Write `bytes` to `path` atomically, readable only by the user from the
/// moment the file exists (a world-readable temporary file would leak keys).
fn write_private(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let dir = path.parent().ok_or("no configuration folder")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let temp = dir.join(format!(".secrets.json.tmp-{}-{nanos}", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> std::io::Result<()> {
        let mut file = options.open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("cannot save {}: {e}", path.display()));
    }
    Ok(())
}

/// Serialises changes, so two keys saved at once are both kept.
static UPDATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Store or clear (`None` or blank) one secret.
pub fn set(
    update: impl FnOnce(&mut Secrets, Option<String>),
    value: Option<String>,
) -> Result<(), String> {
    let _guard = UPDATE.lock().unwrap_or_else(|e| e.into_inner());
    let mut secrets = try_load()?;
    update(
        &mut secrets,
        value
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
    );
    save(&secrets)
}
