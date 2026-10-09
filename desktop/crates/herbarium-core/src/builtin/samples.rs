// Sample pages for a new vault: a welcome page, an interactive explainer of
// spaced repetition and a playground that keeps its state, linked to each
// other, with recall questions, in a "Getting started" reading path. One is
// due for review right away, so Today and review have something to show.

use serde::Deserialize;
use serde_json::{Value, json};

use super::object;
use super::pages::{CreateArgs, create};
use super::paths;
use crate::extension::{Ctx, Extension, OpResult, Operation, Registry};
use crate::models::PageSource;

pub(crate) struct Samples;

const FOLDER: &str = "Getting started";
const PATH_NAME: &str = "Getting started with Herbarium";

/// `(id, html)` of each sample, in reading order. Pages link to each other
/// with `herbarium-app://open/<id>`, so the ids are fixed.
const SAMPLES: [(&str, &str); 3] = [
    (
        "herbarium-welcome",
        include_str!("../../samples/welcome.html"),
    ),
    (
        "herbarium-spaced-repetition",
        include_str!("../../samples/spaced-repetition.html"),
    ),
    (
        "herbarium-flexbox-playground",
        include_str!("../../samples/flexbox-playground.html"),
    ),
];

#[derive(Deserialize)]
struct NoArgs {}

fn add(ctx: &mut Ctx) -> OpResult<Value> {
    let existing = crate::importer::imported_keys(ctx.store)?;
    let mut ids = Vec::new();
    let mut added = 0;
    for (i, (id, html)) in SAMPLES.iter().enumerate() {
        let key = format!("sample:{id}");
        if existing.contains(&key) {
            // Already added: keep it in the path, wherever it is now.
            if let Some(meta) = ctx
                .store
                .all()
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|m| crate::import_key(m) == Some(key.as_str()))
            {
                ids.push(meta.id);
            }
            continue;
        }
        let meta = create(
            ctx,
            CreateArgs {
                html: html.to_string(),
                folder: Some(FOLDER.into()),
                tags: Some(vec!["herbarium".into()]),
                source: Some(PageSource {
                    tool: Some("Herbarium".into()),
                    ..Default::default()
                }),
                import_key: Some(key),
                // The welcome page is due right away; the others in a few days.
                review_in_minutes: Some(if i == 0 { 1 } else { 3 * 1440 }),
                allow_cdn: Some(false),
                ..Default::default()
            },
            None,
            Some(id),
        )?;
        ids.push(meta.id);
        added += 1;
    }
    let vault = ctx.store.vault.clone();
    let mut all = paths::load(&vault);
    let path_id = match all.iter_mut().find(|p| p.name == PATH_NAME) {
        Some(path) => {
            for id in &ids {
                if !path.pages.contains(id) {
                    path.pages.push(id.clone());
                }
            }
            path.id.clone()
        }
        None => {
            let path = paths::ReadingPath {
                id: paths::new_id(PATH_NAME, &all),
                name: PATH_NAME.into(),
                description: "Three short pages that show what Herbarium can do.".into(),
                pages: ids.clone(),
            };
            let id = path.id.clone();
            all.push(path);
            id
        }
    };
    paths::store(&vault, &all)?;
    Ok(json!({ "added": added, "pages": ids, "path": path_id }))
}

impl Extension for Samples {
    fn id(&self) -> &str {
        "core.samples"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.add(
            Operation::new(
                "vault.add_samples",
                "Add the sample pages (welcome, spaced repetition, a playground) in a \"Getting started\" folder and reading path. Samples already added are skipped.",
                object(json!({}), &[]),
                |ctx: &mut Ctx, _: NoArgs| add(ctx),
            )
            .ui_only(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::SAMPLES;
    use crate::content::{extract_page_links, extract_title, looks_like_html};

    #[test]
    fn samples_are_pages_that_link_to_each_other() {
        let ids: Vec<&str> = SAMPLES.iter().map(|(id, _)| *id).collect();
        for (_, html) in SAMPLES {
            assert!(looks_like_html(html));
            assert!(!extract_title(html).is_empty());
            assert!(
                html.contains("data-herbarium-recall"),
                "every sample can quiz"
            );
            for link in extract_page_links(html) {
                assert!(ids.contains(&link.as_str()), "broken sample link {link}");
            }
            assert!(
                !html.contains("http://") && !html.contains("https://"),
                "samples need no network"
            );
        }
    }
}
