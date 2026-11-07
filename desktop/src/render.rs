// Server-side diagram rendering via Kroki (PlantUML, Graphviz, D2, etc.).
// Diagram source travels in the URL, results are cached in
// `.herbarium/cache/render/<key>.svg`, and the diagram server address is
// configured per vault.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use herbarium_core::reserved::{self, Reserved};

use crate::commands::CmdResult;

fn build_client() -> Result<reqwest::blocking::Client, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("Herbarium/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("could not create HTTP client: {e}"))
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let mut rest = bytes;
    if rest.starts_with(b"\xef\xbb\xbf") {
        rest = &rest[3..];
    }
    let s = String::from_utf8_lossy(rest);
    let s_trimmed = s.trim_start();
    let after_prolog = if s_trimmed.starts_with("<?xml") {
        if let Some(end) = s_trimmed.find("?>") {
            s_trimmed[end + 2..].trim_start()
        } else {
            s_trimmed
        }
    } else {
        s_trimmed
    };
    let prefix = if after_prolog.len() > 1024 {
        &after_prolog[..1024]
    } else {
        after_prolog
    };
    prefix.to_ascii_lowercase().contains("<svg")
}

fn post_svg(
    client: &reqwest::blocking::Client,
    base: &str,
    kind: &str,
    source: &str,
) -> Result<Vec<u8>, String> {
    let base = base.trim_end_matches('/');
    let url = format!("{base}/{kind}/svg");
    let req = client
        .post(&url)
        .header("content-type", "text/plain")
        .body(source.to_string());
    let mut resp = req
        .send()
        .map_err(|e| format!("Couldn't reach {base}: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let mut err_bytes = Vec::new();
        let _ = resp.by_ref().take(4096).read_to_end(&mut err_bytes);
        let text = String::from_utf8_lossy(&err_bytes);
        let first_line = text
            .lines()
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("diagram server returned an error");
        return Err(first_line.to_string());
    }

    const MAX_SIZE: u64 = 5 * 1024 * 1024;
    if let Some(len) = resp.content_length()
        && len > MAX_SIZE
    {
        return Err("diagram output exceeds 5 MiB limit".into());
    }
    let mut bytes = Vec::new();
    resp.by_ref()
        .take(MAX_SIZE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("failed reading diagram response: {e}"))?;
    if bytes.len() > MAX_SIZE as usize {
        return Err("diagram output exceeds 5 MiB limit".into());
    }

    if !looks_like_svg(&bytes) {
        return Err("The server answered but not with an SVG".into());
    }

    Ok(bytes)
}

fn read_cache(vault: &Path, kind: &str, source: &str) -> Option<Vec<u8>> {
    let kind = reserved::render_kind(kind)?;
    let r = Reserved::Render {
        kind,
        source: source.to_owned(),
    };
    reserved::resolve(vault, &r).map(|(_, bytes)| bytes.into_owned())
}

/// Ensure a diagram of `kind` with `source` is rendered to SVG bytes.
/// Returns cached bytes if available, else queries the vault's diagram server.
pub fn ensure(vault: &Path, kind: &str, source: &str) -> Result<Vec<u8>, String> {
    if let Some(bytes) = read_cache(vault, kind, source) {
        return Ok(bytes);
    }
    let url = herbarium_core::kroki_url(vault).ok_or_else(|| "no diagram server".to_string())?;
    ensure_from_url(vault, &url, kind, source)
}

pub(crate) fn ensure_from_url(
    vault: &Path,
    url: &str,
    kind: &str,
    source: &str,
) -> Result<Vec<u8>, String> {
    if let Some(bytes) = read_cache(vault, kind, source) {
        return Ok(bytes);
    }
    let client = build_client()?;
    let bytes = post_svg(&client, url, kind, source)?;
    let cache_path = reserved::render_cache_path(vault, kind, source);
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = herbarium_core::vault::write_atomic(&cache_path, &bytes) {
        tracing::warn!(
            "failed caching diagram render to {}: {e}",
            cache_path.display()
        );
    }
    Ok(bytes)
}

/// Prefetch all `_herbarium/render/...` references in asset attributes of `html`,
/// populating the vault cache so subsequent requests and exports are offline.
pub fn prefetch(vault: &Path, html: &str) {
    let mut seen = std::collections::HashSet::new();
    herbarium_core::rewrite::rewrite_attrs(html, |tag, attr, val| {
        if herbarium_core::rewrite::is_asset_attr(tag, attr)
            && let Some(rel) = herbarium_core::rewrite::relative_path(val)
            && let Some(herbarium_core::reserved::Reserved::Render { kind, source }) =
                herbarium_core::reserved::parse(rel)
            && seen.insert((kind, source.clone()))
            && let Err(err) = ensure(vault, kind, &source)
        {
            tracing::warn!("prefetch render failed for {kind}: {err}");
        }
        None
    });
}

/// Test a diagram server URL by rendering a tiny Graphviz diagram.
#[tauri::command]
pub async fn render_test(url: String) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return Err("diagram server URL cannot be empty".into());
        }
        let client = build_client()?;
        post_svg(&client, trimmed, "graphviz", "digraph{a->b}").map(|_| ())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    fn temp_vault(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "herbarium-test-render-{tag}-{}-{}",
            std::process::id(),
            herbarium_core::time::now_ms()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    enum ServerResponse {
        Svg(&'static str),
        Error(u16, &'static str),
        ErrorOwned(u16, String),
        NonSvg(&'static str),
    }

    fn spawn_fake_server(resp: ServerResponse) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_clone = Arc::clone(&hits);

        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                hits_clone.fetch_add(1, Ordering::SeqCst);
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let response = match &resp {
                    ServerResponse::Svg(body) => format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: image/svg+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    ),
                    ServerResponse::Error(code, body) => format!(
                        "HTTP/1.1 {code} Bad Request\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    ),
                    ServerResponse::ErrorOwned(code, body) => format!(
                        "HTTP/1.1 {code} Bad Request\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    ),
                    ServerResponse::NonSvg(body) => format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    ),
                };
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });

        (format!("http://127.0.0.1:{port}"), hits)
    }

    #[test]
    fn fake_server_success_and_cache_hit() {
        let vault = temp_vault("success");
        let (url, hits) = spawn_fake_server(ServerResponse::Svg(
            "<svg viewBox='0 0 10 10'><circle r='5'/></svg>",
        ));

        let bytes = ensure_from_url(&vault, &url, "graphviz", "digraph{a->b}").unwrap();
        assert_eq!(bytes, b"<svg viewBox='0 0 10 10'><circle r='5'/></svg>");
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        // Cache hit avoids network request
        let cached = ensure_from_url(&vault, &url, "graphviz", "digraph{a->b}").unwrap();
        assert_eq!(cached, bytes);
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn fake_server_error_400() {
        let vault = temp_vault("error-400");
        let (url, hits) = spawn_fake_server(ServerResponse::Error(
            400,
            "Syntax error in graph\nLine 2 details",
        ));

        let err = ensure_from_url(&vault, &url, "graphviz", "bad graph").unwrap_err();
        assert_eq!(err, "Syntax error in graph");
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn fake_server_non_svg_200() {
        let vault = temp_vault("non-svg");
        let (url, hits) = spawn_fake_server(ServerResponse::NonSvg(
            "<html><body>Not an SVG</body></html>",
        ));

        let err = ensure_from_url(&vault, &url, "graphviz", "digraph{a->b}").unwrap_err();
        assert_eq!(err, "The server answered but not with an SVG");
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn ensure_without_diagram_server() {
        let vault = temp_vault("no-server");
        let err = ensure(&vault, "graphviz", "digraph{a->b}").unwrap_err();
        assert_eq!(err, "no diagram server");

        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn fake_server_error_capped_at_4096() {
        let vault = temp_vault("error-capped");
        let long_body = "E".repeat(8192);
        let (url, hits) = spawn_fake_server(ServerResponse::ErrorOwned(500, long_body));

        let err = ensure_from_url(&vault, &url, "graphviz", "bad graph").unwrap_err();
        assert_eq!(err.len(), 4096);
        assert_eq!(err, "E".repeat(4096));
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    #[cfg(unix)]
    fn cache_symlink_is_refused() {
        use std::os::unix::fs::symlink;
        let vault = temp_vault("symlink");
        let outside = temp_vault("symlink-outside");
        let secret = outside.join("secret.svg");
        std::fs::write(&secret, b"<svg>secret</svg>").unwrap();

        let cache_path = reserved::render_cache_path(&vault, "graphviz", "digraph{a->b}");
        std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
        symlink(&secret, &cache_path).unwrap();

        // Ensure without server refuses the symlink rather than reading secret.svg from cache.
        let err = ensure(&vault, "graphviz", "digraph{a->b}").unwrap_err();
        assert_eq!(err, "no diagram server");

        // Ensure with server refuses the symlink, contacts server instead of reading secret.svg from cache.
        let (url, hits) = spawn_fake_server(ServerResponse::Svg(
            "<svg viewBox='0 0 10 10'><circle r='5'/></svg>",
        ));
        let bytes = ensure_from_url(&vault, &url, "graphviz", "digraph{a->b}").unwrap();
        assert_eq!(bytes, b"<svg viewBox='0 0 10 10'><circle r='5'/></svg>");
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let _ = std::fs::remove_dir_all(&vault);
        let _ = std::fs::remove_dir_all(&outside);
    }
}
