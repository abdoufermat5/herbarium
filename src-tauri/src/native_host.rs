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

/// Answer one request. `open` hands a deep link to `launch`.
pub fn handle(host: &Host, req: &Value, launch: &dyn Fn(&str) -> Result<(), String>) -> Value {
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
fn save(host: &Host, req: &Value) -> Result<Value, String> {
    let html = req
        .get("html")
        .and_then(Value::as_str)
        .ok_or("save needs `html`")?;
    let mut args = json!({
        "html": html,
        "folder": str_arg(req, "folder").unwrap_or(INBOX),
        "allowCdn": false,
    });
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
    let report =
        crate::ai_import::import(host, &scan.candidates, Some(&keys), Some(folder), Some(false))?;
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
/// With `HERBARIUM_LAUNCH_LOG` set (end-to-end tests), the link is appended
/// to that file instead.
fn launch_app(link: &str) -> Result<(), String> {
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

/// Serve the browser until it closes the connection. The vault is opened once.
pub fn serve(
    vault: Option<String>,
    input: &mut impl Read,
    output: &mut impl Write,
) -> io::Result<()> {
    let mut host = Host::new();
    let opened = resolve_vault(vault).and_then(|v| host.open_vault(&v).map(|_| ()));
    while let Some(req) = read_message(input)? {
        let reply = match &opened {
            Ok(()) => handle(&host, &req, &launch_app),
            Err(e) => {
                json!({ "id": req.get("id").cloned().unwrap_or(Value::Null), "ok": false, "error": e })
            }
        };
        write_message(output, &reply)?;
    }
    Ok(())
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
            // Started by a browser (or by hand with `--vault <path>`).
            let vault = args
                .windows(2)
                .find(|w| w[0] == "--vault")
                .map(|w| w[1].clone());
            let stdin = io::stdin();
            let stdout = io::stdout();
            match serve(vault, &mut stdin.lock(), &mut stdout.lock()) {
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
Vivaldi, Firefox); `uninstall` removes it. Browsers start the host on their
own; run without arguments only to test it by hand.";

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
    let chromium: [(&str, &str); 5] = [
        ("Chrome", "Google/Chrome"),
        ("Chromium", "Chromium"),
        ("Brave", "BraveSoftware/Brave-Browser"),
        ("Edge", "Microsoft Edge"),
        ("Vivaldi", "Vivaldi"),
    ];
    #[cfg(not(target_os = "macos"))]
    let chromium: [(&str, &str); 5] = [
        ("Chrome", "google-chrome"),
        ("Chromium", "chromium"),
        ("Brave", "BraveSoftware/Brave-Browser"),
        ("Edge", "microsoft-edge"),
        ("Vivaldi", "vivaldi"),
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
        let bytes = serde_json::to_vec_pretty(&manifest(&exe, extension_ids, firefox))
            .map_err(|e| e.to_string())?;
        herbarium_core::vault::write_atomic(&path, &bytes)?;
        done.push(format!("{browser}: {}", path.display()));
    }
    if done.is_empty() {
        return Err(
            "no supported browser found (Chrome, Chromium, Brave, Edge, Vivaldi or Firefox)".into(),
        );
    }
    Ok(done)
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
        let bytes = serde_json::to_vec_pretty(&manifest(&exe, extension_ids, firefox))
            .map_err(|e| e.to_string())?;
        herbarium_core::vault::write_atomic(&path, &bytes)?;
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
/// each, for the desktop app's settings.
#[cfg(not(windows))]
pub fn status() -> Vec<(String, bool)> {
    manifest_dirs()
        .into_iter()
        .filter(|(_, root, _, _)| root.exists())
        .map(|(browser, _, dir, _)| {
            (
                browser.to_string(),
                dir.join(format!("{HOST_NAME}.json")).exists(),
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
        let ask = |req: Value| handle(&host, &req, &launch);

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
