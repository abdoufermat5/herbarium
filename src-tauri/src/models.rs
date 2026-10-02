// Shared data models, mirrored on the frontend in `../src/lib/types.ts`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub vault_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageMeta {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub folder: Option<String>,
    pub note: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub interval_days: Option<i64>,
    pub next_review: Option<i64>,
    pub last_review: Option<i64>,
    pub allow_cdn: bool,
}

impl PageMeta {
    pub fn new(id: String) -> Self {
        let now = crate::time::now_ms();
        Self {
            id,
            title: String::new(),
            tags: Vec::new(),
            folder: None,
            note: String::new(),
            created_at: now,
            updated_at: now,
            interval_days: None,
            next_review: None,
            last_review: None,
            allow_cdn: true,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub meta: PageMeta,
    pub html: String,
}

/// Full optional patch for the editable metadata of a page.
/// The frontend always sends every field from its form state.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaPatch {
    pub title: String,
    pub tags: Vec<String>,
    pub folder: Option<String>,
    pub note: String,
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