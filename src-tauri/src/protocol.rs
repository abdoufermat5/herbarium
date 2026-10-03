// Serves page HTML over the `herbarium://` custom scheme inside the isolation
// iframe. Injects a Content-Security-Policy and never grants same-origin access.

use tauri::http::{Request, Response, StatusCode};
use tauri::{Manager, UriSchemeContext};

use crate::commands::AppState;

/// Network allowed: known CDN hosts for scripts/fonts, `https:` for assets.
const CSP_ALLOW: &str = "default-src 'none'; script-src 'unsafe-inline' https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com https://code.jquery.com; style-src 'unsafe-inline' https://fonts.googleapis.com https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com; font-src data: https://fonts.gstatic.com https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com; img-src https: data: blob:; media-src https: data: blob:; connect-src https://fonts.googleapis.com https://fonts.gstatic.com; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'";

/// Network blocked (per-page kill switch).
const CSP_BLOCK: &str = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; font-src data:; img-src data:; media-src data:; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'";

pub fn handle<R: tauri::Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let uri = request.uri();
    // `herbarium://page/{id}` parses as host="page", path="/{id}".
    // Also accept the path form `herbarium:///page/{id}` for robustness.
    let id = if uri.host() == Some("page") {
        uri.path().trim_matches('/')
    } else {
        uri.path()
            .strip_prefix("/page/")
            .unwrap_or("")
            .trim_end_matches('/')
    };
    if id.is_empty() || id.contains('/') {
        return not_found();
    }

    let state = ctx.app_handle().state::<AppState>();
    let Ok(host) = state.host.lock() else {
        return not_found();
    };
    let Some(store) = host.store() else {
        return not_found();
    };
    let Ok(Some(meta)) = store.get_meta(id) else {
        return not_found();
    };
    let html = herbarium_core::vault::read_html(&store.vault, id, meta.folder.as_deref())
        .unwrap_or("<p>Page file missing on disk.</p>".to_string());
    let csp = if meta.allow_cdn { CSP_ALLOW } else { CSP_BLOCK };

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html; charset=utf-8")
        .header("Content-Security-Policy", csp)
        .header("X-Content-Type-Options", "nosniff")
        .body(html.into_bytes())
        .unwrap_or_else(|_| not_found())
}

fn not_found() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header("Content-Type", "text/html; charset=utf-8")
        .body("<p>Page not found.</p>".into())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}