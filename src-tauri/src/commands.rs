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
    /// The last AI export scanned, waiting for the user to pick what to import.
    pub ai_scan: Mutex<Option<herbarium_core::importer::Scan>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            host: Mutex::new(Host::new()),
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
    let mut cfg = config::load().unwrap_or_default();
    cfg.vault_path = canonical.clone();
    if let Some(p) = &canonical {
        config::add_recent(&mut cfg, p);
    }
    config::save(&cfg)?;
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
    let mut cfg = config::load().unwrap_or_default();
    config::remove_recent(&mut cfg, &path);
    config::save(&cfg)?;
    Ok(cfg)
}

#[tauri::command]
pub async fn set_close_to_tray(enabled: bool) -> CmdResult<Config> {
    let mut cfg = config::load().unwrap_or_default();
    cfg.close_to_tray = enabled;
    config::save(&cfg)?;
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
    crate::ai_import::import(&host, &scan.candidates, Some(&keys), folder.as_deref())
}

/// Copy the browser extension bundled with the app to a stable folder and
/// reveal it, for "Load unpacked" until the extension is in the stores. (An
/// AppImage's own files vanish when it exits, so the copy is what browsers
/// keep loading.)
#[tauri::command]
pub async fn reveal_extension(app: tauri::AppHandle) -> CmdResult<String> {
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
    let dest = dirs::data_dir()
        .ok_or("no data folder")?
        .join("Herbarium")
        .join("browser-extension");
    copy_dir(&source, &dest).map_err(|e| format!("cannot copy the extension: {e}"))?;
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
pub async fn save_clipboard_page(app: tauri::AppHandle) -> CmdResult<String> {
    crate::capture::save_clipboard(&app)
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
    let mut cfg = config::load().unwrap_or_default();
    let shortcut = settings
        .capture_shortcut
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Err(e) = crate::capture::set_shortcut(&app, shortcut.as_deref()) {
        // Put the previous shortcut back before reporting the bad one.
        let _ = crate::capture::set_shortcut(&app, cfg.capture_shortcut.as_deref());
        return Err(e);
    }
    cfg.capture_shortcut = shortcut;
    cfg.watch_downloads = settings.watch_downloads;
    cfg.watch_clipboard = settings.watch_clipboard;
    watchers
        .downloads
        .store(cfg.watch_downloads, Ordering::Relaxed);
    watchers
        .clipboard
        .store(cfg.watch_clipboard, Ordering::Relaxed);
    config::save(&cfg)?;
    Ok(cfg)
}

#[derive(serde::Serialize)]
pub struct BrowserStatus {
    browser: String,
    connected: bool,
}

/// Browsers found and whether the extension's native host is registered with each.
#[tauri::command]
pub async fn browser_status() -> Vec<BrowserStatus> {
    crate::native_host::status()
        .into_iter()
        .map(|(browser, connected)| BrowserStatus { browser, connected })
        .collect()
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
