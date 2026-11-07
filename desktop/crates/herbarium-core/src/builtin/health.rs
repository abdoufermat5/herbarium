// Page health: why a page might not work, and fixes. Checks what the page
// loads against what the reader allows: local files that are missing, remote
// resources on a page without network access, CDN hosts outside the
// allowlist; plus links to pages that are gone, oversized pages and pages
// without a title. `health.fix` inlines a page's local files so it stands on
// its own (the replaced HTML is kept in history).

use std::collections::BTreeSet;

use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::pages::overwrite_html_by;
use super::{id_prop, object};
use crate::assets::{inline_local_assets, local_asset, page_folder};
use crate::content::extract_title;
use crate::deps;
use crate::extension::{Caller, Ctx, Extension, OpResult, Operation, Registry};
use crate::models::PageMeta;
use crate::reserved::{self, PREFIX, Reserved};
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
    /// `missing-asset`, `needs-render-server`, `needs-network`, `blocked-host`,
    /// `broken-links`, `local-assets`, `remote-assets`, `large`, `untitled`.
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
    let mut render_server_missing = Vec::new();
    let has_kroki = crate::builtin::kroki_url(&store.vault).is_some();
    rewrite_attrs(&html, |tag, attr, value| {
        if !is_asset_attr(tag, attr) {
            return None;
        }
        let value = value.trim();
        if let Some(rel) = relative_path(value) {
            if rel.starts_with(PREFIX) {
                let decoded = percent_decode_str(rel).decode_utf8().ok();
                let parsed = decoded.as_deref().and_then(reserved::parse);
                let present = match &parsed {
                    Some(Reserved::Render { kind, source }) => {
                        if !has_kroki
                            && !reserved::render_cache_path(&store.vault, kind, source).exists()
                            && !render_server_missing.iter().any(|k| k == kind)
                        {
                            render_server_missing.push(kind.to_string());
                        }
                        true
                    }
                    Some(r) => reserved::resolve(&store.vault, r).is_some(),
                    None => false,
                };
                if !present {
                    missing.insert(rel.to_string());
                }
            } else if local_asset(&store.vault, &folder, value).is_some() {
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

    let mut remote_urls = Vec::new();
    for r in deps::page_refs(&html) {
        if !remote_urls.contains(&r.url) {
            remote_urls.push(r.url);
            if remote_urls.len() == 50 {
                break;
            }
        }
    }

    let mut issues = Vec::new();
    if !missing.is_empty() {
        issues.push(Issue {
            kind: "missing-asset",
            level: "error",
            items: missing.into_iter().collect(),
            fix: None,
        });
    }
    if !render_server_missing.is_empty() {
        issues.push(Issue {
            kind: "needs-render-server",
            level: "warn",
            items: render_server_missing,
            fix: Some("configure-render"),
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
    // Broken links come from the `links` table (kept fresh by `refresh_links`),
    // so no per-link metadata lookup is needed.
    let broken: Vec<String> = store
        .broken_links_from(&meta.id)
        .map_err(|e| e.to_string())?;
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
    if !remote_urls.is_empty() {
        issues.push(Issue {
            kind: "remote-assets",
            level: "info",
            items: remote_urls,
            fix: Some("vendor-deps"),
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
            "Why pages might not work. With `id`: that page's `issues`. Without: every page with an `error` or `warn` issue (or any issue, with `all`). Kinds: missing-asset (local files it loads are gone), needs-render-server (uses diagrams with no server configured; fix: configure-render), needs-network (it loads from the internet but network access is off), blocked-host (it loads from a host outside the CDN allowlist), broken-links (links to pages that are gone), local-assets (depends on files next to it; fix: inline-assets), remote-assets (loads from the internet; fix: vendor-deps), large, untitled.",
            object(
                json!({ "id": { "type": "string", "description": "Page id; omit to check the whole vault." }, "all": { "type": "boolean" } }),
                &[],
            ),
            |ctx: &mut Ctx, a: CheckArgs| -> OpResult<Value> {
                let store = ctx.store;
                store
                    .refresh_links(|meta| {
                        vault::read_html(&store.vault, &meta.id, meta.folder.as_deref()).ok()
                    })
                    .map_err(|e| e.to_string())?;
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
    use std::path::{Path, PathBuf};

    use serde_json::json;

    use super::host_of;
    use crate::time::now_ms;
    use crate::{Caller, Host};

    fn temp_vault(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "herbarium-health-{tag}-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn open(vault: &Path) -> Host {
        let mut host = Host::new();
        host.open_vault(vault.to_str().unwrap()).unwrap();
        host
    }

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

    #[test]
    fn reserved_lib_health() {
        let vault = temp_vault("lib");
        let host = open(&vault);

        let html1 = r#"<!doctype html><html><head><title>Mermaid</title>
            <script src="_herbarium/lib/mermaid.js"></script></head>
            <body>ok</body></html>"#;
        let page1 = host
            .call(Caller::Ui, "pages.create", json!({ "html": html1 }))
            .unwrap();
        let id1 = page1["id"].as_str().unwrap();

        let report1 = host
            .call(Caller::Agent, "health.check", json!({ "id": id1 }))
            .unwrap();
        let issues1 = report1["issues"].as_array().unwrap();
        assert!(!issues1.iter().any(|i| i["kind"] == "missing-asset"));
        assert!(!issues1.iter().any(|i| i["kind"] == "local-assets"));

        let html2 = r#"<!doctype html><html><head><title>Nope</title>
            <script src="_herbarium/lib/nope.js"></script></head>
            <body>ok</body></html>"#;
        let page2 = host
            .call(Caller::Ui, "pages.create", json!({ "html": html2 }))
            .unwrap();
        let id2 = page2["id"].as_str().unwrap();

        let report2 = host
            .call(Caller::Agent, "health.check", json!({ "id": id2 }))
            .unwrap();
        let issues2 = report2["issues"].as_array().unwrap();
        let missing = issues2
            .iter()
            .find(|i| i["kind"] == "missing-asset")
            .expect("should have missing-asset");
        assert_eq!(missing["items"], json!(["_herbarium/lib/nope.js"]));
        assert!(!issues2.iter().any(|i| i["kind"] == "local-assets"));

        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn remote_assets_health() {
        let vault = temp_vault("remote");
        let host = open(&vault);

        let url = "https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js";
        let html = format!(
            r#"<!doctype html><html><head><title>Remote</title>
            <script src="{url}"></script></head>
            <body>ok</body></html>"#
        );
        let page = host
            .call(Caller::Ui, "pages.create", json!({ "html": html }))
            .unwrap();
        let id = page["id"].as_str().unwrap();

        let report = host
            .call(Caller::Agent, "health.check", json!({ "id": id }))
            .unwrap();
        let issues = report["issues"].as_array().unwrap();
        let issue = issues
            .iter()
            .find(|i| i["kind"] == "remote-assets")
            .expect("should have remote-assets");
        assert_eq!(issue["level"], "info");
        assert_eq!(issue["items"], json!([url]));
        assert_eq!(issue["fix"], "vendor-deps");

        let _ = std::fs::remove_dir_all(&vault);
    }
    #[test]
    fn remote_assets_ignores_preconnect() {
        let vault = temp_vault("remote-preconnect");
        let host = open(&vault);

        let css_url = "https://fonts.googleapis.com/css2?family=Roboto";
        let html = format!(
            r#"<!doctype html><html><head><title>Fonts</title>
            <link rel="preconnect" href="https://fonts.googleapis.com">
            <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
            <link href="{css_url}" rel="stylesheet"></head>
            <body>ok</body></html>"#
        );
        let page = host
            .call(Caller::Ui, "pages.create", json!({ "html": html }))
            .unwrap();
        let id = page["id"].as_str().unwrap();

        let report = host
            .call(Caller::Agent, "health.check", json!({ "id": id }))
            .unwrap();
        let issues = report["issues"].as_array().unwrap();
        let issue = issues
            .iter()
            .find(|i| i["kind"] == "remote-assets")
            .expect("should have remote-assets");
        assert_eq!(issue["items"], json!([css_url]));

        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn render_server_health() {
        let vault = temp_vault("render");
        let host = open(&vault);

        let src = crate::reserved::render_src("plantuml", "A->B").unwrap();
        let html = format!(
            r#"<!doctype html><html><head><title>Render</title></head>
            <body><img src="{src}"></body></html>"#
        );
        let page = host
            .call(Caller::Ui, "pages.create", json!({ "html": html }))
            .unwrap();
        let id = page["id"].as_str().unwrap();

        let report1 = host
            .call(Caller::Agent, "health.check", json!({ "id": id }))
            .unwrap();
        let issues1 = report1["issues"].as_array().unwrap();
        let issue1 = issues1
            .iter()
            .find(|i| i["kind"] == "needs-render-server")
            .expect("should have needs-render-server");
        assert_eq!(issue1["level"], "warn");
        assert_eq!(issue1["items"], json!(["plantuml"]));
        assert_eq!(issue1["fix"], "configure-render");

        let dot_herb = vault.join(".herbarium");
        std::fs::create_dir_all(&dot_herb).unwrap();
        let render_json = dot_herb.join("render.json");
        std::fs::write(&render_json, r#"{"krokiUrl":"https://k.example"}"#).unwrap();

        let report2 = host
            .call(Caller::Agent, "health.check", json!({ "id": id }))
            .unwrap();
        let issues2 = report2["issues"].as_array().unwrap();
        assert!(!issues2.iter().any(|i| i["kind"] == "needs-render-server"));

        std::fs::remove_file(&render_json).unwrap();
        let cache_file = crate::reserved::render_cache_path(&vault, "plantuml", "A->B");
        std::fs::create_dir_all(cache_file.parent().unwrap()).unwrap();
        std::fs::write(&cache_file, b"<svg>cached</svg>").unwrap();

        let report3 = host
            .call(Caller::Agent, "health.check", json!({ "id": id }))
            .unwrap();
        let issues3 = report3["issues"].as_array().unwrap();
        assert!(!issues3.iter().any(|i| i["kind"] == "needs-render-server"));

        let _ = std::fs::remove_dir_all(&vault);
    }
}
