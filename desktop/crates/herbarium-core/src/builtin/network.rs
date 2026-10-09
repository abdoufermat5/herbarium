// Network access: the per-page switch (CDN allowlist or no network at all) and
// the vault-wide default applied to new pages.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{id_prop, object};
use crate::extension::{Caller, Ctx, Extension, OpResult, Operation, Registry};
use crate::time::now_ms;
use crate::vault;

pub(crate) struct Network;

/// Vault-wide network settings, stored in `.herbarium/network.json`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NetworkSettings {
    #[serde(default)]
    default_allow_cdn: bool,
}

fn settings_path(vault: &Path) -> PathBuf {
    vault.join(".herbarium").join("network.json")
}

/// Missing file means defaults; a corrupt one falls back to defaults too, but
/// says so on stderr.
fn load_settings(vault: &Path) -> NetworkSettings {
    let path = settings_path(vault);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return NetworkSettings::default();
    };
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        eprintln!("herbarium: ignoring invalid {}: {e}", path.display());
        NetworkSettings::default()
    })
}

fn save_settings(vault: &Path, settings: &NetworkSettings) -> OpResult<()> {
    let path = settings_path(vault);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create settings folder: {e}"))?;
    }
    let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    vault::write_atomic(&path, &bytes)
}

/// Whether new pages in this vault may load from the CDN allowlist.
pub(crate) fn default_allow_cdn(vault: &Path) -> bool {
    load_settings(vault).default_allow_cdn
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetArgs {
    id: String,
    allow_cdn: bool,
}

#[derive(Deserialize)]
struct NoArgs {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigureArgs {
    default_allow_cdn: bool,
}

impl Extension for Network {
    fn id(&self) -> &str {
        "core.network"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.add(Operation::new(
            "network.set",
            "Allow a page to load scripts, styles and fonts from known CDNs (cdnjs, jsDelivr, unpkg, jQuery, Google Fonts), or block all network access for it. Agents may only turn network access off.",
            object(
                json!({ "id": id_prop(), "allowCdn": { "type": "boolean" } }),
                &["id", "allowCdn"],
            ),
            |ctx: &mut Ctx, a: SetArgs| {
                if ctx.caller == Caller::Agent && a.allow_cdn {
                    return Err(
                        "agents may only turn network access off; ask the user to enable it in Herbarium".into(),
                    );
                }
                let mut meta = ctx.page(&a.id)?;
                meta.allow_cdn = a.allow_cdn;
                meta.updated_at = now_ms();
                ctx.save(&meta)?;
                Ok(meta)
            },
        ))?;

        r.add(Operation::new(
            "network.settings",
            "Vault-wide network settings: whether new pages may load from the CDN allowlist by default.",
            object(json!({}), &[]),
            |ctx: &mut Ctx, _: NoArgs| Ok(load_settings(&ctx.store.vault)),
        ))?;

        r.add(
            Operation::new(
                "network.configure",
                "Set whether new pages may load from the CDN allowlist by default.",
                object(
                    json!({ "defaultAllowCdn": { "type": "boolean" } }),
                    &["defaultAllowCdn"],
                ),
                |ctx: &mut Ctx, a: ConfigureArgs| {
                    let settings = NetworkSettings {
                        default_allow_cdn: a.default_allow_cdn,
                    };
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

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "herbarium-net-{tag}-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn default_is_off_and_corrupt_file_falls_back_to_off() {
        let dir = temp_dir("default");
        assert!(!default_allow_cdn(&dir));

        save_settings(
            &dir,
            &NetworkSettings {
                default_allow_cdn: true,
            },
        )
        .unwrap();
        assert!(default_allow_cdn(&dir));

        std::fs::write(settings_path(&dir), "{not json").unwrap();
        assert!(!default_allow_cdn(&dir));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
