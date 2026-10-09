// IPC commands exposed to the frontend. Vault selection is app-level; every
// other feature goes through `invoke_op`, which runs a registered operation.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use herbarium_core::{Caller, Host};
use serde_json::{Value, json};
use tauri::State;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::config::{self, Config};
use crate::editors::{self, Choice, EditorInfo};

pub struct AppState {
    pub host: Mutex<Host>,
    /// Model lists read from the AI services, by `provider|address`.
    pub models: Mutex<std::collections::HashMap<String, Vec<crate::ai::Model>>>,
    /// The last AI export scanned, waiting for the user to pick what to import.
    pub ai_scan: Mutex<Option<herbarium_core::importer::Scan>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            host: Mutex::new(Host::new()),
            models: Mutex::new(std::collections::HashMap::new()),
            ai_scan: Mutex::new(None),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

type CmdResult<T> = Result<T, String>;

/// Open a vault (create dir if needed), index it, persist it in the config.
fn open_vault(state: &State<'_, AppState>, path: &str) -> CmdResult<Config> {
    let mut host = state.host.lock().map_err(|e| e.to_string())?;
    let report = host.open_vault(path)?;
    let canonical = host.vault_path().map(|p| p.to_string_lossy().into_owned());
    eprintln!(
        "herbarium: vault opened at {} (indexed {}, removed {}, total {})",
        canonical.as_deref().unwrap_or_default(),
        report.indexed,
        report.removed,
        report.total
    );
    let (cfg, ()) = config::update(|cfg| {
        cfg.vault_path = canonical.clone();
        if let Some(p) = &canonical {
            config::add_recent(cfg, p);
        }
        Ok(())
    })?;
    Ok(cfg)
}

#[tauri::command]
pub async fn get_config() -> CmdResult<Config> {
    config::load()
}

#[tauri::command]
pub async fn set_vault(state: State<'_, AppState>, path: String) -> CmdResult<Config> {
    open_vault(&state, &path)
}

#[tauri::command]
pub async fn create_vault(
    state: State<'_, AppState>,
    parent_dir: String,
    name: String,
) -> CmdResult<Config> {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
        return Err("invalid folder name".into());
    }
    let target = PathBuf::from(&parent_dir).join(name);
    open_vault(&state, &target.to_string_lossy())
}

#[tauri::command]
pub async fn remove_recent_vault(path: String) -> CmdResult<Config> {
    let (cfg, ()) = config::update(|cfg| {
        config::remove_recent(cfg, &path);
        Ok(())
    })?;
    Ok(cfg)
}

#[tauri::command]
pub async fn set_close_to_tray(enabled: bool) -> CmdResult<Config> {
    let (cfg, ()) = config::update(|cfg| {
        cfg.close_to_tray = enabled;
        Ok(())
    })?;
    Ok(cfg)
}

#[tauri::command]
pub async fn reveal_page(state: State<'_, AppState>, page_id: String) -> CmdResult<()> {
    let path = {
        let host = state.host.lock().map_err(|e| e.to_string())?;
        let vault = host.vault_path().ok_or("no vault open")?.to_path_buf();
        let page = host.call(Caller::Ui, "pages.get", json!({ "id": page_id }))?;
        let folder = page["meta"]["folder"].as_str().map(str::to_owned);
        herbarium_core::vault::html_path(&vault, &page_id, folder.as_deref())
    };
    if !path.is_file() {
        return Err(format!("page file not found: {}", path.display()));
    }
    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reveal_vault(state: State<'_, AppState>) -> CmdResult<()> {
    let vault = {
        let host = state.host.lock().map_err(|e| e.to_string())?;
        host.vault_path().ok_or("no vault open")?.to_path_buf()
    };
    if !vault.is_dir() {
        return Err(format!("vault directory not found: {}", vault.display()));
    }
    tauri_plugin_opener::reveal_item_in_dir(&vault).map_err(|e| e.to_string())
}

fn archive_dir<W: std::io::Write + std::io::Seek>(
    root: &Path,
    dir: &Path,
    zip: &mut ZipWriter<W>,
    options: SimpleFileOptions,
    excluded: &[PathBuf],
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();

        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }

        let Ok(rel_path) = path.strip_prefix(root) else {
            continue;
        };

        let rel_str: String = rel_path
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");

        // Exclusions:
        // 1. Exclude .herbarium/index.sqlite*
        if rel_str == ".herbarium/index.sqlite" || rel_str.starts_with(".herbarium/index.sqlite") {
            continue;
        }
        // 2. Exclude .herbarium/trash and anything inside it
        if rel_str == ".herbarium/trash" || rel_str.starts_with(".herbarium/trash/") {
            continue;
        }
        // 3. Exclude the destination archive and its in-progress temp file
        // when they live inside the vault.
        if !excluded.is_empty()
            && let Ok(canon) = path.canonicalize()
            && excluded.contains(&canon)
        {
            continue;
        }

        if file_type.is_dir() {
            let dir_name = if rel_str.ends_with('/') {
                rel_str.clone()
            } else {
                format!("{rel_str}/")
            };
            zip.add_directory(&dir_name, options)
                .map_err(|e| e.to_string())?;
            archive_dir(root, &path, zip, options, excluded)?;
        } else if file_type.is_file() {
            zip.start_file(&rel_str, options)
                .map_err(|e| e.to_string())?;
            let mut f = std::fs::File::open(&path).map_err(|e| e.to_string())?;
            std::io::copy(&mut f, zip).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Unique temp path beside `dest` so the final rename stays on one filesystem
/// and is atomic.
fn temp_archive_path(dest: &Path) -> PathBuf {
    let parent = dest.parent().filter(|p| !p.as_os_str().is_empty());
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "herbarium-export.zip".to_owned());
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp_name = format!(".{name}.tmp-{}-{unique}", std::process::id());
    match parent {
        Some(p) => p.join(temp_name),
        None => PathBuf::from(temp_name),
    }
}

/// Build a ZIP with `build`, sync it, then atomically rename it over `dest`.
/// Any failure removes the temp file and leaves an existing archive untouched.
fn write_zip_atomically<F>(dest: &Path, build: F) -> Result<(), String>
where
    F: FnOnce(&mut ZipWriter<std::fs::File>, &Path) -> Result<(), String>,
{
    if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create parent dir for zip: {e}"))?;
    }

    let temp_path = temp_archive_path(dest);
    let outcome = (|| -> Result<(), String> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(|e| format!("cannot create zip file: {e}"))?;
        let mut zip = ZipWriter::new(file);
        build(&mut zip, &temp_path)?;
        let file = zip
            .finish()
            .map_err(|e| format!("failed to finish zip: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("failed to sync zip: {e}"))?;
        Ok(())
    })();

    if let Err(e) = outcome {
        let _ = std::fs::remove_file(&temp_path);
        return Err(e);
    }

    std::fs::rename(&temp_path, dest).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        format!("cannot replace {}: {e}", dest.display())
    })
}

fn export_vault_to(vault: &Path, dest: &Path) -> Result<(), String> {
    if !vault.is_dir() {
        return Err(format!("vault directory not found: {}", vault.display()));
    }
    write_zip_atomically(dest, |zip, temp_path| {
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut excluded = Vec::new();
        if let Ok(canon_dest) = dest.canonicalize() {
            excluded.push(canon_dest);
        }
        if let Ok(canon_temp) = temp_path.canonicalize() {
            excluded.push(canon_temp);
        }
        archive_dir(vault, vault, zip, options, &excluded)
            .map_err(|e| format!("failed to archive vault: {e}"))
    })
}

fn export_page_to(html_file: &Path, page_id: &str, dest: &Path) -> Result<(), String> {
    if !html_file.is_file() {
        return Err(format!("page file not found: {}", html_file.display()));
    }
    write_zip_atomically(dest, |zip, _temp_path| {
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file(format!("{page_id}.html"), options)
            .map_err(|e| e.to_string())?;
        let mut f = std::fs::File::open(html_file).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, zip).map_err(|e| e.to_string())?;
        Ok(())
    })
}

#[tauri::command]
pub async fn export_vault(state: State<'_, AppState>, dest_zip: String) -> CmdResult<()> {
    let vault = {
        let host = state.host.lock().map_err(|e| e.to_string())?;
        host.vault_path().ok_or("no vault open")?.to_path_buf()
    };
    export_vault_to(&vault, &PathBuf::from(&dest_zip))
}

#[tauri::command]
pub async fn export_page(
    state: State<'_, AppState>,
    page_id: String,
    dest_zip: String,
) -> CmdResult<()> {
    let (vault, folder) = {
        let host = state.host.lock().map_err(|e| e.to_string())?;
        let vault = host.vault_path().ok_or("no vault open")?.to_path_buf();
        let page = host.call(Caller::Ui, "pages.get", json!({ "id": page_id }))?;
        let folder = page["meta"]["folder"].as_str().map(str::to_owned);
        (vault, folder)
    };
    let html_file = herbarium_core::vault::html_path(&vault, &page_id, folder.as_deref());
    export_page_to(&html_file, &page_id, &PathBuf::from(&dest_zip))
}
/// Read pages for an export while holding the vault lock only as long as
/// needed. `folder` limits it to a folder and its subfolders.
fn pages_for_export(
    state: &State<'_, AppState>,
    folder: Option<&str>,
    only: Option<&str>,
) -> CmdResult<(PathBuf, Vec<crate::export::ExportPage>)> {
    let host = state.host.lock().map_err(|e| e.to_string())?;
    let vault = host.vault_path().ok_or("no vault open")?.to_path_buf();
    let ids: Vec<String> = match only {
        Some(id) => vec![id.to_string()],
        None => {
            let list = host.call(Caller::Ui, "pages.list", json!({ "folder": folder }))?;
            list.as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|m| m["id"].as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        }
    };
    let mut pages = Vec::with_capacity(ids.len());
    for id in ids {
        let page = host.call(
            Caller::Ui,
            "pages.get",
            json!({ "id": id, "format": "html" }),
        )?;
        let meta = &page["meta"];
        pages.push(crate::export::ExportPage {
            title: meta["title"].as_str().unwrap_or(&id).to_string(),
            folder: meta["folder"].as_str().map(str::to_string),
            tags: meta["tags"]
                .as_array()
                .map(|t| {
                    t.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            html: page["html"].as_str().unwrap_or_default().to_string(),
            id,
        });
    }
    Ok((vault, pages))
}

/// Save one page as a self-contained HTML file (local assets inlined).
#[tauri::command]
pub async fn export_page_html(
    state: State<'_, AppState>,
    page_id: String,
    dest: String,
) -> CmdResult<()> {
    let (vault, pages) = pages_for_export(&state, None, Some(&page_id))?;
    let page = pages.first().ok_or("page not found")?;
    let html = crate::export::standalone(&vault, page);
    herbarium_core::vault::write_atomic(&PathBuf::from(dest), html.as_bytes())
}

/// Publish the vault, or one folder of it, as a static website in `dest_dir`
/// (a new or empty folder).
#[tauri::command]
pub async fn export_site(
    state: State<'_, AppState>,
    folder: Option<String>,
    title: String,
    dest_dir: String,
) -> CmdResult<crate::export::SiteReport> {
    let (vault, pages) = pages_for_export(&state, folder.as_deref(), None)?;
    if pages.is_empty() {
        return Err("there are no pages to publish".into());
    }
    crate::export::write_site(&vault, &title, &pages, &PathBuf::from(dest_dir))
}

/// List the HTML artifacts in a Claude or ChatGPT data export (.zip or
/// conversations.json) and mark those already imported.
#[tauri::command]
pub async fn scan_ai_export(
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<crate::ai_import::Listing> {
    // Parsing a large export happens before the vault is locked.
    let scan =
        herbarium_core::importer::scan(&crate::ai_import::read_conversations(Path::new(&path))?)?;
    let have = {
        let host = state.host.lock().map_err(|e| e.to_string())?;
        herbarium_core::importer::imported_keys(host.store().ok_or("no vault open")?)?
    };
    let listing = crate::ai_import::Listing {
        candidates: scan
            .candidates
            .iter()
            .map(|c| crate::ai_import::Listed {
                already_imported: have.contains(&c.key),
                candidate: c.clone(),
            })
            .collect(),
        conversations: scan.conversations,
        unsupported: scan.unsupported,
    };
    *state.ai_scan.lock().map_err(|e| e.to_string())? = Some(scan);
    Ok(listing)
}

/// Import the chosen artifacts (by key) of the last scanned export.
#[tauri::command]
pub async fn import_ai_export(
    state: State<'_, AppState>,
    keys: Vec<String>,
    folder: Option<String>,
) -> CmdResult<crate::ai_import::ImportReport> {
    let scan = state
        .ai_scan
        .lock()
        .map_err(|e| e.to_string())?
        .take()
        .ok_or("scan the export again before importing")?;
    let keys: std::collections::HashSet<String> = keys.into_iter().collect();
    let host = state.host.lock().map_err(|e| e.to_string())?;
    crate::ai_import::import(
        &host,
        &scan.candidates,
        Some(&keys),
        folder.as_deref(),
        None,
    )
}

/// Where the copies of the extension live: one for Chromium browsers, one
/// for Firefox (see [`firefox_manifest`]).
struct ExtensionDirs {
    chromium: PathBuf,
    firefox: PathBuf,
}

/// Copy the browser extension bundled with the app to stable folders, for
/// "Load unpacked" until the extension is in the stores. (An AppImage's own
/// files vanish when it exits, so the copies are what browsers keep loading.)
fn prepare_extension(app: &tauri::AppHandle) -> CmdResult<ExtensionDirs> {
    use tauri::Manager;
    let bundled = app
        .path()
        .resource_dir()
        .map(|d| d.join("browser-extension"))
        .ok()
        .filter(|d| d.join("manifest.json").is_file());
    let source = match bundled {
        Some(dir) => dir,
        // `tauri dev`: the extension next to the app's sources.
        None if cfg!(debug_assertions) => {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../extension")
        }
        None => return Err("the browser extension is not bundled with this build".into()),
    };
    let base = dirs::data_dir().ok_or("no data folder")?.join("Herbarium");
    let dirs = ExtensionDirs {
        chromium: base.join("browser-extension"),
        firefox: base.join("browser-extension-firefox"),
    };
    let copy_err = |e: std::io::Error| format!("cannot copy the extension: {e}");
    copy_dir(&source, &dirs.chromium).map_err(copy_err)?;
    copy_dir(&source, &dirs.firefox).map_err(copy_err)?;
    let manifest_path = dirs.firefox.join("manifest.json");
    let manifest: Value = std::fs::read(&manifest_path)
        .map_err(copy_err)
        .and_then(|raw| serde_json::from_slice(&raw).map_err(|e| e.to_string()))?;
    let bytes =
        serde_json::to_vec_pretty(&firefox_manifest(manifest)).map_err(|e| e.to_string())?;
    herbarium_core::vault::write_atomic(&manifest_path, &bytes)?;
    Ok(dirs)
}

/// The extension's manifest as Firefox needs it. Chrome runs the background
/// as a service worker and warns about `background.scripts` in Manifest V3;
/// Firefox has no background service workers and loads the same code as
/// background scripts, `capture.js` first (`background.js` uses it).
///
/// Firefox also follows the browser theme for the toolbar icon itself
/// (`theme_icons`, unknown to Chrome, where an offscreen page watches the
/// mode, hence `offscreen`, a permission Firefox does not know).
/// Its `light` icon is the one shown on dark themes: the dark icon there,
/// matching the mode, and the light icon on light themes.
fn firefox_manifest(mut manifest: Value) -> Value {
    if let Some(background) = manifest
        .get_mut("background")
        .and_then(Value::as_object_mut)
    {
        let worker = background
            .remove("service_worker")
            .and_then(|w| w.as_str().map(str::to_owned))
            .unwrap_or_else(|| "background.js".into());
        background.insert("scripts".into(), json!(["capture.js", worker]));
    }
    if let Some(permissions) = manifest
        .get_mut("permissions")
        .and_then(Value::as_array_mut)
    {
        permissions.retain(|p| p != "offscreen");
    }
    if let Some(action) = manifest.get_mut("action").and_then(Value::as_object_mut) {
        let icons: Vec<Value> = [16, 32, 64]
            .iter()
            .map(|size| {
                json!({
                    "light": format!("icons/dark-{size}.png"),
                    "dark": format!("icons/light-{size}.png"),
                    "size": size,
                })
            })
            .collect();
        action.insert("theme_icons".into(), Value::Array(icons));
    }
    manifest
}

/// Copy the extension to its stable folders and show the one `browser` loads.
#[tauri::command]
pub async fn reveal_extension(app: tauri::AppHandle, firefox: bool) -> CmdResult<String> {
    let dirs = prepare_extension(&app)?;
    let dest = if firefox { dirs.firefox } else { dirs.chromium };
    tauri_plugin_opener::reveal_item_in_dir(dest.join("manifest.json"))
        .map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().into_owned())
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Save the clipboard's HTML as a page (the UI's answer to a clipboard offer).
#[tauri::command]
pub async fn save_clipboard_page(
    app: tauri::AppHandle,
    watchers: State<'_, std::sync::Arc<crate::capture::Watchers>>,
) -> CmdResult<String> {
    crate::capture::save_offered_clipboard(&app, &watchers)
}

/// Save an HTML file the Downloads watcher offered.
#[tauri::command]
pub async fn save_download(app: tauri::AppHandle, path: String) -> CmdResult<String> {
    crate::capture::save_download(&app, &path)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSettings {
    capture_shortcut: Option<String>,
    watch_downloads: bool,
    watch_clipboard: bool,
}

/// Change the capture shortcut and watchers; the shortcut is checked before
/// anything is saved.
#[tauri::command]
pub async fn set_capture(
    app: tauri::AppHandle,
    watchers: State<'_, std::sync::Arc<crate::capture::Watchers>>,
    settings: CaptureSettings,
) -> CmdResult<Config> {
    use std::sync::atomic::Ordering;
    let shortcut = settings
        .capture_shortcut
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let (cfg, ()) = config::update(|cfg| {
        if let Err(e) = crate::capture::set_shortcut(&app, shortcut.as_deref()) {
            // Put the previous shortcut back before reporting the bad one.
            let _ = crate::capture::set_shortcut(&app, cfg.capture_shortcut.as_deref());
            return Err(e);
        }
        cfg.capture_shortcut = shortcut;
        cfg.watch_downloads = settings.watch_downloads;
        cfg.watch_clipboard = settings.watch_clipboard;
        Ok(())
    })?;
    watchers
        .downloads
        .store(cfg.watch_downloads, Ordering::Relaxed);
    watchers
        .clipboard
        .store(cfg.watch_clipboard, Ordering::Relaxed);
    Ok(cfg)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStatus {
    browser: String,
    /// The native host is registered with it.
    connected: bool,
    firefox: bool,
    /// When its extension last reached the app (unix ms).
    seen: Option<i64>,
    /// The app can start it (to open its extensions page).
    can_open: bool,
}

/// Browsers found, whether the extension's native host is registered with
/// each, and when each one's extension last reached the app.
#[tauri::command]
pub async fn browser_status() -> Vec<BrowserStatus> {
    tauri::async_runtime::spawn_blocking(|| {
        let seen = crate::browsers::seen();
        crate::native_host::status()
            .into_iter()
            .map(|(browser, connected)| BrowserStatus {
                firefox: browser == "Firefox",
                seen: seen.get(&browser).copied(),
                can_open: crate::browsers::can_open(&browser),
                browser,
                connected,
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSetup {
    browsers: Vec<BrowserStatus>,
    /// The folder to choose in "Load unpacked" (Chromium browsers).
    extension_dir: String,
    /// The folder whose `manifest.json` Firefox loads as a temporary add-on.
    firefox_extension_dir: String,
    /// A signed Firefox package, when this build ships one.
    firefox_package: Option<String>,
}

/// Get everything ready for adding the extension: register the native host
/// with every browser found and copy the extension to its stable folders.
#[tauri::command]
pub async fn browser_setup(app: tauri::AppHandle) -> CmdResult<BrowserSetup> {
    // No browser found is not an error here: the list says so.
    let _ = crate::native_host::install_default();
    let dirs = prepare_extension(&app)?;
    let xpi = dirs.firefox.join("herbarium.xpi");
    Ok(BrowserSetup {
        browsers: browser_status().await,
        extension_dir: dirs.chromium.to_string_lossy().into_owned(),
        firefox_extension_dir: dirs.firefox.to_string_lossy().into_owned(),
        firefox_package: xpi.is_file().then(|| xpi.to_string_lossy().into_owned()),
    })
}

/// Open `browser` on its extensions page (`page: "extensions"`), or on the
/// signed Firefox package (`page: "package"`), which Firefox offers to install.
#[tauri::command]
pub async fn open_browser(app: tauri::AppHandle, browser: String, page: String) -> CmdResult<()> {
    let target = match page.as_str() {
        "package" => prepare_extension(&app)?
            .firefox
            .join("herbarium.xpi")
            .to_string_lossy()
            .into_owned(),
        _ => crate::browsers::extensions_page(&browser).to_string(),
    };
    crate::browsers::open_in(&browser, &target)
}

/// Register the native messaging host with every browser found.
#[tauri::command]
pub async fn connect_browsers() -> CmdResult<Vec<String>> {
    crate::native_host::install_default()
}

#[tauri::command]
pub async fn invoke_op(state: State<'_, AppState>, name: String, args: Value) -> CmdResult<Value> {
    let host = state.host.lock().map_err(|e| e.to_string())?;
    host.call(Caller::Ui, &name, args)
}

/// Text editors installed on this system, best candidates first.
#[tauri::command]
pub async fn list_editors() -> Vec<EditorInfo> {
    editors::list()
}

/// Open a page's HTML file in an external editor and return the editor's name.
/// `custom` (a command template) wins over `editor` (a detected id); with
/// neither, the first detected editor is used.
#[tauri::command]
pub async fn open_in_editor(
    state: State<'_, AppState>,
    page_id: String,
    editor: Option<String>,
    custom: Option<String>,
) -> CmdResult<String> {
    let path = {
        let host = state.host.lock().map_err(|e| e.to_string())?;
        let vault = host.vault_path().ok_or("no vault open")?.to_path_buf();
        let page = host.call(Caller::Ui, "pages.get", json!({ "id": page_id }))?;
        let folder = page["meta"]["folder"].as_str().map(str::to_owned);
        herbarium_core::vault::html_path(&vault, &page_id, folder.as_deref())
    };
    if !path.is_file() {
        return Err(format!("page file not found: {}", path.display()));
    }
    editors::open(&Choice { editor, custom }, &path)
}

/// Open a web link in the system browser. The UI calls this only after the
/// user confirms a link a page tried to follow (see `navigation`).
#[tauri::command]
pub async fn open_external(app: tauri::AppHandle, url: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let parsed = tauri::Url::parse(&url).map_err(|e| format!("invalid URL: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(format!("only web links can be opened: {url}"));
    }
    app.opener()
        .open_url(parsed.as_str(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firefox_gets_background_scripts_and_chrome_a_clean_worker() {
        let chrome: Value =
            serde_json::from_str(include_str!("../../extension/manifest.json")).unwrap();
        assert_eq!(
            chrome["background"],
            json!({ "service_worker": "background.js" }),
            "Chrome warns about `background.scripts` in Manifest V3"
        );
        let firefox = firefox_manifest(chrome.clone());
        assert_eq!(
            firefox["background"],
            json!({ "scripts": ["capture.js", "background.js"] })
        );
        assert_eq!(firefox["version"], chrome["version"]);
        assert_eq!(
            firefox["browser_specific_settings"],
            chrome["browser_specific_settings"]
        );
        assert!(
            chrome["permissions"]
                .as_array()
                .unwrap()
                .contains(&json!("offscreen"))
        );
        assert!(
            !firefox["permissions"]
                .as_array()
                .unwrap()
                .contains(&json!("offscreen"))
        );
        // Every icon the manifests name is shipped (`pnpm icons` renders them).
        let ext = Path::new(env!("CARGO_MANIFEST_DIR")).join("../extension");
        let theme_icons = firefox["action"]["theme_icons"].as_array().unwrap();
        let named = theme_icons
            .iter()
            .flat_map(|i| [&i["light"], &i["dark"]])
            .chain(chrome["icons"].as_object().unwrap().values())
            .chain(
                chrome["action"]["default_icon"]
                    .as_object()
                    .unwrap()
                    .values(),
            );
        for icon in named {
            let icon = icon.as_str().unwrap();
            assert!(ext.join(icon).is_file(), "{icon} is missing");
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "herbarium_cmd_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn zip_names(path: &Path) -> Vec<String> {
        let file = std::fs::File::open(path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect()
    }

    /// A failing build must never truncate or replace the selected archive.
    #[test]
    fn failed_zip_preserves_existing_archive() {
        let dir = temp_dir("fail");
        let dest = dir.join("vault.zip");
        std::fs::write(&dest, b"OLD-ARCHIVE").unwrap();

        let err = write_zip_atomically(&dest, |_zip, _temp| -> Result<(), String> {
            Err("boom".to_string())
        })
        .unwrap_err();
        assert!(err.contains("boom"));
        assert_eq!(std::fs::read(&dest).unwrap(), b"OLD-ARCHIVE");

        // The temp file is cleaned up: only the original archive remains.
        let mut names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        names.sort();
        assert_eq!(names, vec![std::ffi::OsString::from("vault.zip")]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Export produces a readable ZIP, hides database/trash internals, keeps
    /// valid double-dot filenames, and never archives its own destination.
    #[test]
    fn export_writes_valid_zip_and_skips_metadata_and_destination() {
        let dir = temp_dir("vault");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::create_dir_all(dir.join(".herbarium/trash/gone")).unwrap();
        std::fs::write(dir.join("notes.html"), b"<p>hi</p>").unwrap();
        std::fs::write(dir.join("sub/release..notes.html"), b"<p>deep</p>").unwrap();
        std::fs::write(dir.join(".herbarium/index.sqlite"), b"db").unwrap();
        std::fs::write(dir.join(".herbarium/index.sqlite-wal"), b"wal").unwrap();
        std::fs::write(dir.join(".herbarium/network.json"), b"{}").unwrap();
        std::fs::write(dir.join(".herbarium/trash/gone/gone.html"), b"t").unwrap();

        // Destination inside the vault must not include itself.
        let dest = dir.join("export.zip");
        export_vault_to(&dir, &dest).unwrap();

        let names = zip_names(&dest);
        assert!(names.contains(&"notes.html".to_string()));
        assert!(names.contains(&"sub/release..notes.html".to_string()));
        assert!(names.contains(&".herbarium/network.json".to_string()));
        assert!(
            !names
                .iter()
                .any(|n| n.starts_with(".herbarium/index.sqlite"))
        );
        assert!(!names.iter().any(|n| n.starts_with(".herbarium/trash")));
        assert!(!names.contains(&"export.zip".to_string()));
        assert!(!names.iter().any(|n| n.contains(".tmp-")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A successful export replaces (not appends to) a prior archive.
    #[test]
    fn successful_export_replaces_previous_archive() {
        let dir = temp_dir("replace");
        std::fs::write(dir.join("page.html"), b"<p>1</p>").unwrap();
        let dest = dir.join("out.zip");
        std::fs::write(&dest, b"not a zip yet").unwrap();

        export_vault_to(&dir, &dest).unwrap();

        assert!(zip_names(&dest).contains(&"page.html".to_string()));
        assert!(!std::fs::read(&dest).unwrap().starts_with(b"not a zip"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettings {
    provider: String,
    model: String,
    base_url: Option<String>,
    /// Providers with a stored key (the keys never leave the backend).
    keys: Vec<&'static str>,
    providers: &'static [crate::ai::Provider],
}

fn ai_settings_of(cfg: &Config) -> AiSettings {
    let secrets = crate::secrets::load();
    AiSettings {
        provider: cfg.ai_provider.clone(),
        model: cfg.ai_model.clone(),
        base_url: cfg.ai_base_url.clone(),
        keys: crate::ai::PROVIDERS
            .iter()
            .filter(|p| secrets.ai_key(p.id).is_some())
            .map(|p| p.id)
            .collect(),
        providers: crate::ai::PROVIDERS,
    }
}

#[tauri::command]
pub async fn ai_settings() -> AiSettings {
    ai_settings_of(&config::load().unwrap_or_default())
}

/// Change the AI provider, model and API address; `key` stores (or, blank,
/// removes) that provider's key, and is left alone when absent.
#[tauri::command]
pub async fn set_ai_settings(
    provider: String,
    model: String,
    base_url: Option<String>,
    key: Option<String>,
) -> CmdResult<AiSettings> {
    if crate::ai::provider(&provider).is_none() {
        return Err(format!("unknown AI provider `{provider}`"));
    }
    let model = model.trim();
    if model.len() > 200 || model.chars().any(char::is_whitespace) {
        return Err("enter a model name such as claude-opus-5-5".into());
    }
    let base_url = base_url
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty());
    if let Some(url) = &base_url {
        crate::ai::check_url(url)?;
    }
    if key.is_some() {
        crate::secrets::set_ai_key(&provider, key)?;
    }
    let (cfg, ()) = config::update(|cfg| {
        cfg.ai_provider = provider;
        // Empty means automatic: the service's recommended model at each remix.
        cfg.ai_model = model.to_string();
        cfg.ai_base_url = base_url;
        Ok(())
    })?;
    Ok(ai_settings_of(&cfg))
}

#[derive(serde::Serialize)]
pub struct ModelList {
    models: Vec<crate::ai::Model>,
    recommended: Option<String>,
}

/// The models `provider` offers with its stored key (and, for the current
/// provider, the configured address), and the one to recommend. Kept for the
/// session unless `refresh`. Reading the list also proves the key works.
#[tauri::command]
pub async fn ai_models(
    state: State<'_, AppState>,
    provider: String,
    refresh: bool,
) -> CmdResult<ModelList> {
    let info = crate::ai::provider(&provider)
        .ok_or_else(|| format!("unknown AI provider `{provider}`"))?;
    let cfg = config::load().unwrap_or_default();
    let base = (cfg.ai_provider == provider)
        .then_some(cfg.ai_base_url)
        .flatten();
    let cache_key = format!("{provider}|{}", base.as_deref().unwrap_or_default());
    let cached = if refresh {
        None
    } else {
        state
            .models
            .lock()
            .map_err(|e| e.to_string())?
            .get(&cache_key)
            .cloned()
    };
    let models = match cached {
        Some(models) => models,
        None => {
            let key = crate::secrets::load().ai_key(&provider);
            let models = tauri::async_runtime::spawn_blocking(move || {
                crate::ai::list_models(info, key.as_deref(), base.as_deref())
            })
            .await
            .map_err(|e| e.to_string())??;
            state
                .models
                .lock()
                .map_err(|e| e.to_string())?
                .insert(cache_key, models.clone());
            models
        }
    };
    let recommended = crate::ai::recommend(info, &models);
    Ok(ModelList {
        models,
        recommended,
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeCodeStatus {
    /// Where it was found, if it was.
    path: Option<String>,
    /// What `claude --version` said.
    version: Option<String>,
    /// Why it cannot be used.
    error: Option<String>,
    /// The user pointed at it themselves.
    chosen: bool,
}

/// Look for Claude Code (where the user put it, or wherever a terminal
/// would find it) and ask it its version.
#[tauri::command]
pub async fn claude_code_status() -> ClaudeCodeStatus {
    let chosen = config::load().unwrap_or_default().claude_code_path;
    tauri::async_runtime::spawn_blocking(move || {
        let found = crate::locate::claude(chosen.as_deref());
        let (path, version, error) = match found {
            Ok(path) => match crate::locate::claude_version(&path) {
                Ok(v) => (Some(path), Some(v), None),
                Err(e) => (Some(path), None, Some(e)),
            },
            Err(e) => (None, None, Some(e)),
        };
        ClaudeCodeStatus {
            path: path.map(|p| p.to_string_lossy().into_owned()),
            version,
            error,
            chosen: chosen.is_some_and(|c| !c.trim().is_empty()),
        }
    })
    .await
    .unwrap_or(ClaudeCodeStatus {
        path: None,
        version: None,
        error: Some("could not look for Claude Code".into()),
        chosen: false,
    })
}

/// Point at Claude Code by hand (None goes back to finding it), after
/// checking the file is Claude Code.
#[tauri::command]
pub async fn set_claude_code_path(path: Option<String>) -> CmdResult<ClaudeCodeStatus> {
    let path = path.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
    if let Some(p) = &path {
        let check = PathBuf::from(p);
        tauri::async_runtime::spawn_blocking(move || crate::locate::claude_version(&check))
            .await
            .map_err(|e| e.to_string())??;
    }
    config::update(|cfg| {
        cfg.claude_code_path = path;
        Ok(())
    })?;
    Ok(claude_code_status().await)
}

/// Remix a page with the configured model and keep the result as a proposal.
/// Emits `remix-progress` `{ id, chars }` while the answer comes in.
#[tauri::command]
pub async fn remix_page(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    preset: String,
    instructions: String,
) -> CmdResult<Value> {
    use tauri::Emitter;

    let running = crate::remix::start(&id)?;
    let (prompt, base_updated_at) =
        crate::remix::prompt_for(&state.host, &id, &preset, &instructions)?;

    let cfg = config::load().unwrap_or_default();
    let key = crate::secrets::load().ai_key(&cfg.ai_provider);
    let page_id = id.clone();
    let emitter = app.clone();
    // `running` stays held until the proposal is saved.
    let stop = running.flag();
    let remixed = tauri::async_runtime::spawn_blocking(move || {
        // No model saved: the service's recommended one.
        let model = if cfg.ai_model.trim().is_empty() {
            crate::ai::provider(&cfg.ai_provider)
                .map(|p| crate::ai::resolve_model(p, key.as_deref(), cfg.ai_base_url.as_deref()))
                .unwrap_or_default()
        } else {
            cfg.ai_model.clone()
        };
        let job = crate::remix::Job {
            provider: &cfg.ai_provider,
            model: &model,
            key: key.as_deref(),
            base_url: cfg.ai_base_url.as_deref(),
            claude_path: cfg.claude_code_path.as_deref(),
            prompt,
        };
        crate::remix::run(job, |chars| {
            let _ = emitter.emit("remix-progress", json!({ "id": page_id, "chars": chars }));
            !stop.load(std::sync::atomic::Ordering::Relaxed)
        })
    })
    .await
    .map_err(|e| e.to_string())??;

    let proposal = crate::remix::propose(&state.host, &id, &remixed, base_updated_at);
    drop(running);
    proposal
}

#[tauri::command]
pub async fn cancel_remix(id: String) {
    crate::remix::cancel(&id);
}

/// What `organize_plan` registers itself as, for progress and cancelling.
pub const ORGANIZE_KEY: &str = "organize:library";

/// Ask the configured model how the library should be organized and return
/// its plan (nothing is changed). `scope` is `unsorted` or `all`. Emits
/// `remix-progress` `{ id: "organize:library", chars }` while the answer comes in.
#[tauri::command]
pub async fn organize_plan(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    scope: String,
    instructions: String,
) -> CmdResult<crate::organize::Plan> {
    use tauri::Emitter;

    let scope = crate::organize::Scope::parse(&scope)?;
    let running = crate::remix::start(ORGANIZE_KEY)?;
    let library = crate::organize::gather(&state.host, scope)?;
    if library.entries.is_empty() {
        return Err("there are no pages to organize".into());
    }
    let prompt = crate::organize::prompt(&library, &instructions);

    let cfg = config::load().unwrap_or_default();
    let key = crate::secrets::load().ai_key(&cfg.ai_provider);
    let emitter = app.clone();
    let stop = running.flag();
    let answer = tauri::async_runtime::spawn_blocking(move || {
        let model = if cfg.ai_model.trim().is_empty() {
            crate::ai::provider(&cfg.ai_provider)
                .map(|p| crate::ai::resolve_model(p, key.as_deref(), cfg.ai_base_url.as_deref()))
                .unwrap_or_default()
        } else {
            cfg.ai_model.clone()
        };
        let job = crate::remix::Job {
            provider: &cfg.ai_provider,
            model: &model,
            key: key.as_deref(),
            base_url: cfg.ai_base_url.as_deref(),
            claude_path: cfg.claude_code_path.as_deref(),
            prompt,
        };
        crate::remix::run_text(job, crate::organize::SYSTEM, |chars| {
            let _ = emitter.emit(
                "remix-progress",
                json!({ "id": ORGANIZE_KEY, "chars": chars }),
            );
            !stop.load(std::sync::atomic::Ordering::Relaxed)
        })
        .map(|text| (text, library))
    })
    .await
    .map_err(|e| e.to_string())??;
    drop(running);
    crate::organize::parse_plan(&answer.0, &answer.1)
}

#[tauri::command]
pub async fn cancel_organize() {
    crate::remix::cancel(ORGANIZE_KEY);
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubSettings {
    has_token: bool,
    login: Option<String>,
    repo: String,
}

fn github_settings_of(cfg: &Config) -> GithubSettings {
    GithubSettings {
        has_token: crate::secrets::load().github_token.is_some(),
        login: cfg.github_login.clone(),
        repo: cfg.publish_repo.clone(),
    }
}

#[tauri::command]
pub async fn github_settings() -> GithubSettings {
    github_settings_of(&config::load().unwrap_or_default())
}

/// Store (checking it with GitHub first) or remove (blank) the token, when
/// `token` is given, and set the site's repository.
#[tauri::command]
pub async fn set_github(token: Option<String>, repo: String) -> CmdResult<GithubSettings> {
    let repo = repo.trim().to_string();
    crate::publish::check_repo_name(&repo)?;
    // Check a new token with GitHub first: the config is not held meanwhile,
    // so other settings changed during the check are kept.
    let token = token.map(|t| t.trim().to_string());
    let login = match token.as_deref() {
        Some(t) if !t.is_empty() => {
            let check = t.to_string();
            Some(
                tauri::async_runtime::spawn_blocking(move || {
                    crate::publish::GitHub::new(&check)?.login()
                })
                .await
                .map_err(|e| e.to_string())??,
            )
        }
        _ => None,
    };
    if let Some(token) = &token {
        crate::secrets::set(
            |s, v| s.github_token = v,
            (!token.is_empty()).then(|| token.clone()),
        )?;
    }
    let (cfg, ()) = config::update(|cfg| {
        if token.is_some() {
            cfg.github_login = login;
        }
        cfg.publish_repo = repo;
        Ok(())
    })?;
    Ok(github_settings_of(&cfg))
}

/// Publish a page (`target`: `gist` or `site`) or, with `unpublish`, take it
/// down. Returns the record (`url`…).
#[tauri::command]
pub async fn publish_page(
    app: tauri::AppHandle,
    id: String,
    target: String,
    unpublish: bool,
) -> CmdResult<Value> {
    use tauri::Manager;
    let token = crate::secrets::load()
        .github_token
        .ok_or("connect your GitHub account in Settings → AI & sharing first")?;
    let repo = config::load().unwrap_or_default().publish_repo;
    // Network calls run off the async runtime, and the vault is locked only
    // while a step reads or records.
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        crate::publish::run(&state.host, &token, &id, &target, &repo, unpublish)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Put rich content on the clipboard: `html`, with `text` for apps that take
/// no HTML.
#[tauri::command]
pub async fn copy_rich(app: tauri::AppHandle, html: String, text: String) -> CmdResult<()> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard()
        .write_html(html, Some(text))
        .map_err(|e| format!("could not copy: {e}"))
}
