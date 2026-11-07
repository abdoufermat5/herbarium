// Shared data models, mirrored on the frontend in `src/lib/types.ts`.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// Version of the sidecar `{id}.json` format written by this build.
pub const SCHEMA_VERSION: u32 = 2;

fn schema_version_default() -> u32 {
    SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PageMeta {
    #[serde(default = "schema_version_default")]
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    /// The `<title>` extracted from the HTML at its last write or index. While
    /// `title` equals it, the user never renamed the page and the title follows
    /// the document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_title: Option<String>,
    pub tags: Vec<String>,
    pub folder: Option<String>,
    pub note: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// Length of the current review interval, in minutes.
    pub interval_minutes: Option<i64>,
    /// Schema 1 sidecars stored `intervalDays`. Read-only; `upgrade` folds it
    /// into `interval_minutes`, and it is never written back.
    #[serde(default, rename = "intervalDays", skip_serializing)]
    pub(crate) legacy_interval_days: Option<i64>,
    pub next_review: Option<i64>,
    pub last_review: Option<i64>,
    pub allow_cdn: bool,
    /// Extension-owned data, keyed by extension id. The core never interprets
    /// it and carries it through every read and write.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ext: BTreeMap<String, Value>,
}

impl PageMeta {
    pub fn new(id: String) -> Self {
        let now = crate::time::now_ms();
        Self {
            schema_version: SCHEMA_VERSION,
            id,
            title: String::new(),
            source_title: None,
            tags: Vec::new(),
            folder: None,
            note: String::new(),
            created_at: now,
            updated_at: now,
            interval_minutes: None,
            legacy_interval_days: None,
            next_review: None,
            last_review: None,
            // Pages found on disk without a sidecar are untrusted: network off.
            allow_cdn: false,
            ext: BTreeMap::new(),
        }
    }
}

/// `ext` keys a list view reads: the icon (`look`), the origin tool (`source`)
/// and the review memory state behind the vitality leaf (`review`).
const SUMMARY_EXT_KEYS: [&str; 3] = [LOOK_KEY, SOURCE_KEY, "review"];

impl PageMeta {
    /// The same metadata with `ext` cut down to what list views draw, so a
    /// list stays small however much a page stores: no highlights, page
    /// state, source prompt or per-day review counts. The review `log` is
    /// dropped too (use [`PageMeta::review_summary`] where it is shown).
    pub fn summary(&self) -> PageMeta {
        self.project(false)
    }

    /// Like [`PageMeta::summary`] but keeps the review `log` that the review
    /// queue lists per page.
    pub fn review_summary(&self) -> PageMeta {
        self.project(true)
    }

    fn project(&self, keep_review_log: bool) -> PageMeta {
        let mut out = self.clone_without_ext();
        for key in SUMMARY_EXT_KEYS {
            let Some(value) = self.ext.get(key) else {
                continue;
            };
            let value = match (key, value) {
                (SOURCE_KEY, Value::Object(o)) => Value::Object(
                    o.iter()
                        .filter(|(k, _)| *k != "prompt")
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                ),
                ("review", Value::Object(o)) => Value::Object(
                    o.iter()
                        .filter(|(k, _)| *k != "days" && (keep_review_log || *k != "log"))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                ),
                _ => value.clone(),
            };
            out.ext.insert(key.into(), value);
        }
        out
    }

    fn clone_without_ext(&self) -> PageMeta {
        PageMeta {
            schema_version: self.schema_version,
            id: self.id.clone(),
            title: self.title.clone(),
            source_title: self.source_title.clone(),
            tags: self.tags.clone(),
            folder: self.folder.clone(),
            note: self.note.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            interval_minutes: self.interval_minutes,
            legacy_interval_days: None,
            next_review: self.next_review,
            last_review: self.last_review,
            allow_cdn: self.allow_cdn,
            ext: BTreeMap::new(),
        }
    }

    /// Bring metadata read from an older sidecar up to the current schema.
    pub(crate) fn upgrade(&mut self) {
        if let Some(days) = self.legacy_interval_days.take() {
            self.interval_minutes
                .get_or_insert(days.saturating_mul(1440));
        }
        self.schema_version = SCHEMA_VERSION;
    }
}

/// Key of a page's [`PageSource`] in `PageMeta::ext`.
pub const SOURCE_KEY: &str = "source";

const SOURCE_TOOL_MAX: usize = 100;
const SOURCE_URL_MAX: usize = 2048;
const SOURCE_PROMPT_MAX: usize = 20_000;

/// Where a page came from: the address it was saved from, the tool that
/// generated it and the prompt that asked for it. Stored in `ext["source"]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageSource {
    /// `http(s)` address the page was saved from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The tool or model that generated the page, e.g. `Claude Code`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// The request that produced the page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

impl PageSource {
    /// Trim every field, drop blank ones and enforce the limits. Returns
    /// `None` when nothing is left.
    pub fn clean(self) -> Result<Option<Self>, String> {
        fn field(value: Option<String>, name: &str, max: usize) -> Result<Option<String>, String> {
            let Some(value) = value
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
            else {
                return Ok(None);
            };
            if value.chars().count() > max {
                return Err(format!("source {name} is longer than {max} characters"));
            }
            Ok(Some(value))
        }
        let url = field(self.url, "url", SOURCE_URL_MAX)?;
        if let Some(url) = &url {
            let lower = url.to_ascii_lowercase();
            if !(lower.starts_with("https://") || lower.starts_with("http://")) {
                return Err("source url must start with http:// or https://".into());
            }
        }
        let source = PageSource {
            url,
            tool: field(self.tool, "tool", SOURCE_TOOL_MAX)?,
            prompt: field(self.prompt, "prompt", SOURCE_PROMPT_MAX)?,
        };
        Ok((source != PageSource::default()).then_some(source))
    }

    /// The source recorded on `meta`, if any.
    pub fn of(meta: &PageMeta) -> Option<Self> {
        meta.ext
            .get(SOURCE_KEY)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Record `source` on `meta`, or remove it when `None`.
    pub fn set(meta: &mut PageMeta, source: Option<Self>) {
        match source.and_then(|s| serde_json::to_value(s).ok()) {
            Some(value) => {
                meta.ext.insert(SOURCE_KEY.into(), value);
            }
            None => {
                meta.ext.remove(SOURCE_KEY);
            }
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub meta: PageMeta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Partial update of a page's editable metadata. Absent fields are left
/// unchanged; `folder: null` moves the page to the vault root.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaPatch {
    pub title: Option<String>,
    pub tags: Option<Vec<String>>,
    #[serde(default, deserialize_with = "present")]
    pub folder: Option<Option<String>>,
    pub note: Option<String>,
    /// `null` removes the page's source.
    #[serde(default, deserialize_with = "present")]
    pub source: Option<Option<PageSource>>,
    /// A short emoji or symbol shown with the page; `null` removes it.
    #[serde(default, deserialize_with = "present")]
    pub icon: Option<Option<String>>,
}

/// Key of `{ icon }` in `PageMeta::ext`.
pub const LOOK_KEY: &str = "look";

/// Most characters in a page or folder icon (an emoji with modifiers).
const MAX_ICON_CHARS: usize = 8;

/// A trimmed icon, `None` for blank; rejects long text and markup.
pub fn clean_icon(icon: &str) -> Result<Option<String>, String> {
    let icon = icon.trim();
    if icon.is_empty() {
        return Ok(None);
    }
    if icon.chars().count() > MAX_ICON_CHARS || icon.contains(['<', '>', '&', '"']) {
        return Err(format!(
            "an icon is an emoji or symbol of at most {MAX_ICON_CHARS} characters"
        ));
    }
    Ok(Some(icon.to_string()))
}

/// Set or clear the page's icon.
pub fn set_icon(meta: &mut PageMeta, icon: Option<String>) {
    match icon {
        Some(icon) => {
            meta.ext
                .insert(LOOK_KEY.into(), serde_json::json!({ "icon": icon }));
        }
        None => {
            meta.ext.remove(LOOK_KEY);
        }
    }
}

/// Distinguishes an explicit `null` (Some(None)) from an absent field (None).
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCount {
    pub tag: String,
    pub count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFile {
    pub name: Option<String>,
    pub content: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: usize,
    pub errors: Vec<String>,
}

/// A search result: the page plus, when the match is in the page text, a
/// short excerpt with the matched terms wrapped in `[` `]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    #[serde(flatten)]
    pub meta: PageMeta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

/// One saved version of a page's HTML, under `.herbarium/history/<id>/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// When the snapshot was taken, in unix ms (also its filename stem).
    pub at: i64,
    /// Who made the change the snapshot preserved: `ui` or `agent`.
    pub caller: String,
    pub bytes: usize,
}

/// A page sitting in the vault trash.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    #[serde(flatten)]
    pub meta: PageMeta,
    /// When the page was trashed, in unix ms.
    pub deleted_at: i64,
}

/// A file a rescan could not index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedFile {
    /// Relative to the vault, `/`-separated.
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexReport {
    pub indexed: usize,
    pub removed: usize,
    pub total: usize,
    pub skipped: Vec<SkippedFile>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summary_keeps_list_keys_and_drops_the_bulk() {
        let mut meta = PageMeta::new("p".into());
        meta.title = "T".into();
        meta.ext.insert("look".into(), json!({ "icon": "x" }));
        meta.ext.insert(
            "source".into(),
            json!({ "tool": "Claude", "url": "https://a.b", "prompt": "long" }),
        );
        meta.ext.insert(
            "review".into(),
            json!({ "count": 2, "log": [{ "at": 1 }], "days": { "1": 1 }, "stability": 3.0 }),
        );
        meta.ext
            .insert("highlights".into(), json!([{ "text": "h" }]));
        meta.ext
            .insert("storage".into(), json!({ "local": { "k": "v" } }));

        let s = meta.summary();
        assert_eq!(s.title, "T");
        assert_eq!(s.ext["look"], json!({ "icon": "x" }));
        assert_eq!(
            s.ext["source"],
            json!({ "tool": "Claude", "url": "https://a.b" })
        );
        assert_eq!(s.ext["review"], json!({ "count": 2, "stability": 3.0 }));
        assert!(!s.ext.contains_key("highlights") && !s.ext.contains_key("storage"));

        let q = meta.review_summary();
        assert_eq!(q.ext["review"]["log"], json!([{ "at": 1 }]));
        assert!(q.ext["review"].get("days").is_none());
    }
}
