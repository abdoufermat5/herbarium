// Per-page persistent storage for interactive pages: values a page writes with
// `localStorage` or the Claude-artifact `window.storage` API are kept in the
// sidecar under `ext["storage"]` and never touch the HTML. A page therefore
// keeps its state across reader reopen, app restart, folder moves and the
// trash round trip.
//
// Storage is not a page revision: writes leave `updatedAt` alone and emit no
// event, so an open reader never mistakes them for an external edit and never
// reloads the frame (which would wipe the page's in-memory state).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{id_prop, object};
use crate::extension::{Ctx, Extension, OpResult, Operation, Registry};
use crate::models::PageMeta;
use crate::vault;

pub(crate) struct Storage;

/// Key of this extension's data in `PageMeta::ext`.
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
    /// The page's state; a missing or malformed entry reads as empty so a
    /// hand-edited sidecar never blocks a page.
    fn from_meta(meta: &PageMeta) -> Self {
        meta.ext
            .get(EXT_KEY)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
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

/// Persist the sidecar and refresh the index without bumping `updated_at` and
/// without emitting `page.updated`: page state is not a page revision, so an
/// open reader must not treat the write as an external change.
fn persist(ctx: &Ctx, meta: &PageMeta) -> OpResult<()> {
    vault::write_meta(&ctx.store.vault, meta)?;
    let text = ctx
        .store
        .text_for(&meta.id)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let mtime = vault::page_mtime(&ctx.store.vault, meta);
    ctx.store
        .upsert(meta, &text, mtime)
        .map_err(|e| e.to_string())
}

fn write_changes(ctx: &mut Ctx, args: WriteArgs) -> OpResult<PageStorage> {
    let mut meta = ctx.page(&args.id)?;
    let mut next = PageStorage::from_meta(&meta);
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
    if next.is_empty() {
        meta.ext.remove(EXT_KEY);
    } else {
        meta.ext.insert(
            EXT_KEY.into(),
            serde_json::to_value(&next).map_err(|e| e.to_string())?,
        );
    }
    persist(ctx, &meta)?;
    Ok(next)
}

impl Extension for Storage {
    fn id(&self) -> &str {
        "core.storage"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.add(
            Operation::new(
                "storage.get",
                "Read the state an interactive page saved for itself, per namespace (`local`, `personal`, `shared`). Empty maps when the page never wrote anything.",
                object(json!({ "id": id_prop() }), &["id"]),
                |ctx: &mut Ctx, a: IdArgs| Ok(PageStorage::from_meta(&ctx.page(&a.id)?)),
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
                    if meta.ext.remove(EXT_KEY).is_some() {
                        persist(ctx, &meta)?;
                    }
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
