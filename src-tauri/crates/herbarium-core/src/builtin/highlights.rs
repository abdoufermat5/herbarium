// Highlights and margin notes. A highlight is anchored by its text (`quote`)
// and a little of what surrounds it (`prefix`, `suffix`), the way the W3C
// text-quote selector does, so it survives edits elsewhere in the page. They
// live with the page, in `ext.highlights`, and their text and notes are
// searchable.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{id_prop, object};
use crate::extension::{Ctx, Extension, OpResult, Operation, Registry};
use crate::models::PageMeta;
use crate::time::now_ms;

pub(crate) struct Highlights;

/// Key of the highlight list in `PageMeta::ext`.
pub const HIGHLIGHTS_KEY: &str = "highlights";
const MAX_HIGHLIGHTS: usize = 500;
const MAX_QUOTE: usize = 2000;
const MAX_CONTEXT: usize = 64;
const MAX_NOTE: usize = 10_000;
const COLORS: [&str; 4] = ["yellow", "green", "blue", "pink"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Highlight {
    pub id: String,
    pub quote: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub suffix: String,
    pub color: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// When it was made, unix ms.
    pub at: i64,
}

/// The highlights stored on `meta`, oldest first.
pub fn of(meta: &PageMeta) -> Vec<Highlight> {
    meta.ext
        .get(HIGHLIGHTS_KEY)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn set(meta: &mut PageMeta, list: &[Highlight]) {
    if list.is_empty() {
        meta.ext.remove(HIGHLIGHTS_KEY);
    } else if let Ok(v) = serde_json::to_value(list) {
        meta.ext.insert(HIGHLIGHTS_KEY.into(), v);
    }
}

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn clean_color(color: Option<String>) -> OpResult<String> {
    let color = color
        .unwrap_or_else(|| "yellow".into())
        .to_ascii_lowercase();
    if COLORS.contains(&color.as_str()) {
        Ok(color)
    } else {
        Err(format!(
            "unknown highlight colour `{color}`; use one of {}",
            COLORS.join(", ")
        ))
    }
}

fn clean_note(note: &str) -> OpResult<String> {
    let note = note.trim();
    if note.chars().count() > MAX_NOTE {
        return Err(format!("a note is at most {MAX_NOTE} characters"));
    }
    Ok(note.to_string())
}

#[derive(Deserialize)]
struct PageArgs {
    page: String,
}

#[derive(Deserialize)]
struct AddArgs {
    page: String,
    quote: String,
    #[serde(default)]
    prefix: String,
    #[serde(default)]
    suffix: String,
    color: Option<String>,
    #[serde(default)]
    note: String,
}

#[derive(Deserialize)]
struct UpdateArgs {
    page: String,
    id: String,
    color: Option<String>,
    note: Option<String>,
}

#[derive(Deserialize)]
struct RemoveArgs {
    page: String,
    id: String,
}

/// Save the page's metadata with `list` and hand back both.
fn save(ctx: &mut Ctx, mut meta: PageMeta, list: Vec<Highlight>) -> OpResult<Value> {
    set(&mut meta, &list);
    meta.updated_at = now_ms();
    ctx.save(&meta)?;
    Ok(json!({ "page": meta, "highlights": list }))
}

impl Extension for Highlights {
    fn id(&self) -> &str {
        "core.highlights"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        let page_prop = json!({ "type": "string", "description": "Page id." });

        r.add(Operation::new(
            "highlights.list",
            "The passages the user highlighted in a page, oldest first: `{ id, quote, prefix, suffix, color, note, at }`. Notes are the user's thoughts on the passage.",
            object(json!({ "page": page_prop.clone() }), &["page"]),
            |ctx: &mut Ctx, a: PageArgs| Ok(of(&ctx.page(&a.page)?)),
        ))?;

        r.add(
            Operation::new(
                "highlights.add",
                "Highlight a passage of a page, optionally with a note.",
                object(
                    json!({
                        "page": page_prop.clone(),
                        "quote": { "type": "string" },
                        "prefix": { "type": "string" },
                        "suffix": { "type": "string" },
                        "color": { "type": "string", "enum": COLORS },
                        "note": { "type": "string" }
                    }),
                    &["page", "quote"],
                ),
                |ctx: &mut Ctx, a: AddArgs| {
                    let meta = ctx.page(&a.page)?;
                    let quote = a.quote.trim();
                    if quote.is_empty() {
                        return Err("nothing to highlight".into());
                    }
                    if quote.chars().count() > MAX_QUOTE {
                        return Err(format!("a highlight is at most {MAX_QUOTE} characters"));
                    }
                    let mut list = of(&meta);
                    if list.len() >= MAX_HIGHLIGHTS {
                        return Err(format!("a page holds at most {MAX_HIGHLIGHTS} highlights"));
                    }
                    let highlight = Highlight {
                        id: format!("h{}", uuid::Uuid::new_v4().simple()),
                        quote: quote.to_string(),
                        prefix: clip(&a.prefix, MAX_CONTEXT),
                        suffix: clip(&a.suffix, MAX_CONTEXT),
                        color: clean_color(a.color)?,
                        note: clean_note(&a.note)?,
                        at: now_ms(),
                    };
                    list.push(highlight.clone());
                    let mut out = save(ctx, meta, list)?;
                    out["highlight"] =
                        serde_json::to_value(highlight).map_err(|e| e.to_string())?;
                    Ok(out)
                },
            )
            .ui_only(),
        )?;

        r.add(
            Operation::new(
                "highlights.update",
                "Change a highlight's colour or note.",
                object(
                    json!({
                        "page": page_prop.clone(),
                        "id": id_prop(),
                        "color": { "type": "string", "enum": COLORS },
                        "note": { "type": "string" }
                    }),
                    &["page", "id"],
                ),
                |ctx: &mut Ctx, a: UpdateArgs| {
                    let meta = ctx.page(&a.page)?;
                    let mut list = of(&meta);
                    let h = list
                        .iter_mut()
                        .find(|h| h.id == a.id)
                        .ok_or_else(|| format!("highlight not found: {}", a.id))?;
                    if let Some(color) = a.color {
                        h.color = clean_color(Some(color))?;
                    }
                    if let Some(note) = a.note {
                        h.note = clean_note(&note)?;
                    }
                    save(ctx, meta, list)
                },
            )
            .ui_only(),
        )?;

        r.add(
            Operation::new(
                "highlights.remove",
                "Remove a highlight and its note.",
                object(
                    json!({ "page": page_prop, "id": id_prop() }),
                    &["page", "id"],
                ),
                |ctx: &mut Ctx, a: RemoveArgs| {
                    let meta = ctx.page(&a.page)?;
                    let mut list = of(&meta);
                    let before = list.len();
                    list.retain(|h| h.id != a.id);
                    if list.len() == before {
                        return Err(format!("highlight not found: {}", a.id));
                    }
                    save(ctx, meta, list)
                },
            )
            .ui_only(),
        )
    }
}
