// Where pages were published: a secret gist, or a page of the user's GitHub
// Pages site. Kept in `.herbarium/published.json` (page id → target →
// record) so publishing again updates the same gist or file, and so the app
// can show and open the public copy. The publishing itself happens in the
// app, which holds the GitHub token; core only keeps the records. A deleted
// page keeps its records: its public copy is still online until unpublished.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{id_prop, object};
use crate::extension::{Ctx, Extension, OpResult, Operation, Registry};
use crate::vault;

pub(crate) struct Published;

const TARGETS: [&str; 2] = ["gist", "site"];

type Records = BTreeMap<String, Map<String, Value>>;

fn path(vault: &Path) -> PathBuf {
    vault.join(".herbarium").join("published.json")
}

fn load(vault: &Path) -> Records {
    std::fs::read_to_string(path(vault))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save(vault: &Path, records: &Records) -> OpResult<()> {
    std::fs::create_dir_all(vault.join(".herbarium")).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(records).map_err(|e| e.to_string())?;
    vault::write_atomic(&path(vault), &bytes)
}

fn check_target(target: &str) -> OpResult<()> {
    if TARGETS.contains(&target) {
        Ok(())
    } else {
        Err(format!(
            "unknown publish target `{target}`; use gist or site"
        ))
    }
}

#[derive(Deserialize)]
struct NoArgs {}

#[derive(Deserialize)]
struct RecordArgs {
    id: String,
    target: String,
    info: Map<String, Value>,
}

#[derive(Deserialize)]
struct ForgetArgs {
    id: String,
    target: String,
}

impl Extension for Published {
    fn id(&self) -> &str {
        "core.published"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        let target_prop = json!({ "type": "string", "enum": TARGETS });

        r.add(Operation::new(
            "published.list",
            "Pages the user published online, by page id: `gist` (a secret GitHub gist) and/or `site` (a page of their GitHub Pages site), each with its public `url` and when it was last published (`at`, unix ms).",
            object(json!({}), &[]),
            |ctx: &mut Ctx, _: NoArgs| Ok(load(&ctx.store.vault)),
        ))?;

        r.add(
            Operation::new(
                "published.record",
                "Remember where a page was published (`info` needs a `url`).",
                object(
                    json!({ "id": id_prop(), "target": target_prop.clone(), "info": { "type": "object" } }),
                    &["id", "target", "info"],
                ),
                |ctx: &mut Ctx, a: RecordArgs| {
                    check_target(&a.target)?;
                    let url = a.info.get("url").and_then(Value::as_str).unwrap_or_default();
                    if !url.starts_with("https://") {
                        return Err("a published page needs an https `url`".into());
                    }
                    let vault = ctx.store.vault.clone();
                    let mut records = load(&vault);
                    records
                        .entry(vault::checked_page_id(&a.id)?.to_string())
                        .or_default()
                        .insert(a.target, Value::Object(a.info));
                    save(&vault, &records)?;
                    Ok(records.get(&a.id).cloned().unwrap_or_default())
                },
            )
            .ui_only(),
        )?;

        r.add(
            Operation::new(
                "published.forget",
                "Forget where a page was published (after unpublishing it).",
                object(
                    json!({ "id": id_prop(), "target": target_prop }),
                    &["id", "target"],
                ),
                |ctx: &mut Ctx, a: ForgetArgs| {
                    check_target(&a.target)?;
                    let vault = ctx.store.vault.clone();
                    let mut records = load(&vault);
                    if let Some(entry) = records.get_mut(&a.id) {
                        entry.remove(&a.target);
                        if entry.is_empty() {
                            records.remove(&a.id);
                        }
                    }
                    save(&vault, &records)?;
                    Ok(json!({ "forgotten": a.id }))
                },
            )
            .ui_only(),
        )
    }
}
