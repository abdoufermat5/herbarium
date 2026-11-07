// Per-page persistent storage for interactive pages: values a page writes with
// `localStorage` or the Claude-artifact `window.storage` API are kept in
// `<vault>/.herbarium/storage/<id>.json` and never touch the HTML, the page
// sidecar or the index. A page therefore keeps its state across reader
// reopen, app restart, folder moves and the trash round trip (the file is
// keyed by id alone, so none of those move it), and the metadata every list
// carries stays small however much a page stores.
//
// Older vaults kept the state in the sidecar under `ext["storage"]`. It is
// moved to the file on the first storage operation for the page and, for every
// page, after each index (`vault.indexed`).
//
// Storage is not a page revision: writes leave `updatedAt` alone and emit no
// event, so an open reader never mistakes them for an external edit and never
// reloads the frame (which would wipe the page's in-memory state).

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{id_prop, object};
use crate::extension::{Ctx, Extension, OpResult, Operation, Registry, events};
use crate::models::PageMeta;
use crate::store::Store;
use crate::vault;

pub(crate) struct Storage;

/// Key the state used to live under in `PageMeta::ext` (migrated away).
pub(crate) const EXT_KEY: &str = "storage";
/// Longest accepted key, in Unicode scalar values (matching JS code points).
const MAX_KEY_CHARS: usize = 200;
/// Cap for the whole page state, in bytes of its JSON serialization.
const MAX_STATE_BYTES: usize = 1024 * 1024;
/// The three persisted namespaces.
const AREAS: [&str; 3] = ["local", "personal", "shared"];

/// The persisted state of one page: three namespaces of string keys to string
/// values. `serialize` order matters: the shim measures the same JSON shape.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PageStorage {
    pub local: BTreeMap<String, String>,
    pub personal: BTreeMap<String, String>,
    pub shared: BTreeMap<String, String>,
}

impl PageStorage {
    /// A legacy `ext["storage"]` value; a malformed entry reads as empty so a
    /// hand-edited sidecar never blocks a page.
    fn from_legacy(value: &Value) -> Self {
        Self::deserialize(value).unwrap_or_default()
    }

    fn is_empty(&self) -> bool {
        self.local.is_empty() && self.personal.is_empty() && self.shared.is_empty()
    }

    /// Bytes of the JSON the shim also measures against [`MAX_STATE_BYTES`].
    fn serialized_len(&self) -> usize {
        serde_json::to_string(self)
            .map(|s| s.len())
            .unwrap_or(usize::MAX)
    }

    fn map_mut(&mut self, area: &str) -> OpResult<&mut BTreeMap<String, String>> {
        match area {
            "local" => Ok(&mut self.local),
            "personal" => Ok(&mut self.personal),
            "shared" => Ok(&mut self.shared),
            other => Err(format!(
                "storage: unknown area {other:?} (expected local, personal or shared)"
            )),
        }
    }
}

#[derive(Deserialize)]
struct IdArgs {
    id: String,
}

#[derive(Deserialize)]
struct WriteArgs {
    id: String,
    changes: Vec<Change>,
}

/// One key operation. `value: null` deletes the key.
#[derive(Deserialize)]
struct Change {
    area: String,
    key: String,
    #[serde(default)]
    value: Option<String>,
}

/// Apply one change to `state`, validating the area and the key.
fn apply(state: &mut PageStorage, change: &Change) -> OpResult<()> {
    let key_chars = change.key.chars().count();
    if key_chars == 0 || key_chars > MAX_KEY_CHARS {
        return Err(format!(
            "storage: key must be 1 to {MAX_KEY_CHARS} characters"
        ));
    }
    let map = state.map_mut(&change.area)?;
    match &change.value {
        Some(value) => {
            map.insert(change.key.clone(), value.clone());
        }
        None => {
            map.remove(&change.key);
        }
    }
    Ok(())
}

/// `<vault>/.herbarium/storage/<id>.json`.
fn state_path(vault: &Path, id: &str) -> OpResult<PathBuf> {
    let id = vault::checked_page_id(id)?;
    Ok(vault
        .join(".herbarium")
        .join("storage")
        .join(format!("{id}.json")))
}

/// The page's saved state; a missing or unreadable file reads as empty.
fn read_state(vault: &Path, id: &str) -> PageStorage {
    state_path(vault, id)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Write the page's state, or remove its file when the state is empty.
fn write_state(vault: &Path, id: &str, state: &PageStorage) -> OpResult<()> {
    if state.is_empty() {
        return remove_state(vault, id);
    }
    let bytes = serde_json::to_vec(state).map_err(|e| e.to_string())?;
    vault::write_atomic(&state_path(vault, id)?, &bytes)
}

/// Delete the page's state file; a file already gone is fine.
pub(crate) fn remove_state(vault: &Path, id: &str) -> OpResult<()> {
    match fs::remove_file(state_path(vault, id)?) {
        Err(e) if e.kind() != ErrorKind::NotFound => {
            Err(format!("cannot delete page state of {id}: {e}"))
        }
        _ => Ok(()),
    }
}

/// Copy the state of page `from` to the new page `to` (duplicate); `legacy` is
/// any `ext["storage"]` the original's metadata still carries.
pub(crate) fn copy_state(
    vault: &Path,
    from: &str,
    legacy: Option<&Value>,
    to: &str,
) -> OpResult<()> {
    let mut state = read_state(vault, from);
    if let Some(legacy) = legacy {
        merge_legacy(&mut state, PageStorage::from_legacy(legacy));
    }
    write_state(vault, to, &state)
}

/// The page's state as JSON for the reader shim; `None` when it saved nothing.
pub fn page_storage_value(vault: &Path, id: &str) -> Option<Value> {
    let state = read_state(vault, id);
    (!state.is_empty())
        .then(|| serde_json::to_value(state).ok())
        .flatten()
}

/// Fold `legacy` into `state`; keys already in the file win.
fn merge_legacy(state: &mut PageStorage, legacy: PageStorage) {
    for (dst, src) in [
        (&mut state.local, legacy.local),
        (&mut state.personal, legacy.personal),
        (&mut state.shared, legacy.shared),
    ] {
        for (k, v) in src {
            dst.entry(k).or_insert(v);
        }
    }
}

/// Move a legacy `ext["storage"]` of `meta` into the page's file and strip it
/// from the sidecar and the index. Returns whether anything was migrated. The
/// file is written first, so a crash in between loses nothing.
fn migrate(store: &Store, meta: &mut PageMeta) -> OpResult<bool> {
    let Some(legacy) = meta.ext.get(EXT_KEY) else {
        return Ok(false);
    };
    let mut state = read_state(&store.vault, &meta.id);
    merge_legacy(&mut state, PageStorage::from_legacy(legacy));
    write_state(&store.vault, &meta.id, &state)?;
    meta.ext.remove(EXT_KEY);
    // Not a page revision: `updated_at` stays and no event is emitted.
    vault::write_meta(&store.vault, meta)?;
    let text = store
        .text_for(&meta.id)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let mtime = vault::page_mtime(&store.vault, meta);
    store
        .upsert(meta, &text, mtime)
        .map_err(|e| e.to_string())?;
    Ok(true)
}

/// The page's state, migrating a legacy `ext["storage"]` first.
fn load(ctx: &Ctx, id: &str) -> OpResult<PageStorage> {
    let mut meta = ctx.page(id)?;
    migrate(ctx.store, &mut meta)?;
    Ok(read_state(&ctx.store.vault, id))
}

fn write_changes(ctx: &mut Ctx, args: WriteArgs) -> OpResult<PageStorage> {
    let mut next = load(ctx, &args.id)?;
    // Validate and apply the whole batch to a copy first: a bad key or a size
    // overrun rejects every change, leaving the stored state untouched.
    for change in &args.changes {
        apply(&mut next, change)?;
    }
    if next.serialized_len() > MAX_STATE_BYTES {
        return Err(format!(
            "storage: this page's state would exceed {} bytes",
            MAX_STATE_BYTES
        ));
    }
    write_state(&ctx.store.vault, &args.id, &next)?;
    Ok(next)
}

impl Extension for Storage {
    fn id(&self) -> &str {
        "core.storage"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        // Older vaults carry the state in the sidecars: move it out after
        // every index so lists and `pages.get` never carry it.
        r.on(events::VAULT_INDEXED, |store, _| {
            let Ok(pages) = store.with_ext_containing("\"storage\"") else {
                return;
            };
            for mut meta in pages {
                let _ = migrate(store, &mut meta);
            }
        });

        r.add(
            Operation::new(
                "storage.get",
                "Read the state an interactive page saved for itself, per namespace (`local`, `personal`, `shared`). Empty maps when the page never wrote anything.",
                object(json!({ "id": id_prop() }), &["id"]),
                |ctx: &mut Ctx, a: IdArgs| load(ctx, &a.id),
            )
            .ui_only(),
        )?;

        r.add(
            Operation::new(
                "storage.write",
                "Apply page-state changes (`value: null` deletes a key) and return the new state. The whole batch is rejected, unchanged, if a key is empty or longer than 200 characters, or if the page's JSON state would exceed 1 MiB. Does not change the page's `updatedAt`.",
                object(
                    json!({
                        "id": id_prop(),
                        "changes": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "area": { "type": "string", "enum": AREAS },
                                    "key": { "type": "string", "minLength": 1, "maxLength": MAX_KEY_CHARS },
                                    "value": { "type": ["string", "null"] }
                                },
                                "required": ["area", "key", "value"],
                                "additionalProperties": false
                            }
                        }
                    }),
                    &["id", "changes"],
                ),
                |ctx: &mut Ctx, a: WriteArgs| write_changes(ctx, a),
            )
            .ui_only(),
        )?;

        r.add(
            Operation::new(
                "storage.clear",
                "Erase everything an interactive page saved for itself. Does not change the page's `updatedAt`.",
                object(json!({ "id": id_prop() }), &["id"]),
                |ctx: &mut Ctx, a: IdArgs| {
                    let mut meta = ctx.page(&a.id)?;
                    migrate(ctx.store, &mut meta)?;
                    remove_state(&ctx.store.vault, &a.id)?;
                    Ok(PageStorage::default())
                },
            )
            .ui_only(),
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_validates_area_and_key_and_deletes_on_null() {
        let mut state = PageStorage::default();
        apply(
            &mut state,
            &Change {
                area: "local".into(),
                key: "k".into(),
                value: Some("v".into()),
            },
        )
        .unwrap();
        assert_eq!(state.local.get("k").map(String::as_str), Some("v"));

        apply(
            &mut state,
            &Change {
                area: "local".into(),
                key: "k".into(),
                value: None,
            },
        )
        .unwrap();
        assert!(state.is_empty(), "a null value deletes the key");

        let err = apply(
            &mut state,
            &Change {
                area: "nowhere".into(),
                key: "k".into(),
                value: Some("v".into()),
            },
        )
        .unwrap_err();
        assert!(err.contains("unknown area"), "{err}");

        let err = apply(
            &mut state,
            &Change {
                area: "local".into(),
                key: String::new(),
                value: Some("v".into()),
            },
        )
        .unwrap_err();
        assert!(err.contains("1 to 200"), "{err}");

        let err = apply(
            &mut state,
            &Change {
                area: "local".into(),
                key: "é".repeat(MAX_KEY_CHARS + 1),
                value: Some("v".into()),
            },
        )
        .unwrap_err();
        assert!(err.contains("1 to 200"), "{err}");
    }

    #[test]
    fn serialized_length_counts_utf8_bytes() {
        let mut state = PageStorage::default();
        state.local.insert("k".into(), "é".into());
        assert_eq!(
            state.serialized_len(),
            r#"{"local":{"k":"é"},"personal":{},"shared":{}}"#.len()
        );
    }
}
