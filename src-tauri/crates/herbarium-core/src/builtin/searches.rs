// Saved searches: named queries (filters included) kept in
// `.herbarium/searches.json`, so they travel with the vault and agents can
// use them too.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::object;
use crate::extension::{Ctx, Extension, OpResult, Operation, Registry};
use crate::query::Query;
use crate::vault;

pub(crate) struct Searches;

const MAX_SEARCHES: usize = 100;
const MAX_NAME: usize = 80;
const MAX_QUERY: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SavedSearch {
    name: String,
    query: String,
}

fn path(vault: &Path) -> PathBuf {
    vault.join(".herbarium").join("searches.json")
}

fn load(vault: &Path) -> Vec<SavedSearch> {
    let path = path(vault);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        eprintln!("herbarium: ignoring invalid {}: {e}", path.display());
        Vec::new()
    })
}

fn store(vault: &Path, searches: &[SavedSearch]) -> OpResult<()> {
    let path = path(vault);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create settings folder: {e}"))?;
    }
    let bytes = serde_json::to_vec_pretty(searches).map_err(|e| e.to_string())?;
    vault::write_atomic(&path, &bytes)
}

#[derive(Deserialize)]
struct SaveArgs {
    name: String,
    query: String,
}

#[derive(Deserialize)]
struct NameArgs {
    name: String,
}

#[derive(Deserialize)]
struct NoArgs {}

impl Extension for Searches {
    fn id(&self) -> &str {
        "core.searches"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.add(Operation::new(
            "searches.list",
            "The user's saved searches, in their order: each is `{ name, query }`; run one with `pages_search`.",
            object(json!({}), &[]),
            |ctx: &mut Ctx, _: NoArgs| Ok(load(&ctx.store.vault)),
        ))?;

        r.add(Operation::new(
            "searches.save",
            "Save a named search (text and filters, as `pages_search` takes it). Saving an existing name replaces its query in place. Returns every saved search.",
            object(
                json!({
                    "name": { "type": "string", "description": "Shown in the sidebar." },
                    "query": { "type": "string", "description": "A `pages_search` query, e.g. `tag:rust is:due`." }
                }),
                &["name", "query"],
            ),
            |ctx: &mut Ctx, a: SaveArgs| {
                let name = a.name.trim().to_string();
                let query = a.query.trim().to_string();
                if name.is_empty() || name.chars().count() > MAX_NAME {
                    return Err(format!("a saved search needs a name of 1 to {MAX_NAME} characters"));
                }
                if query.is_empty() || query.chars().count() > MAX_QUERY {
                    return Err(format!("a saved search needs a query of 1 to {MAX_QUERY} characters"));
                }
                Query::parse(&query)?;
                let vault = &ctx.store.vault;
                let mut all = load(vault);
                match all.iter().position(|s| s.name.eq_ignore_ascii_case(&name)) {
                    Some(i) => all[i] = SavedSearch { name, query },
                    None if all.len() >= MAX_SEARCHES => {
                        return Err(format!("at most {MAX_SEARCHES} saved searches"));
                    }
                    None => all.push(SavedSearch { name, query }),
                }
                store(vault, &all)?;
                Ok(all)
            },
        ))?;

        r.add(Operation::new(
            "searches.delete",
            "Delete a saved search by name. Returns the remaining saved searches; a missing name changes nothing.",
            object(json!({ "name": { "type": "string" } }), &["name"]),
            |ctx: &mut Ctx, a: NameArgs| {
                let vault = &ctx.store.vault;
                let mut all = load(vault);
                let before = all.len();
                all.retain(|s| !s.name.eq_ignore_ascii_case(a.name.trim()));
                if all.len() != before {
                    store(vault, &all)?;
                }
                Ok(all)
            },
        ))
    }
}
