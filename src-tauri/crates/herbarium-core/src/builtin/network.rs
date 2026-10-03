// Per-page network switch: allow the CDN allowlist, or block all network.

use serde::Deserialize;
use serde_json::json;

use super::{id_prop, object};
use crate::extension::{Ctx, Extension, OpResult, Operation, Registry};
use crate::time::now_ms;

pub(crate) struct Network;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetArgs {
    id: String,
    allow_cdn: bool,
}

impl Extension for Network {
    fn id(&self) -> &str {
        "core.network"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.add(Operation::new(
            "network.set",
            "Allow a page to load scripts, styles and fonts from known CDNs (cdnjs, jsDelivr, unpkg, jQuery, Google Fonts), or block all network access for it.",
            object(
                json!({ "id": id_prop(), "allowCdn": { "type": "boolean" } }),
                &["id", "allowCdn"],
            ),
            |ctx: &mut Ctx, a: SetArgs| {
                let mut meta = ctx.page(&a.id)?;
                meta.allow_cdn = a.allow_cdn;
                meta.updated_at = now_ms();
                ctx.save(&meta)?;
                Ok(meta)
            },
        ))
    }
}
