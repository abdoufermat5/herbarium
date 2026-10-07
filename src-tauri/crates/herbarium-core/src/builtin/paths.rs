// Reading paths: named, ordered lists of pages read one after another, like a
// short course. Kept in `.herbarium/paths.json`. A path keeps the id of a page
// that is trashed (it comes back with the page) and reports it as missing.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::object;
use crate::extension::{Ctx, Extension, OpResult, Operation, Registry};
use crate::models::PageMeta;
use crate::vault;

pub(crate) struct Paths;

const MAX_PATHS: usize = 200;
const MAX_PAGES: usize = 500;
const MAX_NAME: usize = 100;
const MAX_DESCRIPTION: usize = 2000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadingPath {
    /// Stable slug, unique among paths.
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Page ids in reading order.
    pub pages: Vec<String>,
}

fn file(vault: &Path) -> PathBuf {
    vault.join(".herbarium").join("paths.json")
}

pub(crate) fn load(vault: &Path) -> Vec<ReadingPath> {
    let path = file(vault);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        eprintln!("herbarium: ignoring invalid {}: {e}", path.display());
        Vec::new()
    })
}

pub(crate) fn store(vault: &Path, paths: &[ReadingPath]) -> OpResult<()> {
    let path = file(vault);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create settings folder: {e}"))?;
    }
    let bytes = serde_json::to_vec_pretty(paths).map_err(|e| e.to_string())?;
    vault::write_atomic(&path, &bytes)
}

fn clean_name(name: &str) -> OpResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME {
        return Err(format!("a path needs a name of 1 to {MAX_NAME} characters"));
    }
    Ok(name.to_string())
}

fn clean_description(text: &str) -> OpResult<String> {
    let text = text.trim();
    if text.chars().count() > MAX_DESCRIPTION {
        return Err(format!(
            "a path description is at most {MAX_DESCRIPTION} characters"
        ));
    }
    Ok(text.to_string())
}

/// Page ids in order without duplicates; every one must be an existing page.
fn clean_pages(ctx: &Ctx, pages: Vec<String>) -> OpResult<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for id in pages {
        let id = id.trim().to_string();
        ctx.page(&id)?;
        if !out.contains(&id) {
            out.push(id);
        }
    }
    if out.len() > MAX_PAGES {
        return Err(format!("a path holds at most {MAX_PAGES} pages"));
    }
    Ok(out)
}

/// A readable, unique id from `name`: lowercase ASCII words joined by `-`.
pub(crate) fn new_id(name: &str, taken: &[ReadingPath]) -> String {
    let mut base = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            base.push(c.to_ascii_lowercase());
        } else if !base.ends_with('-') && !base.is_empty() {
            base.push('-');
        }
    }
    let base = base
        .trim_end_matches('-')
        .chars()
        .take(48)
        .collect::<String>();
    let base = if base.is_empty() {
        "path".to_string()
    } else {
        base
    };
    let mut id = base.clone();
    let mut n = 2;
    while taken.iter().any(|p| p.id == id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

/// The path with its pages resolved: each entry is the page metadata, or
/// `{ id, missing: true }` for a page that is not in the vault (e.g. trashed).
fn resolved(ctx: &Ctx, path: &ReadingPath) -> OpResult<Value> {
    let mut pages = Vec::with_capacity(path.pages.len());
    for id in &path.pages {
        let meta: Option<PageMeta> = ctx.store.get_meta(id).map_err(|e| e.to_string())?;
        pages.push(match meta {
            Some(meta) => serde_json::to_value(meta).map_err(|e| e.to_string())?,
            None => json!({ "id": id, "missing": true }),
        });
    }
    Ok(json!({
        "id": path.id,
        "name": path.name,
        "description": path.description,
        "pages": pages,
    }))
}

fn find(paths: &mut [ReadingPath], id: &str) -> OpResult<usize> {
    paths
        .iter()
        .position(|p| p.id == id)
        .ok_or_else(|| format!("reading path not found: {id}"))
}

#[derive(Deserialize)]
struct CreateArgs {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    pages: Vec<String>,
}

#[derive(Deserialize)]
struct UpdateArgs {
    id: String,
    name: Option<String>,
    description: Option<String>,
    pages: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct AddArgs {
    id: String,
    page: String,
    /// 0-based position; omitted appends.
    position: Option<usize>,
}

#[derive(Deserialize)]
struct RemoveArgs {
    id: String,
    page: String,
}

#[derive(Deserialize)]
struct IdArgs {
    id: String,
}

#[derive(Deserialize)]
struct NoArgs {}

fn path_id_prop() -> Value {
    json!({ "type": "string", "description": "Reading path id (from `paths_list`)." })
}

impl Extension for Paths {
    fn id(&self) -> &str {
        "core.paths"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.add(Operation::new(
            "paths.list",
            "Reading paths: named, ordered lists of pages read one after another, like a short course. Each entry is `{ id, name, description, pages }` with page ids in reading order.",
            object(json!({}), &[]),
            |ctx: &mut Ctx, _: NoArgs| Ok(load(&ctx.store.vault)),
        ))?;

        r.add(Operation::new(
            "paths.get",
            "One reading path with its pages resolved in order: each is the page metadata, or `{ id, missing: true }` for a page no longer in the vault.",
            object(json!({ "id": path_id_prop() }), &["id"]),
            |ctx: &mut Ctx, a: IdArgs| {
                let mut all = load(&ctx.store.vault);
                let i = find(&mut all, &a.id)?;
                resolved(ctx, &all[i])
            },
        ))?;

        r.add(Operation::new(
            "paths.create",
            "Create a reading path from existing pages, in reading order (e.g. a course from basics to advanced). Returns the new path with its `id`.",
            object(
                json!({
                    "name": { "type": "string" },
                    "description": { "type": "string", "description": "What the path teaches; shown to the user." },
                    "pages": { "type": "array", "items": { "type": "string" }, "description": "Page ids in reading order." }
                }),
                &["name"],
            ),
            |ctx: &mut Ctx, a: CreateArgs| {
                let vault = ctx.store.vault.clone();
                let mut all = load(&vault);
                if all.len() >= MAX_PATHS {
                    return Err(format!("at most {MAX_PATHS} reading paths"));
                }
                let name = clean_name(&a.name)?;
                let path = ReadingPath {
                    id: new_id(&name, &all),
                    name,
                    description: clean_description(&a.description)?,
                    pages: clean_pages(ctx, a.pages)?,
                };
                all.push(path.clone());
                store(&vault, &all)?;
                Ok(path)
            },
        ))?;

        r.add(Operation::new(
            "paths.update",
            "Rename a reading path, change its description, or replace its pages (the full list, in the new order).",
            object(
                json!({
                    "id": path_id_prop(),
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "pages": { "type": "array", "items": { "type": "string" }, "description": "Every page id, in reading order." }
                }),
                &["id"],
            ),
            |ctx: &mut Ctx, a: UpdateArgs| {
                let vault = ctx.store.vault.clone();
                let mut all = load(&vault);
                let i = find(&mut all, &a.id)?;
                if let Some(name) = a.name {
                    all[i].name = clean_name(&name)?;
                }
                if let Some(description) = a.description {
                    all[i].description = clean_description(&description)?;
                }
                if let Some(pages) = a.pages {
                    // A trashed page the path already holds may be passed back
                    // and keeps its place; any other id must be a page.
                    let mut order: Vec<String> = Vec::new();
                    for id in pages {
                        let id = id.trim().to_string();
                        let exists = ctx.store.get_meta(&id).map_err(|e| e.to_string())?.is_some();
                        if !exists && !all[i].pages.contains(&id) {
                            return Err(format!("page not found: {id}"));
                        }
                        if !order.contains(&id) {
                            order.push(id);
                        }
                    }
                    if order.len() > MAX_PAGES {
                        return Err(format!("a path holds at most {MAX_PAGES} pages"));
                    }
                    all[i].pages = order;
                }
                let path = all[i].clone();
                store(&vault, &all)?;
                Ok(path)
            },
        ))?;

        r.add(Operation::new(
            "paths.add_page",
            "Add a page to a reading path at `position` (0-based; omitted appends). A page already in the path moves there.",
            object(
                json!({
                    "id": path_id_prop(),
                    "page": { "type": "string", "description": "Page id." },
                    "position": { "type": "integer", "minimum": 0 }
                }),
                &["id", "page"],
            ),
            |ctx: &mut Ctx, a: AddArgs| {
                ctx.page(&a.page)?;
                let vault = ctx.store.vault.clone();
                let mut all = load(&vault);
                let i = find(&mut all, &a.id)?;
                let pages = &mut all[i].pages;
                pages.retain(|p| p != &a.page);
                if pages.len() >= MAX_PAGES {
                    return Err(format!("a path holds at most {MAX_PAGES} pages"));
                }
                let at = a.position.unwrap_or(pages.len()).min(pages.len());
                pages.insert(at, a.page);
                let path = all[i].clone();
                store(&vault, &all)?;
                Ok(path)
            },
        ))?;

        r.add(Operation::new(
            "paths.remove_page",
            "Remove a page from a reading path (the page itself is untouched). Removing a page that is not in the path changes nothing.",
            object(
                json!({ "id": path_id_prop(), "page": { "type": "string", "description": "Page id." } }),
                &["id", "page"],
            ),
            |ctx: &mut Ctx, a: RemoveArgs| {
                let vault = ctx.store.vault.clone();
                let mut all = load(&vault);
                let i = find(&mut all, &a.id)?;
                let before = all[i].pages.len();
                all[i].pages.retain(|p| p != &a.page);
                if all[i].pages.len() != before {
                    store(&vault, &all)?;
                }
                Ok(all[i].clone())
            },
        ))?;

        r.add(Operation::new(
            "paths.delete",
            "Delete a reading path. Its pages stay in the vault.",
            object(json!({ "id": path_id_prop() }), &["id"]),
            |ctx: &mut Ctx, a: IdArgs| {
                let vault = ctx.store.vault.clone();
                let mut all = load(&vault);
                let i = find(&mut all, &a.id)?;
                all.remove(i);
                store(&vault, &all)?;
                Ok(json!({ "deleted": a.id }))
            },
        ))
    }
}
