// All IPC commands exposed to the frontend via `tauri::generate_handler!`.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use tauri::{AppHandle, Manager, State};

use crate::models::{
    Config, ImportFile, ImportResult, MetaPatch, Page, PageMeta, TagCount,
};
use crate::store::Store;
use crate::time::{now_ms, DAY_MS};
use crate::vault;

pub struct AppState {
    pub store: Mutex<Option<Store>>,
}

type CmdResult<T> = Result<T, String>;

fn guard<'a>(state: &'a State<'_, AppState>) -> CmdResult<MutexGuard<'a, Option<Store>>> {
    state.store.lock().map_err(|e| e.to_string())
}

fn store<'a, 'b>(g: &'a mut MutexGuard<'b, Option<Store>>) -> CmdResult<&'a mut Store> {
    g.as_mut().ok_or_else(|| "no vault open".into())
}

fn config_path(app: &AppHandle) -> CmdResult<PathBuf> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("config.json"))
}

fn load_config(app: &AppHandle) -> CmdResult<Config> {
    let path = config_path(app)?;
    let cfg = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Config { vault_path: None });
    Ok(cfg)
}

fn save_config(app: &AppHandle, cfg: &Config) -> CmdResult<()> {
    let path = config_path(app)?;
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Open a vault (create dir if needed), rebuild index, persist config.
fn open_vault(app: &AppHandle, state: &State<'_, AppState>, path: &str) -> CmdResult<Config> {
    let canonical = vault::normalize(path)?;
    let store = Store::open(canonical.clone()).map_err(|e| e.to_string())?;
    let (imported, removed) = vault::index_vault(&store).map_err(|e| e.to_string())?;
    let total = store.count().unwrap_or(0);
    eprintln!(
        "herbarium: vault opened at {} (indexed {imported}, removed {removed}, total {total})",
        canonical.display()
    );
    *state.store.lock().map_err(|e| e.to_string())? = Some(store);

    let cfg = Config {
        vault_path: Some(canonical.to_string_lossy().into_owned()),
    };
    save_config(app, &cfg)?;
    Ok(cfg)
}

fn page_meta(store: &Store, id: &str) -> CmdResult<PageMeta> {
    store
        .get_meta(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "page not found".to_string())
}

fn persist_meta(store: &Store, meta: &PageMeta) -> CmdResult<()> {
    vault::write_meta(&store.vault, meta).map_err(|e| e.to_string())?;
    let text = store.text_for(&meta.id).map_err(|e| e.to_string())?.unwrap_or_default();
    let mtime = store.mtime_for(&meta.id).map_err(|e| e.to_string())?.unwrap_or(crate::time::now_secs());
    store.upsert(meta, &text, mtime).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------- commands

#[tauri::command]
pub async fn get_config(app: AppHandle) -> CmdResult<Config> {
    load_config(&app)
}

#[tauri::command]
pub async fn set_vault(app: AppHandle, state: State<'_, AppState>, path: String) -> CmdResult<Config> {
    open_vault(&app, &state, &path)
}

#[tauri::command]
pub async fn create_vault(
    app: AppHandle,
    state: State<'_, AppState>,
    parent_dir: String,
    name: String,
) -> CmdResult<Config> {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
        return Err("invalid folder name".into());
    }
    let target = PathBuf::from(&parent_dir).join(name);
    open_vault(&app, &state, &target.to_string_lossy())
}

#[tauri::command]
pub async fn import_files(
    state: State<'_, AppState>,
    files: Vec<ImportFile>,
) -> CmdResult<ImportResult> {
    let mut g = guard(&state)?;
    let store = store(&mut g)?;
    let mut result = ImportResult { imported: 0, errors: Vec::new() };

    for f in files {
        if !crate::content::looks_like_html(&f.content) {
            result.errors.push(format!(
                "{}: not an HTML document",
                f.name.as_deref().unwrap_or("pasted content")
            ));
            continue;
        }
        let id = uuid::Uuid::new_v4().to_string();
        let mut meta = PageMeta::new(id.clone());
        let html = f.content;
        let title = crate::content::extract_title(&html);
        meta.title = if title.is_empty() {
            f.name
                .as_deref()
                .map(|n| n.rsplit_once('.').map(|(s, _)| s).unwrap_or(n).to_string())
                .unwrap_or_else(|| id.clone())
        } else {
            title
        };
        let text = crate::content::extract_text(&html);
        let mtime = crate::time::now_secs();

        if let Err(e) = vault::write_page(&store.vault, &meta, &html) {
            result.errors.push(format!("{}: {e}", meta.id));
            continue;
        }
        if let Err(e) = store.upsert(&meta, &text, mtime) {
            let _ = vault::delete_page_files(&store.vault, &meta);
            result.errors.push(format!("{}: {e}", meta.id));
            continue;
        }
        result.imported += 1;
    }
    Ok(result)
}

#[tauri::command]
pub async fn list_pages(state: State<'_, AppState>) -> CmdResult<Vec<PageMeta>> {
    let mut g = guard(&state)?;
    store(&mut g)?.all().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn search_pages(state: State<'_, AppState>, query: String) -> CmdResult<Vec<PageMeta>> {
    let mut g = guard(&state)?;
    store(&mut g)?.search(&query).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_page(state: State<'_, AppState>, id: String) -> CmdResult<Page> {
    let mut g = guard(&state)?;
    let s = store(&mut g)?;
    let meta = page_meta(s, &id)?;
    let html = vault::read_html(&s.vault, &id, meta.folder.as_deref()).unwrap_or_default();
    Ok(Page { meta, html })
}

#[tauri::command]
pub async fn update_page_meta(
    state: State<'_, AppState>,
    id: String,
    patch: MetaPatch,
) -> CmdResult<PageMeta> {
    let mut g = guard(&state)?;
    let s = store(&mut g)?;
    let mut meta = page_meta(s, &id)?;

    let patch_title = patch.title.trim().to_string();
    if patch_title != meta.title {
        meta.title = if patch_title.is_empty() { meta.id.clone() } else { patch_title };
    }
    meta.tags = patch.tags;
    meta.note = patch.note;
    meta.updated_at = now_ms();

    vault::move_page(&s.vault, &mut meta, patch.folder)?;

    persist_meta(s, &meta)?;
    Ok(meta)
}

#[tauri::command]
pub async fn schedule_review(
    state: State<'_, AppState>,
    id: String,
    interval_days: i64,
) -> CmdResult<PageMeta> {
    let mut g = guard(&state)?;
    let s = store(&mut g)?;
    let mut meta = page_meta(s, &id)?;
    let days = interval_days.max(1);
    let now = now_ms();
    meta.interval_days = Some(days);
    meta.next_review = Some(now + days * DAY_MS);
    meta.last_review = Some(now);
    meta.updated_at = now;
    persist_meta(s, &meta)?;
    Ok(meta)
}

#[tauri::command]
pub async fn clear_review(state: State<'_, AppState>, id: String) -> CmdResult<PageMeta> {
    let mut g = guard(&state)?;
    let s = store(&mut g)?;
    let mut meta = page_meta(s, &id)?;
    meta.interval_days = None;
    meta.next_review = None;
    meta.last_review = None;
    meta.updated_at = now_ms();
    persist_meta(s, &meta)?;
    Ok(meta)
}

#[tauri::command]
pub async fn set_network(
    state: State<'_, AppState>,
    id: String,
    allow_cdn: bool,
) -> CmdResult<PageMeta> {
    let mut g = guard(&state)?;
    let s = store(&mut g)?;
    let mut meta = page_meta(s, &id)?;
    meta.allow_cdn = allow_cdn;
    meta.updated_at = now_ms();
    persist_meta(s, &meta)?;
    Ok(meta)
}

#[tauri::command]
pub async fn delete_page(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let mut g = guard(&state)?;
    let s = store(&mut g)?;
    let meta = page_meta(s, &id)?;
    s.delete(&id).map_err(|e| e.to_string())?;
    vault::delete_page_files(&s.vault, &meta)?;
    Ok(())
}

#[tauri::command]
pub async fn review_today(state: State<'_, AppState>) -> CmdResult<Vec<PageMeta>> {
    let mut g = guard(&state)?;
    store(&mut g)?.due(now_ms()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tags(state: State<'_, AppState>) -> CmdResult<Vec<TagCount>> {
    let mut g = guard(&state)?;
    store(&mut g)?.tag_counts().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn folders(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    let mut g = guard(&state)?;
    store(&mut g)?.folders().map_err(|e| e.to_string())
}