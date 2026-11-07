// Diagram server settings: Kroki URL for rendering server-side diagrams
// (PlantUML, Graphviz, D2, etc.) and the vault-wide setting.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::object;
use crate::extension::{Ctx, Extension, OpResult, Operation, Registry};
use crate::vault;

pub(crate) struct Render;

/// Vault diagram server settings, stored in `.herbarium/render.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderSettings {
    #[serde(default)]
    pub kroki_url: Option<String>,
}

fn settings_path(vault: &Path) -> PathBuf {
    vault.join(".herbarium").join("render.json")
}

/// Missing file means defaults; a corrupt one falls back to defaults too, but
/// logs a warning.
fn load_settings(vault: &Path) -> RenderSettings {
    let path = settings_path(vault);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return RenderSettings::default();
    };
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        tracing::warn!("ignoring invalid {}: {e}", path.display());
        RenderSettings::default()
    })
}

fn save_settings(vault: &Path, settings: &RenderSettings) -> OpResult<()> {
    let path = settings_path(vault);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create settings folder: {e}"))?;
    }
    let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    vault::write_atomic(&path, &bytes)
}

/// The diagram server address for this vault, if configured.
pub fn kroki_url(vault: &Path) -> Option<String> {
    load_settings(vault).kroki_url
}

#[derive(Deserialize)]
struct NoArgs {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigureArgs {
    #[serde(default)]
    kroki_url: Option<String>,
}

fn validate_kroki_url(raw: Option<String>) -> Result<Option<String>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let (scheme, rest) = if let Some(after) = trimmed.strip_prefix("https://") {
        ("https://", after)
    } else if let Some(after) = trimmed.strip_prefix("http://") {
        ("http://", after)
    } else {
        return Err("diagram server URL must start with http:// or https://".into());
    };
    let rest = rest.trim_end_matches('/');
    // Extract the host part (before port or path/query/fragment)
    let host_and_rest = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host_and_rest.split(':').next().unwrap_or("");
    if host.is_empty() {
        return Err("diagram server URL must have a host".into());
    }
    Ok(Some(format!("{scheme}{rest}")))
}

impl Extension for Render {
    fn id(&self) -> &str {
        "core.render"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.add(Operation::new(
            "render.settings",
            "Vault diagram server settings: the Kroki address used to render diagrams.",
            object(json!({}), &[]),
            |ctx: &mut Ctx, _: NoArgs| Ok(load_settings(&ctx.store.vault)),
        ))?;

        r.add(
            Operation::new(
                "render.configure",
                "Set or clear the diagram server URL.",
                object(
                    json!({
                        "krokiUrl": {
                            "type": ["string", "null"],
                            "description": "Diagram server URL (http:// or https://), or null to clear."
                        }
                    }),
                    &[],
                ),
                |ctx: &mut Ctx, a: ConfigureArgs| {
                    let kroki_url = validate_kroki_url(a.kroki_url)?;
                    let settings = RenderSettings { kroki_url };
                    save_settings(&ctx.store.vault, &settings)?;
                    Ok(settings)
                },
            )
            .ui_only(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Host;
    use crate::extension::Caller;
    use crate::time::now_ms;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "herbarium-render-{tag}-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn default_is_none_and_corrupt_file_falls_back() {
        let dir = temp_dir("default");
        assert_eq!(kroki_url(&dir), None);

        save_settings(
            &dir,
            &RenderSettings {
                kroki_url: Some("https://kroki.example.com".into()),
            },
        )
        .unwrap();
        assert_eq!(kroki_url(&dir), Some("https://kroki.example.com".into()));

        std::fs::write(settings_path(&dir), "{corrupt json").unwrap();
        assert_eq!(kroki_url(&dir), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validation_rules() {
        assert_eq!(validate_kroki_url(None).unwrap(), None);
        assert_eq!(validate_kroki_url(Some("".into())).unwrap(), None);
        assert_eq!(validate_kroki_url(Some("   ".into())).unwrap(), None);

        assert_eq!(
            validate_kroki_url(Some("  https://kroki.example.com/  ".into())).unwrap(),
            Some("https://kroki.example.com".into())
        );
        assert_eq!(
            validate_kroki_url(Some("http://127.0.0.1:8000/kroki///".into())).unwrap(),
            Some("http://127.0.0.1:8000/kroki".into())
        );

        assert!(validate_kroki_url(Some("ftp://kroki.example.com".into())).is_err());
        assert!(validate_kroki_url(Some("https://".into())).is_err());
        assert!(validate_kroki_url(Some("http:///".into())).is_err());
        assert!(validate_kroki_url(Some("not-a-url".into())).is_err());
    }

    #[test]
    fn configure_is_ui_only_and_settings_accessible_to_agents() {
        let dir = temp_dir("ui-only");
        let mut host = Host::new();
        host.open_vault(dir.to_str().unwrap()).unwrap();

        // Agent cannot configure
        let err = host
            .call(
                Caller::Agent,
                "render.configure",
                json!({ "krokiUrl": "https://kroki.io" }),
            )
            .unwrap_err();
        assert!(err.contains("unknown operation"));
        assert!(
            host.operations(Caller::Agent)
                .all(|op| op.name != "render.configure")
        );

        // Agent can read settings
        let settings = host
            .call(Caller::Agent, "render.settings", json!({}))
            .unwrap();
        assert_eq!(settings["krokiUrl"], serde_json::Value::Null);

        // UI can configure
        let configured = host
            .call(
                Caller::Ui,
                "render.configure",
                json!({ "krokiUrl": "https://kroki.io/" }),
            )
            .unwrap();
        assert_eq!(configured["krokiUrl"], "https://kroki.io");

        assert_eq!(kroki_url(&dir), Some("https://kroki.io".into()));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
