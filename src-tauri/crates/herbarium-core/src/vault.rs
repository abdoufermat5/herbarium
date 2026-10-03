// Filesystem duties for the vault: a folder of `.html` files on disk with a
// sidecar `{id}.json` (full PageMeta) next to each page. Folders are nested
// physical directories.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::content;
use crate::models::{PageMeta, SkippedFile, TrashEntry};
use crate::store::Store;

pub type VaultResult<T> = Result<T, String>;

struct TempFileGuard<'a> {
    path: &'a Path,
    active: bool,
}

impl<'a> Drop for TempFileGuard<'a> {
    fn drop(&mut self) {
        if self.active {
            let _ = fs::remove_file(self.path);
        }
    }
}

/// Write bytes to a temporary file in the same directory, fsync, and atomically rename over the target.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> VaultResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| format!("cannot create directory: {e}"))?;
    let file_stem = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();

    let temp_name = format!(".{file_stem}.tmp-{}", uuid::Uuid::new_v4());
    let temp_path = parent.join(temp_name);
    let mut guard = TempFileGuard {
        path: &temp_path,
        active: true,
    };

    let write_res = (|| -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        use std::io::Write;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp_path, path)?;
        Ok(())
    })();

    if let Err(e) = write_res {
        drop(guard);
        return Err(format!(
            "cannot write atomically to {}: {e}",
            path.display()
        ));
    }

    guard.active = false;
    Ok(())
}

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

/// Returns true if the path is within `base_canonical` (or is base itself).
/// Resolves symlinks via canonicalize. If path does not exist, checks nearest existing ancestor.
pub fn is_within_vault(path: &Path, base_canonical: &Path) -> bool {
    if let Ok(c) = path.canonicalize() {
        return c.starts_with(base_canonical);
    }
    let mut cur = path;
    while let Some(parent) = cur.parent() {
        if parent.as_os_str().is_empty() {
            break;
        }
        if let Ok(c) = parent.canonicalize() {
            return c.starts_with(base_canonical);
        }
        cur = parent;
    }
    false
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
        None if raw
            .split(['/', '\\'])
            .all(|s| s.trim().is_empty() || s == ".") =>
        {
            Ok(None)
        }
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

/// Write both files of a page (html first, then sidecar meta JSON, using write_atomic).
pub fn write_page(vault: &Path, meta: &PageMeta, html: &str) -> VaultResult<()> {
    let dir = dir_for(vault, meta.folder.as_deref());
    let vault_canonical = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
    if !is_within_vault(&dir, &vault_canonical) {
        return Err("folder escapes vault".to_string());
    }
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create folder: {e}"))?;
    if !is_within_vault(&dir, &vault_canonical) {
        return Err("folder escapes vault".to_string());
    }
    let hp = html_path(vault, &meta.id, meta.folder.as_deref());
    if hp.exists() && !is_within_vault(&hp, &vault_canonical) {
        return Err("file escapes vault".to_string());
    }
    write_atomic(&hp, html.as_bytes())?;
    write_meta(vault, meta)?;
    Ok(())
}

/// Write only the sidecar meta JSON using write_atomic.
pub fn write_meta(vault: &Path, meta: &PageMeta) -> VaultResult<()> {
    let dir = dir_for(vault, meta.folder.as_deref());
    let vault_canonical = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
    if !is_within_vault(&dir, &vault_canonical) {
        return Err("folder escapes vault".to_string());
    }
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create folder: {e}"))?;
    if !is_within_vault(&dir, &vault_canonical) {
        return Err("folder escapes vault".to_string());
    }
    let path = meta_path(vault, &meta.id, meta.folder.as_deref());
    if path.exists() && !is_within_vault(&path, &vault_canonical) {
        return Err("file escapes vault".to_string());
    }
    let json = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
    write_atomic(&path, json.as_bytes())?;
    Ok(())
}

pub fn read_html(vault: &Path, id: &str, folder: Option<&str>) -> VaultResult<String> {
    let path = html_path(vault, id, folder);
    let vault_canonical = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
    if path.exists() && !is_within_vault(&path, &vault_canonical) {
        return Err("path escapes vault".to_string());
    }
    fs::read_to_string(&path).map_err(|e| format!("cannot read page: {e}"))
}

/// The sidecar of a page: `Ok(None)` when there is none, an error when it
/// exists but cannot be read or parsed (so callers never overwrite it blindly).
pub fn read_meta(vault: &Path, id: &str, folder: Option<&str>) -> VaultResult<Option<PageMeta>> {
    let path = meta_path(vault, id, folder);
    let vault_canonical = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
    if path.exists() && !is_within_vault(&path, &vault_canonical) {
        return Err("path escapes vault".to_string());
    }
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let mut meta: PageMeta = serde_json::from_str(&raw)
        .map_err(|e| format!("invalid metadata {}: {e}", path.display()))?;
    meta.upgrade();
    Ok(Some(meta))
}

/// Delete both files. Files already gone are fine; any other failure is reported.
/// Does not prune empty folders (B5).
pub fn delete_page_files(vault: &Path, meta: &PageMeta) -> VaultResult<()> {
    let vault_canonical = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
    for p in [
        html_path(vault, &meta.id, meta.folder.as_deref()),
        meta_path(vault, &meta.id, meta.folder.as_deref()),
    ] {
        if p.exists() && !is_within_vault(&p, &vault_canonical) {
            return Err(format!("cannot delete {}: path escapes vault", p.display()));
        }
        match fs::remove_file(&p) {
            Err(e) if e.kind() != ErrorKind::NotFound => {
                return Err(format!("cannot delete {}: {e}", p.display()));
            }
            _ => {}
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
/// at the destination is never overwritten. Does not prune empty folders (B5).
pub fn move_page(vault: &Path, meta: &mut PageMeta, new_folder: Option<&str>) -> VaultResult<()> {
    let new_folder_clean = clean_folder(new_folder)?;
    if new_folder_clean == meta.folder {
        return Ok(());
    }
    let vault_canonical = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
    let old_dir = dir_for(vault, meta.folder.as_deref());
    let new_dir = dir_for(vault, new_folder_clean.as_deref());
    if old_dir.exists() && !is_within_vault(&old_dir, &vault_canonical) {
        return Err("source folder escapes vault".to_string());
    }
    if !is_within_vault(&new_dir, &vault_canonical) {
        return Err("destination folder escapes vault".to_string());
    }
    let moves: Vec<(PathBuf, PathBuf)> = [format!("{}.html", meta.id), format!("{}.json", meta.id)]
        .into_iter()
        .map(|name| (old_dir.join(&name), new_dir.join(&name)))
        .filter(|(from, _)| from.exists())
        .collect();
    if let Some((_, to)) = moves.iter().find(|(_, to)| to.exists()) {
        return Err(format!("cannot move page: {} already exists", to.display()));
    }
    fs::create_dir_all(&new_dir).map_err(|e| format!("cannot create folder: {e}"))?;
    if !is_within_vault(&new_dir, &vault_canonical) {
        return Err("destination folder escapes vault".to_string());
    }
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
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexOutcome {
    pub indexed: usize,
    pub removed: usize,
    pub skipped: Vec<SkippedFile>,
}

fn decode_html(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    let head_len = bytes.len().min(1024);
    let head_ascii = bytes[..head_len]
        .iter()
        .map(|&b| {
            if b.is_ascii() {
                (b as char).to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>();

    let is_windows_1252 = head_ascii.contains("charset=windows-1252")
        || head_ascii.contains("charset=\"windows-1252\"")
        || head_ascii.contains("charset='windows-1252'");
    let is_iso_8859_1 = head_ascii.contains("charset=iso-8859-1")
        || head_ascii.contains("charset=\"iso-8859-1\"")
        || head_ascii.contains("charset='iso-8859-1'");

    if is_windows_1252 {
        bytes.iter().map(|&b| decode_windows_1252(b)).collect()
    } else if is_iso_8859_1 {
        bytes.iter().map(|&b| b as char).collect()
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

fn decode_windows_1252(b: u8) -> char {
    match b {
        0x00..=0x7F => b as char,
        0xA0..=0xFF => b as char,
        0x80 => '€',
        0x82 => '‚',
        0x83 => 'ƒ',
        0x84 => '„',
        0x85 => '…',
        0x86 => '†',
        0x87 => '‡',
        0x88 => 'ˆ',
        0x89 => '‰',
        0x8A => 'Š',
        0x8B => '‹',
        0x8C => 'Œ',
        0x8E => 'Ž',
        0x91 => '‘',
        0x92 => '’',
        0x93 => '“',
        0x94 => '”',
        0x95 => '•',
        0x96 => '–',
        0x97 => '—',
        0x98 => '˜',
        0x99 => '™',
        0x9A => 'š',
        0x9B => '›',
        0x9C => 'œ',
        0x9E => 'ž',
        0x9F => 'Ÿ',
        _ => b as char,
    }
}

/// Rescan the whole vault into the store: import new/changed/moved files,
/// refresh titles/text, and drop indexes for removed files. Returns IndexOutcome.
pub fn index_vault(store: &Store) -> VaultResult<IndexOutcome> {
    let mut indexed_count = 0usize;
    let mut skipped = Vec::new();

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
            let chosen_display = folder.as_deref().unwrap_or("root");
            for f in &folders {
                if *f != folder {
                    let dup_path = match f.as_deref() {
                        Some(fld) => format!("{fld}/{stem}.html"),
                        None => format!("{stem}.html"),
                    };
                    skipped.push(SkippedFile {
                        path: dup_path,
                        reason: format!(
                            "duplicate page id '{stem}' (already indexed in {chosen_display})"
                        ),
                    });
                }
            }
        }
        keep.insert(stem.clone());

        let abs = html_path(&store.vault, &stem, folder.as_deref());
        let rel_str = match folder.as_deref() {
            Some(f) => format!("{f}/{stem}.html"),
            None => format!("{stem}.html"),
        };

        // Determine modification state using mtime and fingerprint
        let mtime = page_mtime(
            &store.vault,
            &PageMeta {
                id: stem.clone(),
                folder: folder.clone(),
                ..PageMeta::new(stem.clone())
            },
        );
        let cur_fp = page_fingerprint(&store.vault, &stem, folder.as_deref());
        let stored_mtime = store.mtime_for(&stem).map_err(|e| e.to_string())?;
        let stored_fp = store.fingerprint_for(&stem).map_err(|e| e.to_string())?;

        // Fast path: unchanged file, still in the indexed folder.
        if indexed.as_ref().is_some_and(|m| m.folder == folder) {
            let unchanged = match (&stored_fp, &cur_fp) {
                (Some(s_fp), c_fp) if !s_fp.is_empty() && !c_fp.is_empty() => s_fp == c_fp,
                _ => stored_mtime == Some(mtime),
            };
            if unchanged {
                continue;
            }
        }

        let raw_bytes = match fs::read(&abs) {
            Ok(b) => b,
            Err(e) => {
                skipped.push(SkippedFile {
                    path: rel_str,
                    reason: format!("cannot read file: {e}"),
                });
                continue;
            }
        };

        let html = decode_html(&raw_bytes);
        if !content::looks_like_html(&html) {
            skipped.push(SkippedFile {
                path: rel_str,
                reason: "not an HTML document".to_string(),
            });
            continue;
        }

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
        if meta.source_title.is_none()
            && let Some(existing) = &indexed
        {
            meta.source_title = existing.source_title.clone();
        }

        let html_ms = file_mtime_ms(&abs).unwrap_or(0);
        let extracted_title = content::extract_title(&html);

        // Check if HTML changed on disk
        let is_new = indexed.is_none();
        let prev_text = store.text_for(&meta.id).ok().flatten();
        let cur_text = content::extract_text(&html);
        let html_changed =
            is_new || prev_text.as_ref() != Some(&cur_text) || html_ms > meta.updated_at;

        if html_changed {
            meta.updated_at = html_ms;
            let user_never_renamed = meta
                .source_title
                .as_ref()
                .is_some_and(|st| st == &meta.title);
            if user_never_renamed || meta.title.is_empty() {
                meta.title = if extracted_title.is_empty() {
                    meta.id.clone()
                } else {
                    extracted_title.clone()
                };
            }
        }
        // Always refresh source_title
        meta.source_title = Some(extracted_title);

        // Persist the sidecar first so the fingerprint stored by `upsert`
        // reflects the final file state (otherwise every rescan sees a change).
        let mtime = if write_sidecar {
            let _ = write_meta(&store.vault, &meta);
            page_mtime(&store.vault, &meta)
        } else {
            mtime
        };
        store
            .upsert(&meta, &cur_text, mtime)
            .map_err(|e| e.to_string())?;
        indexed_count += 1;
    }

    let removed = store.remove_all_except(&keep).map_err(|e| e.to_string())?;
    store.set_synced_at();
    Ok(IndexOutcome {
        indexed: indexed_count,
        removed,
        skipped,
    })
}

/// Recursively list `.html` files relative to `vault`, skipping hidden dirs.
fn walk_html(vault: &Path) -> VaultResult<(Vec<PathBuf>, Vec<String>)> {
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    let vault_canonical = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
    fn walk(
        dir: &Path,
        base: &Path,
        vault_canonical: &Path,
        files: &mut Vec<PathBuf>,
        skipped: &mut Vec<String>,
    ) -> VaultResult<()> {
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if ft.is_symlink() && !is_within_vault(&path, vault_canonical) {
                skipped.push(name);
                continue;
            }
            if ft.is_symlink() && path.is_dir() {
                // Directory symlinks are never followed (loops, duplicate indexing).
                skipped.push(name);
                continue;
            }
            if ft.is_dir() {
                if name.starts_with('.') {
                    skipped.push(name);
                    continue;
                }
                if !is_within_vault(&path, vault_canonical) {
                    skipped.push(name);
                    continue;
                }
                walk(&path, base, vault_canonical, files, skipped)?;
            } else if name.ends_with(".html") {
                if !is_within_vault(&path, vault_canonical) {
                    skipped.push(name);
                    continue;
                }
                let rel = path
                    .strip_prefix(base)
                    .map_err(|e| e.to_string())?
                    .to_path_buf();
                files.push(rel);
            }
        }
        Ok(())
    }
    walk(vault, vault, &vault_canonical, &mut files, &mut skipped)?;
    Ok((files, skipped))
}

/// Every folder in the vault as `a/b/c` paths, including empty ones. Hidden
/// directories (such as `.herbarium`) are skipped.
pub fn list_folders(vault: &Path) -> VaultResult<Vec<String>> {
    let vault_canonical = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
    fn walk(
        dir: &Path,
        rel: &str,
        vault_canonical: &Path,
        out: &mut Vec<String>,
    ) -> VaultResult<()> {
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if ft.is_symlink() && !is_within_vault(&entry.path(), vault_canonical) {
                continue;
            }
            if !ft.is_dir() || name.starts_with('.') {
                continue;
            }
            if !is_within_vault(&entry.path(), vault_canonical) {
                continue;
            }
            let path = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            walk(&entry.path(), &path, vault_canonical, out)?;
            out.push(path);
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(vault, "", &vault_canonical, &mut out)?;
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
pub fn file_mtime_ms(path: &Path) -> Option<i64> {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
}

pub fn file_mtime_nanos(path: &Path) -> Option<u128> {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
}

/// The max of the html and json file mtimes in secs, or 0 if neither exists.
pub fn page_mtime(vault: &Path, meta: &PageMeta) -> i64 {
    let hp = html_path(vault, &meta.id, meta.folder.as_deref());
    let mp = meta_path(vault, &meta.id, meta.folder.as_deref());
    let h = file_mtime(&hp);
    let m = file_mtime(&mp);
    match (h, m) {
        (Some(a), Some(b)) => a.max(b),
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => 0,
    }
}

fn file_stat_fingerprint(meta: Option<fs::Metadata>) -> String {
    match meta {
        None => "none".to_string(),
        Some(m) => {
            let len = m.len();
            let nanos = match m.modified() {
                Ok(t) => match t.duration_since(UNIX_EPOCH) {
                    Ok(d) => d.as_nanos() as i128,
                    Err(e) => -(e.duration().as_nanos() as i128),
                },
                Err(_) => 0,
            };
            format!("{len}:{nanos}")
        }
    }
}

/// Compute a reliable fingerprint of the page files (html + sidecar), capturing
/// sub-second mtime changes, file lengths, or missing files.
pub fn page_fingerprint(vault: &Path, id: &str, folder: Option<&str>) -> String {
    let vault_canonical = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
    let hp = html_path(vault, id, folder);
    let mp = meta_path(vault, id, folder);

    let h_meta = if hp.exists() && is_within_vault(&hp, &vault_canonical) {
        fs::metadata(&hp).ok()
    } else {
        None
    };
    let m_meta = if mp.exists() && is_within_vault(&mp, &vault_canonical) {
        fs::metadata(&mp).ok()
    } else {
        None
    };

    format!(
        "h:{}:m:{}",
        file_stat_fingerprint(h_meta),
        file_stat_fingerprint(m_meta)
    )
}

/// Return the disk's effective updated_at: sidecar updated_at if present and newer than
/// HTML mtime, otherwise HTML mtime in ms.
pub fn disk_updated_at(vault: &Path, meta: &PageMeta) -> Option<i64> {
    let hp = html_path(vault, &meta.id, meta.folder.as_deref());
    let html_ms = file_mtime_ms(&hp);
    let sidecar_meta = read_meta(vault, &meta.id, meta.folder.as_deref())
        .ok()
        .flatten();
    match (html_ms, sidecar_meta) {
        (Some(h_ms), Some(sm)) => {
            if h_ms > sm.updated_at {
                Some(h_ms)
            } else {
                Some(sm.updated_at)
            }
        }
        (None, Some(sm)) => Some(sm.updated_at),
        (Some(h_ms), None) => Some(h_ms),
        (None, None) => None,
    }
}

/// A page id usable as a trash directory name: exactly one normal path
/// component (no separators, `.`, `..`, drive prefixes, or empty string).
fn checked_trash_id(id: &str) -> VaultResult<&str> {
    use std::path::Component;
    let mut components = Path::new(id).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(name)), None)
            if name == std::ffi::OsStr::new(id) && !id.contains(['/', '\\']) =>
        {
            Ok(id)
        }
        _ => Err(format!("invalid page id: {id:?}")),
    }
}

/// Move both page files to `<vault>/.herbarium/trash/<id>/`. Replaces existing entry if present.
pub fn trash_page(vault: &Path, meta: &PageMeta) -> VaultResult<()> {
    checked_trash_id(&meta.id)?;
    let vault_canonical = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
    let src_html = html_path(vault, &meta.id, meta.folder.as_deref());
    let src_meta = meta_path(vault, &meta.id, meta.folder.as_deref());
    if src_html.exists() && !is_within_vault(&src_html, &vault_canonical) {
        return Err("source html escapes vault".to_string());
    }
    if src_meta.exists() && !is_within_vault(&src_meta, &vault_canonical) {
        return Err("source json escapes vault".to_string());
    }

    let trash_root = vault.join(".herbarium").join("trash");
    fs::create_dir_all(&trash_root).map_err(|e| format!("cannot create trash directory: {e}"))?;

    let trash_dir = trash_root.join(&meta.id);
    let stage_name = format!(".stage-{}-{}", meta.id, uuid::Uuid::new_v4());
    let stage_dir = trash_root.join(&stage_name);
    fs::create_dir_all(&stage_dir)
        .map_err(|e| format!("cannot create trash staging directory: {e}"))?;

    let tgt_html = stage_dir.join(format!("{}.html", meta.id));
    let tgt_meta = stage_dir.join(format!("{}.json", meta.id));
    let tgt_trashed = stage_dir.join("trashed.json");

    let mut moved_html = false;
    let mut moved_meta = false;

    let stage_res = (|| -> VaultResult<()> {
        if src_html.exists() {
            move_file(&src_html, &tgt_html).map_err(|e| format!("cannot trash html: {e}"))?;
            moved_html = true;
        }
        if src_meta.exists() {
            move_file(&src_meta, &tgt_meta).map_err(|e| format!("cannot trash json: {e}"))?;
            moved_meta = true;
        } else {
            let json = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
            write_atomic(&tgt_meta, json.as_bytes())?;
        }

        let trashed_json = serde_json::json!({ "deletedAt": crate::time::now_ms() });
        let trashed_bytes =
            serde_json::to_string_pretty(&trashed_json).map_err(|e| e.to_string())?;
        write_atomic(&tgt_trashed, trashed_bytes.as_bytes())?;
        Ok(())
    })();

    // Put the live files back and drop the stage. The stage is kept whenever
    // restoring fails, because it then holds the only surviving copy.
    let rollback_stage = |cause: String| -> String {
        let mut failures = Vec::new();
        for (moved, from, to) in [
            (moved_html, &tgt_html, &src_html),
            (moved_meta, &tgt_meta, &src_meta),
        ] {
            if !moved || !from.exists() {
                continue;
            }
            if to.exists() {
                failures.push(format!("{} was recreated", to.display()));
            } else if let Err(e) = move_file(from, to) {
                failures.push(format!("cannot restore {}: {e}", to.display()));
            }
        }
        if failures.is_empty() {
            let _ = fs::remove_dir_all(&stage_dir);
            cause
        } else {
            format!(
                "{cause}; rollback failed ({}); surviving copy kept in {}",
                failures.join(", "),
                stage_dir.display()
            )
        }
    };

    if let Err(e) = stage_res {
        return Err(rollback_stage(e));
    }

    if trash_dir.exists() {
        let backup_name = format!(".backup-{}-{}", meta.id, uuid::Uuid::new_v4());
        let backup_dir = trash_root.join(&backup_name);
        if let Err(e) = move_file(&trash_dir, &backup_dir) {
            return Err(rollback_stage(format!(
                "cannot stage existing trash replacement: {e}"
            )));
        }

        if let Err(e) = move_file(&stage_dir, &trash_dir) {
            let mut cause = format!("cannot activate trash directory: {e}");
            if let Err(re) = move_file(&backup_dir, &trash_dir) {
                cause.push_str(&format!(
                    "; previous trash entry kept in {} ({re})",
                    backup_dir.display()
                ));
            }
            return Err(rollback_stage(cause));
        }

        let _ = fs::remove_dir_all(&backup_dir);
    } else if let Err(e) = move_file(&stage_dir, &trash_dir) {
        return Err(rollback_stage(format!(
            "cannot activate trash directory: {e}"
        )));
    }

    Ok(())
}

/// List all pages in the trash, newest first.
pub fn list_trash(vault: &Path) -> VaultResult<Vec<TrashEntry>> {
    let trash_root = vault.join(".herbarium").join("trash");
    if !trash_root.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&trash_root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let dir_path = entry.path();
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        let meta_file = dir_path.join(format!("{id}.json"));
        let mut meta = match fs::read_to_string(&meta_file) {
            Ok(raw) => {
                serde_json::from_str::<PageMeta>(&raw).unwrap_or_else(|_| PageMeta::new(id.clone()))
            }
            Err(_) => PageMeta::new(id.clone()),
        };
        meta.upgrade();
        meta.id = id.clone();

        let trashed_file = dir_path.join("trashed.json");
        let deleted_at = match fs::read_to_string(&trashed_file) {
            Ok(raw) => {
                let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
                v.get("deletedAt")
                    .and_then(|n| n.as_i64())
                    .unwrap_or_else(crate::time::now_ms)
            }
            Err(_) => file_mtime_ms(&dir_path).unwrap_or_else(crate::time::now_ms),
        };

        entries.push(TrashEntry { meta, deleted_at });
    }

    entries.sort_by_key(|e| Reverse(e.deleted_at));
    Ok(entries)
}

/// A non-colliding path beside the restored page where a damaged trash sidecar
/// is preserved, so the user can recover any hand-written metadata.
fn unique_backup_path(dir: &Path, id: &str) -> PathBuf {
    let candidate = dir.join(format!("{id}.json.corrupt"));
    if !candidate.exists() {
        candidate
    } else {
        dir.join(format!("{id}.json.corrupt-{}", uuid::Uuid::new_v4()))
    }
}

/// Restore a trashed page back to its original folder (or root if invalid).
pub fn restore_page(vault: &Path, id: &str) -> VaultResult<PageMeta> {
    let id = checked_trash_id(id)?;
    let trash_dir = vault.join(".herbarium").join("trash").join(id);
    if !trash_dir.exists() {
        return Err(format!("trash entry not found: {id}"));
    }

    let vault_canonical = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
    // The entry must itself be inside the vault (a symlinked entry could move
    // files from outside it back into the vault).
    if !is_within_vault(&trash_dir, &vault_canonical) {
        return Err(format!("trash entry escapes vault: {id}"));
    }

    let meta_file = trash_dir.join(format!("{id}.json"));
    // Tolerant restore: `list_trash` already surfaces entries whose sidecar is
    // damaged with fallback metadata, so they must remain restorable. The
    // damaged original is backed up beside the restored page below.
    let mut damaged_sidecar: Option<String> = None;
    let mut meta = match fs::read_to_string(&meta_file) {
        Ok(raw) => match serde_json::from_str::<PageMeta>(&raw) {
            Ok(meta) => meta,
            Err(_) => {
                eprintln!(
                    "herbarium: trash sidecar {id}.json is invalid; restoring with fallback metadata"
                );
                damaged_sidecar = Some(raw);
                PageMeta::new(id.to_string())
            }
        },
        Err(_) => PageMeta::new(id.to_string()),
    };
    meta.upgrade();
    meta.id = id.to_string();

    // Validate folder
    let valid_folder = clean_folder(meta.folder.as_deref()).unwrap_or(None);
    meta.folder = valid_folder;

    let dest_dir = dir_for(vault, meta.folder.as_deref());
    if !is_within_vault(&dest_dir, &vault_canonical) {
        return Err("destination folder escapes vault".to_string());
    }

    let dest_html = html_path(vault, id, meta.folder.as_deref());
    let dest_meta = meta_path(vault, id, meta.folder.as_deref());

    if dest_html.exists() || dest_meta.exists() {
        return Err("cannot restore page: destination files already exist".to_string());
    }

    fs::create_dir_all(&dest_dir).map_err(|e| format!("cannot create folder: {e}"))?;
    if !is_within_vault(&dest_dir, &vault_canonical) {
        return Err("destination folder escapes vault".to_string());
    }

    let src_html = trash_dir.join(format!("{id}.html"));
    let src_meta = trash_dir.join(format!("{id}.json"));

    let mut moved_html = false;
    if src_html.exists() {
        move_file(&src_html, &dest_html).map_err(|e| format!("cannot restore html: {e}"))?;
        moved_html = true;
    }
    let rollback_html = || {
        if moved_html {
            let _ = move_file(&dest_html, &src_html);
        }
    };

    match damaged_sidecar {
        Some(raw) => {
            // Keep the damaged bytes for the user, then write valid fallback
            // metadata so the restored page is immediately usable.
            let backup = unique_backup_path(&dest_dir, id);
            if let Err(e) = fs::write(&backup, raw.as_bytes()) {
                rollback_html();
                return Err(format!("cannot preserve damaged sidecar: {e}"));
            }
            if let Err(e) = write_meta(vault, &meta) {
                let _ = fs::remove_file(&backup);
                rollback_html();
                return Err(e);
            }
        }
        None if src_meta.exists() => {
            if let Err(e) = move_file(&src_meta, &dest_meta) {
                rollback_html();
                return Err(format!("cannot restore json: {e}"));
            }
        }
        None => {
            if let Err(e) = write_meta(vault, &meta) {
                rollback_html();
                return Err(e);
            }
        }
    }

    let _ = fs::remove_dir_all(&trash_dir);
    Ok(meta)
}

/// Delete one or all trash entries. Returns the number of entries removed.
pub fn purge_trash(vault: &Path, id: Option<&str>) -> VaultResult<usize> {
    let trash_root = vault.join(".herbarium").join("trash");
    if !trash_root.exists() {
        return Ok(0);
    }

    if let Some(id) = id {
        let id = checked_trash_id(id)?;
        let entry_dir = trash_root.join(id);
        if entry_dir.exists() {
            let vault_canonical = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
            if !is_within_vault(&entry_dir, &vault_canonical) {
                return Err(format!("trash entry escapes vault: {id}"));
            }
            fs::remove_dir_all(&entry_dir).map_err(|e| format!("cannot purge trash: {e}"))?;
            Ok(1)
        } else {
            Ok(0)
        }
    } else {
        let mut count = 0;
        for entry in fs::read_dir(&trash_root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                fs::remove_dir_all(entry.path())
                    .map_err(|e| format!("cannot purge trash entry: {e}"))?;
                count += 1;
            }
        }
        Ok(count)
    }
}

/// Rename a folder and update the folder field of every sidecar under it.
pub fn rename_folder(vault: &Path, from: &str, to: &str) -> VaultResult<String> {
    let from_clean =
        clean_folder(Some(from))?.ok_or_else(|| "invalid source folder".to_string())?;
    let to_clean =
        clean_folder(Some(to))?.ok_or_else(|| "invalid destination folder".to_string())?;
    if from_clean == to_clean {
        return Ok(to_clean);
    }
    let vault_canonical = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
    let from_dir = vault.join(&from_clean);
    let to_dir = vault.join(&to_clean);
    if !from_dir.exists() {
        return Err(format!("folder not found: {from_clean}"));
    }
    if from_dir.is_symlink() || to_dir.is_symlink() {
        return Err("folder rename does not follow symlink aliases".to_string());
    }
    if !is_within_vault(&from_dir, &vault_canonical) {
        return Err("source folder escapes vault".to_string());
    }
    // On a case-insensitive filesystem a case-only rename resolves source and
    // destination to the same directory; that is not a conflicting destination.
    let same_dir = match (fs::canonicalize(&from_dir), fs::canonicalize(&to_dir)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    if to_dir.exists() && !same_dir {
        return Err(format!("destination folder already exists: {to_clean}"));
    }
    let from_prefix = format!("{from_clean}/");
    if to_clean.starts_with(&from_prefix) {
        return Err("cannot move a folder inside itself".to_string());
    }
    // Containment is checked before creating the destination parents, so a
    // symlinked ancestor cannot make us create directories outside the vault,
    // and rechecked afterwards to close the race.
    if !is_within_vault(&to_dir, &vault_canonical) {
        return Err("destination folder escapes vault".to_string());
    }
    if !same_dir && let Some(parent) = to_dir.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("cannot create parent directory: {e}"))?;
    }
    if !is_within_vault(&to_dir, &vault_canonical) {
        return Err("destination folder escapes vault".to_string());
    }
    if same_dir {
        // Rename through a unique sibling so the destination ends up with the
        // requested casing even when it currently names the same directory.
        let parent = to_dir.parent().unwrap_or(vault);
        if !is_within_vault(parent, &vault_canonical) {
            return Err("destination folder escapes vault".to_string());
        }
        let temp_dir = parent.join(format!(".herbarium-rename-{}", uuid::Uuid::new_v4()));
        fs::rename(&from_dir, &temp_dir).map_err(|e| format!("cannot rename folder: {e}"))?;
        if let Err(e) = fs::rename(&temp_dir, &to_dir) {
            return match fs::rename(&temp_dir, &from_dir) {
                Ok(()) => Err(format!("cannot rename folder: {e}")),
                Err(rollback) => Err(format!(
                    "cannot rename folder: {e}; rollback failed ({rollback}); folder remains at {}",
                    temp_dir.display()
                )),
            };
        }
    } else {
        fs::rename(&from_dir, &to_dir).map_err(|e| format!("cannot rename folder: {e}"))?;
    }

    let mut updated_sidecars: Vec<(PathBuf, String)> = Vec::new();

    fn update_sidecars(
        dir: &Path,
        vault: &Path,
        vault_canonical: &Path,
        updated: &mut Vec<(PathBuf, String)>,
    ) -> VaultResult<()> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            entries.push(entry.map_err(|e| e.to_string())?);
        }
        entries.sort_by_key(|e| e.path());

        for entry in entries {
            let path = entry.path();
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            // Never follow or rewrite symlinks: a directory symlink can loop or
            // escape, and a file symlink is not a page this rename created.
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                if !is_within_vault(&path, vault_canonical) {
                    continue;
                }
                update_sidecars(&path, vault, vault_canonical, updated)?;
            } else if path.extension().is_some_and(|ext| ext == "json") {
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                if stem.is_empty() || stem.starts_with('.') || stem == "trashed" {
                    continue;
                }
                // Only a genuine page sidecar is rewritten: a same-stem HTML
                // page must exist and the metadata id must match the stem.
                // Every other user JSON is left byte-for-byte untouched.
                if !path.with_file_name(format!("{stem}.html")).is_file() {
                    continue;
                }
                let raw = fs::read_to_string(&path)
                    .map_err(|e| format!("cannot read sidecar {}: {e}", path.display()))?;
                let mut meta = serde_json::from_str::<PageMeta>(&raw)
                    .map_err(|e| format!("cannot parse sidecar {}: {e}", path.display()))?;
                if meta.id != stem {
                    continue;
                }
                meta.upgrade();
                let parent_dir = path.parent().unwrap_or(vault);
                let rel = parent_dir.strip_prefix(vault).map_err(|e| e.to_string())?;
                meta.folder = clean_folder(Some(&rel.to_string_lossy()))?;
                let json = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
                write_atomic(&path, json.as_bytes())?;
                updated.push((path, raw));
            }
        }
        Ok(())
    }

    if let Err(err) = update_sidecars(&to_dir, vault, &vault_canonical, &mut updated_sidecars) {
        for (path, prev_content) in updated_sidecars.iter().rev() {
            let _ = write_atomic(path, prev_content.as_bytes());
        }
        match fs::rename(&to_dir, &from_dir) {
            Ok(_) => {
                return Err(format!(
                    "cannot update sidecars after renaming folder: {err}; folder restored to {from_clean}"
                ));
            }
            Err(rollback_err) => {
                return Err(format!(
                    "cannot update sidecars after renaming folder: {err}; rollback failed ({rollback_err}); folder remains at {to_clean}"
                ));
            }
        }
    }

    Ok(to_clean)
}

/// Delete a folder tree only if it contains no files at any depth (empty except directories).
pub fn delete_folder(vault: &Path, folder: &str) -> VaultResult<()> {
    let clean = clean_folder(Some(folder))?.ok_or_else(|| "folder name is empty".to_string())?;
    let dir = vault.join(&clean);
    if !dir.exists() {
        return Ok(());
    }
    let vault_canonical = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
    if !is_within_vault(&dir, &vault_canonical) {
        return Err("folder escapes vault".to_string());
    }

    fn has_any_files(dir: &Path, vault_canonical: &Path) -> bool {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let ft = match entry.file_type() {
                    Ok(ft) => ft,
                    Err(_) => return true,
                };
                if ft.is_symlink() {
                    return true;
                }
                if ft.is_dir() {
                    let path = entry.path();
                    if !is_within_vault(&path, vault_canonical) {
                        return true;
                    }
                    if has_any_files(&path, vault_canonical) {
                        return true;
                    }
                } else {
                    return true;
                }
            }
        }
        false
    }

    if has_any_files(&dir, &vault_canonical) {
        return Err("folder is not empty".to_string());
    }

    fs::remove_dir_all(&dir).map_err(|e| format!("cannot delete folder: {e}"))?;
    Ok(())
}

/// Pages in the folder and its subfolders, from the index.
pub fn folder_pages(store: &Store, folder: &str) -> VaultResult<Vec<PageMeta>> {
    let clean = clean_folder(Some(folder))?.ok_or_else(|| "folder name is empty".to_string())?;
    let all = store.all().map_err(|e| e.to_string())?;
    let prefix = format!("{clean}/");
    let matches = all
        .into_iter()
        .filter(|m| {
            m.folder
                .as_deref()
                .is_some_and(|f| f == clean || f.starts_with(&prefix))
        })
        .collect();
    Ok(matches)
}

fn transliterate(c: char) -> &'static str {
    match c {
        'a' | 'A' | 'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' | 'À' | 'Á' | 'Â' | 'Ã'
        | 'Ä' | 'Å' | 'Ā' | 'Ă' | 'Ą' => "a",
        'æ' | 'Æ' => "ae",
        'b' | 'B' => "b",
        'c' | 'C' | 'ç' | 'ć' | 'č' | 'ĉ' | 'ċ' | 'Ç' | 'Ć' | 'Č' | 'Ĉ' | 'Ċ' => "c",
        'd' | 'D' | 'ď' | 'đ' | 'Ď' | 'Đ' => "d",
        'e' | 'E' | 'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' | 'È' | 'É' | 'Ê' | 'Ë'
        | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' => "e",
        'f' | 'F' => "f",
        'g' | 'G' | 'ğ' | 'ĝ' | 'ġ' | 'ģ' | 'Ğ' | 'Ĝ' | 'Ġ' | 'Ģ' => "g",
        'h' | 'H' | 'ĥ' | 'ħ' | 'Ĥ' | 'Ħ' => "h",
        'i' | 'I' | 'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ĭ' | 'į' | 'ı' | 'Ì' | 'Í' | 'Î' | 'Ï' | 'Ī'
        | 'Ĭ' | 'Į' | 'İ' => "i",
        'j' | 'J' | 'ĵ' | 'Ĵ' => "j",
        'k' | 'K' | 'ķ' | 'Ķ' => "k",
        'l' | 'L' | 'ĺ' | 'ļ' | 'ľ' | 'ł' | 'Ĺ' | 'Ļ' | 'Ľ' | 'Ł' => "l",
        'm' | 'M' => "m",
        'n' | 'N' | 'ñ' | 'ń' | 'ņ' | 'ň' | 'ŋ' | 'Ñ' | 'Ń' | 'Ņ' | 'Ň' | 'Ŋ' => "n",
        'o' | 'O' | 'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' | 'Ò' | 'Ó' | 'Ô' | 'Õ'
        | 'Ö' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' => "o",
        'œ' | 'Œ' => "oe",
        'p' | 'P' => "p",
        'q' | 'Q' => "q",
        'r' | 'R' | 'ŕ' | 'ŗ' | 'ř' | 'Ŕ' | 'Ŗ' | 'Ř' => "r",
        's' | 'S' | 'ś' | 'ŝ' | 'ş' | 'š' | 'ß' | 'Ś' | 'Ŝ' | 'Ş' | 'Š' => {
            if c == 'ß' {
                "ss"
            } else {
                "s"
            }
        }
        't' | 'T' | 'ţ' | 'ť' | 'ŧ' | 'Ţ' | 'Ť' | 'Ŧ' => "t",
        'u' | 'U' | 'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' | 'Ù' | 'Ú' | 'Û'
        | 'Ü' | 'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' => "u",
        'v' | 'V' => "v",
        'w' | 'W' | 'ŵ' | 'Ŵ' => "w",
        'x' | 'X' => "x",
        'y' | 'Y' | 'ý' | 'ÿ' | 'ŷ' | 'Ý' | 'Ÿ' | 'Ŷ' => "y",
        'z' | 'Z' | 'ź' | 'ż' | 'ž' | 'Ź' | 'Ż' | 'Ž' => "z",
        '0'..='9' => match c {
            '0' => "0",
            '1' => "1",
            '2' => "2",
            '3' => "3",
            '4' => "4",
            '5' => "5",
            '6' => "6",
            '7' => "7",
            '8' => "8",
            '9' => "9",
            _ => "",
        },
        _ => "-",
    }
}

pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    for c in title.chars() {
        let rep = transliterate(c);
        if rep == "-" {
            if !last_dash {
                out.push('-');
                last_dash = true;
            }
        } else {
            out.push_str(rep);
            last_dash = false;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.len() > 60 {
        out.truncate(60);
        while out.ends_with('-') {
            out.pop();
        }
    }
    if out.is_empty() {
        "page".to_string()
    } else {
        out
    }
}

fn id_exists_on_disk(vault: &Path, id: &str) -> bool {
    let vault_canonical = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
    fn check_dir(dir: &Path, id: &str, vault_canonical: &Path) -> bool {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                let ft = match entry.file_type() {
                    Ok(ft) => ft,
                    Err(_) => continue,
                };
                if ft.is_symlink() && !is_within_vault(&path, vault_canonical) {
                    continue;
                }
                if ft.is_symlink() && path.is_dir() {
                    continue;
                }
                if ft.is_dir() {
                    if !name.starts_with('.')
                        && is_within_vault(&path, vault_canonical)
                        && check_dir(&path, id, vault_canonical)
                    {
                        return true;
                    }
                } else if name == format!("{id}.html") || name == format!("{id}.json") {
                    return true;
                }
            }
        }
        false
    }
    check_dir(vault, id, &vault_canonical)
}

fn is_id_available(store: &Store, id: &str) -> bool {
    if let Ok(Some(_)) = store.get_meta(id) {
        return false;
    }
    if store
        .vault
        .join(".herbarium")
        .join("trash")
        .join(id)
        .exists()
    {
        return false;
    }
    if id_exists_on_disk(&store.vault, id) {
        return false;
    }
    true
}

pub fn new_page_id(store: &Store, title: &str) -> String {
    let base = slugify(title);
    let mut candidate = base.clone();
    let mut n = 2;
    while !is_id_available(store, &candidate) {
        candidate = format!("{base}-{n}");
        n += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{DAY_MS, now_ms};

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
        let demo = fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../examples/semver.html"
        ))
        .expect("examples/semver.html present");
        let mut meta = PageMeta::new("semver-demo".into());
        meta.title = "semver — Semantic Versioning Reference".into();
        write_page(&vault, &meta, &demo).expect("write_page ok");

        let outcome = index_vault(&store).unwrap();
        assert_eq!(outcome.indexed, 1, "index_vault imported the demo page");
        assert_eq!(outcome.removed, 0, "nothing removed on first index");
        assert_eq!(store.count().unwrap(), 1);

        // Title was auto-extracted/kept.
        let found = store.get_meta("semver-demo").unwrap().unwrap();
        assert_eq!(found.title, "semver — Semantic Versioning Reference");

        // Stage 3 — search "semver" finds it (FTS over title/tags/text).
        let hits = store.search("semver", 50).unwrap();
        assert!(
            hits.iter().any(|m| m.meta.id == "semver-demo"),
            "search 'semver' returns the demo page"
        );

        // Stage 5 — "Review in 3 days".
        let future = now_ms() + 3 * DAY_MS;
        let mut due_meta = found.clone();
        due_meta.interval_minutes = Some(3 * 1440);
        due_meta.next_review = Some(future);
        due_meta.last_review = Some(now_ms());
        due_meta.updated_at = now_ms();
        store
            .upsert(&due_meta, &crate::content::extract_text(&demo), 0)
            .unwrap();

        // Not due today…
        assert!(
            !store
                .due(now_ms())
                .unwrap()
                .iter()
                .any(|m| m.id == "semver-demo")
        );
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
        file.set_modified(UNIX_EPOCH + std::time::Duration::from_secs(secs))
            .unwrap();
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
            fs::rename(
                vault.join(format!("a/x.{ext}")),
                vault.join(format!("b/x.{ext}")),
            )
            .unwrap();
        }
        index_vault(&store).unwrap();
        assert_eq!(
            store.get_meta("x").unwrap().unwrap().folder.as_deref(),
            Some("b")
        );

        // Renaming the pair renames the page, whatever id the sidecar holds.
        for ext in ["html", "json"] {
            fs::rename(
                vault.join(format!("b/x.{ext}")),
                vault.join(format!("b/y.{ext}")),
            )
            .unwrap();
        }
        index_vault(&store).unwrap();
        assert!(store.get_meta("x").unwrap().is_none());
        let renamed = store
            .get_meta("y")
            .unwrap()
            .expect("renamed page stays indexed");
        assert_eq!(renamed.tags, vec!["kept".to_string()]);
        index_vault(&store).unwrap();
        assert!(
            store.get_meta("y").unwrap().is_some(),
            "a second rescan keeps it"
        );

        // A damaged sidecar is reported, never replaced by blank metadata.
        fs::write(vault.join("b/y.json"), "{ \"tags\": [\"kept\"").unwrap();
        set_mtime(&vault.join("b/y.html"), 1_000);
        index_vault(&store).unwrap();
        assert!(
            store.get_meta("y").unwrap().is_some(),
            "the page is still indexed from its HTML"
        );
        assert_eq!(
            fs::read_to_string(vault.join("b/y.json")).unwrap(),
            "{ \"tags\": [\"kept\""
        );

        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn same_stem_in_two_folders_indexes_one_and_moving_never_overwrites_the_other() {
        let vault = temp_vault("dup-stem");
        let store = Store::open(vault.clone()).unwrap();
        for dir in ["a", "b"] {
            fs::create_dir_all(vault.join(dir)).unwrap();
            fs::write(
                vault.join(format!("{dir}/dup.html")),
                format!("<!DOCTYPE html><html><body><p>{dir}</p></body></html>"),
            )
            .unwrap();
        }
        let outcome = index_vault(&store).unwrap();
        assert_eq!(store.count().unwrap(), 1);
        assert_eq!(
            outcome.skipped.len(),
            1,
            "duplicate stem reported in skipped"
        );
        assert_eq!(outcome.skipped[0].path, "b/dup.html");

        let mut meta = store.get_meta("dup").unwrap().unwrap();
        assert_eq!(
            meta.folder.as_deref(),
            Some("a"),
            "first path in sorted order wins"
        );
        index_vault(&store).unwrap();
        assert_eq!(
            store.get_meta("dup").unwrap().unwrap().folder.as_deref(),
            Some("a"),
            "and keeps winning"
        );

        let err = move_page(&vault, &mut meta, Some("b")).unwrap_err();
        assert!(err.contains("already exists"), "{err}");
        assert_eq!(meta.folder.as_deref(), Some("a"));

        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn folders_cannot_be_hidden_directories_that_rescans_skip() {
        for hidden in [".herbarium", "a/.git", "./.x"] {
            assert!(clean_folder(Some(hidden)).is_err(), "{hidden}");
        }
        assert_eq!(
            clean_folder(Some("./a/b/")).unwrap().as_deref(),
            Some("a/b")
        );
    }

    #[test]
    fn write_atomic_creates_and_replaces_target_file() {
        let vault = temp_vault("atomic");
        let file = vault.join("sub/test.txt");
        write_atomic(&file, b"initial content").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "initial content");

        write_atomic(&file, b"updated content").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "updated content");
        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn delete_and_move_page_do_not_prune_empty_directories() {
        let vault = temp_vault("no-prune");
        let mut meta = PageMeta::new("p1".into());
        meta.folder = Some("folder/sub".into());
        write_page(
            &vault,
            &meta,
            "<!DOCTYPE html><html><body><p>1</p></body></html>",
        )
        .unwrap();

        move_page(&vault, &mut meta, Some("other/dest")).unwrap();
        assert!(
            vault.join("folder/sub").exists(),
            "source dir must not be pruned on move"
        );

        delete_page_files(&vault, &meta).unwrap();
        assert!(
            vault.join("other/dest").exists(),
            "dest dir must not be pruned on delete"
        );
        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn trash_lifecycle_trash_list_restore_purge() {
        let vault = temp_vault("trash");
        let mut meta = PageMeta::new("doc1".into());
        meta.title = "Trashed Doc".into();
        meta.folder = Some("myfolder".into());
        write_page(
            &vault,
            &meta,
            "<!DOCTYPE html><html><body><p>Trashed</p></body></html>",
        )
        .unwrap();

        // Trash the page
        trash_page(&vault, &meta).unwrap();
        assert!(!html_path(&vault, "doc1", Some("myfolder")).exists());
        assert!(vault.join(".herbarium/trash/doc1/doc1.html").exists());
        assert!(vault.join(".herbarium/trash/doc1/trashed.json").exists());
        assert!(
            vault.join("myfolder").exists(),
            "folder not pruned on trash"
        );

        // List trash
        let trashed = list_trash(&vault).unwrap();
        assert_eq!(trashed.len(), 1);
        assert_eq!(trashed[0].meta.id, "doc1");
        assert_eq!(trashed[0].meta.title, "Trashed Doc");

        // Restore page
        let restored = restore_page(&vault, "doc1").unwrap();
        assert_eq!(restored.id, "doc1");
        assert_eq!(restored.folder.as_deref(), Some("myfolder"));
        assert!(html_path(&vault, "doc1", Some("myfolder")).exists());
        assert!(!vault.join(".herbarium/trash/doc1").exists());

        // Trash again and purge
        trash_page(&vault, &restored).unwrap();
        assert_eq!(purge_trash(&vault, Some("doc1")).unwrap(), 1);
        assert_eq!(list_trash(&vault).unwrap().len(), 0);

        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn folder_operations_rename_delete_folder_pages() {
        let vault = temp_vault("folder-ops");
        let store = Store::open(vault.clone()).unwrap();

        let mut p1 = PageMeta::new("p1".into());
        p1.folder = Some("src/nested".into());
        write_page(
            &vault,
            &p1,
            "<!DOCTYPE html><html><body><p>p1</p></body></html>",
        )
        .unwrap();
        index_vault(&store).unwrap();

        // folder_pages returns nested pages
        let pages = folder_pages(&store, "src").unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].id, "p1");

        // Rename folder
        let renamed = rename_folder(&vault, "src", "dest").unwrap();
        assert_eq!(renamed, "dest");
        assert!(vault.join("dest/nested/p1.html").exists());
        let sidecar = read_meta(&vault, "p1", Some("dest/nested"))
            .unwrap()
            .unwrap();
        assert_eq!(sidecar.folder.as_deref(), Some("dest/nested"));

        // Delete folder rejects non-empty
        let err = delete_folder(&vault, "dest").unwrap_err();
        assert_eq!(err, "folder is not empty");

        // Delete page files then delete folder
        delete_page_files(&vault, &sidecar).unwrap();
        delete_folder(&vault, "dest").unwrap();
        assert!(!vault.join("dest").exists());

        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn slugify_and_new_page_id_uniqueness() {
        let vault = temp_vault("slug");
        let store = Store::open(vault.clone()).unwrap();

        assert_eq!(
            slugify("Café & Restaurant Été 2026"),
            "cafe-restaurant-ete-2026"
        );
        assert_eq!(slugify("---!@#$---"), "page");

        let id1 = new_page_id(&store, "My Note");
        assert_eq!(id1, "my-note");

        let meta1 = PageMeta::new(id1.clone());
        store.upsert(&meta1, "", 0).unwrap();

        let id2 = new_page_id(&store, "My Note");
        assert_eq!(id2, "my-note-2");

        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn title_and_date_refresh_with_source_title() {
        let vault = temp_vault("title-refresh");
        let store = Store::open(vault.clone()).unwrap();

        let meta = PageMeta::new("page-a".into());
        write_page(&vault, &meta, "<!DOCTYPE html><html><head><title>First Title</title></head><body><p>body</p></body></html>").unwrap();
        index_vault(&store).unwrap();

        let indexed1 = store.get_meta("page-a").unwrap().unwrap();
        assert_eq!(indexed1.title, "First Title");
        assert_eq!(indexed1.source_title, Some("First Title".into()));

        // User never renamed, so new HTML title updates title and source_title
        write_page(&vault, &indexed1, "<!DOCTYPE html><html><head><title>Second Title</title></head><body><p>body 2</p></body></html>").unwrap();
        index_vault(&store).unwrap();

        let indexed2 = store.get_meta("page-a").unwrap().unwrap();
        assert_eq!(indexed2.title, "Second Title");
        assert_eq!(indexed2.source_title, Some("Second Title".into()));

        // User manually renamed the page
        let mut user_renamed = indexed2.clone();
        user_renamed.title = "User Custom Name".into();
        write_meta(&vault, &user_renamed).unwrap();

        // New HTML written with a third title
        let hp = html_path(&vault, "page-a", None);
        write_atomic(&hp, b"<!DOCTYPE html><html><head><title>Third Title</title></head><body><p>body 3</p></body></html>").unwrap();
        index_vault(&store).unwrap();

        let indexed3 = store.get_meta("page-a").unwrap().unwrap();
        assert_eq!(
            indexed3.title, "User Custom Name",
            "user rename is preserved"
        );
        assert_eq!(
            indexed3.source_title,
            Some("Third Title".into()),
            "source_title always refreshed"
        );

        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn non_utf8_windows_1252_and_skipped_reports() {
        let vault = temp_vault("charset-skipped");
        let store = Store::open(vault.clone()).unwrap();

        // Write a Windows-1252 encoded file with smart quote 0x93 and 0x94
        let mut raw = Vec::new();
        raw.extend_from_slice(b"<!DOCTYPE html><html><head><meta charset=\"windows-1252\"><title>Quotes</title></head><body><p>");
        raw.push(0x93);
        raw.extend_from_slice(b"hello");
        raw.push(0x94);
        raw.extend_from_slice(b"</p></body></html>");
        fs::write(vault.join("windows.html"), &raw).unwrap();

        // Write a non-HTML file ending in .html
        fs::write(vault.join("not-html.html"), b"just plain text without html").unwrap();

        let outcome = index_vault(&store).unwrap();
        assert_eq!(outcome.indexed, 1);
        let win_meta = store.get_meta("windows").unwrap().unwrap();
        assert_eq!(win_meta.id, "windows");
        let text = store.text_for("windows").unwrap().unwrap();
        assert!(
            text.contains('“') && text.contains('”'),
            "windows-1252 smart quotes decoded: {text}"
        );

        assert_eq!(outcome.skipped.len(), 1);
        assert_eq!(outcome.skipped[0].path, "not-html.html");
        assert_eq!(outcome.skipped[0].reason, "not an HTML document");

        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn delete_folder_rejects_non_page_user_files() {
        let vault = temp_vault("delete-safety");
        let folder = vault.join("my-folder");
        fs::create_dir_all(&folder).unwrap();
        let user_file = folder.join("notes.txt");
        fs::write(&user_file, "important user note").unwrap();

        let err = delete_folder(&vault, "my-folder").unwrap_err();
        assert_eq!(err, "folder is not empty");
        assert!(user_file.exists(), "user file must not be erased");

        // Now test empty subdirectories are allowed to be deleted
        fs::remove_file(&user_file).unwrap();
        let sub = folder.join("empty-sub");
        fs::create_dir_all(&sub).unwrap();
        delete_folder(&vault, "my-folder").unwrap();
        assert!(
            !vault.join("my-folder").exists(),
            "empty directory tree deleted"
        );

        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn symlink_outside_vault_never_traversed() {
        let vault = temp_vault("symlink-vault");
        let store = Store::open(vault.clone()).unwrap();

        let outside = temp_vault("symlink-outside");
        fs::write(
            outside.join("secret.html"),
            "<!DOCTYPE html><html><head><title>Secret</title></head><body><p>sensitive</p></body></html>",
        )
        .unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link = vault.join("outside_link");
            let _ = symlink(&outside, &link);

            let outcome = index_vault(&store).unwrap();
            assert_eq!(outcome.indexed, 0, "must not index outside symlinks");
            assert!(store.get_meta("secret").unwrap().is_none());

            let folders = list_folders(&vault).unwrap();
            assert!(
                !folders.contains(&"outside_link".to_string()),
                "must not list outside symlink folder"
            );

            let err = delete_folder(&vault, "outside_link");
            assert!(err.is_err(), "must reject deleting folder outside vault");
        }

        let _ = fs::remove_dir_all(&outside);
        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn trash_replacement_preserves_existing_trash() {
        let vault = temp_vault("trash-replace");
        let meta1 = PageMeta::new("doc1".into());
        write_page(&vault, &meta1, "<p>version 1</p>").unwrap();
        trash_page(&vault, &meta1).unwrap();

        let trashed_file = vault.join(".herbarium/trash/doc1/doc1.html");
        assert_eq!(
            fs::read_to_string(&trashed_file).unwrap(),
            "<p>version 1</p>"
        );

        // Now write version 2 and trash again
        let meta2 = PageMeta::new("doc1".into());
        write_page(&vault, &meta2, "<p>version 2</p>").unwrap();
        trash_page(&vault, &meta2).unwrap();

        assert_eq!(
            fs::read_to_string(&trashed_file).unwrap(),
            "<p>version 2</p>"
        );
        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn folder_rename_rolls_back_on_sidecar_failure() {
        let vault = temp_vault("rename-rollback");
        let mut p1 = PageMeta::new("p1".into());
        p1.folder = Some("src".into());
        write_page(&vault, &p1, "<p>p1</p>").unwrap();

        let mut p2 = PageMeta::new("p2".into());
        p2.folder = Some("src".into());
        write_page(&vault, &p2, "<p>p2</p>").unwrap();
        // Invalidate p2 sidecar so that updating sidecars encounters a malformed sidecar and aborts rename
        fs::write(vault.join("src/p2.json"), b"invalid json content").unwrap();

        // Renaming folder should fail on p2.json and rollback src
        let err = rename_folder(&vault, "src", "dest");
        assert!(err.is_err(), "rename should fail when sidecar write fails");
        let err_msg = err.unwrap_err();
        assert!(
            err_msg.contains("cannot parse sidecar") || err_msg.contains("cannot read sidecar"),
            "error should describe sidecar failure: {err_msg}"
        );
        assert!(
            err_msg.contains("folder restored to src"),
            "error should indicate rollback to src: {err_msg}"
        );
        assert!(vault.join("src").exists(), "source folder must be restored");
        assert!(
            !vault.join("dest").exists(),
            "dest folder must not exist after rollback"
        );
        let p1_restored = read_meta(&vault, "p1", Some("src")).unwrap().unwrap();
        assert_eq!(
            p1_restored.folder.as_deref(),
            Some("src"),
            "p1 sidecar folder field must be rolled back to src"
        );
        assert_eq!(
            fs::read_to_string(vault.join("src/p2.json")).unwrap(),
            "invalid json content",
            "p2 sidecar must remain unchanged after rollback"
        );
        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn folder_rename_rolls_back_on_nested_sidecar_failure() {
        let vault = temp_vault("rename-nested-rollback");
        let mut p1 = PageMeta::new("p1".into());
        p1.folder = Some("src".into());
        write_page(&vault, &p1, "<p>p1</p>").unwrap();

        let mut p2 = PageMeta::new("p2".into());
        p2.folder = Some("src/sub".into());
        write_page(&vault, &p2, "<p>p2</p>").unwrap();
        fs::write(vault.join("src/sub/p2.json"), b"corrupt json").unwrap();

        let err = rename_folder(&vault, "src", "dest");
        assert!(err.is_err(), "rename should fail on nested corrupt sidecar");
        assert!(vault.join("src").exists(), "source folder must be restored");
        assert!(!vault.join("dest").exists(), "dest folder must not exist");
        let p1_restored = read_meta(&vault, "p1", Some("src")).unwrap().unwrap();
        assert_eq!(p1_restored.folder.as_deref(), Some("src"));
        assert_eq!(
            fs::read_to_string(vault.join("src/sub/p2.json")).unwrap(),
            "corrupt json"
        );

        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn page_fingerprint_distinguishes_missing_from_empty() {
        let vault = temp_vault("fp-precision");
        let fp_missing = page_fingerprint(&vault, "nonexistent", None);
        assert_eq!(fp_missing, "h:none:m:none");

        let hp = html_path(&vault, "test", None);
        write_atomic(&hp, b"").unwrap();
        let fp_empty_html = page_fingerprint(&vault, "test", None);
        assert!(fp_empty_html.starts_with("h:0:"), "empty file has len 0");
        assert_ne!(
            fp_missing, fp_empty_html,
            "missing is distinct from 0-byte file"
        );

        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn atomic_write_cleans_up_temp_on_failure() {
        let vault = temp_vault("atomic-fail");
        let dir = vault.join("target_dir");
        fs::create_dir_all(&dir).unwrap();
        let err = write_atomic(&dir, b"content");
        assert!(err.is_err());
        let entries = fs::read_dir(&vault).unwrap().flatten().collect::<Vec<_>>();
        assert!(
            !entries
                .iter()
                .any(|e| e.file_name().to_string_lossy().contains(".tmp-")),
            "temp file must be cleaned up on failure"
        );
        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn trash_restore_and_purge_reject_ids_that_are_not_single_path_components() {
        let vault = temp_vault("trash-id-guard");
        let meta = PageMeta::new("doc1".into());
        write_page(&vault, &meta, "<p>doc</p>").unwrap();
        trash_page(&vault, &meta).unwrap();

        for bad in ["..", ".", "a/b", "", "a\\b", "../doc1", "/doc1", "doc1/.."] {
            let restore_err = restore_page(&vault, bad).unwrap_err();
            assert!(
                restore_err.contains("invalid page id"),
                "restore must reject {bad:?}: {restore_err}"
            );
            let purge_err = purge_trash(&vault, Some(bad)).unwrap_err();
            assert!(
                purge_err.contains("invalid page id"),
                "purge must reject {bad:?}: {purge_err}"
            );
        }

        // The untouched vault internals must survive: without validation,
        // `..` resolves the entry to `.herbarium` and deleting it destroys the
        // index, settings and every trashed page.
        assert!(vault.join(".herbarium/trash/doc1/doc1.html").exists());
        assert!(vault.join(".herbarium").exists());
        assert_eq!(purge_trash(&vault, Some("doc1")).unwrap(), 1);
        assert_eq!(list_trash(&vault).unwrap().len(), 0);
        let _ = fs::remove_dir_all(&vault);
    }

    #[cfg(unix)]
    #[test]
    fn directory_symlink_loops_are_never_followed_by_scan_list_or_rename() {
        use std::os::unix::fs::symlink;
        let vault = temp_vault("symlink-loop");
        let store = Store::open(vault.clone()).unwrap();
        fs::create_dir_all(vault.join("a")).unwrap();
        fs::write(
            vault.join("a/page.html"),
            "<!DOCTYPE html><html><head><title>Loop</title></head><body><p>x</p></body></html>",
        )
        .unwrap();
        symlink(vault.join("a"), vault.join("a/back")).unwrap();

        let outcome = index_vault(&store).unwrap();
        assert_eq!(outcome.indexed, 1, "only the real page is indexed");
        assert!(store.get_meta("page").unwrap().is_some());
        assert_eq!(list_folders(&vault).unwrap(), vec!["a".to_string()]);
        assert!(!new_page_id(&store, "unrelated").is_empty());

        let renamed = rename_folder(&vault, "a", "b").unwrap();
        assert_eq!(renamed, "b");
        assert_eq!(
            read_meta(&vault, "page", Some("b"))
                .unwrap()
                .unwrap()
                .folder
                .as_deref(),
            Some("b")
        );
        assert_eq!(list_folders(&vault).unwrap(), vec!["b".to_string()]);
        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn malformed_trash_sidecar_restores_with_original_backup() {
        let vault = temp_vault("trash-corrupt");
        let mut meta = PageMeta::new("doc1".into());
        meta.title = "Damaged".into();
        meta.folder = Some("notes".into());
        write_page(
            &vault,
            &meta,
            "<!DOCTYPE html><html><body><p>body</p></body></html>",
        )
        .unwrap();
        trash_page(&vault, &meta).unwrap();

        let damaged = "{ \"title\": [oops";
        fs::write(vault.join(".herbarium/trash/doc1/doc1.json"), damaged).unwrap();

        // Listing still exposes the entry with fallback metadata.
        let listed = list_trash(&vault).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].meta.id, "doc1");

        // Restoring must not be impossible just because the sidecar is damaged.
        let restored = restore_page(&vault, "doc1").unwrap();
        assert_eq!(restored.id, "doc1");
        assert_eq!(
            restored.folder, None,
            "damaged metadata cannot name a folder"
        );

        assert!(vault.join("doc1.html").exists(), "html restored at root");
        let round: PageMeta =
            serde_json::from_str(&fs::read_to_string(vault.join("doc1.json")).unwrap()).unwrap();
        assert_eq!(round.id, "doc1");
        assert_eq!(
            fs::read_to_string(vault.join("doc1.json.corrupt")).unwrap(),
            damaged,
            "the damaged original is preserved as a backup"
        );
        assert!(!vault.join(".herbarium/trash/doc1").exists());
        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn folder_rename_rewrites_only_genuine_page_sidecars() {
        let vault = temp_vault("rename-user-json");
        let mut p1 = PageMeta::new("p1".into());
        p1.folder = Some("src".into());
        write_page(&vault, &p1, "<p>p1</p>").unwrap();

        let user_json = "{\"theme\":\"dark\",\"volume\":7}";
        fs::write(vault.join("src/settings.json"), user_json).unwrap();
        fs::write(
            vault.join("src/ghost.json"),
            serde_json::to_string(&PageMeta::new("ghost".into())).unwrap(),
        )
        .unwrap();
        fs::write(vault.join("src/other.html"), "<p>not a page</p>").unwrap();
        fs::write(
            vault.join("src/other.json"),
            serde_json::to_string(&PageMeta::new("mismatch".into())).unwrap(),
        )
        .unwrap();

        rename_folder(&vault, "src", "dest").unwrap();

        assert_eq!(
            fs::read_to_string(vault.join("dest/settings.json")).unwrap(),
            user_json
        );
        let ghost: PageMeta =
            serde_json::from_str(&fs::read_to_string(vault.join("dest/ghost.json")).unwrap())
                .unwrap();
        assert_eq!(ghost.id, "ghost");
        assert_eq!(ghost.folder, None, "id-only JSON left untouched");
        let mismatch: PageMeta =
            serde_json::from_str(&fs::read_to_string(vault.join("dest/other.json")).unwrap())
                .unwrap();
        assert_eq!(mismatch.id, "mismatch");
        assert_eq!(mismatch.folder, None, "id-mismatched JSON left untouched");
        assert_eq!(
            read_meta(&vault, "p1", Some("dest"))
                .unwrap()
                .unwrap()
                .folder
                .as_deref(),
            Some("dest"),
            "the real page sidecar is rewritten"
        );
        let _ = fs::remove_dir_all(&vault);
    }

    #[cfg(unix)]
    #[test]
    fn folder_rename_checks_containment_before_creating_destination_parents() {
        use std::os::unix::fs::symlink;
        let vault = temp_vault("rename-escape");
        let outside = temp_vault("rename-escape-outside");
        let mut meta = PageMeta::new("p1".into());
        meta.folder = Some("src".into());
        write_page(&vault, &meta, "<p>p1</p>").unwrap();
        symlink(&outside, vault.join("escape")).unwrap();

        let err = rename_folder(&vault, "src", "escape/new/child").unwrap_err();
        assert!(err.contains("escapes vault"), "{err}");
        assert!(
            !outside.join("new").exists(),
            "no destination parent may be created outside the vault"
        );
        assert!(vault.join("src/p1.html").exists(), "source untouched");
        let _ = fs::remove_dir_all(&outside);
        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn case_only_folder_rename_succeeds_and_rewrites_sidecars() {
        let vault = temp_vault("case-rename");
        let mut meta = PageMeta::new("p1".into());
        meta.folder = Some("Notes".into());
        write_page(&vault, &meta, "<p>case</p>").unwrap();

        let renamed = rename_folder(&vault, "Notes", "notes").unwrap();
        assert_eq!(renamed, "notes");
        assert!(vault.join("notes/p1.html").exists());
        assert_eq!(
            read_meta(&vault, "p1", Some("notes"))
                .unwrap()
                .unwrap()
                .folder
                .as_deref(),
            Some("notes")
        );
        let _ = fs::remove_dir_all(&vault);
    }

    #[cfg(unix)]
    #[test]
    fn folder_rename_rejects_symlink_alias_without_modifying_real_folder() {
        use std::os::unix::fs::symlink;
        let vault = temp_vault("same-dir-rename");
        let mut meta = PageMeta::new("p1".into());
        meta.folder = Some("Notes".into());
        write_page(&vault, &meta, "<p>case</p>").unwrap();
        symlink(vault.join("Notes"), vault.join("alias")).unwrap();

        assert!(
            rename_folder(&vault, "Notes", "alias")
                .unwrap_err()
                .contains("symlink")
        );
        assert!(
            rename_folder(&vault, "alias", "Target")
                .unwrap_err()
                .contains("symlink")
        );
        assert!(vault.join("Notes/p1.html").exists());
        assert!(vault.join("alias").is_symlink());
        assert_eq!(
            read_meta(&vault, "p1", Some("Notes"))
                .unwrap()
                .unwrap()
                .folder
                .as_deref(),
            Some("Notes")
        );
        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn rescan_fingerprint_is_stable_between_unchanged_indexes() {
        let vault = temp_vault("fp-stable");
        let store = Store::open(vault.clone()).unwrap();
        let meta = PageMeta::new("stable".into());
        write_page(
            &vault,
            &meta,
            "<!DOCTYPE html><html><head><title>Stable</title></head><body><p>s</p></body></html>",
        )
        .unwrap();

        let first = index_vault(&store).unwrap();
        assert_eq!(first.indexed, 1);
        let second = index_vault(&store).unwrap();
        assert_eq!(
            second.indexed, 0,
            "an unchanged page takes the fingerprint fast path"
        );
        assert_eq!(second.removed, 0);

        // A sidecar-only edit is still detected (A1).
        let mut edited = store.get_meta("stable").unwrap().unwrap();
        edited.title = "Renamed by sidecar".into();
        write_meta(&vault, &edited).unwrap();
        let third = index_vault(&store).unwrap();
        assert_eq!(third.indexed, 1, "a sidecar-only edit re-reads the page");
        assert_eq!(
            store.get_meta("stable").unwrap().unwrap().title,
            "Renamed by sidecar"
        );
        let _ = fs::remove_dir_all(&vault);
    }
}
