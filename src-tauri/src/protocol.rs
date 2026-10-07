// Serves page HTML over the `herbarium://` custom scheme inside the isolation
// iframe. Injects a Content-Security-Policy and never grants same-origin access.

use std::borrow::Cow;
#[cfg(test)]
use std::path::{Path, PathBuf};

pub use herbarium_core::assets::{mime_for_path, resolve_safe_asset_path};
use percent_encoding::percent_decode_str;
use tauri::http::{Request, Response, StatusCode, Uri};
use tauri::{AppHandle, Manager, UriSchemeContext, UriSchemeResponder};

use crate::commands::AppState;

/// Network allowed: known CDN hosts for scripts/fonts, `https:` for assets.
pub const CSP_ALLOW: &str = "default-src 'none'; script-src 'self' 'unsafe-inline' 'unsafe-eval' 'wasm-unsafe-eval' https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com https://code.jquery.com; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com; font-src 'self' data: https://fonts.gstatic.com https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com; img-src 'self' https: data: blob:; media-src 'self' https: data: blob:; connect-src 'self' https://fonts.googleapis.com https://fonts.gstatic.com https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com https://code.jquery.com https://esm.sh; worker-src blob: 'self'; frame-src 'none'; object-src 'none'; base-uri 'self'; form-action 'none'";

/// Network blocked (per-page kill switch).
pub const CSP_BLOCK: &str = "default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; font-src 'self' data:; img-src 'self' data: blob:; media-src 'self' data: blob:; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'self'; form-action 'none'";

const FRAGMENT: &percent_encoding::AsciiSet = &percent_encoding::CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'/')
    .add(b'\\')
    .add(b'?')
    .add(b'#');

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ParsedRequest {
    pub page_id: String,
    pub relative_path: Option<String>,
}

/// Answer off the UI thread: the host lock can be held by a long rescan, and
/// the synchronous scheme callback runs on the GTK main thread.
pub fn handle<R: tauri::Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || responder.respond(serve(&app, &request)));
}

/// Percent-decode one path segment exactly once. `None` for invalid UTF-8, an
/// exact `.`/`..` component, an empty segment, or a decoded separator/NUL.
/// A `..` inside a longer name (`release..notes.html`) is a valid filename.
fn decode_segment(raw: &str) -> Option<Cow<'_, str>> {
    let decoded = percent_decode_str(raw).decode_utf8().ok()?;
    if decoded.is_empty()
        || decoded == "."
        || decoded == ".."
        || decoded.contains(['/', '\\', '\0'])
    {
        return None;
    }
    Some(decoded)
}

pub fn validate_relative_path(raw_rel: &str) -> Option<String> {
    if raw_rel.starts_with('/') || raw_rel.starts_with('\\') {
        return None;
    }
    if raw_rel.ends_with('/') || raw_rel.ends_with('\\') {
        return None;
    }

    let mut out = String::with_capacity(raw_rel.len());
    for (i, seg) in raw_rel.split('/').enumerate() {
        let decoded = decode_segment(seg)?;
        if decoded.starts_with('.')
            || decoded.contains(':')
            || decoded.to_ascii_lowercase().ends_with(".json")
        {
            return None;
        }
        if i > 0 {
            out.push('/');
        }
        out.push_str(&decoded);
    }
    Some(out)
}

fn raw_page_path(uri: &Uri) -> Option<&str> {
    if uri.host() == Some("page") {
        Some(uri.path())
    } else if uri.host() == Some("herbarium.localhost")
        || uri.host().is_none()
        || uri.host() == Some("")
    {
        uri.path().strip_prefix("/page")
    } else {
        None
    }
}

pub fn parse_uri_and_referer(uri: &Uri, referer: Option<&str>) -> Option<ParsedRequest> {
    let raw_path = raw_page_path(uri)?;

    let trimmed = raw_path.trim_start_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    let (first_seg, rest) = match trimmed.split_once('/') {
        Some((f, r)) => (f, Some(r)),
        None => (trimmed, None),
    };

    let rest = rest.filter(|r| !r.is_empty());

    let decoded_first = decode_segment(first_seg)?;

    if let Some(rel) = rest {
        let validated_rel = validate_relative_path(rel)?;
        return Some(ParsedRequest {
            page_id: decoded_first.into_owned(),
            relative_path: Some(validated_rel),
        });
    }

    if let Some((ref_id, validated_rel)) = referer
        .and_then(extract_page_id_from_referer)
        .filter(|id| id != decoded_first.as_ref())
        .zip(validate_relative_path(first_seg))
    {
        return Some(ParsedRequest {
            page_id: ref_id,
            relative_path: Some(validated_rel),
        });
    }

    Some(ParsedRequest {
        page_id: decoded_first.into_owned(),
        relative_path: None,
    })
}

pub fn extract_page_id_from_referer(referer: &str) -> Option<String> {
    let uri: Uri = referer.parse().ok()?;
    let raw_path = raw_page_path(&uri)?;
    let trimmed = raw_path.trim_start_matches('/');
    let id_part = trimmed.split('/').next()?;
    decode_segment(id_part).map(Cow::into_owned)
}

pub fn inject_base_tag(html: &str, base_href: &str) -> String {
    let base_tag = format!("<base href=\"{base_href}\">");
    let lower = html.to_ascii_lowercase();

    if let Some(insert_at) = lower
        .find("<head")
        .and_then(|pos| lower[pos..].find('>').map(|close| pos + close + 1))
    {
        let mut result = String::with_capacity(html.len() + base_tag.len() + 2);
        result.push_str(&html[..insert_at]);
        result.push('\n');
        result.push_str(&base_tag);
        result.push_str(&html[insert_at..]);
        return result;
    }

    format!("{base_tag}\n{html}")
}

/// Escape a JSON document for embedding inside an inline `<script>`. Only `<`
/// can end the script early; U+2028/U+2029 are valid JSON but illegal in
/// JavaScript source, so all three become `\uXXXX` escapes.
fn escape_script_json(json: &str) -> String {
    json.replace('<', "\\u003c")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// The state a page may read, normalised to `{local, personal, shared}` string
/// maps. Unknown areas and non-string values are dropped.
fn storage_state_json(storage: Option<&serde_json::Value>) -> String {
    let mut out = serde_json::Map::new();
    for area in ["local", "personal", "shared"] {
        let mut map = serde_json::Map::new();
        if let Some(entries) = storage
            .and_then(|v| v.get(area))
            .and_then(|v| v.as_object())
        {
            for (key, value) in entries {
                if let Some(value) = value.as_str() {
                    map.insert(key.clone(), serde_json::Value::String(value.to_string()));
                }
            }
        }
        out.insert(area.to_string(), serde_json::Value::Object(map));
    }
    serde_json::to_string(&serde_json::Value::Object(out)).unwrap_or_else(|_| "{}".into())
}

/// Client-side shim injected into every served page, right after the base tag
/// and before any page script. It exposes a Storage-compatible `localStorage`,
/// a memory-only `sessionStorage`, and the Claude-artifact `window.storage`
/// API, backed by the state embedded in `window.__herbariumState`. Writes are
/// coalesced per microtask and posted to the reader, which owns the page id
/// and persists them into the sidecar.
const SHIM_JS: &str = include_str!("storage_shim.js");

/// Recall mode: hides `data-herbarium-recall` answers until revealed. Injected
/// after the storage shim when the page URL carries `recall=1`.
const RECALL_JS: &str = include_str!("recall.js");

/// Lets the reader list the page's headings, jump to one and set the zoom.
/// Injected into every served page, after the storage shim.
const BRIDGE_JS: &str = include_str!("reader_bridge.js");

/// True when the request asks for recall mode (`?recall=1`).
fn wants_recall(uri: &Uri) -> bool {
    uri.query()
        .is_some_and(|q| q.split('&').any(|pair| pair == "recall=1"))
}

/// Inject the storage shim (and, with `recall`, the recall script) immediately
/// after the base tag. `state_json` is the normalised page state (see
/// [`storage_state_json`]).
fn inject_storage_shim(html: &str, state_json: &str, recall: bool) -> String {
    let recall_script = if recall {
        format!("<script>{RECALL_JS}</script>")
    } else {
        String::new()
    };
    let script = format!(
        "<script>window.__herbariumState={};window.__herbariumPersist=true;{SHIM_JS}</script><script>{BRIDGE_JS}</script>{recall_script}",
        escape_script_json(state_json)
    );
    if let Some(base_at) = html.find("<base ")
        && let Some(rel) = html[base_at..].find('>')
    {
        let at = base_at + rel + 1;
        let mut out = String::with_capacity(html.len() + script.len() + 1);
        out.push_str(&html[..at]);
        out.push('\n');
        out.push_str(&script);
        out.push_str(&html[at..]);
        return out;
    }
    format!("{script}\n{html}")
}

fn percent_encode_segment(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, FRAGMENT).to_string()
}

fn serve<R: tauri::Runtime>(
    app: &AppHandle<R>,
    request: &Request<Vec<u8>>,
) -> Response<Cow<'static, [u8]>> {
    let uri = request.uri();
    let referer = request
        .headers()
        .get("referer")
        .and_then(|v| v.to_str().ok());

    let parsed = match parse_uri_and_referer(uri, referer) {
        Some(p) => p,
        None => return not_found(),
    };

    // Step 1: Query Host/Store with early lock release
    let lookup_result = {
        let state = app.state::<AppState>();
        let Ok(host) = state.host.lock() else {
            return not_found();
        };
        let Some(store) = host.store() else {
            return not_found();
        };
        match store.get_meta(&parsed.page_id) {
            Ok(Some(meta)) => Some((
                store.vault.clone(),
                meta.folder.clone(),
                meta.allow_cdn,
                meta.ext.get("storage").cloned(),
            )),
            Ok(None) => None,
            Err(_) => return not_found(),
        }
    }; // host lock dropped HERE, before disk I/O!
    let (vault, folder, allow_cdn, storage) = match lookup_result {
        Some(info) => info,
        None => return not_found(),
    };

    // Step 2: Serve asset or page HTML (with NO host lock held!)
    if let Some(rel) = parsed.relative_path {
        // Asset request
        let html_path =
            herbarium_core::vault::html_path(&vault, &parsed.page_id, folder.as_deref());
        let page_folder = match html_path.parent() {
            Some(p) => p,
            None => &vault,
        };
        let file_path = match resolve_safe_asset_path(&vault, page_folder, &rel) {
            Some(p) => p,
            None => return not_found(),
        };

        let bytes = match std::fs::read(&file_path) {
            Ok(b) => b,
            Err(_) => return not_found(),
        };

        let mime = mime_for_path(&file_path);

        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", mime)
            .header("X-Content-Type-Options", "nosniff")
            .body(Cow::Owned(bytes))
            .unwrap_or_else(|_| not_found())
    } else {
        // Page HTML request
        let raw_html =
            match herbarium_core::vault::read_html(&vault, &parsed.page_id, folder.as_deref()) {
                Ok(h) => h,
                Err(_) => return not_found(), // Missing file returns 404
            };

        let base_href = if uri.host() == Some("herbarium.localhost") {
            format!(
                "http://herbarium.localhost/page/{}/",
                percent_encode_segment(&parsed.page_id)
            )
        } else {
            format!(
                "herbarium://page/{}/",
                percent_encode_segment(&parsed.page_id)
            )
        };
        let html = inject_base_tag(&raw_html, &base_href);
        let html = inject_storage_shim(
            &html,
            &storage_state_json(storage.as_ref()),
            wants_recall(uri),
        );

        let csp = if allow_cdn { CSP_ALLOW } else { CSP_BLOCK };

        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html; charset=utf-8")
            .header("Content-Security-Policy", csp)
            .header("X-Content-Type-Options", "nosniff")
            .body(Cow::Owned(html.into_bytes()))
            .unwrap_or_else(|_| not_found())
    }
}

pub fn not_found() -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(Cow::Borrowed(b"<p>Page not found.</p>".as_slice()))
        .unwrap_or_else(|_| Response::new(Cow::Borrowed(b"".as_slice())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(uri: &str) -> Option<ParsedRequest> {
        parse_uri_and_referer(&uri.parse().unwrap(), None)
    }

    #[test]
    fn page_id_is_percent_decoded_and_cannot_hold_a_path() {
        let req = parse("herbarium://page/Mon%20cours%20%C3%A9t%C3%A9?v=2").unwrap();
        assert_eq!(req.page_id, "Mon cours été");
        assert_eq!(req.relative_path, None);

        let req = parse("http://herbarium.localhost/page/abc/").unwrap();
        assert_eq!(req.page_id, "abc");
        assert_eq!(req.relative_path, None);

        assert_eq!(parse("herbarium://page/a%2F..%2Fb"), None);
        assert_eq!(parse("herbarium://page/%FF"), None);
        assert_eq!(parse("herbarium://other/abc"), None);
    }

    #[test]
    fn parse_with_referer_routes_relative_asset() {
        let req = parse_uri_and_referer(
            &"herbarium://page/style.css".parse().unwrap(),
            Some("herbarium://page/my-page/"),
        )
        .unwrap();
        assert_eq!(req.page_id, "my-page");
        assert_eq!(req.relative_path.as_deref(), Some("style.css"));
    }

    #[test]
    fn asset_paths_are_parsed_and_percent_decoded() {
        let req = parse("herbarium://page/abc/style.css").unwrap();
        assert_eq!(req.page_id, "abc");
        assert_eq!(req.relative_path.as_deref(), Some("style.css"));

        let req = parse("herbarium://page/abc/assets/images/logo.png").unwrap();
        assert_eq!(req.page_id, "abc");
        assert_eq!(req.relative_path.as_deref(), Some("assets/images/logo.png"));

        let req = parse("http://herbarium.localhost/page/my-page/theme.css").unwrap();
        assert_eq!(req.page_id, "my-page");
        assert_eq!(req.relative_path.as_deref(), Some("theme.css"));

        let req = parse("herbarium://page/my%20page/my%20image.png").unwrap();
        assert_eq!(req.page_id, "my page");
        assert_eq!(req.relative_path.as_deref(), Some("my image.png"));
    }

    #[test]
    fn rejects_traversal_attacks() {
        assert_eq!(parse("herbarium://page/abc/.."), None);
        assert_eq!(parse("herbarium://page/abc/../secret.txt"), None);
        assert_eq!(parse("herbarium://page/abc/%2e%2e"), None);
        assert_eq!(parse("herbarium://page/abc/%2e%2e/secret.txt"), None);
        assert_eq!(parse("herbarium://page/abc/%2E%2E/secret.txt"), None);
        assert_eq!(parse("herbarium://page/abc/a%2F..%2Fb"), None);
        assert_eq!(parse("herbarium://page/abc/assets/..%2fsecret"), None);
        assert_eq!(parse("herbarium://page/abc/assets//logo.png"), None);
    }

    #[test]
    fn rejects_hidden_segments() {
        assert_eq!(parse("herbarium://page/abc/.herbarium"), None);
        assert_eq!(parse("herbarium://page/abc/.git/config"), None);
        assert_eq!(parse("herbarium://page/abc/.env"), None);
        assert_eq!(parse("herbarium://page/abc/assets/.secret.png"), None);
    }

    #[test]
    fn rejects_json_sidecars() {
        assert_eq!(parse("herbarium://page/abc/page.json"), None);
        assert_eq!(parse("herbarium://page/abc/abc.JSON"), None);
        assert_eq!(parse("herbarium://page/abc/assets/data.json"), None);
    }

    #[test]
    fn extract_page_id_from_referer_header() {
        assert_eq!(
            extract_page_id_from_referer("herbarium://page/my-doc?v=1").as_deref(),
            Some("my-doc")
        );
        assert_eq!(
            extract_page_id_from_referer("http://herbarium.localhost/page/my-doc/").as_deref(),
            Some("my-doc")
        );
        assert_eq!(
            extract_page_id_from_referer("herbarium://page/my%20doc").as_deref(),
            Some("my doc")
        );
        assert_eq!(extract_page_id_from_referer("herbarium://page/.."), None);
    }

    #[test]
    fn mime_by_extension_resolution() {
        assert_eq!(
            mime_for_path(Path::new("file.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            mime_for_path(Path::new("file.css")),
            "text/css; charset=utf-8"
        );
        assert_eq!(
            mime_for_path(Path::new("file.js")),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            mime_for_path(Path::new("file.mjs")),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(mime_for_path(Path::new("file.png")), "image/png");
        assert_eq!(mime_for_path(Path::new("file.jpg")), "image/jpeg");
        assert_eq!(mime_for_path(Path::new("file.svg")), "image/svg+xml");
        assert_eq!(mime_for_path(Path::new("file.woff2")), "font/woff2");
        assert_eq!(mime_for_path(Path::new("file.wasm")), "application/wasm");
        assert_eq!(
            mime_for_path(Path::new("file.xyz")),
            "application/octet-stream"
        );
    }

    #[test]
    fn csp_contains_required_directives() {
        assert!(CSP_ALLOW.contains("'unsafe-eval'"));
        assert!(CSP_ALLOW.contains("'wasm-unsafe-eval'"));
        assert!(CSP_ALLOW.contains("worker-src blob: 'self'"));
        assert!(CSP_ALLOW.contains("https://esm.sh"));
        assert!(CSP_ALLOW.contains("img-src 'self'"));
        assert!(CSP_ALLOW.contains("style-src 'self'"));
        assert!(CSP_ALLOW.contains("font-src 'self'"));
        assert!(CSP_ALLOW.contains("media-src 'self'"));
        assert!(CSP_ALLOW.contains("base-uri 'self'"));

        assert!(CSP_BLOCK.contains("img-src 'self'"));
        assert!(CSP_BLOCK.contains("style-src 'self'"));
        assert!(CSP_BLOCK.contains("font-src 'self'"));
        assert!(CSP_BLOCK.contains("media-src 'self'"));
        assert!(CSP_BLOCK.contains("connect-src 'none'"));
        assert!(CSP_BLOCK.contains("base-uri 'self'"));
    }

    #[test]
    fn base_tag_injection_behavior() {
        let html_head = "<html><head><title>Test</title></head><body>Hello</body></html>";
        let injected = inject_base_tag(html_head, "herbarium://page/my-page/");
        assert!(injected.contains("<head>\n<base href=\"herbarium://page/my-page/\">"));

        let html_no_head = "<div>Snippet</div>";
        let injected_no_head = inject_base_tag(html_no_head, "herbarium://page/my-page/");
        assert!(injected_no_head.starts_with("<base href=\"herbarium://page/my-page/\">"));
    }

    #[test]
    fn storage_shim_runs_after_the_base_tag() {
        let html = inject_base_tag(
            "<html><head><title>t</title></head><body>x</body></html>",
            "herbarium://page/p/",
        );
        let out = inject_storage_shim(&html, r#"{"local":{},"personal":{},"shared":{}}"#, false);
        let base_at = out.find("<base ").expect("base tag");
        let script_at = out
            .find("<script>window.__herbariumState=")
            .expect("shim script");
        assert!(
            base_at < script_at,
            "the shim must come after the base tag, before page scripts"
        );
        assert!(out.contains("window.__herbariumPersist=true"));
    }

    #[test]
    fn recall_script_is_injected_only_when_asked_and_after_the_shim() {
        let html = inject_base_tag(
            "<html><head><title>T</title></head><body><p data-herbarium-recall>A</p></body></html>",
            "herbarium://page/p/",
        );
        let state = r#"{"local":{},"personal":{},"shared":{}}"#;
        let plain = inject_storage_shim(&html, state, false);
        assert!(!plain.contains(RECALL_JS), "no recall script unless asked");
        let recall = inject_storage_shim(&html, state, true);
        let shim_at = recall.find("__herbariumPersist").unwrap();
        let recall_at = recall.find(RECALL_JS).unwrap();
        let title_at = recall.find("<title>").unwrap();
        assert!(shim_at < recall_at && recall_at < title_at, "{recall}");
        assert!(
            !RECALL_JS.contains("</script"),
            "the script cannot close its own tag"
        );
        assert!(!BRIDGE_JS.contains("</script"));
        assert!(
            plain.contains("herbarium:headings"),
            "the bridge is always injected"
        );

        let uri = |s: &str| s.parse::<Uri>().unwrap();
        assert!(wants_recall(&uri("herbarium://page/p?v=3&recall=1")));
        assert!(wants_recall(&uri("herbarium://page/p?recall=1")));
        assert!(!wants_recall(&uri("herbarium://page/p?v=3")));
        assert!(!wants_recall(&uri("herbarium://page/p?recall=10")));
        assert!(!wants_recall(&uri("herbarium://page/p")));
    }

    #[test]
    fn storage_shim_escapes_script_breaks_and_line_separators() {
        let state = "{\"local\":{\"a\":\"</script><img>\",\"b\":\"x\u{2028}y\u{2029}z\"},\"personal\":{},\"shared\":{}}";
        let out = inject_storage_shim("<base href=\"herbarium://page/p/\">", state, false);
        assert!(!out.contains("</script><img>"), "raw `<` must be escaped");
        assert!(out.contains("\\u003c/script>"));
        assert!(!out.contains('\u{2028}'));
        assert!(!out.contains('\u{2029}'));
        assert!(out.contains("\\u2028"));
        assert!(out.contains("\\u2029"));
        // The document still ends with our own closing script tag.
        assert!(out.trim_end().ends_with("</script>"));
    }

    #[test]
    fn storage_state_json_keeps_only_string_maps_in_known_areas() {
        let ext = serde_json::json!({
            "local": { "a": "1", "n": 2 },
            "personal": {},
            "shared": { "c": "3" },
            "other": { "x": "y" }
        });
        let parsed: serde_json::Value =
            serde_json::from_str(&storage_state_json(Some(&ext))).unwrap();
        assert_eq!(parsed["local"]["a"], "1");
        assert!(
            parsed["local"].get("n").is_none(),
            "non-strings are dropped"
        );
        assert_eq!(parsed["shared"]["c"], "3");
        assert!(parsed.get("other").is_none());

        let empty: serde_json::Value = serde_json::from_str(&storage_state_json(None)).unwrap();
        assert_eq!(
            empty,
            serde_json::json!({ "local": {}, "personal": {}, "shared": {} })
        );
    }

    #[test]
    fn safe_asset_path_disk_validation() {
        let unique = format!(
            "herbarium_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(temp_dir.join("sub")).unwrap();

        let valid_file = temp_dir.join("sub").join("test.png");
        std::fs::write(&valid_file, b"fake png").unwrap();

        let json_file = temp_dir.join("sub").join("sidecar.json");
        std::fs::write(&json_file, b"{}").unwrap();

        #[cfg(unix)]
        let symlink_file = temp_dir.join("sym.png");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&valid_file, &symlink_file).unwrap();

        // 1. Valid file resolves
        assert!(resolve_safe_asset_path(&temp_dir, &temp_dir, "sub/test.png").is_some());

        // 2. Sidecar .json is rejected
        assert_eq!(
            resolve_safe_asset_path(&temp_dir, &temp_dir, "sub/sidecar.json"),
            None
        );

        // 3. Symlink is rejected
        #[cfg(unix)]
        assert_eq!(
            resolve_safe_asset_path(&temp_dir, &temp_dir, "sym.png"),
            None
        );

        // 4. Missing file returns None
        assert_eq!(
            resolve_safe_asset_path(&temp_dir, &temp_dir, "sub/missing.png"),
            None
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn double_dots_inside_filenames_are_served_but_components_are_not() {
        let req = parse("herbarium://page/abc/release..notes.png").unwrap();
        assert_eq!(req.relative_path.as_deref(), Some("release..notes.png"));
        let req = parse("herbarium://page/release..notes").unwrap();
        assert_eq!(req.page_id, "release..notes");
        assert_eq!(parse("herbarium://page/abc/a/%2e%2e/b"), None);
        assert_eq!(parse("herbarium://page/abc/a%5Cb"), None);

        let dir = tempfile_dir("dots");
        std::fs::write(dir.join("release..notes.png"), b"x").unwrap();
        assert!(resolve_safe_asset_path(&dir, &dir, "release..notes.png").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn page_folder_swapped_for_outside_symlink_is_rejected() {
        let root = tempfile_dir("swap");
        let vault = root.join("vault");
        let outside = root.join("outside");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.png"), b"secret").unwrap();
        std::os::unix::fs::symlink(&outside, vault.join("folder")).unwrap();

        assert_eq!(
            resolve_safe_asset_path(&vault, &vault.join("folder"), "secret.png"),
            None
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_subdirectory_cannot_escape_vault() {
        let root = tempfile_dir("nested");
        let vault = root.join("vault");
        let outside = root.join("outside");
        std::fs::create_dir_all(vault.join("assets")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(vault.join("assets").join("ok.png"), b"ok").unwrap();
        std::fs::write(outside.join("secret.png"), b"secret").unwrap();
        std::os::unix::fs::symlink(&outside, vault.join("assets").join("link")).unwrap();

        // A real in-vault asset still resolves.
        assert!(resolve_safe_asset_path(&vault, &vault.join("assets"), "ok.png").is_some());
        // Anything reached through the escaping directory symlink is refused.
        assert_eq!(
            resolve_safe_asset_path(&vault, &vault.join("assets"), "link/secret.png"),
            None
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn sibling_directory_with_shared_prefix_is_not_inside_vault() {
        let root = tempfile_dir("prefix");
        let vault = root.join("vault");
        let sibling = root.join("vault-evil");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(sibling.join("secret.png"), b"secret").unwrap();

        assert_eq!(
            resolve_safe_asset_path(&vault, &sibling, "secret.png"),
            None
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    fn tempfile_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "herbarium_proto_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
