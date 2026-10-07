// Quick capture outside the window: a global shortcut (and the tray) saves
// the clipboard's HTML as a page; optional watchers offer to save HTML files
// that land in the Downloads folder and whole HTML pages that get copied.
// Everything lands in the Inbox with network access off, like the browser
// extension's saves.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use herbarium_core::Caller;
use herbarium_core::content::{extract_title, looks_like_html};
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_notification::NotificationExt;

use crate::AppState;
use crate::native_host::INBOX;

/// Event telling the UI the vault changed outside it (reload the lists).
pub const VAULT_CHANGED: &str = "vault-changed";
/// Event offering a downloaded HTML file: `Offer`.
pub const DOWNLOAD_OFFER: &str = "download-offer";
/// Event offering a copied HTML page: `Offer` without a path.
pub const CLIPBOARD_OFFER: &str = "clipboard-offer";

/// Largest file or clipboard text saved.
const MAX_BYTES: u64 = 20 * 1024 * 1024;
const POLL: Duration = Duration::from_secs(3);

/// Which watchers run; flipped from settings without restarting threads.
#[derive(Default)]
pub struct Watchers {
    pub downloads: AtomicBool,
    pub clipboard: AtomicBool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub title: String,
    pub bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// The clipboard text as a page: a whole document as is, an HTML fragment
/// wrapped in one; plain text is not a page.
pub fn page_from_text(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty()
        || text.len() as u64 > MAX_BYTES
        || !text.contains('<')
        || !looks_like_html(text)
    {
        return None;
    }
    let lower: String = text
        .chars()
        .take(256)
        .collect::<String>()
        .to_ascii_lowercase();
    if lower.starts_with("<!doctype html") || lower.contains("<html") {
        return Some(text.to_string());
    }
    Some(format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>Clipping</title></head><body>\n{text}\n</body></html>"
    ))
}

/// A whole HTML page (not a fragment): what the clipboard watcher offers.
fn is_whole_page(text: &str) -> bool {
    let lower: String = text
        .trim_start()
        .chars()
        .take(256)
        .collect::<String>()
        .to_ascii_lowercase();
    (lower.starts_with("<!doctype html") || lower.starts_with("<html")) && looks_like_html(text)
}

/// Save `html` into the Inbox and tell the UI. Returns the new page's title.
fn save_page<R: Runtime>(
    app: &AppHandle<R>,
    html: &str,
    title: Option<&str>,
) -> Result<String, String> {
    let state = app.state::<AppState>();
    let meta = {
        let host = state.host.lock().map_err(|e| e.to_string())?;
        let mut args = json!({ "html": html, "folder": INBOX, "allowCdn": false });
        if let Some(t) = title.filter(|t| !t.trim().is_empty()) {
            args["title"] = json!(t);
        }
        host.call(Caller::Ui, "pages.create", args)?
    };
    let _ = app.emit(VAULT_CHANGED, ());
    Ok(meta["title"].as_str().unwrap_or_default().to_string())
}

fn notify<R: Runtime>(app: &AppHandle<R>, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

/// Save the clipboard's HTML (shortcut, tray, or the UI's "Save" on an offer).
pub fn save_clipboard<R: Runtime>(app: &AppHandle<R>) -> Result<String, String> {
    let text = app
        .clipboard()
        .read_text()
        .map_err(|_| "the clipboard holds no text".to_string())?;
    let html = page_from_text(&text).ok_or("the clipboard holds no HTML")?;
    save_page(app, &html, None)
}

/// The shortcut and tray action: save, then say what happened.
pub fn capture_clipboard<R: Runtime>(app: &AppHandle<R>) {
    match save_clipboard(app) {
        Ok(title) => notify(
            app,
            "Saved to Herbarium",
            &format!("“{title}” is in your Inbox."),
        ),
        Err(e) => notify(
            app,
            "Nothing saved",
            &format!("Herbarium could not save the clipboard: {e}."),
        ),
    }
}

/// The Downloads folder, if this system has one.
pub fn downloads_dir() -> Option<PathBuf> {
    dirs::download_dir()
}

fn is_html_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"))
}

/// HTML files in `dir` modified after `since` and not in `seen` (which is
/// updated), with what an offer needs.
pub fn new_html_files(dir: &Path, since: SystemTime, seen: &mut HashSet<PathBuf>) -> Vec<Offer> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_html_file(&path) || seen.contains(&path) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() || meta.len() > MAX_BYTES || meta.modified().is_ok_and(|m| m <= since) {
            continue;
        }
        seen.insert(path.clone());
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if !looks_like_html(&text) {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let title = Some(extract_title(&text))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| name.clone());
        out.push(Offer {
            title,
            bytes: text.len(),
            path: Some(path.to_string_lossy().into_owned()),
            name: Some(name),
        });
    }
    out
}

/// Save a downloaded file the UI offered; only HTML files in Downloads.
pub fn save_download<R: Runtime>(app: &AppHandle<R>, path: &str) -> Result<String, String> {
    let dir = downloads_dir().ok_or("no Downloads folder")?;
    let path = PathBuf::from(path);
    let canonical = path.canonicalize().map_err(|e| e.to_string())?;
    let dir = dir.canonicalize().map_err(|e| e.to_string())?;
    if !canonical.starts_with(&dir) || !is_html_file(&canonical) {
        return Err("only HTML files in the Downloads folder can be saved this way".into());
    }
    if std::fs::metadata(&canonical)
        .map_err(|e| e.to_string())?
        .len()
        > MAX_BYTES
    {
        return Err("the file is larger than 20 MiB".into());
    }
    let html = std::fs::read_to_string(&canonical).map_err(|e| e.to_string())?;
    save_page(app, &html, None)
}

fn window_visible<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// Start the watcher threads; they idle while their setting is off.
pub fn start_watchers<R: Runtime>(app: &AppHandle<R>, watchers: Arc<Watchers>) {
    let handle = app.clone();
    let flags = watchers.clone();
    std::thread::spawn(move || {
        let since = SystemTime::now();
        let mut seen = HashSet::new();
        loop {
            std::thread::sleep(POLL);
            if !flags.downloads.load(Ordering::Relaxed) {
                continue;
            }
            let Some(dir) = downloads_dir() else { continue };
            for offer in new_html_files(&dir, since, &mut seen) {
                if !window_visible(&handle) {
                    notify(
                        &handle,
                        "HTML page downloaded",
                        &format!("Open Herbarium to save “{}”.", offer.title),
                    );
                }
                let _ = handle.emit(DOWNLOAD_OFFER, offer);
            }
        }
    });

    let handle = app.clone();
    std::thread::spawn(move || {
        let mut last = String::new();
        let mut primed = false;
        loop {
            std::thread::sleep(POLL);
            if !watchers.clipboard.load(Ordering::Relaxed) {
                primed = false;
                continue;
            }
            let text = handle.clipboard().read_text().unwrap_or_default();
            // What was on the clipboard when watching started is not "new".
            if !primed {
                last = text;
                primed = true;
                continue;
            }
            if text == last {
                continue;
            }
            last = text.clone();
            if text.len() as u64 > MAX_BYTES || !is_whole_page(&text) {
                continue;
            }
            let title = Some(extract_title(&text))
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| "HTML page".into());
            if !window_visible(&handle) {
                notify(
                    &handle,
                    "HTML page copied",
                    &format!("Press the capture shortcut to save “{title}”."),
                );
            }
            let _ = handle.emit(
                CLIPBOARD_OFFER,
                Offer {
                    title,
                    bytes: text.len(),
                    path: None,
                    name: None,
                },
            );
        }
    });
}

/// Register (or clear, with None) the global capture shortcut.
pub fn set_shortcut<R: Runtime>(app: &AppHandle<R>, shortcut: Option<&str>) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| e.to_string())?;
    if let Some(s) = shortcut.map(str::trim).filter(|s| !s.is_empty()) {
        gs.register(s)
            .map_err(|e| format!("cannot use “{s}” as a shortcut: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_text_becomes_a_page_only_when_it_is_html() {
        assert!(page_from_text("just words").is_none());
        assert!(page_from_text("2 < 3 and 4 > 1").is_none());
        let doc = "<!DOCTYPE html><html><head><title>T</title></head><body><p>x</p></body></html>";
        assert_eq!(page_from_text(doc).as_deref(), Some(doc));
        let wrapped = page_from_text("<h2>Note</h2><p>fragment</p>").unwrap();
        assert!(wrapped.starts_with("<!doctype html>") && wrapped.contains("<p>fragment</p>"));
        assert!(is_whole_page(doc));
        assert!(!is_whole_page("<p>fragment</p>"));
    }

    #[test]
    fn downloads_offer_new_html_files_once() {
        let dir = std::env::temp_dir().join(format!("herbarium-downloads-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("old.html"), "<title>Old</title><p>o</p>").unwrap();
        let since = SystemTime::now();
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(
            dir.join("artifact.html"),
            "<!doctype html><title>Solar</title><p>s</p>",
        )
        .unwrap();
        std::fs::write(dir.join("notes.txt"), "<p>not html file</p>").unwrap();
        std::fs::write(dir.join("fake.html"), "plain text").unwrap();

        let mut seen = HashSet::new();
        let offers = new_html_files(&dir, since, &mut seen);
        assert_eq!(offers.len(), 1, "{offers:?}");
        assert_eq!(offers[0].title, "Solar");
        assert_eq!(offers[0].name.as_deref(), Some("artifact.html"));
        assert!(
            new_html_files(&dir, since, &mut seen).is_empty(),
            "offered once"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
