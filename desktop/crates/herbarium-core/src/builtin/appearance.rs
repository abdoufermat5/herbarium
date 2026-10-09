// How folders and tags look: an icon and a colour per folder, a colour per
// tag. Kept in `.herbarium/appearance.json`; a page's own icon lives with the
// page (`ext.look`, set through `pages.update`). Colours are names from a
// fixed palette that the app renders in both themes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::object;
use crate::extension::{Ctx, Extension, OpResult, Operation, Registry};
use crate::models::clean_icon;
use crate::vault;

pub(crate) struct Appearance;

/// The palette, in the order the app offers it.
pub const COLORS: [&str; 8] = [
    "sage", "sky", "plum", "rose", "amber", "clay", "teal", "slate",
];

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct FolderLook {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Looks {
    #[serde(default)]
    folders: BTreeMap<String, FolderLook>,
    #[serde(default)]
    tags: BTreeMap<String, String>,
}

fn path(vault: &Path) -> PathBuf {
    vault.join(".herbarium").join("appearance.json")
}

fn load(vault: &Path) -> Looks {
    std::fs::read_to_string(path(vault))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn store(vault: &Path, looks: &Looks) -> OpResult<()> {
    let path = path(vault);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create settings folder: {e}"))?;
    }
    vault::write_atomic(
        &path,
        &serde_json::to_vec_pretty(looks).map_err(|e| e.to_string())?,
    )
}

fn clean_color(color: Option<String>) -> OpResult<Option<String>> {
    match color
        .map(|c| c.trim().to_ascii_lowercase())
        .filter(|c| !c.is_empty())
    {
        None => Ok(None),
        Some(c) if COLORS.contains(&c.as_str()) => Ok(Some(c)),
        Some(c) => Err(format!(
            "unknown colour `{c}`; use one of {}",
            COLORS.join(", ")
        )),
    }
}

#[derive(Deserialize)]
struct FolderArgs {
    path: String,
    icon: Option<String>,
    color: Option<String>,
}

#[derive(Deserialize)]
struct TagArgs {
    tag: String,
    color: Option<String>,
}

#[derive(Deserialize)]
struct NoArgs {}

impl Extension for Appearance {
    fn id(&self) -> &str {
        "core.appearance"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.add(Operation::new(
            "appearance.get",
            "Icons and colours of folders and tags: `{ folders: { path: { icon?, color? } }, tags: { tag: color }, colors }`.",
            object(json!({}), &[]),
            |ctx: &mut Ctx, _: NoArgs| {
                let looks = load(&ctx.store.vault);
                Ok(json!({ "folders": looks.folders, "tags": looks.tags, "colors": COLORS }))
            },
        ))?;

        r.add(Operation::new(
            "appearance.set_folder",
            "Set a folder's icon (an emoji) and colour (one of the palette); omitted or empty values remove them.",
            object(
                json!({
                    "path": { "type": "string", "description": "Folder path such as `rust/cargo`." },
                    "icon": { "type": ["string", "null"] },
                    "color": { "type": ["string", "null"], "enum": [COLORS[0], COLORS[1], COLORS[2], COLORS[3], COLORS[4], COLORS[5], COLORS[6], COLORS[7], null] }
                }),
                &["path"],
            ),
            |ctx: &mut Ctx, a: FolderArgs| {
                let folder = vault::clean_folder(Some(&a.path))?.ok_or("a folder path is required")?;
                let look = FolderLook {
                    icon: a.icon.as_deref().map(clean_icon).transpose()?.flatten(),
                    color: clean_color(a.color)?,
                };
                let vault = &ctx.store.vault;
                let mut looks = load(vault);
                if look == FolderLook::default() {
                    looks.folders.remove(&folder);
                } else {
                    looks.folders.insert(folder, look);
                }
                store(vault, &looks)?;
                Ok(json!({ "folders": looks.folders, "tags": looks.tags }))
            },
        ))?;

        r.add(Operation::new(
            "appearance.set_tag",
            "Set a tag's colour (one of the palette); omitted or empty removes it.",
            object(
                json!({
                    "tag": { "type": "string" },
                    "color": { "type": ["string", "null"] }
                }),
                &["tag"],
            ),
            |ctx: &mut Ctx, a: TagArgs| {
                let tag = a.tag.trim().to_string();
                if tag.is_empty() {
                    return Err("a tag is required".into());
                }
                let vault = &ctx.store.vault;
                let mut looks = load(vault);
                match clean_color(a.color)? {
                    Some(c) => {
                        looks.tags.insert(tag, c);
                    }
                    None => {
                        looks.tags.remove(&tag);
                    }
                }
                store(vault, &looks)?;
                Ok(json!({ "folders": looks.folders, "tags": looks.tags }))
            },
        ))
    }
}
