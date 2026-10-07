// Page previews: a small layout digest of each page's first screen (blocks
// with their colours, text lines, one image), measured inside the page by
// the reader bridge and drawn by the app as a miniature. Kept in
// `.herbarium/previews/<id>.json` with the page's `updatedAt`, so a changed
// page gets a new one. The page reports the digest, so everything in it is
// checked here: numbers clamped, colours only `rgb()`/`rgba()`, the image
// only a PNG, JPEG or WebP data URL.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{id_prop, object};
use crate::extension::{Ctx, Extension, OpResult, Operation, Registry, events};
use crate::vault;

pub(crate) struct Previews;

const MAX_BLOCKS: usize = 120;
const MAX_COORD: f64 = 4000.0;
const MAX_IMAGE: usize = 80 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Block {
    /// `box`, `text` or `img`.
    k: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    /// Fill (box) or ink (text).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    c: Option<String>,
    /// Corner radius.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    r: Option<f64>,
    /// Font size (text).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    s: Option<f64>,
    /// Line height (text).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lh: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Image {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    src: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Digest {
    w: f64,
    h: f64,
    bg: String,
    blocks: Vec<Block>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    image: Option<Image>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    updated_at: i64,
    digest: Digest,
}

fn num(v: f64, max: f64) -> OpResult<f64> {
    if !v.is_finite() {
        return Err("preview: a number is not finite".into());
    }
    Ok((v.clamp(-max, max) * 10.0).round() / 10.0)
}

/// `rgb(r, g, b)` or `rgba(r, g, b, a)` as `getComputedStyle` writes it.
fn color(c: &str) -> OpResult<String> {
    let c = c.trim();
    let inner = c
        .strip_prefix("rgba(")
        .or_else(|| c.strip_prefix("rgb("))
        .and_then(|s| s.strip_suffix(')'))
        .ok_or("preview: colours must be rgb() or rgba()")?;
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if !(parts.len() == 3 || parts.len() == 4) {
        return Err("preview: bad colour".into());
    }
    for (i, p) in parts.iter().enumerate() {
        let v: f64 = p.parse().map_err(|_| "preview: bad colour")?;
        let max = if i == 3 { 1.0 } else { 255.0 };
        if !(0.0..=max).contains(&v) {
            return Err("preview: bad colour".into());
        }
    }
    Ok(c.to_string())
}

fn clean(d: Digest) -> OpResult<Digest> {
    let w = num(d.w, MAX_COORD)?.max(1.0);
    let h = num(d.h, MAX_COORD)?.max(1.0);
    let mut blocks = Vec::new();
    for b in d.blocks.into_iter().take(MAX_BLOCKS) {
        if !matches!(b.k.as_str(), "box" | "text" | "img") {
            return Err("preview: unknown block kind".into());
        }
        blocks.push(Block {
            k: b.k,
            x: num(b.x, MAX_COORD)?,
            y: num(b.y, MAX_COORD)?,
            w: num(b.w, MAX_COORD)?.max(0.0),
            h: num(b.h, MAX_COORD)?.max(0.0),
            c: b.c.as_deref().map(color).transpose()?,
            r: b.r.map(|r| num(r, 200.0)).transpose()?,
            s: b.s.map(|s| num(s, 200.0)).transpose()?,
            lh: b.lh.map(|s| num(s, 400.0)).transpose()?,
        });
    }
    let image = match d.image {
        None => None,
        Some(img) => {
            let ok_type = [
                "data:image/png;base64,",
                "data:image/jpeg;base64,",
                "data:image/webp;base64,",
            ]
            .iter()
            .any(|p| img.src.starts_with(p));
            let payload_ok = img.src.split_once(',').is_some_and(|(_, b64)| {
                b64.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'='))
            });
            if !ok_type || !payload_ok || img.src.len() > MAX_IMAGE {
                None
            } else {
                Some(Image {
                    x: num(img.x, MAX_COORD)?,
                    y: num(img.y, MAX_COORD)?,
                    w: num(img.w, MAX_COORD)?.max(0.0),
                    h: num(img.h, MAX_COORD)?.max(0.0),
                    src: img.src,
                })
            }
        }
    };
    Ok(Digest {
        w,
        h,
        bg: color(&d.bg)?,
        blocks,
        image,
    })
}

fn dir(vault: &Path) -> PathBuf {
    vault.join(".herbarium").join("previews")
}

fn file(vault: &Path, id: &str) -> OpResult<PathBuf> {
    Ok(dir(vault).join(format!("{}.json", vault::checked_page_id(id)?)))
}

#[derive(Deserialize)]
struct SaveArgs {
    id: String,
    digest: Digest,
}

#[derive(Deserialize)]
struct NoArgs {}

impl Extension for Previews {
    fn id(&self) -> &str {
        "core.previews"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.on(events::PAGE_DELETED, |store, event| {
            if let Some(id) = event.payload["page"]["id"].as_str()
                && let Ok(path) = file(&store.vault, id)
            {
                let _ = std::fs::remove_file(path);
            }
        });

        r.add(
            Operation::new(
                "previews.save",
                "Store a page's preview digest, measured by the reader bridge.",
                object(
                    json!({ "id": id_prop(), "digest": { "type": "object" } }),
                    &["id", "digest"],
                ),
                |ctx: &mut Ctx, a: SaveArgs| {
                    let meta = ctx.page(&a.id)?;
                    let digest = clean(a.digest)?;
                    let vault = &ctx.store.vault;
                    std::fs::create_dir_all(dir(vault)).map_err(|e| e.to_string())?;
                    let stored = Stored {
                        updated_at: meta.updated_at,
                        digest,
                    };
                    vault::write_atomic(
                        &file(vault, &meta.id)?,
                        &serde_json::to_vec(&stored).map_err(|e| e.to_string())?,
                    )?;
                    Ok(json!({ "saved": meta.id }))
                },
            )
            .ui_only(),
        )?;

        r.add(
            Operation::new(
                "previews.list",
                "Preview digests of every page whose preview is current (`previews`), and the ids of pages that need one (`missing`).",
                object(json!({}), &[]),
                |ctx: &mut Ctx, _: NoArgs| {
                    let vault = ctx.store.vault.clone();
                    let mut previews: BTreeMap<String, Value> = BTreeMap::new();
                    let mut missing = Vec::new();
                    for meta in ctx.store.all().map_err(|e| e.to_string())? {
                        let stored: Option<Stored> = file(&vault, &meta.id)
                            .ok()
                            .and_then(|p| std::fs::read_to_string(p).ok())
                            .and_then(|raw| serde_json::from_str(&raw).ok());
                        match stored {
                            Some(s) if s.updated_at == meta.updated_at => {
                                previews.insert(meta.id, serde_json::to_value(s.digest).unwrap_or(Value::Null));
                            }
                            _ => missing.push(meta.id),
                        }
                    }
                    Ok(json!({ "previews": previews, "missing": missing }))
                },
            )
            .ui_only(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(v: Value) -> OpResult<Digest> {
        clean(serde_json::from_value(v).map_err(|e| e.to_string())?)
    }

    #[test]
    fn digests_are_validated() {
        let ok = digest(json!({
            "w": 1024.04, "h": 768, "bg": "rgb(255, 255, 255)",
            "blocks": [
                { "k": "box", "x": 0, "y": 0, "w": 1024, "h": 80, "c": "rgba(20, 30, 40, 0.5)", "r": 8 },
                { "k": "text", "x": 40, "y": 120, "w": 600, "h": 48, "c": "rgb(0, 0, 0)", "s": 16, "lh": 24 }
            ],
            "image": { "x": 0, "y": 0, "w": 10, "h": 10, "src": "data:image/png;base64,AAAA" }
        }))
        .unwrap();
        assert_eq!(ok.w, 1024.0);
        assert_eq!(ok.blocks.len(), 2);
        assert!(ok.image.is_some());

        for bad in [
            json!({ "w": 1, "h": 1, "bg": "url(javascript:x)", "blocks": [] }),
            json!({ "w": 1, "h": 1, "bg": "rgb(0,0,0)", "blocks": [{ "k": "script", "x": 0, "y": 0, "w": 1, "h": 1 }] }),
            json!({ "w": 1, "h": 1, "bg": "rgb(0,0,0)", "blocks": [{ "k": "box", "x": 0, "y": 0, "w": 1, "h": 1, "c": "red; x" }] }),
            json!({ "w": 1, "h": 1, "bg": "rgb(300, 0, 0)", "blocks": [] }),
        ] {
            assert!(digest(bad.clone()).is_err(), "{bad}");
        }
        let svg_image = digest(json!({
            "w": 1, "h": 1, "bg": "rgb(0, 0, 0)", "blocks": [],
            "image": { "x": 0, "y": 0, "w": 1, "h": 1, "src": "data:image/svg+xml;base64,PHN2Zz4=" }
        }))
        .unwrap();
        assert!(svg_image.image.is_none(), "SVG images are dropped");
        let huge = digest(json!({ "w": 1e9, "h": 1, "bg": "rgb(0, 0, 0)", "blocks": [] })).unwrap();
        assert_eq!(huge.w, MAX_COORD);
    }
}
