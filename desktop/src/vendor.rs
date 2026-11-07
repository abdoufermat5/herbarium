// Vendoring: copy what a page loads from the internet into the vault
// (`.herbarium/deps/<key>/<file>`) and point the page at the copies
// (`_herbarium/deps/<key>/<file>`), so it renders with the network off.
//
// The network work (`Run`) is a plain struct over a `reqwest` client and a
// vault path, so tests can aim it at a local server. The Tauri command only
// reads the page, runs it, and saves the result through `pages.set_html`.
// The host lock is never held while downloading.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use herbarium_core::deps::{self, DepMeta, RefKind, RemoteRef};
use herbarium_core::{Caller, assets, libs, time};
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::{Value, json};
use tauri::Manager;

use crate::commands::{AppState, CmdResult};

const TIMEOUT: Duration = Duration::from_secs(20);
const MAX_FILE: u64 = 25 * 1024 * 1024;
const MAX_TOTAL: u64 = 100 * 1024 * 1024;
const MAX_FILES: usize = 400;
/// How deep imports (modules, stylesheets) are followed from the page.
const MAX_DEPTH: usize = 8;
const MAX_REDIRECTS: usize = 5;
const JS_MIME: &str = "text/javascript";
const LIB_PREFIX: &str = "_herbarium/lib/";

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Failed {
    pub url: String,
    pub reason: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorReport {
    /// Everything now stored in the vault for this page, nested modules included.
    pub vendored: Vec<String>,
    /// URLs served by a library bundled with the app instead of downloaded.
    pub bundled: Vec<String>,
    /// URLs that could not be copied; the page keeps the original reference.
    pub failed: Vec<Failed>,
    /// The page's metadata (after saving, when anything changed).
    pub meta: Value,
}

/// Only https is fetched. Tests may also use a local HTTP server.
fn allowed_url(url: &str) -> bool {
    let Ok(parsed) = tauri::Url::parse(url) else {
        return false;
    };
    parsed.scheme() == "https"
        || (cfg!(test)
            && parsed.scheme() == "http"
            && matches!(parsed.host_str(), Some("127.0.0.1" | "localhost")))
}

/// The HTTP client for one run. Redirects must stay on allowed schemes too.
fn http_client() -> Result<Client, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    Client::builder()
        .timeout(TIMEOUT)
        .user_agent(concat!("Herbarium/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.error("too many redirects")
            } else if !allowed_url(attempt.url().as_str()) {
                attempt.error("the server redirected to a link that is not https")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| format!("could not create HTTP client: {e}"))
}

/// `reference` made absolute against `base`, or `None` when it is not a web
/// link. Page and import references go through `deps::resolve_url` so they
/// key the same files as the page's own references; only tests (whose server
/// is plain HTTP) resolve by hand.
fn resolve(base: &str, reference: &str) -> Option<String> {
    if !cfg!(test) || base.starts_with("https://") {
        return deps::resolve_url(base, reference);
    }
    let mut url = tauri::Url::parse(base).ok()?.join(reference).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    url.set_fragment(None);
    Some(url.to_string())
}

fn url_path(url: &str) -> String {
    tauri::Url::parse(url)
        .map(|u| u.path().to_ascii_lowercase())
        .unwrap_or_default()
}

/// The error text for a failed request, ending at its root cause.
fn describe(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        return format!("timed out after {} seconds", TIMEOUT.as_secs());
    }
    let mut cause = None;
    let mut source = std::error::Error::source(e);
    while let Some(s) = source {
        cause = Some(s.to_string());
        source = s.source();
    }
    let reason = cause.unwrap_or_else(|| e.to_string());
    if e.is_connect() {
        format!("could not connect ({reason})")
    } else {
        reason
    }
}

/// `file` of the `_herbarium/lib/<file>` path `libs::map_cdn_url` returns.
fn lib_file(path: &str) -> &str {
    path.strip_prefix(LIB_PREFIX).unwrap_or(path)
}

fn dep_file(vault: &Path, key: &str, file: &str) -> PathBuf {
    vault.join(".herbarium").join("deps").join(key).join(file)
}

/// Where a stored dependency lives: its key and file name.
#[derive(Clone, Debug)]
struct Dep {
    key: String,
    file: String,
}

impl Dep {
    fn page_path(&self) -> String {
        deps::rel_path(&self.key, &self.file)
    }
}

/// What a run changed, for the report and the page rewrite.
struct Outcome {
    /// The reference as written in the page → page-relative path.
    map: HashMap<String, String>,
    vendored: Vec<String>,
    bundled: Vec<String>,
    failed: Vec<Failed>,
}

struct Run<'a> {
    client: &'a Client,
    vault: &'a Path,
    files: usize,
    bytes: u64,
    /// URL → result of vendoring it. Filled before a module's imports are
    /// followed, so an import cycle finds its own entry instead of looping.
    done: HashMap<String, Result<Dep, String>>,
    /// Stylesheets already flattened, and sub-resources already read, by URL.
    css_cache: HashMap<String, Result<String, String>>,
    data_cache: HashMap<String, Result<String, String>>,
    vendored: Vec<String>,
    bundled: Vec<String>,
    failed: Vec<Failed>,
}

impl<'a> Run<'a> {
    fn new(client: &'a Client, vault: &'a Path) -> Self {
        Run {
            client,
            vault,
            files: 0,
            bytes: 0,
            done: HashMap::new(),
            css_cache: HashMap::new(),
            data_cache: HashMap::new(),
            vendored: Vec::new(),
            bundled: Vec::new(),
            failed: Vec::new(),
        }
    }

    fn fail(&mut self, url: &str, reason: String) {
        tracing::warn!(url, reason, "could not vendor");
        if !self.failed.iter().any(|f| f.url == url) {
            self.failed.push(Failed {
                url: url.to_owned(),
                reason,
            });
        }
    }

    fn note_bundled(&mut self, url: &str) {
        if !self.bundled.iter().any(|u| u == url) {
            self.bundled.push(url.to_owned());
        }
    }

    /// Download `url` within the run's limits. Returns the bytes and a
    /// lowercase MIME type without parameters.
    fn download(&mut self, url: &str) -> Result<(Vec<u8>, String), String> {
        if !allowed_url(url) {
            return Err("only https links can be downloaded".into());
        }
        if self.files >= MAX_FILES {
            return Err(format!("more than {MAX_FILES} files in one run"));
        }
        let total_left = MAX_TOTAL.saturating_sub(self.bytes);
        if total_left == 0 {
            return Err("the run's 100 MiB limit was reached".into());
        }
        let limit = MAX_FILE.min(total_left);
        let too_big = || {
            if limit < MAX_FILE {
                "the run's 100 MiB limit was reached".to_string()
            } else {
                "the file is larger than 25 MiB".to_string()
            }
        };
        let response = self.client.get(url).send().map_err(|e| describe(&e))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("the server answered {}", status.as_u16()));
        }
        if response.content_length().is_some_and(|n| n > limit) {
            return Err(too_big());
        }
        let mut mime = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .map(|v| v.trim().to_ascii_lowercase())
            .unwrap_or_default();
        if mime.is_empty() || mime == "application/octet-stream" {
            mime = assets::mime_for_path(Path::new(&url_path(url))).to_owned();
        }
        let mut bytes = Vec::new();
        response
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("the download was interrupted ({e})"))?;
        if bytes.len() as u64 > limit {
            return Err(too_big());
        }
        self.files += 1;
        self.bytes += bytes.len() as u64;
        Ok((bytes, mime))
    }

    fn store(
        &self,
        url: &str,
        dep: &Dep,
        mime: &str,
        bytes: &[u8],
        module: bool,
        has_imports: bool,
    ) -> Result<(), String> {
        let meta = DepMeta {
            url: url.to_owned(),
            file: dep.file.clone(),
            mime: mime.to_owned(),
            bytes: bytes.len() as u64,
            module,
            has_imports,
            fetched_at: time::now_ms(),
        };
        deps::store(self.vault, &meta, bytes).map(|_| ())
    }

    /// Vendor `url` (a page reference, or something a module imports) and
    /// return where it is stored. Each URL is processed once per run.
    fn vendor(&mut self, url: &str, as_module: bool, depth: usize) -> Result<Dep, String> {
        if let Some(done) = self.done.get(url) {
            return done.clone();
        }
        if depth > MAX_DEPTH {
            let reason = format!("imports are nested more than {MAX_DEPTH} levels deep");
            self.fail(url, reason.clone());
            return Err(reason);
        }
        let result = self.vendor_new(url, as_module, depth);
        if let Err(reason) = &result {
            self.done.insert(url.to_owned(), result.clone());
            self.fail(url, reason.clone());
        }
        result
    }

    fn vendor_new(&mut self, url: &str, as_module: bool, depth: usize) -> Result<Dep, String> {
        let key = deps::key_for(url);
        // A copy from an earlier run (of this or another page) is kept; a
        // classic copy cannot stand in for a module, whose imports need rewriting.
        if let Some(meta) = deps::load(self.vault, &key)
            && meta.url == url
            && (meta.module || !as_module)
            && dep_file(self.vault, &key, &meta.file).is_file()
        {
            let dep = Dep {
                key,
                file: meta.file,
            };
            self.done.insert(url.to_owned(), Ok(dep.clone()));
            self.vendored.push(url.to_owned());
            return Ok(dep);
        }

        let (bytes, mime) = self.download(url)?;

        if as_module {
            let text = String::from_utf8(bytes).map_err(|_| "the module is not UTF-8 text")?;
            let dep = Dep {
                file: deps::file_name_for(url, JS_MIME),
                key,
            };
            self.done.insert(url.to_owned(), Ok(dep.clone()));
            let mut has_imports = false;
            let rewritten = deps::rewrite_specifiers(&text, |specifier| {
                let target = resolve(url, specifier)?;
                let path = if let Some(lib) = libs::map_cdn_url(&target, true) {
                    self.note_bundled(&target);
                    format!("../../lib/{}", lib_file(lib))
                } else {
                    // Its own failure is already reported by `vendor`.
                    let dep = self.vendor(&target, true, depth + 1).ok()?;
                    format!("../{}/{}", dep.key, dep.file)
                };
                has_imports = true;
                Some(path)
            });
            self.store(url, &dep, JS_MIME, rewritten.as_bytes(), true, has_imports)?;
            self.vendored.push(url.to_owned());
            return Ok(dep);
        }

        if mime == "text/css" || url_path(url).ends_with(".css") {
            let text = String::from_utf8(bytes).map_err(|_| "the stylesheet is not UTF-8 text")?;
            let dep = Dep {
                file: deps::file_name_for(url, "text/css"),
                key,
            };
            self.done.insert(url.to_owned(), Ok(dep.clone()));
            let mut stack = vec![url.to_owned()];
            let flat = self.flatten_css(&text, url, depth, &mut stack);
            self.store(url, &dep, "text/css", flat.as_bytes(), false, false)?;
            self.vendored.push(url.to_owned());
            return Ok(dep);
        }

        let dep = Dep {
            file: deps::file_name_for(url, &mime),
            key,
        };
        self.store(url, &dep, &mime, &bytes, false, false)?;
        self.done.insert(url.to_owned(), Ok(dep.clone()));
        self.vendored.push(url.to_owned());
        Ok(dep)
    }

    /// `css` with every `@import` replaced by the imported stylesheet (itself
    /// flattened) and every other `url()` by a `data:` URI. References that
    /// cannot be fetched stay as written. `stack` holds the stylesheets being
    /// flattened, to break `@import` cycles.
    fn flatten_css(
        &mut self,
        css: &str,
        base: &str,
        depth: usize,
        stack: &mut Vec<String>,
    ) -> String {
        let mut specs = deps::css_urls(css);
        specs.sort_by_key(|s| s.start);
        // (start, end, replacement), ascending and non-overlapping.
        let mut edits: Vec<(usize, usize, String)> = Vec::new();
        for spec in &specs {
            let value = spec.value.trim();
            if value.is_empty() || value.starts_with('#') || value.starts_with("data:") {
                continue;
            }
            let Some(target) = resolve(base, value) else {
                continue;
            };
            if let Some((start, end, media)) = import_rule(css, spec.start, spec.end) {
                if edits
                    .last()
                    .is_some_and(|(_, last_end, _)| start < *last_end)
                {
                    continue;
                }
                match self.import_css(&target, depth + 1, stack) {
                    Ok(text) => {
                        let wrapped = if media.is_empty() || text.trim().is_empty() {
                            text
                        } else {
                            format!("@media {media} {{\n{text}\n}}")
                        };
                        edits.push((start, end, wrapped));
                    }
                    Err(reason) => self.fail(&target, reason),
                }
            } else {
                if edits
                    .last()
                    .is_some_and(|(_, last_end, _)| spec.start < *last_end)
                {
                    continue;
                }
                match self.data_uri(&target) {
                    Ok(uri) => edits.push((spec.start, spec.end, uri)),
                    Err(reason) => self.fail(&target, reason),
                }
            }
        }
        let mut out = css.to_owned();
        for (start, end, text) in edits.into_iter().rev() {
            out.replace_range(start..end, &text);
        }
        out
    }

    fn import_css(
        &mut self,
        url: &str,
        depth: usize,
        stack: &mut Vec<String>,
    ) -> Result<String, String> {
        if stack.iter().any(|u| u == url) {
            return Ok(String::new());
        }
        if let Some(cached) = self.css_cache.get(url) {
            return cached.clone();
        }
        if depth > MAX_DEPTH {
            return Err(format!(
                "imports are nested more than {MAX_DEPTH} levels deep"
            ));
        }
        let result = (|| -> Result<String, String> {
            let (bytes, _) = self.download(url)?;
            let text = String::from_utf8(bytes).map_err(|_| "the stylesheet is not UTF-8 text")?;
            stack.push(url.to_owned());
            let flat = self.flatten_css(&text, url, depth, stack);
            stack.pop();
            Ok(strip_charset(&flat))
        })();
        self.css_cache.insert(url.to_owned(), result.clone());
        result
    }

    fn data_uri(&mut self, url: &str) -> Result<String, String> {
        if let Some(cached) = self.data_cache.get(url) {
            return cached.clone();
        }
        let result = self
            .download(url)
            .map(|(bytes, mime)| format!("data:{mime};base64,{}", assets::base64(&bytes)));
        self.data_cache.insert(url.to_owned(), result.clone());
        result
    }

    /// Vendor (or map to a bundled library) every reference of a page.
    fn refs(mut self, refs: &[RemoteRef]) -> Outcome {
        let mut map = HashMap::new();
        for r in refs {
            if map.contains_key(&r.raw) {
                continue;
            }
            let as_module = matches!(r.kind, RefKind::ModuleImport | RefKind::ImportMap)
                || url_path(&r.url).ends_with(".mjs");
            if let Some(lib) = libs::map_cdn_url(&r.url, as_module) {
                self.note_bundled(&r.url);
                map.insert(r.raw.clone(), format!("./{LIB_PREFIX}{}", lib_file(lib)));
                continue;
            }
            if let Ok(dep) = self.vendor(&r.url, as_module, 0) {
                map.insert(r.raw.clone(), format!("./{}", dep.page_path()));
            }
        }
        Outcome {
            map,
            vendored: self.vendored,
            bundled: self.bundled,
            failed: self.failed,
        }
    }
}

/// The `@import` rule whose URL is `css[url_start..url_end]`: its byte range
/// (through the closing `;`) and its media condition. `None` for a `url()`
/// that is not an import.
fn import_rule(css: &str, url_start: usize, url_end: usize) -> Option<(usize, usize, String)> {
    let before = &css[..url_start];
    // ASCII lowercasing keeps byte offsets.
    let at = before.to_ascii_lowercase().rfind("@import")?;
    let mut middle = before[at + "@import".len()..].trim();
    if let Some(head) = middle.get(..4)
        && head.eq_ignore_ascii_case("url(")
    {
        middle = middle[4..].trim_start();
    }
    if !middle.trim_start_matches(['"', '\'']).is_empty() {
        return None;
    }
    let (end, tail) = match css[url_end..].find(';') {
        Some(i) => (url_end + i + 1, &css[url_end..url_end + i]),
        None => (css.len(), &css[url_end..]),
    };
    let media = tail
        .trim_start_matches(['"', '\''])
        .trim_start()
        .trim_start_matches(')')
        .trim();
    // Cascade layers and `supports()` conditions are not media queries.
    let media = if media.starts_with("layer") || media.starts_with("supports") {
        ""
    } else {
        media
    };
    Some((at, end, media.to_owned()))
}

/// Drop a leading `@charset "…";`, which is invalid in the middle of a file.
fn strip_charset(css: &str) -> String {
    let trimmed = css.trim_start();
    if trimmed.len() >= 8
        && trimmed.is_char_boundary(8)
        && trimmed[..8].eq_ignore_ascii_case("@charset")
        && let Some(end) = trimmed.find(';')
    {
        return trimmed[end + 1..].trim_start().to_owned();
    }
    css.to_owned()
}

/// Download what `html` loads from the internet and return the page pointing
/// at the vault copies. Pure over the client and vault, so tests can use a
/// local server.
fn vendor_html(client: &Client, vault: &Path, html: &str) -> (String, Outcome) {
    let refs = deps::page_refs(html);
    let outcome = Run::new(client, vault).refs(&refs);
    let rewritten = deps::rewrite_page(html, &outcome.map);
    (rewritten, outcome)
}

fn vendor_page(app: &tauri::AppHandle, page_id: &str) -> CmdResult<VendorReport> {
    let state = app.state::<AppState>();
    // Read, then let go of the host: downloads can take a while and every
    // other command needs the lock.
    let (vault, page) = {
        let host = state.host.lock().map_err(|e| e.to_string())?;
        let vault = host.vault_path().ok_or("no vault open")?.to_path_buf();
        let page = host.call(
            Caller::Ui,
            "pages.get",
            json!({ "id": page_id, "format": "html" }),
        )?;
        (vault, page)
    };
    let html = page["html"].as_str().ok_or("the page has no HTML")?;
    let updated_at = page["meta"]["updatedAt"].as_i64();

    let client = http_client()?;
    let (new_html, outcome) = vendor_html(&client, &vault, html);
    tracing::info!(
        page = page_id,
        vendored = outcome.vendored.len(),
        bundled = outcome.bundled.len(),
        failed = outcome.failed.len(),
        "vendored page dependencies"
    );

    let meta = if new_html == html {
        page["meta"].clone()
    } else {
        let mut args = json!({ "id": page_id, "html": new_html });
        if let Some(at) = updated_at {
            args["baseUpdatedAt"] = json!(at);
        }
        let saved = {
            let host = state.host.lock().map_err(|e| e.to_string())?;
            host.call(Caller::Ui, "pages.set_html", args)
        }
        .map_err(|e| {
            if e.starts_with("conflict:") {
                "The page was edited while its files were downloading, so it was not changed. \
                 Try again."
                    .to_owned()
            } else {
                e
            }
        })?;
        // A reviewed edit answers `{ pendingApproval, page }`; the UI caller is never held back.
        saved.get("page").cloned().unwrap_or(saved)
    };

    Ok(VendorReport {
        vendored: outcome.vendored,
        bundled: outcome.bundled,
        failed: outcome.failed,
        meta,
    })
}

/// Copy what a page loads from the internet into the vault and point the
/// page at the copies. Anything that cannot be copied keeps its URL.
#[tauri::command]
pub async fn vendor_page_deps(app: tauri::AppHandle, page_id: String) -> CmdResult<VendorReport> {
    tauri::async_runtime::spawn_blocking(move || vendor_page(&app, &page_id))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    struct Server {
        base: String,
        hits: Arc<Mutex<Vec<String>>>,
    }

    impl Server {
        fn url(&self, path: &str) -> String {
            format!("{}{path}", self.base)
        }

        fn hits_for(&self, path: &str) -> usize {
            self.hits
                .lock()
                .unwrap()
                .iter()
                .filter(|p| *p == path)
                .count()
        }
    }

    /// A minimal HTTP/1.1 server: `routes` are `(path, mime, body)`, anything
    /// else answers 404.
    fn serve(routes: Vec<(&'static str, &'static str, Vec<u8>)>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let hits = Arc::new(Mutex::new(Vec::new()));
        let seen = hits.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut request = Vec::new();
                let mut buf = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                }
                let text = String::from_utf8_lossy(&request);
                let path = text
                    .lines()
                    .next()
                    .and_then(|l| l.split(' ').nth(1))
                    .unwrap_or("/")
                    .to_owned();
                seen.lock().unwrap().push(path.clone());
                let reply = match routes.iter().find(|(p, _, _)| *p == path) {
                    Some((_, mime, body)) => {
                        let mut out = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        out.extend_from_slice(body);
                        out
                    }
                    None => {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec()
                    }
                };
                let _ = stream.write_all(&reply);
            }
        });
        Server { base, hits }
    }

    fn temp_vault(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "herbarium_vendor_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn module_ref(url: &str) -> RemoteRef {
        RemoteRef {
            raw: url.to_owned(),
            url: url.to_owned(),
            kind: RefKind::ModuleImport,
        }
    }

    fn asset_ref(tag: &str, attr: &str, url: &str) -> RemoteRef {
        RemoteRef {
            raw: url.to_owned(),
            url: url.to_owned(),
            kind: RefKind::Asset {
                tag: tag.to_owned(),
                attr: attr.to_owned(),
            },
        }
    }

    /// The stored meta and text of the dependency vendored from `url`.
    fn stored(vault: &Path, url: &str) -> (DepMeta, String) {
        let meta = deps::load(vault, &deps::key_for(url)).expect("dep.json");
        let text = std::fs::read_to_string(dep_file(vault, &deps::key_for(url), &meta.file))
            .expect("stored file");
        (meta, text)
    }

    fn fixture() -> Server {
        serve(vec![
            (
                "/a.mjs",
                "text/javascript",
                b"import b from \"./b.mjs\";\nimport { c } from \"/root/c.mjs\";\nexport default b + c;".to_vec(),
            ),
            ("/b.mjs", "text/javascript", b"export default 1;".to_vec()),
            (
                "/root/c.mjs",
                "application/javascript",
                b"import a from \"../a.mjs\";\nexport const c = a;".to_vec(),
            ),
            (
                "/style.css",
                "text/css",
                b"@import \"./base.css\";\n@font-face{font-family:F;src:url(font.woff2) format(\"woff2\")}\nbody{background:url(\"img/bg.png\")}"
                    .to_vec(),
            ),
            ("/base.css", "text/css", b"p{color:red}".to_vec()),
            ("/font.woff2", "font/woff2", vec![0, 1, 2, 3]),
            ("/logo.png", "image/png", vec![137, 80, 78, 71]),
        ])
    }

    #[test]
    fn module_graph_with_a_cycle_is_vendored_and_rewritten() {
        let server = fixture();
        let vault = temp_vault("modules");
        let client = http_client().unwrap();
        let a_url = server.url("/a.mjs");
        let outcome = Run::new(&client, &vault).refs(&[module_ref(&a_url)]);

        let (a, a_text) = stored(&vault, &a_url);
        let (b, b_text) = stored(&vault, &server.url("/b.mjs"));
        let (c, c_text) = stored(&vault, &server.url("/root/c.mjs"));
        let key = |url: &str| deps::key_for(&server.url(url));

        assert_eq!(
            outcome.map[&a_url],
            format!("./{}", deps::rel_path(&deps::key_for(&a_url), &a.file))
        );
        assert!(a.module && a.has_imports);
        assert!(a_text.contains(&format!("\"../{}/{}\"", key("/b.mjs"), b.file)));
        assert!(a_text.contains(&format!("\"../{}/{}\"", key("/root/c.mjs"), c.file)));
        assert!(!a_text.contains("./b.mjs") && !a_text.contains("/root/c.mjs"));

        // The cycle: c imports a back, through the already-recorded entry.
        assert!(c.module && c.has_imports);
        assert!(c_text.contains(&format!("\"../{}/{}\"", key("/a.mjs"), a.file)));
        // A module without imports is stored untouched.
        assert!(b.module && !b.has_imports);
        assert_eq!(b_text, "export default 1;");
        assert_eq!(a.mime, "text/javascript");

        assert_eq!(outcome.vendored.len(), 3);
        assert!(outcome.failed.is_empty());
        assert!(outcome.bundled.is_empty());
        // One request per URL, even with the cycle.
        for path in ["/a.mjs", "/b.mjs", "/root/c.mjs"] {
            assert_eq!(server.hits_for(path), 1, "{path}");
        }

        // A second run reuses the stored copies without asking the server.
        let again = Run::new(&client, &vault).refs(&[module_ref(&a_url)]);
        assert_eq!(again.map, outcome.map);
        assert_eq!(server.hits_for("/a.mjs"), 1);
        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn stylesheets_are_flattened_and_their_resources_inlined() {
        let server = fixture();
        let vault = temp_vault("css");
        let client = http_client().unwrap();
        let css_url = server.url("/style.css");
        let outcome = Run::new(&client, &vault).refs(&[asset_ref("link", "href", &css_url)]);

        let (meta, text) = stored(&vault, &css_url);
        assert_eq!(meta.mime, "text/css");
        assert!(!meta.module && !meta.has_imports);
        assert!(!text.contains("@import"));
        assert!(text.contains("p{color:red}"));
        // [0, 1, 2, 3] in base64.
        assert!(text.contains("url(data:font/woff2;base64,AAECAw==)"));
        // The image is missing: its reference stays and the failure is listed.
        assert!(text.contains("url(\"img/bg.png\")"));
        assert_eq!(outcome.failed.len(), 1);
        assert_eq!(outcome.failed[0].url, server.url("/img/bg.png"));
        assert!(outcome.failed[0].reason.contains("404"));
        // The stylesheet itself is a vendored dep; its pieces are not.
        assert_eq!(outcome.vendored, vec![css_url]);
        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn plain_assets_are_stored_as_is_and_failures_keep_their_reference() {
        let server = fixture();
        let vault = temp_vault("assets");
        let client = http_client().unwrap();
        let logo = server.url("/logo.png");
        let missing = server.url("/nope.js");
        let outcome = Run::new(&client, &vault).refs(&[
            asset_ref("img", "src", &logo),
            asset_ref("script", "src", &missing),
        ]);

        let meta = deps::load(&vault, &deps::key_for(&logo)).unwrap();
        assert_eq!(meta.mime, "image/png");
        assert_eq!(
            std::fs::read(dep_file(&vault, &deps::key_for(&logo), &meta.file)).unwrap(),
            vec![137, 80, 78, 71]
        );
        assert!(outcome.map.contains_key(&logo));
        assert!(!outcome.map.contains_key(&missing));
        assert_eq!(
            outcome.failed,
            vec![Failed {
                url: missing,
                reason: "the server answered 404".into()
            }]
        );
        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn bundled_libraries_are_mapped_not_downloaded() {
        let vault = temp_vault("bundled");
        let client = http_client().unwrap();
        // A module that imports Mermaid from a CDN, next to a direct page reference.
        let cdn = "https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs";
        let host = serve(vec![(
            "/diagram.mjs",
            "text/javascript",
            format!("import mermaid from \"{cdn}\";\nexport default mermaid;").into_bytes(),
        )]);
        let module_url = host.url("/diagram.mjs");
        let outcome = Run::new(&client, &vault).refs(&[module_ref(cdn), module_ref(&module_url)]);

        assert!(libs::map_cdn_url(cdn, true).is_some());
        assert!(outcome.map[cdn].starts_with("./_herbarium/lib/"));
        assert_eq!(outcome.map[cdn], "./_herbarium/lib/mermaid.mjs");
        assert!(outcome.bundled.contains(&cdn.to_owned()));
        assert!(!outcome.vendored.contains(&cdn.to_owned()));
        assert!(outcome.failed.is_empty(), "{:?}", outcome.failed);
        assert!(
            !deps::key_for(cdn).is_empty() && deps::load(&vault, &deps::key_for(cdn)).is_none()
        );

        let (meta, text) = stored(&vault, &module_url);
        assert!(meta.has_imports);
        assert!(text.contains("\"../../lib/mermaid.mjs\""), "{text}");
        assert!(!text.contains("jsdelivr"));
        assert_eq!(host.hits_for("/diagram.mjs"), 1);
        // An inline module's specifier is rewritten to a relative specifier starting with `./`.
        let inline_html =
            format!("<script type=\"module\">import mermaid from \"{cdn}\";</script>");
        let (rewritten, _) = vendor_html(&client, &vault, &inline_html);
        assert!(rewritten.contains("import mermaid from \"./_herbarium/lib/mermaid.mjs\""));
        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn only_https_is_allowed_outside_tests() {
        assert!(allowed_url("https://example.com/a.js"));
        assert!(!allowed_url("http://example.com/a.js"));
        assert!(!allowed_url("file:///etc/passwd"));
        assert!(!allowed_url("ftp://example.com/a.js"));
    }

    #[test]
    fn import_rule_finds_the_rule_media_and_skips_plain_urls() {
        let css = "@import url(\"a.css\") screen and (min-width:1px);\nbody{background:url(b.png)}";
        let start = css.find("a.css").unwrap();
        let (from, to, media) = import_rule(css, start, start + 5).unwrap();
        assert_eq!(
            &css[from..to],
            "@import url(\"a.css\") screen and (min-width:1px);"
        );
        assert_eq!(media, "screen and (min-width:1px)");
        let png = css.find("b.png").unwrap();
        assert!(import_rule(css, png, png + 5).is_none());
        assert_eq!(strip_charset("@charset \"utf-8\";\np{}"), "p{}");
    }
}
