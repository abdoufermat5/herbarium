// Serves page HTML over the `herbarium://` custom scheme inside the isolation
// iframe. Injects a Content-Security-Policy and never grants same-origin access.

use std::borrow::Cow;

use percent_encoding::percent_decode_str;
use tauri::http::{Request, Response, StatusCode, Uri};
use tauri::{AppHandle, Manager, UriSchemeContext, UriSchemeResponder};

use crate::commands::AppState;

/// Network allowed: known CDN hosts for scripts/fonts, `https:` for assets.
const CSP_ALLOW: &str = "default-src 'none'; script-src 'unsafe-inline' https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com https://code.jquery.com; style-src 'unsafe-inline' https://fonts.googleapis.com https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com; font-src data: https://fonts.gstatic.com https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com; img-src https: data: blob:; media-src https: data: blob:; connect-src https://fonts.googleapis.com https://fonts.gstatic.com; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'";

/// Network blocked (per-page kill switch).
const CSP_BLOCK: &str = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; font-src data:; img-src data:; media-src data:; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'";

/// Answer off the UI thread: the host lock can be held by a long rescan, and
/// the synchronous scheme callback runs on the GTK main thread.
pub fn handle<R: tauri::Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || responder.respond(serve(&app, request.uri())));
}

fn serve<R: tauri::Runtime>(app: &AppHandle<R>, uri: &Uri) -> Response<Cow<'static, [u8]>> {
    let Some(id) = page_id(uri) else {
        return not_found();
    };
    let state = app.state::<AppState>();
    let Ok(host) = state.host.lock() else {
        return not_found();
    };
    let Some(store) = host.store() else {
        return not_found();
    };
    let Ok(Some(meta)) = store.get_meta(&id) else {
        return not_found();
    };
    let html = herbarium_core::vault::read_html(&store.vault, &id, meta.folder.as_deref())
        .unwrap_or("<p>Page file missing on disk.</p>".to_string());
    let csp = if meta.allow_cdn { CSP_ALLOW } else { CSP_BLOCK };

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html; charset=utf-8")
        .header("Content-Security-Policy", csp)
        .header("X-Content-Type-Options", "nosniff")
        .body(Cow::Owned(html.into_bytes()))
        .unwrap_or_else(|_| not_found())
}

/// The page id of `herbarium://page/{id}` (host="page", path="/{id}"), or of
/// the path form `http://herbarium.localhost/page/{id}` (how Windows WebView2
/// exposes custom schemes). The URL carries it percent-encoded
/// (file stems may hold spaces or accents); a decoded `/` is rejected.
fn page_id(uri: &Uri) -> Option<String> {
    let raw = if uri.host() == Some("page") {
        uri.path().trim_matches('/')
    } else {
        uri.path().strip_prefix("/page/")?.trim_end_matches('/')
    };
    let id = percent_decode_str(raw).decode_utf8().ok()?;
    (!id.is_empty() && !id.contains(['/', '\\'])).then(|| id.into_owned())
}

fn not_found() -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(Cow::Borrowed(b"<p>Page not found.</p>".as_slice()))
        .unwrap_or_else(|_| Response::new(Cow::Borrowed(b"".as_slice())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(uri: &str) -> Option<String> {
        page_id(&uri.parse().unwrap())
    }

    #[test]
    fn page_id_is_percent_decoded_and_cannot_hold_a_path() {
        assert_eq!(id("herbarium://page/Mon%20cours%20%C3%A9t%C3%A9?v=2").as_deref(), Some("Mon cours été"));
        assert_eq!(id("http://herbarium.localhost/page/abc/").as_deref(), Some("abc"));
        assert_eq!(id("herbarium://page/a%2F..%2Fb"), None);
        assert_eq!(id("herbarium://page/a/b"), None);
        assert_eq!(id("herbarium://page/%FF"), None);
        assert_eq!(id("herbarium://other/abc"), None);
    }
}
