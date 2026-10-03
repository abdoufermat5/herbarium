// Filesystem duties for the vault: a folder of `.html` files on disk with a
// sidecar `{id}.json` (full PageMeta) next to each page. Folders are nested
// physical directories.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::ErrorKind;
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
/// the vault root (`None`); a path escaping the vault, or one through a hidden
/// directory (which rescans skip, such as `.herbarium`), is an error.
pub fn clean_folder(folder: Option<&str>) -> VaultResult<Option<String>> {
    let Some(raw) = folder.filter(|f| !f.trim().is_empty()) else {
        return Ok(None);
    };
    match safe_rel(raw) {
        Some(rel) => {
            let clean = rel.to_string_lossy().replace('\\', "/");
            if clean.split('/').any(|s| s.starts_with('.')) {
                return Err("folder names cannot start with a dot".into());
            }
            Ok(Some(clean))
        }
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

/// The sidecar of a page: `Ok(None)` when there is none, an error when it
/// exists but cannot be read or parsed (so callers never overwrite it blindly).
fn read_meta(vault: &Path, id: &str, folder: Option<&str>) -> VaultResult<Option<PageMeta>> {
    let path = meta_path(vault, id, folder);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let mut meta: PageMeta =
        serde_json::from_str(&raw).map_err(|e| format!("invalid metadata {}: {e}", path.display()))?;
    meta.upgrade();
    Ok(Some(meta))
}

/// Delete both files, then prune empty folders up the tree (best effort).
/// Files already gone are fine; any other failure is reported.
pub fn delete_page_files(vault: &Path, meta: &PageMeta) -> VaultResult<()> {
    for p in [
        html_path(vault, &meta.id, meta.folder.as_deref()),
        meta_path(vault, &meta.id, meta.folder.as_deref()),
    ] {
        match fs::remove_file(&p) {
            Err(e) if e.kind() != ErrorKind::NotFound => {
                return Err(format!("cannot delete {}: {e}", p.display()));
            }
            _ => {}
        }
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

/// Rename, falling back to copy + delete across devices.
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to).or_else(|_| {
        fs::copy(from, to)?;
        fs::remove_file(from)
    })
}

/// Move a page (html + meta) from `meta.folder` to `new_folder` and update
/// `meta.folder` in place. Missing old files are tolerated; an existing file
/// at the destination is never overwritten.
pub fn move_page(vault: &Path, meta: &mut PageMeta, new_folder: Option<&str>) -> VaultResult<()> {
    let new_folder_clean = clean_folder(new_folder)?;
    if new_folder_clean == meta.folder {
        return Ok(());
    }
    let old_dir = dir_for(vault, meta.folder.as_deref());
    let new_dir = dir_for(vault, new_folder_clean.as_deref());
    let moves: Vec<(PathBuf, PathBuf)> = [format!("{}.html", meta.id), format!("{}.json", meta.id)]
        .into_iter()
        .map(|name| (old_dir.join(&name), new_dir.join(&name)))
        .filter(|(from, _)| from.exists())
        .collect();
    if let Some((_, to)) = moves.iter().find(|(_, to)| to.exists()) {
        return Err(format!("cannot move page: {} already exists", to.display()));
    }
    fs::create_dir_all(&new_dir).map_err(|e| format!("cannot create folder: {e}"))?;
    for (i, (from, to)) in moves.iter().enumerate() {
        if let Err(e) = move_file(from, to) {
            // Put back what already moved so the page stays in one piece.
            for (done_from, done_to) in &moves[..i] {
                let _ = move_file(done_to, done_from);
            }
            return Err(format!("cannot move {}: {e}", from.display()));
        }
    }
    meta.folder = new_folder_clean;
    // prune the old dir if it became empty
    if old_dir != vault {
        let _ = fs::remove_dir(old_dir);
    }
    Ok(())
}

/// Rescan the whole vault into the store: import new/changed/moved files,
/// refresh titles/text, and drop indexes for removed files. Returns
/// (imported, removed).
///
/// The page id is the file stem. When several folders hold the same stem, the
/// copy already indexed wins (else the first path in sorted order) and the
/// others are skipped with a warning, so ids stay unique and stable.
pub fn index_vault(store: &Store) -> VaultResult<(usize, usize)> {
    let mut imported = 0usize;

    let (mut html_files, skipped_dirs) = walk_html(&store.vault)?;
    if !skipped_dirs.is_empty() {
        eprintln!("herbarium: skipped hidden dirs {skipped_dirs:?}");
    }
    html_files.sort();

    // stem -> folders holding `{stem}.html`, in sorted path order
    let mut by_stem: BTreeMap<String, Vec<Option<String>>> = BTreeMap::new();
    for rel in &html_files {
        let Some(stem) = rel.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        if stem.is_empty() || stem.starts_with('.') {
            continue;
        }
        let folder = rel
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_string_lossy().replace('\\', "/"));
        by_stem.entry(stem).or_default().push(folder);
    }

    let mut keep = HashSet::new();
    for (stem, folders) in by_stem {
        let indexed = store.get_meta(&stem).map_err(|e| e.to_string())?;
        let folder = match &indexed {
            Some(m) if folders.contains(&m.folder) => m.folder.clone(),
            _ => folders[0].clone(),
        };
        if folders.len() > 1 {
            eprintln!(
                "herbarium: page id {stem:?} exists in several folders {folders:?}; indexing {:?}, skipping the others",
                folder.as_deref().unwrap_or("")
            );
        }
        keep.insert(stem.clone());

        let abs = html_path(&store.vault, &stem, folder.as_deref());
        let mtime = file_mtime(&abs).unwrap_or(0);

        // Fast path: unchanged file, still in the indexed folder.
        if indexed.as_ref().is_some_and(|m| m.folder == folder)
            && store.mtime_for(&stem).map_err(|e| e.to_string())? == Some(mtime)
        {
            continue;
        }

        let html = match fs::read_to_string(&abs) {
            Ok(h) => h,
            Err(_) => continue,
        };

        // Existing sidecar meta (tags, note, review state) wins; folders are
        // path-derived so manual moves on disk are respected. The id always
        // follows the file name, so renaming a pair on disk renames the page.
        // A sidecar that exists but cannot be parsed is left untouched for the
        // user to repair; the page is still indexed from its HTML.
        let (mut meta, write_sidecar) = match read_meta(&store.vault, &stem, folder.as_deref()) {
            Ok(Some(meta)) => (meta, true),
            Ok(None) => (PageMeta::new(stem.clone()), true),
            Err(e) => {
                eprintln!("herbarium: {e}; indexing the page without overwriting it");
                (PageMeta::new(stem.clone()), false)
            }
        };
        meta.id = stem;
        meta.folder = folder;

        if meta.title.is_empty() {
            let t = content::extract_title(&html);
            meta.title = if t.is_empty() { meta.id.clone() } else { t };
        }

        let text = content::extract_text(&html);
        store.upsert(&meta, &text, mtime).map_err(|e| e.to_string())?;
        // persist the sidecar (fills title for foreign files, keeps in sync)
        if write_sidecar {
            let _ = write_meta(&store.vault, &meta);
        }
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

    fn set_mtime(path: &Path, secs: u64) {
        let file = fs::File::options().write(true).open(path).unwrap();
        file.set_modified(UNIX_EPOCH + std::time::Duration::from_secs(secs)).unwrap();
    }

    #[test]
    fn rescan_follows_moves_and_renames_on_disk_and_never_rewrites_a_bad_sidecar() {
        let vault = temp_vault("disk-edits");
        let store = Store::open(vault.clone()).unwrap();
        let mut meta = PageMeta::new("x".into());
        meta.folder = Some("a".into());
        meta.tags = vec!["kept".into()];
        write_page(&vault, &meta, "<title>X</title>").unwrap();
        index_vault(&store).unwrap();

        // `mv` keeps the mtime: the rescan must still notice the new folder.
        fs::create_dir_all(vault.join("b")).unwrap();
        for ext in ["html", "json"] {
            fs::rename(vault.join(format!("a/x.{ext}")), vault.join(format!("b/x.{ext}"))).unwrap();
        }
        index_vault(&store).unwrap();
        assert_eq!(store.get_meta("x").unwrap().unwrap().folder.as_deref(), Some("b"));

        // Renaming the pair renames the page, whatever id the sidecar holds.
        for ext in ["html", "json"] {
            fs::rename(vault.join(format!("b/x.{ext}")), vault.join(format!("b/y.{ext}"))).unwrap();
        }
        index_vault(&store).unwrap();
        assert!(store.get_meta("x").unwrap().is_none());
        let renamed = store.get_meta("y").unwrap().expect("renamed page stays indexed");
        assert_eq!(renamed.tags, vec!["kept".to_string()]);
        index_vault(&store).unwrap();
        assert!(store.get_meta("y").unwrap().is_some(), "a second rescan keeps it");

        // A damaged sidecar is reported, never replaced by blank metadata.
        fs::write(vault.join("b/y.json"), "{ \"tags\": [\"kept\"").unwrap();
        set_mtime(&vault.join("b/y.html"), 1_000);
        index_vault(&store).unwrap();
        assert!(store.get_meta("y").unwrap().is_some(), "the page is still indexed from its HTML");
        assert_eq!(fs::read_to_string(vault.join("b/y.json")).unwrap(), "{ \"tags\": [\"kept\"");

        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn same_stem_in_two_folders_indexes_one_and_moving_never_overwrites_the_other() {
        let vault = temp_vault("dup-stem");
        let store = Store::open(vault.clone()).unwrap();
        for dir in ["a", "b"] {
            fs::create_dir_all(vault.join(dir)).unwrap();
            fs::write(vault.join(format!("{dir}/dup.html")), format!("<title>{dir}</title>")).unwrap();
        }
        index_vault(&store).unwrap();
        assert_eq!(store.count().unwrap(), 1);
        let mut meta = store.get_meta("dup").unwrap().unwrap();
        assert_eq!(meta.folder.as_deref(), Some("a"), "first path in sorted order wins");
        index_vault(&store).unwrap();
        assert_eq!(store.get_meta("dup").unwrap().unwrap().folder.as_deref(), Some("a"), "and keeps winning");

        let err = move_page(&vault, &mut meta, Some("b")).unwrap_err();
        assert!(err.contains("already exists"), "{err}");
        assert_eq!(meta.folder.as_deref(), Some("a"));
        assert_eq!(fs::read_to_string(vault.join("b/dup.html")).unwrap(), "<title>b</title>");
        assert_eq!(fs::read_to_string(vault.join("a/dup.html")).unwrap(), "<title>a</title>");

        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn folders_cannot_be_hidden_directories_that_rescans_skip() {
        for hidden in [".herbarium", "a/.git", "./.x"] {
            assert!(clean_folder(Some(hidden)).is_err(), "{hidden}");
        }
        assert_eq!(clean_folder(Some("./a/b/")).unwrap().as_deref(), Some("a/b"));
    }
}