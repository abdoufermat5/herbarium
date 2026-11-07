// Page assets: the files next to a page that it loads by relative URL
// (images, scripts, styles, media). Lookup never leaves the page's folder,
// never follows a symlink and never serves a sidecar, the same rules the
// reader's protocol applies, so exports and health checks see exactly what
// the reader can load.

use std::path::{Path, PathBuf};

use percent_encoding::percent_decode_str;

use crate::reserved::{self, Reserved};
use crate::rewrite::{is_asset_attr, relative_path, rewrite_attrs};
use crate::{deps, vault};

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

/// A `data:` URI for `bytes`. In a module context the MIME type loses its
/// parameters (`text/javascript`), the form `import` of a data URL expects.
fn data_uri(mime: &str, bytes: &[u8], module: bool) -> String {
    // `text/css; charset=utf-8` → `text/css;charset=utf-8`: no spaces in a data URI.
    let mime = if module {
        mime.split(';').next().unwrap_or(mime).trim().to_string()
    } else {
        mime.replace(' ', "")
    };
    format!("data:{mime};base64,{}", base64(bytes))
}

/// What an export puts in place of a reserved `_herbarium/` path: bundled
/// libraries and stored dependencies as `data:` URIs, except a dependency
/// with rewritten imports (a `data:` module cannot resolve relative imports),
/// which goes back to its original URL. A diagram that was never rendered
/// becomes a placeholder. `None` keeps the reference as it is.
fn reserved_replacement(
    vault_dir: &Path,
    r: &Reserved,
    module: bool,
    budget: &mut u64,
) -> Option<String> {
    if module && matches!(r, Reserved::Render { .. }) {
        return None;
    }
    if let Reserved::Dep { key, file } = r {
        let meta = deps::load(vault_dir, key)?;
        if meta.file != *file {
            return None;
        }
        if meta.has_imports {
            return Some(meta.url);
        }
    }
    match reserved::resolve(vault_dir, r) {
        Some((mime, bytes)) => {
            let size = bytes.len() as u64;
            if size > MAX_ASSET || size > *budget {
                return None;
            }
            *budget -= size;
            Some(data_uri(&mime, &bytes, module))
        }
        None if matches!(r, Reserved::Render { .. }) => {
            let svg = reserved::placeholder_svg("Diagram not rendered yet");
            Some(data_uri("image/svg+xml", svg.as_bytes(), false))
        }
        None => None,
    }
}

/// The reserved path `url` names, when it is a page-relative `_herbarium/` path.
fn reserved_ref(url: &str) -> Option<Option<Reserved>> {
    let rel = relative_path(url)?;
    if !rel.starts_with(reserved::PREFIX) {
        return None;
    }
    // `Some(None)`: inside the reserved namespace but not a valid path, which
    // must not fall back to a real file next to the page.
    Some(
        percent_decode_str(rel)
            .decode_utf8()
            .ok()
            .and_then(|decoded| reserved::parse(&decoded)),
    )
}

/// `html` with its local assets inlined as `data:` URIs, so the document
/// works on its own. Assets too large, missing or outside the page's folder
/// keep their URL. Reserved `_herbarium/` references are left untouched.
pub fn inline_local_assets(vault_dir: &Path, id: &str, folder: Option<&str>, html: &str) -> String {
    let page_folder = page_folder(vault_dir, id, folder);
    let mut budget = MAX_INLINED;
    rewrite_attrs(html, |tag, attr, value| {
        if !is_asset_attr(tag, attr) {
            return None;
        }
        if reserved_ref(value).is_some() {
            return None;
        }
        let file = local_asset(vault_dir, &page_folder, value)?;
        let size = std::fs::metadata(&file).ok()?.len();
        if size > MAX_ASSET || size > budget {
            return None;
        }
        let bytes = std::fs::read(&file).ok()?;
        budget -= size;
        Some(data_uri(mime_for_path(&file), &bytes, false))
    })
}

/// `html` prepared for export: local page assets are inlined as `data:` URIs,
/// and reserved `_herbarium/` paths (bundled libraries, vendored dependencies,
/// diagram renders) are inlined too, in attributes and in the specifiers of
/// inline module scripts and import maps.
pub fn inline_for_export(vault_dir: &Path, id: &str, folder: Option<&str>, html: &str) -> String {
    let page_folder = page_folder(vault_dir, id, folder);
    let mut budget = MAX_INLINED;
    let inlined = rewrite_attrs(html, |tag, attr, value| {
        if !is_asset_attr(tag, attr) {
            return None;
        }
        if let Some(parsed) = reserved_ref(value) {
            return reserved_replacement(vault_dir, &parsed?, false, &mut budget);
        }
        let file = local_asset(vault_dir, &page_folder, value)?;
        let size = std::fs::metadata(&file).ok()?.len();
        if size > MAX_ASSET || size > budget {
            return None;
        }
        let bytes = std::fs::read(&file).ok()?;
        budget -= size;
        Some(data_uri(mime_for_path(&file), &bytes, false))
    });
    deps::rewrite_inline_scripts(&inlined, |_, spec| {
        let parsed = reserved_ref(spec)??;
        reserved_replacement(vault_dir, &parsed, true, &mut budget)
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

    fn temp_vault(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "herbarium-assets-{tag}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn put_dep(
        vault: &Path,
        url: &str,
        file: &str,
        mime: &str,
        bytes: &[u8],
        has_imports: bool,
    ) -> String {
        let meta = deps::DepMeta {
            url: url.into(),
            file: file.into(),
            mime: mime.into(),
            bytes: bytes.len() as u64,
            module: file.ends_with(".mjs"),
            has_imports,
            fetched_at: 1,
        };
        deps::store(vault, &meta, bytes).unwrap()
    }

    #[test]
    fn export_inlines_every_kind_of_reserved_path() {
        let vault = temp_vault("reserved");
        let css = put_dep(
            &vault,
            "https://cdn.x/a.css",
            "a.css",
            "text/css",
            b"a{}",
            false,
        );
        let plain = put_dep(
            &vault,
            "https://cdn.x/c.mjs",
            "c.mjs",
            "text/javascript",
            b"export default 1",
            false,
        );
        let linked = put_dep(
            &vault,
            "https://cdn.x/m.mjs",
            "m.mjs",
            "text/javascript",
            b"import '../x/y.js'",
            true,
        );
        let cached = reserved::render_src("d2", "a -> b").unwrap();
        let missing = reserved::render_src("d2", "x -> y").unwrap();
        crate::vault::write_atomic(
            &reserved::render_cache_path(&vault, "d2", "a -> b"),
            b"<svg id=\"cached\"/>",
        )
        .unwrap();
        std::fs::create_dir_all(vault.join("_herbarium")).unwrap();
        std::fs::write(vault.join("_herbarium/real.txt"), b"shadowed").unwrap();

        let html = format!(
            r#"<link rel="stylesheet" href="{css}"><script src="_herbarium/lib/mermaid.js"></script>
<script type="module" src="{linked}"></script><img src="{cached}"><img src="{missing}">
<script type="module">import c from "./{plain}"; import b from "./{linked}"; import "./_herbarium/lib/mermaid.mjs"; import n from "./_herbarium/lib/nope.mjs";</script>
<script type="importmap">{{"imports":{{"c":"./{plain}","b":"./{linked}"}}}}</script>
<img src="_herbarium/lib/nope.js"><img src="_herbarium/render/zzz/abc.svg"><img src="_herbarium/real.txt">"#
        );
        let out = inline_for_export(&vault, "p", None, &html);

        // Library: data URI.
        assert!(!out.contains(r#"src="_herbarium/lib/mermaid.js""#));
        assert!(out.contains(r#"<script src="data:text/javascript"#));
        // Dependency without imports: data URI; with imports: its original URL.
        assert!(out.contains(&format!(
            r#"href="data:text/css;base64,{}""#,
            base64(b"a{}")
        )));
        assert!(out.contains(r#"<script type="module" src="https://cdn.x/m.mjs">"#));
        // Render: cached SVG, or the placeholder.
        assert!(out.contains(&format!(
            r#"<img src="data:image/svg+xml;base64,{}">"#,
            base64(b"<svg id=\"cached\"/>")
        )));
        let placeholder = reserved::placeholder_svg("Diagram not rendered yet");
        assert!(out.contains(&format!(
            r#"<img src="data:image/svg+xml;base64,{}">"#,
            base64(placeholder.as_bytes())
        )));
        // Inline module specifiers and import-map values.
        let plain_uri = format!(
            "data:text/javascript;base64,{}",
            base64(b"export default 1")
        );
        assert!(out.contains(&format!(r#"import c from "{plain_uri}";"#)));
        assert!(out.contains(r#"import b from "https://cdn.x/m.mjs";"#));
        assert!(out.contains(r#"import "data:text/javascript;base64,"#));
        assert!(out.contains(r#"import n from "./_herbarium/lib/nope.mjs";"#));
        assert!(out.contains(&format!(r#""c":"{plain_uri}","b":"https://cdn.x/m.mjs""#)));
        // Unknown or invalid reserved paths stay, and never fall back to a real
        // file of the same name next to the page.
        assert!(out.contains(r#"<img src="_herbarium/lib/nope.js">"#));
        assert!(out.contains(r#"<img src="_herbarium/render/zzz/abc.svg">"#));
        assert!(out.contains(r#"<img src="_herbarium/real.txt">"#));
        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn export_keeps_reserved_paths_that_exceed_the_budget() {
        let vault = temp_vault("budget");
        let big = vec![b'x'; (MAX_ASSET + 1) as usize];
        let rel = put_dep(
            &vault,
            "https://cdn.x/big.js",
            "big.js",
            "text/javascript",
            &big,
            false,
        );
        let html = format!(r#"<script src="{rel}"></script>"#);
        assert_eq!(inline_for_export(&vault, "p", None, &html), html);
        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn inline_local_assets_leaves_reserved_paths_untouched() {
        let vault = temp_vault("untouched");
        std::fs::write(vault.join("dot.png"), b"PNG").unwrap();
        let render = reserved::render_src("d2", "a -> b").unwrap();
        let html = format!(
            r#"<script src="_herbarium/lib/mermaid.js"></script><img src="{render}"><img src="dot.png">"#
        );
        let out = inline_local_assets(&vault, "p", None, &html);
        assert!(out.contains(r#"src="_herbarium/lib/mermaid.js""#));
        assert!(out.contains(&format!(r#"src="{render}""#)));
        assert!(out.contains(r#"src="data:image/png;base64,UE5H""#));
        let _ = std::fs::remove_dir_all(&vault);
    }
}
