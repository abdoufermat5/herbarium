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

impl PageMeta {
    /// Bring metadata read from an older sidecar up to the current schema.
    pub(crate) fn upgrade(&mut self) {
        if let Some(days) = self.legacy_interval_days.take() {
            self.interval_minutes
                .get_or_insert(days.saturating_mul(1440));
        }
        self.schema_version = SCHEMA_VERSION;
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
