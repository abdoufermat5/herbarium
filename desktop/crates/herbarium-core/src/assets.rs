// Page assets: the files next to a page that it loads by relative URL
// (images, scripts, styles, media). Lookup never leaves the page's folder,
// never follows a symlink and never serves a sidecar, the same rules the
// reader's protocol applies, so exports and health checks see exactly what
// the reader can load.

use std::path::{Path, PathBuf};

use percent_encoding::percent_decode_str;

use crate::rewrite::{is_asset_attr, relative_path, rewrite_attrs};
use crate::vault;

/// Largest single asset inlined; bigger ones keep their relative URL.
const MAX_ASSET: u64 = 25 * 1024 * 1024;
/// Most bytes inlined into one page, before encoding.
const MAX_INLINED: u64 = 100 * 1024 * 1024;

pub fn mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("avif") => "image/avif",
        Some("bmp") => "image/bmp",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("ogg" | "oga") => "audio/ogg",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("ogv") => "video/ogg",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("txt" | "text") => "text/plain; charset=utf-8",
        Some("csv") => "text/csv; charset=utf-8",
        Some("xml") => "application/xml",
        Some("pdf") => "application/pdf",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}

/// Resolve `rel_path` under `page_folder`, which itself must canonically live
/// inside `vault` (an indexed folder may have been swapped for a symlink).
pub fn resolve_safe_asset_path(
    vault: &Path,
    page_folder: &Path,
    rel_path: &str,
) -> Option<PathBuf> {
    let canon_vault = vault.canonicalize().ok()?;
    let canon_folder = page_folder.canonicalize().ok()?;
    if !canon_folder.is_dir() || !canon_folder.starts_with(&canon_vault) {
        return None;
    }

    let mut current = canon_folder.clone();
    for seg in rel_path.split('/') {
        if seg.is_empty() || seg.starts_with('.') || seg == ".." {
            return None;
        }
        current.push(seg);
        let meta = std::fs::symlink_metadata(&current).ok()?;
        if meta.file_type().is_symlink() {
            return None; // Reject symlinks
        }
    }

    if !current.is_file() {
        return None;
    }

    let canon_file = current.canonicalize().ok()?;
    if !canon_file.starts_with(&canon_folder) {
        return None;
    }

    if canon_file
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
    {
        return None;
    }

    Some(current)
}

/// Standard base64 (RFC 4648, padded).
pub fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// The local asset `url` refers to from a page in `page_folder`, when it is a
/// relative URL to a file the reader would serve.
pub fn local_asset(vault_dir: &Path, page_folder: &Path, url: &str) -> Option<PathBuf> {
    let rel = relative_path(url)?;
    let rel = percent_decode_str(rel).decode_utf8().ok()?;
    resolve_safe_asset_path(vault_dir, page_folder, &rel)
}

/// The folder holding page `id`'s files.
pub fn page_folder(vault_dir: &Path, id: &str, folder: Option<&str>) -> PathBuf {
    let html_file = vault::html_path(vault_dir, id, folder);
    html_file.parent().unwrap_or(vault_dir).to_path_buf()
}

/// `html` with its local assets inlined as `data:` URIs, so the document
/// works on its own. Assets too large, missing or outside the page's folder
/// keep their URL.
pub fn inline_local_assets(vault_dir: &Path, id: &str, folder: Option<&str>, html: &str) -> String {
    let page_folder = page_folder(vault_dir, id, folder);
    let mut budget = MAX_INLINED;
    rewrite_attrs(html, |tag, attr, value| {
        if !is_asset_attr(tag, attr) {
            return None;
        }
        let file = local_asset(vault_dir, &page_folder, value)?;
        let size = std::fs::metadata(&file).ok()?.len();
        if size > MAX_ASSET || size > budget {
            return None;
        }
        let bytes = std::fs::read(&file).ok()?;
        budget -= size;
        // `text/css; charset=utf-8` → `text/css;charset=utf-8`: no spaces in a data URI.
        let mime = mime_for_path(&file).replace(' ', "");
        Some(format!("data:{mime};base64,{}", base64(&bytes)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_vectors() {
        for (input, want) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), want);
        }
    }
}
