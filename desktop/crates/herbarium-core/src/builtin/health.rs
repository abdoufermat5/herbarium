// Page health: why a page might not work, and fixes. Checks what the page
// loads against what the reader allows: local files that are missing, remote
// resources on a page without network access, CDN hosts outside the
// allowlist; plus links to pages that are gone, oversized pages and pages
// without a title. `health.fix` inlines a page's local files so it stands on
// its own (the replaced HTML is kept in history).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::pages::overwrite_html_by;
use super::{id_prop, object};
use crate::assets::{inline_local_assets, local_asset, page_folder};
use crate::content::{extract_page_links, extract_title};
use crate::extension::{Caller, Ctx, Extension, OpResult, Operation, Registry};
use crate::models::PageMeta;
use crate::rewrite::{is_asset_attr, relative_path, rewrite_attrs};
use crate::vault;

pub(crate) struct Health;

/// Hosts the reader's CSP lets a page with network access load scripts from.
pub const SCRIPT_HOSTS: [&str; 4] = [
    "cdnjs.cloudflare.com",
    "cdn.jsdelivr.net",
    "unpkg.com",
    "code.jquery.com",
];
/// Hosts it lets stylesheets come from.
pub const STYLE_HOSTS: [&str; 4] = [
    "fonts.googleapis.com",
    "cdnjs.cloudflare.com",
    "cdn.jsdelivr.net",
    "unpkg.com",
];

const LARGE_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    /// `missing-asset`, `needs-network`, `blocked-host`, `broken-links`,
    /// `local-assets`, `large`, `untitled`.
    pub kind: &'static str,
    /// `error` (part of the page is broken), `warn` or `info`.
    pub level: &'static str,
    /// The files, hosts or page ids concerned.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
    /// A fix `health.fix` can apply, or a setting the user can change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<&'static str>,
}

fn host_of(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?.split(':').next()?;
    Some(host.to_ascii_lowercase())
}

/// The issues of one page.
pub fn check(ctx: &Ctx, meta: &PageMeta) -> OpResult<Vec<Issue>> {
    let store = ctx.store;
    let html = vault::read_html(&store.vault, &meta.id, meta.folder.as_deref())?;
    let folder = page_folder(&store.vault, &meta.id, meta.folder.as_deref());
    let mut missing = BTreeSet::new();
    let mut local = BTreeSet::new();
    let mut remote = BTreeSet::new();
    let mut blocked = BTreeSet::new();
    rewrite_attrs(&html, |tag, attr, value| {
        if !is_asset_attr(tag, attr) {
            return None;
        }
        let value = value.trim();
        if let Some(rel) = relative_path(value) {
            if local_asset(&store.vault, &folder, value).is_some() {
                local.insert(rel.to_string());
            } else {
                missing.insert(rel.to_string());
            }
        } else if let Some(host) = host_of(value) {
            remote.insert(host.clone());
            let insecure = value.starts_with("http://");
            let allowed = match (tag, attr) {
                ("script", _) => SCRIPT_HOSTS.contains(&host.as_str()) && !insecure,
                ("link", _) => STYLE_HOSTS.contains(&host.as_str()) && !insecure,
                _ => !insecure,
            };
            if !allowed {
                blocked.insert(host);
            }
        }
        None
    });

    let mut issues = Vec::new();
    if !missing.is_empty() {
        issues.push(Issue {
            kind: "missing-asset",
            level: "error",
            items: missing.into_iter().collect(),
            fix: None,
        });
    }
    if !remote.is_empty() && !meta.allow_cdn {
        issues.push(Issue {
            kind: "needs-network",
            level: "warn",
            items: remote.into_iter().collect(),
            fix: Some("enable-network"),
        });
    } else if !blocked.is_empty() {
        issues.push(Issue {
            kind: "blocked-host",
            level: "warn",
            items: blocked.into_iter().collect(),
            fix: None,
        });
    }
    let broken: Vec<String> = extract_page_links(&html)
        .into_iter()
        .filter(|id| id != &meta.id && store.get_meta(id).ok().flatten().is_none())
        .collect();
    if !broken.is_empty() {
        issues.push(Issue {
            kind: "broken-links",
            level: "warn",
            items: broken,
            fix: None,
        });
    }
    if !local.is_empty() {
        issues.push(Issue {
            kind: "local-assets",
            level: "info",
            items: local.into_iter().collect(),
            fix: Some("inline-assets"),
        });
    }
    if html.len() > LARGE_BYTES {
        issues.push(Issue {
            kind: "large",
            level: "info",
            items: vec![format!("{}", html.len())],
            fix: None,
        });
    }
    if extract_title(&html).is_empty() {
        issues.push(Issue {
            kind: "untitled",
            level: "info",
            items: Vec::new(),
            fix: None,
        });
    }
    Ok(issues)
}

#[derive(Deserialize, Default)]
struct CheckArgs {
    id: Option<String>,
    /// Include pages whose only notes are `info`.
    #[serde(default)]
    all: bool,
}

#[derive(Deserialize)]
struct FixArgs {
    id: String,
    fix: String,
}

impl Extension for Health {
    fn id(&self) -> &str {
        "core.health"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.add(Operation::new(
            "health.check",
            "Why pages might not work. With `id`: that page's `issues`. Without: every page with an `error` or `warn` issue (or any issue, with `all`). Kinds: missing-asset (local files it loads are gone), needs-network (it loads from the internet but network access is off), blocked-host (it loads from a host outside the CDN allowlist), broken-links (links to pages that are gone), local-assets (depends on files next to it; fix: inline-assets), large, untitled.",
            object(
                json!({ "id": { "type": "string", "description": "Page id; omit to check the whole vault." }, "all": { "type": "boolean" } }),
                &[],
            ),
            |ctx: &mut Ctx, a: CheckArgs| -> OpResult<Value> {
                if let Some(id) = a.id {
                    let meta = ctx.page(&id)?;
                    return Ok(json!({ "id": meta.id, "issues": check(ctx, &meta)? }));
                }
                let mut pages = Vec::new();
                for meta in ctx.store.all().map_err(|e| e.to_string())? {
                    let issues = check(ctx, &meta).unwrap_or_default();
                    if issues.iter().any(|i| a.all || i.level != "info") {
                        pages.push(json!({ "id": meta.id, "title": meta.title, "folder": meta.folder, "issues": issues }));
                    }
                }
                Ok(json!({ "pages": pages }))
            },
        ))?;

        r.add(
            Operation::new(
                "health.fix",
                "Apply a fix: `inline-assets` makes the page self-contained by inlining the local files it loads (the previous HTML is kept in history).",
                object(
                    json!({ "id": id_prop(), "fix": { "type": "string", "enum": ["inline-assets"] } }),
                    &["id", "fix"],
                ),
                |ctx: &mut Ctx, a: FixArgs| {
                    let meta = ctx.page(&a.id)?;
                    match a.fix.as_str() {
                        "inline-assets" => {
                            let html = vault::read_html(&ctx.store.vault, &meta.id, meta.folder.as_deref())?;
                            let inlined = inline_local_assets(&ctx.store.vault, &meta.id, meta.folder.as_deref(), &html);
                            overwrite_html_by(ctx, &meta.id, &inlined, None, Caller::Ui.tag())
                        }
                        other => Err(format!("unknown fix `{other}`")),
                    }
                },
            )
            .ui_only(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::host_of;

    #[test]
    fn hosts() {
        assert_eq!(
            host_of("https://CDN.jsdelivr.net/npm/x").as_deref(),
            Some("cdn.jsdelivr.net")
        );
        assert_eq!(
            host_of("http://user@ex.com:8080/a").as_deref(),
            Some("ex.com")
        );
        assert_eq!(host_of("img/a.png"), None);
    }
}
