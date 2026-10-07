// Links between pages: a page links to another with
// `<a href="herbarium-app://open/<id>">`. The index is refreshed lazily from
// the files, so links written by any tool are picked up.

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{id_prop, object};
use crate::extension::{Ctx, Extension, OpResult, Operation, Registry};
use crate::models::PageMeta;
use crate::vault;

pub(crate) struct Links;

#[derive(Deserialize)]
struct IdArgs {
    id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PageLinks {
    /// Pages this page links to, in id order.
    links: Vec<PageMeta>,
    /// Linked ids with no page in the vault.
    broken: Vec<String>,
    /// Pages linking to this page, by title.
    backlinks: Vec<PageMeta>,
}

impl Extension for Links {
    fn id(&self) -> &str {
        "core.links"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.add(Operation::new(
            "pages.links",
            "A page's links: `links` (pages it links to), `broken` (linked ids with no page) and `backlinks` (pages linking to it). Pages link to each other with `<a href=\"herbarium-app://open/<id>\">`; clicking such a link in Herbarium opens that page.",
            object(json!({ "id": id_prop() }), &["id"]),
            |ctx: &mut Ctx, a: IdArgs| {
                ctx.page(&a.id)?;
                let store = ctx.store;
                store
                    .refresh_links(|meta| {
                        vault::read_html(&store.vault, &meta.id, meta.folder.as_deref()).ok()
                    })
                    .map_err(|e| e.to_string())?;
                let mut links = Vec::new();
                let mut broken = Vec::new();
                for dst in store.links_from(&a.id).map_err(|e| e.to_string())? {
                    match store.get_meta(&dst).map_err(|e| e.to_string())? {
                        Some(meta) => links.push(meta),
                        None => broken.push(dst),
                    }
                }
                let backlinks = store.links_to(&a.id).map_err(|e| e.to_string())?;
                Ok(PageLinks { links, broken, backlinks })
            },
        ))
    }
}
