// Native messaging host: the bridge between the Herbarium browser extension
// and the vault. The browser starts `herbarium` with the extension's origin
// as an argument and talks over stdin/stdout, each message a 32-bit
// native-endian length followed by that many bytes of JSON. No port is
// opened, and only the extension IDs listed in the installed manifest may
// start it.
//
// Requests are `{ id?, type, … }`; every reply echoes `id` with `ok: true`
// and its fields, or `ok: false` and an `error`.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};

use herbarium_core::{Caller, Host};
use serde_json::{Value, json};

use crate::cli::{LINK_PREFIX, resolve_vault};

/// Registered name of the host (lowercase, dots allowed).
pub const HOST_NAME: &str = "app.herbarium.host";
/// The extension's ID in Firefox (`browser_specific_settings.gecko.id`).
pub const FIREFOX_EXTENSION_ID: &str = "herbarium@herbarium.app";
/// The extension's ID in Chromium browsers, derived from the `key` in its
/// manifest, so an unpacked install has it too.
pub const CHROME_EXTENSION_ID: &str = "fimfpdpamppfkhnbgodhgegfaefenmml";

/// Pages saved from the browser land here unless the request says otherwise.
pub const INBOX: &str = "Inbox";
/// Largest message accepted from the browser (a page with inlined images).
const MAX_REQUEST: usize = 64 * 1024 * 1024;
/// Chrome refuses messages from a host larger than 1 MiB.
const MAX_REPLY: usize = 1024 * 1024;

/// Whether these process arguments are a browser starting the native host:
/// Chromium passes the caller's origin, Firefox the manifest path and the
/// extension ID.
pub fn is_browser_launch(args: &[String]) -> bool {
    args.iter()
        .any(|a| a.starts_with("chrome-extension://") || a == FIREFOX_EXTENSION_ID)
}

/// Read one framed message; `Ok(None)` at a clean end of input.
pub fn read_message(input: &mut impl Read) -> io::Result<Option<Value>> {
    let mut len = [0u8; 4];
    match input.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_ne_bytes(len) as usize;
    if len > MAX_REQUEST {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too large",
        ));
    }
    let mut body = vec![0u8; len];
    input.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Write one framed message.
pub fn write_message(output: &mut impl Write, value: &Value) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_REPLY {
        bytes = serde_json::to_vec(&json!({
            "id": value.get("id").cloned().unwrap_or(Value::Null),
            "ok": false,
            "error": "reply too large",
        }))?;
    }
    output.write_all(&(bytes.len() as u32).to_ne_bytes())?;
    output.write_all(&bytes)?;
    output.flush()
}

fn str_arg<'a>(req: &'a Value, key: &str) -> Option<&'a str> {
    req.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// What a page looks like to the extension.
fn brief(meta: &Value) -> Value {
    json!({
        "id": meta["id"],
        "title": meta["title"],
        "folder": meta["folder"],
        "link": format!("{LINK_PREFIX}{}", meta["id"].as_str().unwrap_or_default()),
    })
}

/// Answer one request. `open` hands a deep link to `launch`. The request is
/// consumed so a saved page's HTML moves into the operation instead of being
/// copied.
pub fn handle(host: &Host, mut req: Value, launch: &dyn Fn(&str) -> Result<(), String>) -> Value {
    let req = &mut req;
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let reply = match req.get("type").and_then(Value::as_str).unwrap_or_default() {
        "ping" => ping(host),
        "save" => save(host, req),
        "lookup" => lookup(host, req),
        "search" => search(host, req),
        "today" => host
            .call(Caller::Ui, "today.summary", json!({}))
            .map(|s| json!({ "today": s })),
        "open" => open(host, req, launch),
        "scan" => scan(host, req),
        "import" => import(host, req),
        other => Err(format!("unknown request type: {other}")),
    };
    match reply {
        Ok(Value::Object(mut fields)) => {
            fields.insert("id".into(), id);
            fields.insert("ok".into(), Value::Bool(true));
            Value::Object(fields)
        }
        Ok(other) => json!({ "id": id, "ok": true, "value": other }),
        Err(error) => json!({ "id": id, "ok": false, "error": error }),
    }
}

fn ping(host: &Host) -> Result<Value, String> {
    let vault = host.vault_path().ok_or("no vault open")?;
    let pages = host.store().map(|s| s.count().unwrap_or(0)).unwrap_or(0);
    Ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "vault": vault.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        "pages": pages,
    }))
}

/// Save a page from the browser. Web content is untrusted: network access
/// is always off, whatever the vault default, and it lands in the Inbox.
fn save(host: &Host, req: &mut Value) -> Result<Value, String> {
    // Take the HTML out of the request: `json!` would copy it.
    let Some(Value::String(html)) = req.get_mut("html").map(Value::take) else {
        return Err("save needs `html`".into());
    };
    let mut args = json!({
        "folder": str_arg(req, "folder").unwrap_or(INBOX),
        "allowCdn": false,
    });
    args["html"] = Value::String(html);
    if let Some(title) = str_arg(req, "title") {
        args["title"] = json!(title);
    }
    if let Some(tags) = req.get("tags").and_then(Value::as_array) {
        args["tags"] = Value::Array(tags.iter().filter(|t| t.is_string()).cloned().collect());
    }
    let mut source = serde_json::Map::new();
    for key in ["url", "tool", "prompt"] {
        if let Some(v) = str_arg(req, key) {
            source.insert(key.into(), json!(v));
        }
    }
    if !source.is_empty() {
        args["source"] = Value::Object(source);
    }
    let meta = host.call(Caller::Ui, "pages.create", args)?;
    Ok(json!({ "page": brief(&meta) }))
}

/// The artifacts in a conversation the extension fetched from Claude or
/// ChatGPT (`json`: the conversations export format), without their HTML.
fn scan(host: &Host, req: &Value) -> Result<Value, String> {
    let json = req
        .get("json")
        .and_then(Value::as_str)
        .ok_or("scan needs `json`")?;
    let scan = herbarium_core::importer::scan(json)?;
    let have = herbarium_core::importer::imported_keys(host.store().ok_or("no vault open")?)?;
    let candidates: Vec<Value> = scan
        .candidates
        .iter()
        .map(|c| {
            let mut v = serde_json::to_value(c).unwrap_or(Value::Null);
            v["alreadyImported"] = json!(have.contains(&c.key));
            v
        })
        .collect();
    Ok(json!({ "candidates": candidates, "unsupported": scan.unsupported }))
}

/// Save the chosen artifacts (`keys`) of a conversation, like an export import.
fn import(host: &Host, req: &Value) -> Result<Value, String> {
    let json = req
        .get("json")
        .and_then(Value::as_str)
        .ok_or("import needs `json`")?;
    let keys: std::collections::HashSet<String> = req
        .get("keys")
        .and_then(Value::as_array)
        .ok_or("import needs `keys`")?
        .iter()
        .filter_map(|k| k.as_str().map(str::to_string))
        .collect();
    let scan = herbarium_core::importer::scan(json)?;
    let folder = str_arg(req, "folder").unwrap_or(INBOX);
    // Conversations come from the web: their pages never get network access.
    let report = crate::ai_import::import(
        host,
        &scan.candidates,
        Some(&keys),
        Some(folder),
        Some(false),
    )?;
    let by_key: std::collections::HashMap<String, Value> = host
        .store()
        .ok_or("no vault open")?
        .all()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|m| {
            let key = herbarium_core::import_key(&m)?.to_string();
            keys.contains(&key)
                .then(|| (key, brief(&serde_json::to_value(&m).unwrap_or(Value::Null))))
        })
        .collect();
    let saved: Vec<Value> = scan
        .candidates
        .iter()
        .filter_map(|c| by_key.get(&c.key).cloned())
        .collect();
    Ok(
        json!({ "imported": report.imported, "skipped": report.skipped, "errors": report.errors, "pages": saved }),
    )
}

fn lookup(host: &Host, req: &Value) -> Result<Value, String> {
    let url = str_arg(req, "url").ok_or("lookup needs `url`")?;
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Ok(json!({ "pages": [] }));
    }
    let found = host.call(Caller::Ui, "pages.find_by_url", json!({ "url": url }))?;
    let pages: Vec<Value> = found.as_array().into_iter().flatten().map(brief).collect();
    Ok(json!({ "pages": pages }))
}

fn search(host: &Host, req: &Value) -> Result<Value, String> {
    let query = req.get("query").and_then(Value::as_str).unwrap_or_default();
    let limit = req
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(8)
        .clamp(1, 20);
    let hits = host.call(
        Caller::Ui,
        "pages.search",
        json!({ "query": query, "limit": limit }),
    )?;
    let results: Vec<Value> = hits
        .as_array()
        .into_iter()
        .flatten()
        .map(|h| {
            let mut b = brief(h);
            b["snippet"] = h.get("snippet").cloned().unwrap_or(Value::Null);
            b
        })
        .collect();
    Ok(json!({ "results": results }))
}

fn open(
    host: &Host,
    req: &Value,
    launch: &dyn Fn(&str) -> Result<(), String>,
) -> Result<Value, String> {
    let link = if req.get("review").and_then(Value::as_bool) == Some(true) {
        "herbarium-app://review".to_string()
    } else {
        let id = str_arg(req, "page").ok_or("open needs `page` or `review: true`")?;
        let meta = host.call(Caller::Ui, "pages.get", json!({ "id": id }))?;
        format!("{LINK_PREFIX}{}", meta["meta"]["id"].as_str().unwrap_or(id))
    };
    launch(&link)?;
    Ok(json!({ "opened": link }))
}

/// Open the app on `link`: a new launch hands it to the running window.
/// In debug builds, with `HERBARIUM_LAUNCH_LOG` set (the end-to-end tests run
/// the debug binary), the link is appended to that file instead. Release
/// builds carry no such hook.
fn launch_app(link: &str) -> Result<(), String> {
    #[cfg(debug_assertions)]
    if let Some(log) = std::env::var_os("HERBARIUM_LAUNCH_LOG").filter(|p| !p.is_empty()) {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)
            .map_err(|e| e.to_string())?;
        return writeln!(file, "{link}").map_err(|e| e.to_string());
    }
    let exe = app_executable()?;
    std::process::Command::new(exe)
        .arg(link)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("cannot start Herbarium: {e}"))
}

/// The executable browsers should start: the AppImage itself when running
/// from one (its mount point changes every run), else this binary.
pub fn app_executable() -> Result<PathBuf, String> {
    if let Some(appimage) = std::env::var_os("APPIMAGE").filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(appimage));
    }
    std::env::current_exe().map_err(|e| format!("cannot locate the Herbarium executable: {e}"))
}

/// Requests answered from a read-only connection while a write may be running.
fn is_read_request(req: &Value) -> bool {
    matches!(
        req.get("type").and_then(Value::as_str),
        Some("ping" | "lookup" | "search")
    )
}

fn error_reply(req: &Value, error: &str) -> Value {
    json!({ "id": req.get("id").cloned().unwrap_or(Value::Null), "ok": false, "error": error })
}

/// Serve the browser until it closes the connection.
///
/// Requests are answered by two workers so a cheap badge `lookup`/`search`
/// never waits behind a large `save`: the write worker owns the read/write
/// host (writes stay serialised), the read worker answers `ping`, `lookup` and
/// `search` from its own read-only connection. Replies carry the request `id`,
/// so they may leave out of order. The vault is opened lazily by the write
/// worker and a failed open is retried on the next request.
pub fn serve(
    vault: Option<String>,
    input: &mut impl Read,
    output: impl Write + Send,
) -> io::Result<()> {
    let output = Mutex::new(output);
    // The opened vault's canonical path, once the write worker has indexed it.
    let opened: OnceLock<PathBuf> = OnceLock::new();
    let send = |reply: &Value| -> io::Result<()> {
        let mut out = output.lock().unwrap_or_else(|e| e.into_inner());
        write_message(&mut *out, reply)
    };

    std::thread::scope(|scope| {
        let (write_tx, write_rx) = mpsc::channel::<Value>();
        let (read_tx, read_rx) = mpsc::channel::<Value>();

        let forward = write_tx.clone();
        scope.spawn(|| write_worker(vault, write_rx, &opened, &send));
        scope.spawn(|| read_worker(read_rx, forward, &opened, &send));

        let result = loop {
            match read_message(input) {
                Ok(Some(req)) => {
                    let target = if opened.get().is_some() && is_read_request(&req) {
                        &read_tx
                    } else {
                        &write_tx
                    };
                    if target.send(req).is_err() {
                        break Ok(());
                    }
                }
                Ok(None) => break Ok(()),
                Err(e) => break Err(e),
            }
        };
        // Closing the channels lets the workers finish what is queued.
        drop(read_tx);
        drop(write_tx);
        result
    })
}

/// Owns the read/write host; opens the vault on first need and again after a
/// failed attempt.
fn write_worker(
    vault: Option<String>,
    requests: Receiver<Value>,
    opened: &OnceLock<PathBuf>,
    send: &(dyn Fn(&Value) -> io::Result<()> + Sync),
) {
    let mut host: Option<Host> = None;
    for req in requests {
        let reply = match ensure_host(&mut host, &vault) {
            Ok(host) => {
                let _ = opened.set(host.vault_path().map(Path::to_path_buf).unwrap_or_default());
                handle(host, req, &launch_app)
            }
            Err(e) => error_reply(&req, &e),
        };
        if send(&reply).is_err() {
            return;
        }
    }
}

fn ensure_host<'a>(host: &'a mut Option<Host>, vault: &Option<String>) -> Result<&'a Host, String> {
    if host.is_none() {
        let mut fresh = Host::new();
        fresh.open_vault(&resolve_vault(vault.clone())?)?;
        *host = Some(fresh);
    }
    host.as_ref().ok_or_else(|| "no vault open".into())
}

/// Answers read requests from a read-only connection to the opened vault. When
/// that connection cannot be had, the request goes to the write worker.
fn read_worker(
    requests: Receiver<Value>,
    fallback: Sender<Value>,
    opened: &OnceLock<PathBuf>,
    send: &(dyn Fn(&Value) -> io::Result<()> + Sync),
) {
    let mut host: Option<Host> = None;
    for req in requests {
        if host.is_none()
            && let Some(vault) = opened.get()
        {
            let mut fresh = Host::new();
            if fresh.open_vault_read_only(vault).is_ok() {
                host = Some(fresh);
            }
        }
        let Some(host) = &host else {
            if fallback.send(req).is_err() {
                return;
            }
            continue;
        };
        if send(&handle(host, req, &launch_app)).is_err() {
            return;
        }
    }
}

/// `herbarium native-host …`: serve, or install/uninstall the manifests.
pub fn run(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("install") => match install(&extension_ids(&args[1..])) {
            Ok(done) => {
                for line in done {
                    println!("{line}");
                }
                0
            }
            Err(e) => {
                eprintln!("herbarium native-host install: {e}");
                1
            }
        },
        Some("uninstall") => match uninstall() {
            Ok(done) => {
                for line in done {
                    println!("{line}");
                }
                0
            }
            Err(e) => {
                eprintln!("herbarium native-host uninstall: {e}");
                1
            }
        },
        Some("--help" | "-h" | "help") => {
            println!("{NATIVE_HOST_USAGE}");
            0
        }
        _ => {
            // Started by a browser (or by hand with `--vault <path>`). The
            // app's browser setup shows the extension as connected from now.
            if is_browser_launch(args) {
                crate::browsers::record_seen(&crate::browsers::launching_browser(args));
            }
            let vault = args
                .windows(2)
                .find(|w| w[0] == "--vault")
                .map(|w| w[1].clone());
            let stdin = io::stdin();
            match serve(vault, &mut stdin.lock(), io::stdout()) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("herbarium native-host: {e}");
                    1
                }
            }
        }
    }
}

pub const NATIVE_HOST_USAGE: &str =
    "usage: herbarium native-host [install [--extension-id <id>]… | uninstall]

Connects the Herbarium browser extension to the vault. `install` registers
the host with every supported browser found (Chrome, Chromium, Brave, Edge,
Vivaldi, Helium, Firefox); `uninstall` removes it. Browsers start the host on
their own; run without arguments only to test it by hand.";

/// Extension IDs to allow: the built-in one plus any `--extension-id`.
fn extension_ids(args: &[String]) -> Vec<String> {
    let mut ids = vec![CHROME_EXTENSION_ID.to_string()];
    for w in args.windows(2) {
        if w[0] == "--extension-id" && !ids.contains(&w[1]) {
            ids.push(w[1].clone());
        }
    }
    ids
}

/// The host manifest for Chromium browsers (`firefox: false`) or Firefox.
pub fn manifest(exe: &Path, extension_ids: &[String], firefox: bool) -> Value {
    let mut m = json!({
        "name": HOST_NAME,
        "description": "Herbarium: save pages from the browser into your vault",
        "path": exe.to_string_lossy(),
        "type": "stdio",
    });
    if firefox {
        m["allowed_extensions"] = json!([FIREFOX_EXTENSION_ID]);
    } else {
        m["allowed_origins"] = Value::Array(
            extension_ids
                .iter()
                .map(|id| json!(format!("chrome-extension://{id}/")))
                .collect(),
        );
    }
    m
}

/// `(browser, config dir that must exist, manifest dir, firefox?)` for this OS.
#[cfg(not(windows))]
fn manifest_dirs() -> Vec<(&'static str, PathBuf, PathBuf, bool)> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    #[cfg(target_os = "macos")]
    let (base, firefox_base) = (
        home.join("Library/Application Support"),
        home.join("Library/Application Support/Mozilla"),
    );
    #[cfg(not(target_os = "macos"))]
    let (base, firefox_base) = (home.join(".config"), home.join(".mozilla"));
    #[cfg(target_os = "macos")]
    let chromium: [(&str, &str); 6] = [
        ("Chrome", "Google/Chrome"),
        ("Chromium", "Chromium"),
        ("Brave", "BraveSoftware/Brave-Browser"),
        ("Edge", "Microsoft Edge"),
        ("Vivaldi", "Vivaldi"),
        ("Helium", "net.imput.helium"),
    ];
    #[cfg(not(target_os = "macos"))]
    let chromium: [(&str, &str); 6] = [
        ("Chrome", "google-chrome"),
        ("Chromium", "chromium"),
        ("Brave", "BraveSoftware/Brave-Browser"),
        ("Edge", "microsoft-edge"),
        ("Vivaldi", "vivaldi"),
        ("Helium", "net.imput.helium"),
    ];
    let mut out: Vec<(&'static str, PathBuf, PathBuf, bool)> = chromium
        .iter()
        .map(|(name, dir)| {
            let root = base.join(dir);
            (
                *name,
                root.clone(),
                root.join("NativeMessagingHosts"),
                false,
            )
        })
        .collect();
    #[cfg(target_os = "macos")]
    let firefox_dir = firefox_base.join("NativeMessagingHosts");
    #[cfg(not(target_os = "macos"))]
    let firefox_dir = firefox_base.join("native-messaging-hosts");
    out.push(("Firefox", firefox_base, firefox_dir, true));
    out
}

/// The executable a registered manifest starts, when the file is readable.
#[cfg(not(windows))]
fn registered_exe(manifest: &Path) -> Option<PathBuf> {
    let raw = std::fs::read(manifest).ok()?;
    let value: Value = serde_json::from_slice(&raw).ok()?;
    value.get("path")?.as_str().map(PathBuf::from)
}

/// Whether the manifest at `path` starts an executable that still exists.
/// A manifest left by an older install or a moved build does not: the
/// browser then fails with "native host has exited" and nothing tells why.
#[cfg(not(windows))]
fn registration_works(path: &Path) -> bool {
    registered_exe(path).is_some_and(|exe| exe.is_file())
}

fn write_manifest(path: &Path, manifest: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())?;
    herbarium_core::vault::write_atomic(path, &bytes)
}

#[cfg(not(windows))]
fn install(extension_ids: &[String]) -> Result<Vec<String>, String> {
    let exe = app_executable()?;
    let mut done = Vec::new();
    for (browser, root, dir, firefox) in manifest_dirs() {
        if !root.exists() {
            continue;
        }
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("{browser}: cannot create {}: {e}", dir.display()))?;
        let path = dir.join(format!("{HOST_NAME}.json"));
        write_manifest(&path, &manifest(&exe, extension_ids, firefox))?;
        done.push(format!("{browser}: {}", path.display()));
    }
    if done.is_empty() {
        return Err(
            "no supported browser found (Chrome, Chromium, Brave, Edge, Vivaldi, Helium or Firefox)"
                .into(),
        );
    }
    Ok(done)
}

/// Keep browsers connected across updates, moves and newly installed
/// browsers. Once the host is registered with any browser, register it with
/// every browser in `targets` whose manifest is missing or starts an
/// executable that no longer exists. Working manifests are left alone (they
/// may belong to another install the user chose); a stale one keeps its
/// allowed extensions and only gets the new `exe`.
#[cfg(not(windows))]
fn refresh_in(targets: &[(&str, PathBuf, PathBuf, bool)], exe: &Path) -> Vec<String> {
    let file = |dir: &Path| dir.join(format!("{HOST_NAME}.json"));
    let found: Vec<_> = targets
        .iter()
        .filter(|(_, root, _, _)| root.exists())
        .collect();
    if !found.iter().any(|(_, _, dir, _)| file(dir).exists()) {
        return Vec::new();
    }
    let mut done = Vec::new();
    for (browser, _, dir, firefox) in found {
        let path = file(dir);
        if registration_works(&path) {
            continue;
        }
        let existing = std::fs::read(&path)
            .ok()
            .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
            .filter(|v| v.get("name").and_then(Value::as_str) == Some(HOST_NAME));
        let next = match existing {
            Some(mut m) => {
                m["path"] = json!(exe.to_string_lossy());
                m
            }
            None => manifest(exe, &extension_ids(&[]), *firefox),
        };
        if std::fs::create_dir_all(dir).is_ok() && write_manifest(&path, &next).is_ok() {
            done.push(format!("{browser}: {}", path.display()));
        }
    }
    done
}

/// [`refresh_in`] for the browsers on this computer, at app start.
#[cfg(not(windows))]
pub fn refresh() -> Vec<String> {
    match app_executable() {
        Ok(exe) => refresh_in(&manifest_dirs(), &exe),
        Err(_) => Vec::new(),
    }
}

#[cfg(windows)]
pub fn refresh() -> Vec<String> {
    Vec::new()
}

#[cfg(not(windows))]
fn uninstall() -> Result<Vec<String>, String> {
    let mut done = Vec::new();
    for (browser, _, dir, _) in manifest_dirs() {
        let path = dir.join(format!("{HOST_NAME}.json"));
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("{browser}: {e}"))?;
            done.push(format!("{browser}: removed {}", path.display()));
        }
    }
    Ok(done)
}

/// Windows: manifests live in the app's data folder and each browser finds
/// them through a registry key under HKCU.
#[cfg(windows)]
const REGISTRY_KEYS: [(&str, &str, bool); 6] = [
    (
        "Chrome",
        r"Software\Google\Chrome\NativeMessagingHosts",
        false,
    ),
    ("Chromium", r"Software\Chromium\NativeMessagingHosts", false),
    (
        "Brave",
        r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts",
        false,
    ),
    (
        "Edge",
        r"Software\Microsoft\Edge\NativeMessagingHosts",
        false,
    ),
    ("Vivaldi", r"Software\Vivaldi\NativeMessagingHosts", false),
    ("Firefox", r"Software\Mozilla\NativeMessagingHosts", true),
];

#[cfg(windows)]
fn install(extension_ids: &[String]) -> Result<Vec<String>, String> {
    let exe = app_executable()?;
    let dir = dirs::data_dir()
        .ok_or("no data folder")?
        .join("Herbarium")
        .join("native-host");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut done = Vec::new();
    for firefox in [false, true] {
        let path = dir.join(if firefox {
            "firefox.json"
        } else {
            "chromium.json"
        });
        write_manifest(&path, &manifest(&exe, extension_ids, firefox))?;
        for (browser, key, for_firefox) in REGISTRY_KEYS {
            if for_firefox != firefox {
                continue;
            }
            let status = std::process::Command::new("reg")
                .args([
                    "add",
                    &format!(r"HKCU\{key}\{HOST_NAME}"),
                    "/ve",
                    "/t",
                    "REG_SZ",
                    "/d",
                ])
                .arg(&path)
                .arg("/f")
                .output()
                .map_err(|e| format!("cannot run reg: {e}"))?;
            if status.status.success() {
                done.push(format!("{browser}: {}", path.display()));
            }
        }
    }
    Ok(done)
}

#[cfg(windows)]
fn uninstall() -> Result<Vec<String>, String> {
    let mut done = Vec::new();
    for (browser, key, _) in REGISTRY_KEYS {
        let status = std::process::Command::new("reg")
            .args(["delete", &format!(r"HKCU\{key}\{HOST_NAME}"), "/f"])
            .output()
            .map_err(|e| format!("cannot run reg: {e}"))?;
        if status.status.success() {
            done.push(format!("{browser}: removed"));
        }
    }
    Ok(done)
}

/// Browsers found on this computer and whether the host is registered with
/// each, for the desktop app's settings. A manifest naming a missing
/// executable does not count.
#[cfg(not(windows))]
pub fn status() -> Vec<(String, bool)> {
    manifest_dirs()
        .into_iter()
        .filter(|(_, root, _, _)| root.exists())
        .map(|(browser, _, dir, _)| {
            (
                browser.to_string(),
                registration_works(&dir.join(format!("{HOST_NAME}.json"))),
            )
        })
        .collect()
}

#[cfg(windows)]
pub fn status() -> Vec<(String, bool)> {
    REGISTRY_KEYS
        .iter()
        .map(|(browser, key, _)| {
            let ok = std::process::Command::new("reg")
                .args(["query", &format!(r"HKCU\{key}\{HOST_NAME}")])
                .output()
                .is_ok_and(|o| o.status.success());
            (browser.to_string(), ok)
        })
        .collect()
}

/// Install for the desktop app's "Connect browsers" button.
pub fn install_default() -> Result<Vec<String>, String> {
    install(&extension_ids(&[]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(v: &Value) -> Vec<u8> {
        let mut out = Vec::new();
        write_message(&mut out, v).unwrap();
        out
    }

    #[test]
    fn framing_round_trips_and_rejects_oversized_messages() {
        let v = json!({ "type": "ping", "id": 1 });
        let bytes = frame(&v);
        assert_eq!(
            u32::from_ne_bytes(bytes[..4].try_into().unwrap()) as usize,
            bytes.len() - 4
        );
        let mut input = bytes.as_slice();
        assert_eq!(read_message(&mut input).unwrap(), Some(v));
        assert_eq!(
            read_message(&mut input).unwrap(),
            None,
            "clean end of input"
        );

        let mut huge = ((MAX_REQUEST + 1) as u32).to_ne_bytes().to_vec();
        huge.extend_from_slice(b"{}");
        assert!(read_message(&mut huge.as_slice()).is_err());

        let big = json!({ "id": 7, "blob": "x".repeat(MAX_REPLY) });
        let mut input = frame(&big);
        let back = read_message(&mut input.as_slice()).unwrap().unwrap();
        assert_eq!(back["ok"], false, "a reply over 1 MiB becomes an error");
        assert_eq!(back["id"], 7);
        input.clear();
    }

    /// A writer the test can read back while `serve` still owns it.
    #[derive(Clone, Default)]
    struct Sink(std::sync::Arc<Mutex<Vec<u8>>>);

    impl Write for Sink {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Sink {
        fn replies(&self) -> Vec<Value> {
            let bytes = self.0.lock().unwrap().clone();
            let mut input = bytes.as_slice();
            std::iter::from_fn(|| read_message(&mut input).unwrap()).collect()
        }
    }

    /// Framed requests, each preceded by a hook that runs when `serve` asks for it.
    type Step = (Box<dyn FnMut()>, Vec<u8>);
    struct Script(Vec<Step>, std::io::Cursor<Vec<u8>>);

    impl Read for Script {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = self.1.read(buf)?;
            if n > 0 || self.0.is_empty() {
                return Ok(n);
            }
            let (mut hook, bytes) = self.0.remove(0);
            hook();
            self.1 = std::io::Cursor::new(bytes);
            self.1.read(buf)
        }
    }

    #[test]
    fn a_failed_vault_open_is_retried_and_reads_keep_working() {
        let dir =
            std::env::temp_dir().join(format!("herbarium-native-retry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // A file where the vault folder should be: the first open fails.
        let vault = dir.join("vault");
        std::fs::write(&vault, "in the way").unwrap();

        let sink = Sink::default();
        let wait_for = |sink: Sink, replies: usize| {
            move || {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                while sink.replies().len() < replies && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
        };
        let (s1, s2, s3) = (sink.clone(), sink.clone(), sink.clone());
        let unblock = vault.clone();
        let first = wait_for(s1, 0);
        let second = wait_for(s2, 1);
        let third = wait_for(s3, 2);
        let script = Script(
            vec![
                (Box::new(first), frame(&json!({ "id": 1, "type": "ping" }))),
                (
                    Box::new(move || {
                        second();
                        std::fs::remove_file(&unblock).unwrap();
                    }),
                    frame(&json!({ "id": 2, "type": "ping" })),
                ),
                (
                    Box::new(third),
                    frame(&json!({ "id": 3, "type": "lookup", "url": "https://example.com/" })),
                ),
            ],
            std::io::Cursor::new(Vec::new()),
        );
        let mut input = script;
        serve(
            Some(vault.to_string_lossy().into_owned()),
            &mut input,
            sink.clone(),
        )
        .unwrap();

        let replies = sink.replies();
        let by_id = |id: u64| {
            replies
                .iter()
                .find(|r| r["id"] == id)
                .unwrap_or_else(|| panic!("no reply {id}: {replies:?}"))
        };
        assert_eq!(by_id(1)["ok"], false, "the vault is not there yet");
        assert_eq!(
            by_id(2)["ok"],
            true,
            "the failure is not cached: {replies:?}"
        );
        assert_eq!(by_id(3)["pages"], json!([]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn requests_save_find_search_and_open() {
        let vault = std::env::temp_dir().join(format!("herbarium-native-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&vault);
        let mut host = Host::new();
        host.open_vault(vault.to_str().unwrap()).unwrap();
        // The vault default allows network; browser pages still get none.
        host.call(
            Caller::Ui,
            "network.configure",
            json!({ "defaultAllowCdn": true }),
        )
        .unwrap();
        let launched = std::cell::RefCell::new(Vec::<String>::new());
        let launch = |link: &str| -> Result<(), String> {
            launched.borrow_mut().push(link.to_string());
            Ok(())
        };
        let ask = |req: Value| handle(&host, req, &launch);

        assert_eq!(ask(json!({ "id": 1, "type": "ping" }))["ok"], true);
        let saved = ask(json!({
            "id": 2, "type": "save", "html": "<title>Solar</title><p>sun</p>",
            "url": "https://claude.ai/chat/abc", "tool": "Claude", "prompt": "make it", "tags": ["space", 3]
        }));
        assert_eq!(saved["ok"], true, "{saved}");
        assert_eq!(saved["id"], 2);
        let page_id = saved["page"]["id"].as_str().unwrap().to_string();
        assert_eq!(saved["page"]["folder"], INBOX);
        assert_eq!(
            saved["page"]["link"],
            format!("herbarium-app://open/{page_id}")
        );
        let meta = host
            .call(Caller::Ui, "pages.get", json!({ "id": page_id }))
            .unwrap();
        assert_eq!(
            meta["meta"]["allowCdn"], false,
            "web content never gets network"
        );
        assert_eq!(meta["meta"]["tags"], json!(["space"]));
        assert_eq!(meta["meta"]["ext"]["source"]["tool"], "Claude");

        let found = ask(json!({ "type": "lookup", "url": "https://claude.ai/chat/abc#x" }));
        assert_eq!(found["pages"][0]["id"], page_id.as_str());
        assert_eq!(
            ask(json!({ "type": "lookup", "url": "chrome://newtab" }))["pages"],
            json!([])
        );

        let hits = ask(json!({ "type": "search", "query": "solar" }));
        assert_eq!(hits["results"][0]["id"], page_id.as_str());
        assert!(ask(json!({ "type": "today" }))["today"]["totalPages"].as_u64() == Some(1));

        assert_eq!(ask(json!({ "type": "open", "page": page_id }))["ok"], true);
        assert_eq!(ask(json!({ "type": "open", "review": true }))["ok"], true);
        assert_eq!(ask(json!({ "type": "open", "page": "nope" }))["ok"], false);
        assert_eq!(
            *launched.borrow(),
            [
                format!("herbarium-app://open/{page_id}"),
                "herbarium-app://review".to_string()
            ]
        );

        // A conversation fetched from Claude: listed, then imported once.
        let conversation = json!([{ "uuid": "c9", "name": "Chat", "created_at": "2025-01-02T10:00:00Z", "chat_messages": [
            { "uuid": "h", "sender": "human", "text": "Draw a tree" },
            { "uuid": "a", "sender": "assistant", "content": [
                { "type": "tool_use", "name": "artifacts", "input": { "id": "tree", "command": "create", "type": "text/html", "title": "Tree", "content": "<!doctype html><title>Tree</title><p>tree</p>" } }
            ] }
        ] }])
        .to_string();
        let listed = ask(json!({ "type": "scan", "json": conversation }));
        assert_eq!(listed["candidates"][0]["key"], "claude:c9:tree");
        assert_eq!(listed["candidates"][0]["alreadyImported"], false);
        assert!(
            listed["candidates"][0].get("html").is_none(),
            "no HTML in the listing"
        );
        let imported =
            ask(json!({ "type": "import", "json": conversation, "keys": ["claude:c9:tree"] }));
        assert_eq!(imported["imported"], 1, "{imported}");
        assert_eq!(imported["pages"][0]["folder"], INBOX);
        let again = ask(json!({ "type": "scan", "json": conversation }));
        assert_eq!(again["candidates"][0]["alreadyImported"], true);

        let bad = ask(json!({ "id": "x", "type": "explode" }));
        assert_eq!(
            (bad["ok"].clone(), bad["id"].clone()),
            (json!(false), json!("x"))
        );
        assert_eq!(ask(json!({ "type": "save" }))["ok"], false);
        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn the_bundled_extension_carries_the_app_version() {
        let manifest: Value =
            serde_json::from_str(include_str!("../../extension/manifest.json")).unwrap();
        // Browsers only take numbers: a pre-release ships as its base version.
        let app = env!("CARGO_PKG_VERSION").split('-').next().unwrap();
        assert_eq!(manifest["version"], app, "run `pnpm release` to bump both");
    }

    #[test]
    fn manifests_name_the_extension_and_detect_browser_launches() {
        let exe = Path::new("/opt/herbarium/herbarium");
        let chrome = manifest(
            exe,
            &extension_ids(&["--extension-id".into(), "abc".into()]),
            false,
        );
        assert_eq!(chrome["name"], HOST_NAME);
        assert_eq!(chrome["type"], "stdio");
        assert_eq!(
            chrome["allowed_origins"],
            json!([
                format!("chrome-extension://{CHROME_EXTENSION_ID}/"),
                "chrome-extension://abc/"
            ])
        );
        let firefox = manifest(exe, &[], true);
        assert_eq!(firefox["allowed_extensions"], json!([FIREFOX_EXTENSION_ID]));
        assert!(firefox.get("allowed_origins").is_none());

        assert!(is_browser_launch(&[format!(
            "chrome-extension://{CHROME_EXTENSION_ID}/"
        )]));
        assert!(is_browser_launch(&[
            "/x/app.herbarium.host.json".into(),
            FIREFOX_EXTENSION_ID.into()
        ]));
        assert!(!is_browser_launch(&["herbarium-app://open/x".into()]));
    }
}

#[cfg(all(test, not(windows)))]
mod refresh_tests {
    use super::*;

    #[test]
    fn refresh_repairs_stale_and_missing_manifests_once_connected() {
        let root = std::env::temp_dir().join(format!("herbarium-refresh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let exe = root.join("new/herbarium");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"").unwrap();
        let other = root.join("other/herbarium");
        std::fs::create_dir_all(other.parent().unwrap()).unwrap();
        std::fs::write(&other, b"").unwrap();
        let target = |name: &'static str, firefox: bool| {
            let base = root.join(name);
            (name, base.clone(), base.join("hosts"), firefox)
        };
        let targets = vec![
            target("Stale", false),
            target("Missing", false),
            target("Working", false),
            target("Gone", false),
            target("Fox", true),
        ];
        for (_, base, _, _) in &targets[..4] {
            std::fs::create_dir_all(base).unwrap();
        }
        std::fs::create_dir_all(&targets[4].1).unwrap();
        std::fs::remove_dir_all(&targets[3].1).unwrap();
        let file = |i: usize| targets[i].2.join(format!("{HOST_NAME}.json"));

        // Nothing registered yet: the user never connected a browser.
        assert!(refresh_in(&targets, &exe).is_empty());
        assert!(!file(1).exists());

        std::fs::create_dir_all(&targets[0].2).unwrap();
        let ids = vec![CHROME_EXTENSION_ID.to_string(), "custom".to_string()];
        write_manifest(
            &file(0),
            &manifest(Path::new("/gone/herbarium"), &ids, false),
        )
        .unwrap();
        std::fs::create_dir_all(&targets[2].2).unwrap();
        write_manifest(&file(2), &manifest(&other, &ids, false)).unwrap();

        let done = refresh_in(&targets, &exe);
        assert_eq!(done.len(), 3, "{done:?}");
        let read = |i: usize| -> Value {
            serde_json::from_slice(&std::fs::read(file(i)).unwrap()).unwrap()
        };
        assert_eq!(read(0)["path"], exe.to_string_lossy().as_ref());
        assert_eq!(
            read(0)["allowed_origins"][1],
            "chrome-extension://custom/",
            "keeps its extensions"
        );
        assert_eq!(
            read(1)["path"],
            exe.to_string_lossy().as_ref(),
            "a newly found browser is registered"
        );
        assert_eq!(
            read(2)["path"],
            other.to_string_lossy().as_ref(),
            "a working install is left alone"
        );
        assert!(
            !file(3).exists(),
            "a browser that is not installed is skipped"
        );
        assert_eq!(read(4)["allowed_extensions"], json!([FIREFOX_EXTENSION_ID]));
        assert!(registration_works(&file(0)));
        assert!(refresh_in(&targets, &exe).is_empty(), "nothing left to do");
        let _ = std::fs::remove_dir_all(&root);
    }
}
