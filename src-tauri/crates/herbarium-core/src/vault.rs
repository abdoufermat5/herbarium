// Filesystem duties for the vault: a folder of `.html` files on disk with a
// sidecar `{id}.json` (full PageMeta) next to each page. Folders are nested
// physical directories.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::content;
use crate::models::PageMeta;
use crate::store::Store;

pub type VaultResult<T> = Result<T, String>;

/// Create the vault directory if needed and return its canonical path.
pub fn normalize(path: &str) -> VaultResult<PathBuf> {
    let dir = PathBuf::from(path);
    if dir.as_os_str().is_empty() {
        return Err("empty path".into());
    }
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create vault: {e}"))?;
    dir.canonicalize()
        .map_err(|e| format!("cannot access vault: {e}"))
}

/// Split a folder string into a safe relative path. Rejects `..`, leading `/`,
/// and drives. Returns None when the folder is empty or invalid.
pub fn safe_rel(folder: &str) -> Option<PathBuf> {
    let mut parts = Vec::new();
    for seg in folder.trim().split(['/', '\\']) {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." || seg.contains(':') {
            return None;
        }
        parts.push(seg);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.iter().collect())
    }
}

/// Normalize a user-supplied folder into the stored `a/b/c` form. Blank means
/// the vault root (`None`); a path escaping the vault is an error.
pub fn clean_folder(folder: Option<&str>) -> VaultResult<Option<String>> {
    let Some(raw) = folder.filter(|f| !f.trim().is_empty()) else {
        return Ok(None);
    };
    match safe_rel(raw) {
        Some(rel) => Ok(Some(rel.to_string_lossy().replace('\\', "/"))),
        None if raw.split(['/', '\\']).all(|s| s.trim().is_empty() || s == ".") => Ok(None),
        None => Err(format!("invalid folder: {raw}")),
    }
}

fn dir_for(vault: &Path, folder: Option<&str>) -> PathBuf {
    match folder.and_then(safe_rel) {
        Some(rel) => vault.join(rel),
        None => vault.to_path_buf(),
    }
}

pub fn html_path(vault: &Path, id: &str, folder: Option<&str>) -> PathBuf {
    dir_for(vault, folder).join(format!("{id}.html"))
}

pub fn meta_path(vault: &Path, id: &str, folder: Option<&str>) -> PathBuf {
    dir_for(vault, folder).join(format!("{id}.json"))
}

/// Write both files of a page (html as-is + sidecar meta JSON).
pub fn write_page(vault: &Path, meta: &PageMeta, html: &str) -> VaultResult<()> {
    let dir = dir_for(vault, meta.folder.as_deref());
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create folder: {e}"))?;
    write_meta(vault, meta)?;
    fs::write(html_path(vault, &meta.id, meta.folder.as_deref()), html)
        .map_err(|e| format!("cannot write page: {e}"))
}

/// Write only the sidecar meta JSON.
pub fn write_meta(vault: &Path, meta: &PageMeta) -> VaultResult<()> {
    let path = meta_path(vault, &meta.id, meta.folder.as_deref());
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let json = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| format!("cannot write metadata: {e}"))
}

pub fn read_html(vault: &Path, id: &str, folder: Option<&str>) -> VaultResult<String> {
    fs::read_to_string(html_path(vault, id, folder))
        .map_err(|e| format!("cannot read page: {e}"))
}

fn read_meta(vault: &Path, id: &str, folder: Option<&str>) -> Option<PageMeta> {
    let raw = fs::read_to_string(meta_path(vault, id, folder)).ok()?;
    let mut meta: PageMeta = serde_json::from_str(&raw).ok()?;
    meta.upgrade();
    Some(meta)
}

/// Delete both files, then prune empty folders up the tree (best effort).
pub fn delete_page_files(vault: &Path, meta: &PageMeta) -> VaultResult<()> {
    for p in [
        html_path(vault, &meta.id, meta.folder.as_deref()),
        meta_path(vault, &meta.id, meta.folder.as_deref()),
    ] {
        let _ = fs::remove_file(&p);
    }
    // prune emptied dirs
    let mut dir = dir_for(vault, meta.folder.as_deref());
    while dir != vault {
        let removed = fs::remove_dir(&dir).is_ok();
        if !removed {
            break;
        }
        if !dir.pop() {
            break;
        }
    }
    Ok(())
}

/// Move a page (html + meta) from `meta.folder` to `new_folder` and update
/// `meta.folder` in place. Missing old files are tolerated.
pub fn move_page(vault: &Path, meta: &mut PageMeta, new_folder: Option<&str>) -> VaultResult<()> {
    let new_folder_clean = clean_folder(new_folder)?;
    if new_folder_clean == meta.folder {
        return Ok(());
    }
    let old_dir = html_path(vault, &meta.id, meta.folder.as_deref())
        .parent()
        .map(|p| p.to_path_buf());
    let old_html = html_path(vault, &meta.id, meta.folder.as_deref());
    let old_json = meta_path(vault, &meta.id, meta.folder.as_deref());
    let new_dir = dir_for(vault, new_folder_clean.as_deref());
    fs::create_dir_all(&new_dir).map_err(|e| format!("cannot create folder: {e}"))?;
    let new_html = new_dir.join(format!("{}.html", meta.id));
    let new_json = new_dir.join(format!("{}.json", meta.id));
    for (from, to) in [(old_html, new_html), (old_json, new_json)] {
        if from.exists() {
            let _ = fs::rename(&from, &to).or_else(|_| fs::copy(&from, &to).map(|_| ()));
        }
    }
    meta.folder = new_folder_clean;
    // prune the old dir if it became empty
    if let Some(old_dir) = old_dir
        && old_dir != vault
    {
        let _ = fs::remove_dir(old_dir);
    }
    Ok(())
}

/// Rescan the whole vault into the store: import new/changed files, refresh
/// titles/text, and drop indexes for removed files. Returns (imported, removed).
pub fn index_vault(store: &Store) -> VaultResult<(usize, usize)> {
    let mut imported = 0usize;
    let mut keep = HashSet::new();

    let (html_files, skipped_dirs) = walk_html(&store.vault)?;
    if !skipped_dirs.is_empty() {
        eprintln!("herbarium: skipped hidden dirs {skipped_dirs:?}");
    }

    for rel in html_files {
        // relative path like "sub/folder/abc.html"
        let Some(stem) = rel.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        if stem.is_empty() || stem.starts_with('.') {
            continue;
        }
        keep.insert(stem.clone());

        let folder = rel
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_string_lossy().replace('\\', "/").to_string());

        let abs = store.vault.join(&rel);
        let mtime = file_mtime(&abs).unwrap_or(0);

        // Fast path: unchanged file content, already indexed.
        if store.mtime_for(&stem).map(|m| m == Some(mtime)).unwrap_or(false)
            && store.get_meta(&stem).map(|m| m.is_some()).unwrap_or(false)
        {
            continue;
        }

        let html = match fs::read_to_string(&abs) {
            Ok(h) => h,
            Err(_) => continue,
        };

        // Existing sidecar meta (tags, note, review state) wins; folders are
        // path-derived so manual moves on disk are respected.
        let mut meta = read_meta(&store.vault, &stem, folder.as_deref()).unwrap_or_else(|| PageMeta::new(stem.clone()));
        meta.folder = folder;

        if meta.title.is_empty() {
            let t = content::extract_title(&html);
            meta.title = if t.is_empty() { meta.id.clone() } else { t };
        }

        let text = content::extract_text(&html);
        store.upsert(&meta, &text, mtime).map_err(|e| e.to_string())?;
        // persist the sidecar (fills title for foreign files, keeps in sync)
        let _ = write_meta(&store.vault, &meta);
        imported += 1;
    }

    let removed = store.remove_all_except(&keep).map_err(|e| e.to_string())?;
    store.set_synced_at();
    Ok((imported, removed))
}

/// Recursively list `.html` files relative to `vault`, skipping hidden dirs.
fn walk_html(vault: &Path) -> VaultResult<(Vec<PathBuf>, Vec<String>)> {
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    fn walk(dir: &Path, base: &Path, files: &mut Vec<PathBuf>, skipped: &mut Vec<String>) -> VaultResult<()> {
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir {
                if name.starts_with('.') {
                    skipped.push(name);
                    continue;
                }
                walk(&path, base, files, skipped)?;
            } else if name.ends_with(".html") {
                let rel = path.strip_prefix(base).map_err(|e| e.to_string())?.to_path_buf();
                files.push(rel);
            }
        }
        Ok(())
    }
    walk(vault, vault, &mut files, &mut skipped)?;
    Ok((files, skipped))
}

/// Every folder in the vault as `a/b/c` paths, including empty ones. Hidden
/// directories (such as `.herbarium`) are skipped.
pub fn list_folders(vault: &Path) -> VaultResult<Vec<String>> {
    fn walk(dir: &Path, rel: &str, out: &mut Vec<String>) -> VaultResult<()> {
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if !is_dir || name.starts_with('.') {
                continue;
            }
            let path = if rel.is_empty() { name } else { format!("{rel}/{name}") };
            walk(&entry.path(), &path, out)?;
            out.push(path);
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(vault, "", &mut out)?;
    out.sort_by_key(|p| p.to_lowercase());
    Ok(out)
}

/// Create a folder (and any missing parents) and return its normalized path.
pub fn create_folder(vault: &Path, folder: &str) -> VaultResult<String> {
    let clean = clean_folder(Some(folder))?.ok_or("folder name is empty")?;
    if clean.split('/').any(|s| s.starts_with('.')) {
        return Err("folder names cannot start with a dot".into());
    }
    fs::create_dir_all(vault.join(&clean)).map_err(|e| format!("cannot create folder: {e}"))?;
    Ok(clean)
}

pub fn file_mtime(path: &Path) -> Option<i64> {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{now_ms, DAY_MS};

    fn temp_vault(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "herbarium-test-{tag}-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// CONTEXT.md scenario, backend-level: import the demo page, index it,
    /// search "semver", schedule a 3-day review, confirm it is (then is not)
    /// due at the right times.
    #[test]
    fn scenario_import_search_review() {
        let vault = temp_vault("scenario");
        let store = Store::open(vault.clone()).unwrap();

        // Stage 1 — import: copy examples/semver.html into the vault as-is.
        let demo = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../examples/semver.html"))
            .expect("examples/semver.html present");
        let mut meta = PageMeta::new("semver-demo".into());
        meta.title = "semver — Semantic Versioning Reference".into();
        write_page(&vault, &meta, &demo).expect("write_page ok");

        let (imported, removed) = index_vault(&store).unwrap();
        assert_eq!(imported, 1, "index_vault imported the demo page");
        assert_eq!(removed, 0, "nothing removed on first index");
        assert_eq!(store.count().unwrap(), 1);

        // Title was auto-extracted/kept.
        let found = store.get_meta("semver-demo").unwrap().unwrap();
        assert_eq!(found.title, "semver — Semantic Versioning Reference");

        // Stage 3 — search "semver" finds it (FTS over title/tags/text).
        let hits = store.search("semver").unwrap();
        assert!(
            hits.iter().any(|m| m.id == "semver-demo"),
            "search 'semver' returns the demo page"
        );

        // Stage 5 — "Review in 3 days".
        let future = now_ms() + 3 * DAY_MS;
        let mut due_meta = found.clone();
        due_meta.interval_minutes = Some(3 * 1440);
        due_meta.next_review = Some(future);
        due_meta.last_review = Some(now_ms());
        due_meta.updated_at = now_ms();
        store.upsert(&due_meta, &crate::content::extract_text(&demo), 0).unwrap();

        // Not due today…
        assert!(!store.due(now_ms()).unwrap().iter().any(|m| m.id == "semver-demo"));
        // …but due once the 3 days have passed.
        let over = store.due(future + 1).unwrap();
        assert!(
            over.iter().any(|m| m.id == "semver-demo"),
            "review_today shows the page after its 3-day interval"
        );

        // Stage 4 — folders/tags plumbing is reachable.
        assert_eq!(store.folders().unwrap(), Vec::<String>::new());

        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn safe_rel_rejects_escape() {
        assert!(safe_rel("../..").is_none());
        assert!(safe_rel("a/../../b").is_none());
        assert!(safe_rel("c:\\evil").is_none());
        assert_eq!(safe_rel("a/b/c"), Some(PathBuf::from("a/b/c")));
    }
}