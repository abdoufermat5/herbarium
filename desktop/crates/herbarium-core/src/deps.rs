// Vendored dependencies: what a page loads from the internet, stored in the
// vault under `.herbarium/deps/<key>/` and served as `_herbarium/deps/<key>/<file>`.
// This module holds the offline half: the on-disk layout, finding the remote
// references of a page, and rewriting URLs in HTML, ES modules and CSS. The
// desktop crate does the downloading.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};

use crate::rewrite::is_asset_attr;
use crate::vault;

/// The metadata file next to a stored dependency.
pub(crate) const META_FILE: &str = "dep.json";
/// Largest stored file read back (the vendorer stores at most 25 MiB).
const MAX_READ: u64 = 64 * 1024 * 1024;

/// `.herbarium/deps/<key>/dep.json`: what a stored file is and where it came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepMeta {
    /// The absolute https URL it was fetched from; hashes to the folder's key.
    pub url: String,
    /// The file name inside the folder; the only file served.
    pub file: String,
    pub mime: String,
    pub bytes: u64,
    /// Loaded as an ES module (imported, or by `<script type=module>`).
    pub module: bool,
    /// Its import specifiers were rewritten to relative dependency paths.
    pub has_imports: bool,
    pub fetched_at: i64,
}

/// FNV-1a 64 of `url`, as 16 lowercase hex digits. Stable across Rust versions
/// and needs no hashing crate; `dep.json` keeps the URL to detect collisions.
pub fn key_for(url: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in url.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Whether `s` has the shape of [`key_for`]'s output.
pub(crate) fn is_key(s: &str) -> bool {
    s.len() == 16 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Extensions that suit `mime`, the default first; `None` for unknown types.
fn mime_exts(mime: &str) -> Option<&'static [&'static str]> {
    let base = mime
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let exts: &'static [&'static str] = match base.as_str() {
        "text/javascript"
        | "application/javascript"
        | "application/x-javascript"
        | "text/ecmascript" => &["js", "mjs", "cjs"],
        "text/css" => &["css"],
        "application/json" => &["json"],
        "image/svg+xml" => &["svg"],
        "image/png" => &["png"],
        "image/jpeg" => &["jpg", "jpeg"],
        "image/gif" => &["gif"],
        "image/webp" => &["webp"],
        "image/avif" => &["avif"],
        "image/x-icon" | "image/vnd.microsoft.icon" => &["ico"],
        "font/woff2" => &["woff2"],
        "font/woff" => &["woff"],
        "font/ttf" => &["ttf"],
        "font/otf" => &["otf"],
        "application/wasm" => &["wasm"],
        "text/html" => &["html", "htm"],
        "text/plain" => &["txt"],
        _ => return None,
    };
    Some(exts)
}

/// A file name for the copy of `url`: its last path segment made safe, with an
/// extension that matches `mime` (browsers need one for modules and styles),
/// at most 80 characters.
pub fn file_name_for(url: &str, mime: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let path = rest.find('/').map_or("", |i| &rest[i..]);
    let path = path.split(['?', '#']).next().unwrap_or("");
    let last = path.rsplit('/').find(|s| !s.is_empty()).unwrap_or("");
    let decoded = percent_decode_str(last).decode_utf8_lossy();
    let mut name: String = decoded
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    name = name.trim_start_matches('.').to_string();
    if name.is_empty() {
        name = "index".to_string();
    }
    let ext_of = |n: &str| n.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match mime_exts(mime) {
        Some(exts) => {
            if !ext_of(&name).is_some_and(|e| exts.contains(&e.as_str())) {
                name.push('.');
                name.push_str(exts[0]);
            }
        }
        None => {
            if ext_of(&name).is_none() {
                name.push_str(".bin");
            }
        }
    }
    if name.len() > 80 {
        let (stem, ext) = match name.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() && e.len() <= 12 => (s.to_string(), format!(".{e}")),
            _ => (name.clone(), String::new()),
        };
        let mut stem = stem;
        stem.truncate(80 - ext.len());
        name = format!("{stem}{ext}");
    }
    if name == META_FILE {
        name = format!("_{name}");
    }
    if name.to_ascii_lowercase().ends_with(".json") {
        name.push_str(".data");
    }
    name
}

/// `_herbarium/deps/<key>/<file>`
pub fn rel_path(key: &str, file: &str) -> String {
    format!("{}deps/{key}/{file}", crate::reserved::PREFIX)
}

/// Read `base/<segments…>` when no segment is a symlink and the last is a
/// regular file of sane size. Stored data is never reached through a link.
pub(crate) fn read_plain(base: &Path, segments: &[&str]) -> Option<Vec<u8>> {
    let mut path = base.to_path_buf();
    for (n, seg) in segments.iter().enumerate() {
        path.push(seg);
        let meta = fs::symlink_metadata(&path).ok()?;
        if meta.file_type().is_symlink() {
            return None;
        }
        if n + 1 == segments.len() {
            if !meta.is_file() || meta.len() > MAX_READ {
                return None;
            }
        } else if !meta.is_dir() {
            return None;
        }
    }
    fs::read(&path).ok()
}

/// Write `bytes` and then `dep.json` for the dependency described by `meta`;
/// returns the page-relative path to reference it by.
pub fn store(vault: &Path, meta: &DepMeta, bytes: &[u8]) -> Result<String, String> {
    let key = key_for(&meta.url);
    let file = meta.file.as_str();
    if file.is_empty()
        || file == "."
        || file == ".."
        || file == META_FILE
        || file.contains(['/', '\\', '\0'])
    {
        return Err(format!("invalid dependency file name `{file}`"));
    }
    let dir = vault.join(".herbarium").join("deps").join(&key);
    // The data first: a crash between the two writes leaves no dep.json, so
    // the half-stored file is never served.
    vault::write_atomic(&dir.join(file), bytes)?;
    let json = serde_json::to_vec_pretty(meta).map_err(|e| e.to_string())?;
    vault::write_atomic(&dir.join(META_FILE), &json)?;
    Ok(rel_path(&key, file))
}

/// The stored metadata for `key`, when it is well-formed and its URL hashes
/// back to `key`.
pub fn load(vault: &Path, key: &str) -> Option<DepMeta> {
    if !is_key(key) {
        return None;
    }
    let raw = read_plain(vault, &[".herbarium", "deps", key, META_FILE])?;
    let meta: DepMeta = serde_json::from_slice(&raw).ok()?;
    (key_for(&meta.url) == key).then_some(meta)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefKind {
    /// An asset attribute (`<link href>`, `<img src>`, a classic
    /// `<script src>` …); `tag` and `attr` are lowercase.
    Asset { tag: String, attr: String },
    /// Loaded as an ES module: `<script type=module src>`, or a specifier of
    /// an inline `<script type=module>`.
    ModuleImport,
    /// A value of an inline `<script type=importmap>` (`imports`, `scopes`).
    ImportMap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRef {
    /// The absolute https URL (a protocol-relative reference is upgraded).
    pub url: String,
    /// The reference exactly as the page spells it (with `&amp;` decoded in
    /// attributes); the key [`rewrite_page`] looks up.
    pub raw: String,
    pub kind: RefKind,
}

/// `s` as an absolute https URL: `https://…` as is, `//host/…` upgraded.
/// `http://` and everything else is not a remote reference.
fn to_https(s: &str) -> Option<String> {
    if has_https_prefix(s) {
        Some(s.to_string())
    } else if s.starts_with("//") && s.len() > 2 && !s[2..].starts_with('/') {
        Some(format!("https:{s}"))
    } else {
        None
    }
}

fn has_https_prefix(s: &str) -> bool {
    s.len() > 8 && s.as_bytes()[..8].eq_ignore_ascii_case(b"https://")
}

/// Everything a page loads from the internet over https: asset attributes,
/// inline module specifiers and import-map values, in document order within
/// each group.
pub fn page_refs(html: &str) -> Vec<RemoteRef> {
    let mut refs = Vec::new();
    walk(html, &mut |kind, raw| {
        if let Some(url) = to_https(raw.trim()) {
            refs.push(RemoteRef {
                url,
                raw: raw.to_string(),
                kind: kind.clone(),
            });
        }
        None
    });
    refs
}

/// `html` with each reference whose raw form is a key of `map` replaced by the
/// mapped value, in the places [`page_refs`] looks at and nowhere else.
pub fn rewrite_page(html: &str, map: &HashMap<String, String>) -> String {
    if map.is_empty() {
        return html.to_string();
    }
    walk(html, &mut |_, raw| map.get(raw).cloned())
}

/// Rewrite the specifiers of inline module scripts and the values of import
/// maps (every one, not only remote ones; the callback filters).
pub(crate) fn rewrite_inline_scripts(
    html: &str,
    mut f: impl FnMut(&RefKind, &str) -> Option<String>,
) -> String {
    walk_scripts(html, false, &mut f)
}

fn is_resource_link(attrs: &[Attr]) -> bool {
    attrs.iter().filter(|a| a.name == "rel").any(|rel| {
        rel.value.split_ascii_whitespace().any(|tok| {
            matches!(
                tok.to_ascii_lowercase().as_str(),
                "stylesheet"
                    | "icon"
                    | "apple-touch-icon"
                    | "modulepreload"
                    | "preload"
                    | "manifest"
            )
        })
    })
}

fn remove_attr_edit(attrs: &[Attr], name: &str) -> Option<(usize, usize, String)> {
    let pos = attrs.iter().position(|a| a.name == name)?;
    let attr = &attrs[pos];
    let (start, end) = if pos > 0 {
        (attr.span_start, attr.span_end)
    } else if attrs.len() > 1 {
        let next = &attrs[1];
        (attr.name_start, next.name_start)
    } else {
        (attr.span_start, attr.span_end)
    };
    Some((start, end, String::new()))
}

fn walk_tag_attrs(html: &str, f: &mut dyn FnMut(&RefKind, &str) -> Option<String>) -> String {
    let bytes = html.as_bytes();
    let len = bytes.len();
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut i = 0;
    while i < len {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if html[i..].starts_with("<!--") {
            i = html[i + 4..].find("-->").map_or(len, |e| i + 4 + e + 3);
            continue;
        }
        let Some(first) = bytes.get(i + 1) else { break };
        if !first.is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < len && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-') {
            j += 1;
        }
        let tag = html[i + 1..j].to_ascii_lowercase();
        // Skip script and style tags — scripts are handled in walk_scripts,
        // and style bodies have no asset attributes.
        if tag == "script" || tag == "style" {
            let close = format!("</{tag}");
            i = find_ci(&html[j..], &close).map_or(len, |e| j + e);
            continue;
        }
        let (attrs, open_end) = parse_attrs(html, j);
        let mut tag_edits = Vec::new();
        let mut replaced_src_or_href = false;
        for attr in &attrs {
            let is_dep = if tag == "link" && attr.name == "href" {
                is_resource_link(&attrs)
            } else if tag == "script" {
                false
            } else {
                is_asset_attr(&tag, &attr.name)
            };
            if !is_dep {
                continue;
            }
            let kind = RefKind::Asset {
                tag: tag.clone(),
                attr: attr.name.clone(),
            };
            if let Some(new) = f(&kind, &attr.value) {
                if attr.name == "src" || attr.name == "href" {
                    replaced_src_or_href = true;
                }
                let new = escape_attr(&new);
                let text = if attr.quoted {
                    new
                } else {
                    format!("\"{new}\"")
                };
                tag_edits.push((attr.start, attr.end, text));
            }
        }
        if replaced_src_or_href && let Some(integ_edit) = remove_attr_edit(&attrs, "integrity") {
            tag_edits.push(integ_edit);
        }
        edits.extend(tag_edits);
        i = open_end;
    }
    edits.sort_by_key(|e| e.0);
    splice(html, edits)
}

/// Visit every place a page names a dependency. `f(kind, raw)` may return a
/// replacement. Asset attributes first, then `<script>` elements.
fn walk(html: &str, f: &mut dyn FnMut(&RefKind, &str) -> Option<String>) -> String {
    let attrs = walk_tag_attrs(html, f);
    walk_scripts(&attrs, true, f)
}

struct Attr {
    name: String,
    /// With `&amp;` decoded.
    value: String,
    /// Byte range of the value's contents, delimiting quotes excluded.
    start: usize,
    end: usize,
    quoted: bool,
    /// Start of attribute name.
    name_start: usize,
    /// Start of attribute span, including leading whitespace if any.
    span_start: usize,
    /// End of attribute span.
    span_end: usize,
}

/// Attributes of a start tag whose name ends at `j`; returns them and the
/// index just past the closing `>`.
fn parse_attrs(html: &str, mut j: usize) -> (Vec<Attr>, usize) {
    let bytes = html.as_bytes();
    let len = bytes.len();
    let mut attrs = Vec::new();
    loop {
        let ws_start = j;
        while j < len && (bytes[j].is_ascii_whitespace() || bytes[j] == b'/') {
            j += 1;
        }
        if j >= len {
            return (attrs, len);
        }
        if bytes[j] == b'>' {
            return (attrs, j + 1);
        }
        let name_start = j;
        while j < len && !bytes[j].is_ascii_whitespace() && !matches!(bytes[j], b'=' | b'>' | b'/')
        {
            j += 1;
        }
        let name = html[name_start..j].to_ascii_lowercase();
        let after_name = j;
        let mut eq_pos = j;
        while eq_pos < len && bytes[eq_pos].is_ascii_whitespace() {
            eq_pos += 1;
        }
        if bytes.get(eq_pos) != Some(&b'=') {
            attrs.push(Attr {
                name,
                value: String::new(),
                start: name_start,
                end: after_name,
                quoted: false,
                name_start,
                span_start: ws_start,
                span_end: after_name,
            });
            j = after_name;
            continue;
        }
        j = eq_pos + 1;
        while j < len && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        match bytes.get(j) {
            Some(q @ (b'"' | b'\'')) => {
                let start = j + 1;
                let end = html[start..].find(*q as char).map_or(len, |e| start + e);
                let span_end = (end + 1).min(len);
                attrs.push(Attr {
                    name,
                    value: html[start..end].replace("&amp;", "&"),
                    start,
                    end,
                    quoted: true,
                    name_start,
                    span_start: ws_start,
                    span_end,
                });
                j = span_end;
            }
            _ => {
                let start = j;
                while j < len && !bytes[j].is_ascii_whitespace() && bytes[j] != b'>' {
                    j += 1;
                }
                let span_end = j;
                attrs.push(Attr {
                    name,
                    value: html[start..j].replace("&amp;", "&"),
                    start,
                    end: j,
                    quoted: false,
                    name_start,
                    span_start: ws_start,
                    span_end,
                });
            }
        }
    }
}

fn starts_with_ci(bytes: &[u8], i: usize, needle: &str) -> bool {
    bytes
        .get(i..i + needle.len())
        .is_some_and(|s| s.eq_ignore_ascii_case(needle.as_bytes()))
}

fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    let h = hay.as_bytes();
    let n = needle.as_bytes();
    (0..=h.len().checked_sub(n.len())?).find(|&k| h[k..k + n.len()].eq_ignore_ascii_case(n))
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn splice(src: &str, edits: Vec<(usize, usize, String)>) -> String {
    if edits.is_empty() {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len());
    let mut at = 0;
    for (start, end, text) in edits {
        out.push_str(&src[at..start]);
        out.push_str(&text);
        at = end;
    }
    out.push_str(&src[at..]);
    out
}

/// The `<script>` elements of `html`: `src` (if `include_src`), module bodies
/// and import maps. `rewrite_attrs` skips script bodies, so this scans the
/// open tags itself, case-insensitively, skipping comments and `<style>`.
fn walk_scripts(
    html: &str,
    include_src: bool,
    f: &mut dyn FnMut(&RefKind, &str) -> Option<String>,
) -> String {
    let bytes = html.as_bytes();
    let len = bytes.len();
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut i = 0;
    while i < len {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if html[i..].starts_with("<!--") {
            i = html[i + 4..].find("-->").map_or(len, |e| i + 4 + e + 3);
            continue;
        }
        let tag_ends = move |n: usize| matches!(bytes.get(i + n), Some(b) if b.is_ascii_whitespace() || *b == b'>' || *b == b'/');
        if starts_with_ci(bytes, i, "<style") && tag_ends(6) {
            i = find_ci(&html[i..], "</style").map_or(len, |e| i + e);
            continue;
        }
        if !(starts_with_ci(bytes, i, "<script") && tag_ends(7)) {
            i += 1;
            continue;
        }
        let (attrs, open_end) = parse_attrs(html, i + 7);
        let body_end = find_ci(&html[open_end..], "</script").map_or(len, |e| open_end + e);
        let attr = |name: &str| attrs.iter().find(|a| a.name == name);
        let ty = attr("type")
            .map(|a| a.value.trim().to_ascii_lowercase())
            .unwrap_or_default();
        if let Some(src) = attr("src") {
            if include_src {
                let kind = if ty == "module" {
                    RefKind::ModuleImport
                } else {
                    RefKind::Asset {
                        tag: "script".to_string(),
                        attr: "src".to_string(),
                    }
                };
                if let Some(new) = f(&kind, &src.value) {
                    let new = escape_attr(&new);
                    let text = if src.quoted {
                        new
                    } else {
                        format!("\"{new}\"")
                    };
                    edits.push((src.start, src.end, text));
                    if let Some(integ_edit) = remove_attr_edit(&attrs, "integrity") {
                        edits.push(integ_edit);
                    }
                }
            }
        } else if ty == "module" || ty == "importmap" {
            let body = &html[open_end..body_end];
            let rewritten = if ty == "module" {
                rewrite_specifiers(body, |s| f(&RefKind::ModuleImport, s))
            } else {
                rewrite_importmap(body, |s| f(&RefKind::ImportMap, s))
            };
            if rewritten != body {
                edits.push((open_end, body_end, rewritten));
            }
        }
        i = body_end;
    }
    edits.sort_by_key(|e| e.0);
    splice(html, edits)
}

// ---------------------------------------------------------------- import maps

/// The URL values of an import map: `imports` entries and the entries of each
/// `scopes` map. `start..end` is the string's raw contents (JSON escapes
/// intact), `value` the decoded string.
fn importmap_values(json: &str) -> Vec<Specifier> {
    let b = json.as_bytes();
    let mut out = Vec::new();
    // Keys of the objects we are inside, outermost first (`None` for the root
    // and for arrays).
    let mut stack: Vec<Option<String>> = Vec::new();
    let mut key: Option<String> = None;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' => {
                let start = i + 1;
                let mut j = start;
                while j < b.len() && b[j] != b'"' {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                let end = j.min(b.len());
                i = (end + 1).min(b.len());
                let Ok(value) = serde_json::from_str::<String>(&json[start - 1..i.min(b.len())])
                else {
                    continue;
                };
                let mut k = i;
                while k < b.len() && b[k].is_ascii_whitespace() {
                    k += 1;
                }
                if b.get(k) == Some(&b':') {
                    key = Some(value);
                } else {
                    let in_imports = stack.len() == 2 && stack[1].as_deref() == Some("imports");
                    let in_scope = stack.len() == 3 && stack[1].as_deref() == Some("scopes");
                    let entry_key = key.take();
                    if in_imports || in_scope {
                        let is_prefix = entry_key.as_deref().is_some_and(|k| k.ends_with('/'))
                            || value.ends_with('/');
                        if !is_prefix {
                            out.push(Specifier { start, end, value });
                        }
                    }
                }
            }
            b'{' | b'[' => {
                stack.push(key.take());
                i += 1;
            }
            b'}' | b']' => {
                stack.pop();
                key = None;
                i += 1;
            }
            b',' => {
                key = None;
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

fn rewrite_importmap(json: &str, mut f: impl FnMut(&str) -> Option<String>) -> String {
    let mut edits = Vec::new();
    for spec in importmap_values(json) {
        if let Some(new) = f(&spec.value)
            && let Ok(quoted) = serde_json::to_string(&new)
        {
            edits.push((
                spec.start,
                spec.end,
                quoted[1..quoted.len() - 1].to_string(),
            ));
        }
    }
    splice(json, edits)
}

// -------------------------------------------------------------- ES modules

/// A string's contents inside source text: the byte range between the quotes
/// and the text itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Specifier {
    pub start: usize,
    pub end: usize,
    pub value: String,
}

fn is_specifier(s: &str) -> bool {
    !s.contains('\\')
        && ["./", "../", "/", "http://", "https://"]
            .iter()
            .any(|p| s.starts_with(p))
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$' || b >= 0x80
}

fn is_ident(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit()
}

fn read_ident(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && is_ident(b[i]) {
        i += 1;
    }
    i
}

/// `i` is at an opening quote; returns the index after the closing quote (or
/// at a newline, which ends an unterminated string).
fn skip_string(b: &[u8], mut i: usize) -> usize {
    let q = b[i];
    i += 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            c if c == q => return i + 1,
            b'\n' => return i,
            _ => i += 1,
        }
    }
    b.len()
}

/// `i` is at a backtick; returns the index after the closing one. `${…}`
/// holes are skipped as code.
fn skip_template(b: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'`' => return i + 1,
            b'$' if b.get(i + 1) == Some(&b'{') => i = skip_hole(b, i + 2),
            _ => i += 1,
        }
    }
    b.len()
}

/// After `${`: to the index after the matching `}`.
fn skip_hole(b: &[u8], mut i: usize) -> usize {
    let mut depth = 1;
    while i < b.len() {
        match b[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            b'"' | b'\'' => {
                i = skip_string(b, i);
                continue;
            }
            b'`' => {
                i = skip_template(b, i);
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    b.len()
}

/// `i` is at the `/` that opens a regular expression literal.
fn skip_regex(b: &[u8], mut i: usize) -> usize {
    i += 1;
    let mut in_class = false;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'[' => {
                in_class = true;
                i += 1;
            }
            b']' => {
                in_class = false;
                i += 1;
            }
            b'/' if !in_class => return read_ident(b, i + 1),
            b'\n' => return i,
            _ => i += 1,
        }
    }
    b.len()
}

/// Whitespace and comments.
fn skip_ws_comments(b: &[u8], mut i: usize) -> usize {
    loop {
        match b.get(i) {
            Some(c) if c.is_ascii_whitespace() => i += 1,
            Some(b'/') if b.get(i + 1) == Some(&b'/') => {
                i = b[i..]
                    .iter()
                    .position(|&c| c == b'\n')
                    .map_or(b.len(), |e| i + e);
            }
            Some(b'/') if b.get(i + 1) == Some(&b'*') => {
                i = find_bytes(b, i + 2, b"*/").map_or(b.len(), |e| e + 2);
            }
            _ => return i,
        }
    }
}

fn find_bytes(b: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    b.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| from + p)
}

/// A quoted string at `i` without escapes: (start, end, next).
fn read_quoted(b: &[u8], i: usize) -> Option<(usize, usize, usize)> {
    let q = *b.get(i)?;
    if q != b'"' && q != b'\'' {
        return None;
    }
    let mut k = i + 1;
    while k < b.len() {
        match b[k] {
            c if c == q => return Some((i + 1, k, k + 1)),
            b'\\' | b'\n' => return None,
            _ => k += 1,
        }
    }
    None
}

fn push_spec(js: &str, start: usize, end: usize, out: &mut Vec<Specifier>) {
    let value = &js[start..end];
    if is_specifier(value) {
        out.push(Specifier {
            start,
            end,
            value: value.to_string(),
        });
    }
}

/// After `{`: the index after the matching `}`, skipping strings.
fn skip_braces(b: &[u8], mut i: usize) -> Option<usize> {
    let limit = (i + 4096).min(b.len());
    while i < limit {
        match b[i] {
            b'}' => return Some(i + 1),
            b'"' | b'\'' => i = skip_string(b, i),
            _ => i += 1,
        }
    }
    None
}

/// `from "…"` at `i`; returns the index after the string.
fn expect_from(js: &str, i: usize, out: &mut Vec<Specifier>) -> Option<usize> {
    let b = js.as_bytes();
    let i = skip_ws_comments(b, i);
    let end = read_ident(b, i);
    if &js[i..end] != "from" {
        return None;
    }
    let (s, e, next) = read_quoted(b, skip_ws_comments(b, end))?;
    push_spec(js, s, e, out);
    Some(next)
}

/// After the `import` keyword: `import("x")`, `import "x"` or
/// `import … from "x"`. Returns where scanning resumes when one was consumed.
fn scan_import(js: &str, after: usize, out: &mut Vec<Specifier>) -> Option<usize> {
    let b = js.as_bytes();
    let mut j = skip_ws_comments(b, after);
    match *b.get(j)? {
        b'(' => {
            j = skip_ws_comments(b, j + 1);
            let (s, e, next) = read_quoted(b, j)?;
            let k = skip_ws_comments(b, next);
            if matches!(b.get(k), Some(b')' | b',')) {
                push_spec(js, s, e, out);
                return Some(next);
            }
            None
        }
        b'"' | b'\'' => {
            let (s, e, next) = read_quoted(b, j)?;
            push_spec(js, s, e, out);
            Some(next)
        }
        _ => {
            // The import clause: names, `* as x`, `{ a as b }`, then `from`.
            let limit = (j + 4096).min(b.len());
            loop {
                j = skip_ws_comments(b, j);
                if j >= limit {
                    return None;
                }
                match b[j] {
                    b'{' => j = skip_braces(b, j + 1)?,
                    b'*' | b',' => j += 1,
                    c if is_ident_start(c) => {
                        let end = read_ident(b, j);
                        if &js[j..end] == "from" {
                            let (s, e, next) = read_quoted(b, skip_ws_comments(b, end))?;
                            push_spec(js, s, e, out);
                            return Some(next);
                        }
                        j = end;
                    }
                    _ => return None,
                }
            }
        }
    }
}

/// After the `export` keyword: `export * from "x"`, `export * as n from "x"`
/// and `export { … } from "x"`.
fn scan_export(js: &str, after: usize, out: &mut Vec<Specifier>) -> Option<usize> {
    let b = js.as_bytes();
    let j = skip_ws_comments(b, after);
    match *b.get(j)? {
        b'*' => {
            let mut k = skip_ws_comments(b, j + 1);
            let end = read_ident(b, k);
            if &js[k..end] == "as" {
                k = skip_ws_comments(b, end);
                k = read_ident(b, k);
            }
            expect_from(js, k, out)
        }
        b'{' => {
            let k = skip_braces(b, j + 1)?;
            expect_from(js, k, out)
        }
        _ => None,
    }
}

/// A `/` after one of these starts a regular expression, not a division.
fn regex_may_start(prev: u8, prev_word: &str) -> bool {
    match prev {
        0 => true,
        b'a' => matches!(
            prev_word,
            "return"
                | "typeof"
                | "case"
                | "in"
                | "of"
                | "delete"
                | "void"
                | "throw"
                | "new"
                | "else"
                | "do"
                | "instanceof"
                | "yield"
                | "await"
        ),
        c => b"(,=:[!&|?{};+-*%<>~^".contains(&c),
    }
}

/// The string literals of static `import … from`, `export … from`, side-effect
/// `import "x"` and `import("x")` in `js`, when they start with `./`, `../`,
/// `/`, `http://` or `https://` (`//host` is covered by `/`). Comments,
/// template literals, regular expressions and other strings are skipped, and
/// minified code (`import x from"…"`, `export*from'…'`) is understood.
pub fn module_specifiers(js: &str) -> Vec<Specifier> {
    let b = js.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    // The previous significant byte: `a` for an identifier, `0` at the start.
    let mut prev = 0u8;
    let mut prev_word = "";
    while i < b.len() {
        let c = b[i];
        match c {
            _ if c.is_ascii_whitespace() => i += 1,
            b'/' if b.get(i + 1) == Some(&b'/') => {
                i = b[i..]
                    .iter()
                    .position(|&x| x == b'\n')
                    .map_or(b.len(), |e| i + e);
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = find_bytes(b, i + 2, b"*/").map_or(b.len(), |e| e + 2);
            }
            b'"' | b'\'' => {
                i = skip_string(b, i);
                prev = b')';
            }
            b'`' => {
                i = skip_template(b, i);
                prev = b')';
            }
            b'/' => {
                if regex_may_start(prev, prev_word) {
                    i = skip_regex(b, i);
                    prev = b')';
                } else {
                    i += 1;
                    prev = b'/';
                }
                prev_word = "";
            }
            _ if is_ident_start(c) => {
                let end = read_ident(b, i);
                let word = &js[i..end];
                let resumed = if prev == b'.' {
                    None
                } else if word == "import" {
                    scan_import(js, end, &mut out)
                } else if word == "export" {
                    scan_export(js, end, &mut out)
                } else {
                    None
                };
                match resumed {
                    Some(next) => {
                        i = next;
                        prev = b')';
                        prev_word = "";
                    }
                    None => {
                        i = end;
                        prev = b'a';
                        prev_word = word;
                    }
                }
            }
            _ if c.is_ascii_digit() => {
                i = read_ident(b, i);
                prev = b')';
                prev_word = "";
            }
            _ => {
                prev = c;
                prev_word = "";
                i += 1;
            }
        }
    }
    out
}

/// `js` with every [`module_specifiers`] value for which `f` returns `Some`
/// replaced (the new text is escaped for the string's own quote).
pub fn rewrite_specifiers(js: &str, mut f: impl FnMut(&str) -> Option<String>) -> String {
    let mut edits = Vec::new();
    for spec in module_specifiers(js) {
        if let Some(new) = f(&spec.value) {
            let quote = js.as_bytes()[spec.start - 1] as char;
            let new = new
                .replace('\\', "\\\\")
                .replace(quote, &format!("\\{quote}"));
            edits.push((spec.start, spec.end, new));
        }
    }
    splice(js, edits)
}

// --------------------------------------------------------------------- CSS

/// A quoted CSS string at `i`: (start, end, next).
fn read_css_string(b: &[u8], i: usize) -> Option<(usize, usize, usize)> {
    let q = b[i];
    let mut k = i + 1;
    while k < b.len() {
        match b[k] {
            c if c == q => return Some((i + 1, k, k + 1)),
            b'\\' => k += 2,
            b'\n' => return None,
            _ => k += 1,
        }
    }
    None
}

fn skip_css_ws_comments(b: &[u8], mut i: usize) -> usize {
    loop {
        match b.get(i) {
            Some(c) if c.is_ascii_whitespace() => i += 1,
            Some(b'/') if b.get(i + 1) == Some(&b'*') => {
                i = find_bytes(b, i + 2, b"*/").map_or(b.len(), |e| e + 2);
            }
            _ => return i,
        }
    }
}

/// The URLs of a stylesheet: `url(…)` (quoted or not) and `@import "…"`.
/// `data:` URLs, `#fragment` references and comments are left out.
pub fn css_urls(css: &str) -> Vec<Specifier> {
    let b = css.as_bytes();
    let mut out = Vec::new();
    let mut push = |start: usize, end: usize| {
        let value = &css[start..end];
        let trimmed = value.trim();
        let lower = trimmed.as_bytes();
        let is_data = lower.len() >= 5 && lower[..5].eq_ignore_ascii_case(b"data:");
        if !trimmed.is_empty() && !trimmed.starts_with('#') && !is_data {
            out.push(Specifier {
                start,
                end,
                value: value.to_string(),
            });
        }
    };
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = find_bytes(b, i + 2, b"*/").map_or(b.len(), |e| e + 2);
            }
            b'"' | b'\'' => {
                i = read_css_string(b, i).map_or(i + 1, |(_, _, next)| next);
            }
            b'@' if starts_with_ci(b, i, "@import") => {
                let j = skip_css_ws_comments(b, i + 7);
                match b.get(j) {
                    Some(b'"' | b'\'') => match read_css_string(b, j) {
                        Some((s, e, next)) => {
                            push(s, e);
                            i = next;
                        }
                        None => i = j + 1,
                    },
                    _ => i = j.max(i + 1),
                }
            }
            b'u' | b'U'
                if starts_with_ci(b, i, "url(")
                    && (i == 0 || !is_ident(b[i - 1]) && b[i - 1] != b'-') =>
            {
                let j = skip_css_ws_comments(b, i + 4);
                match b.get(j) {
                    Some(b'"' | b'\'') => match read_css_string(b, j) {
                        Some((s, e, next)) => {
                            push(s, e);
                            i = next;
                        }
                        None => i = j + 1,
                    },
                    Some(_) => {
                        let Some(close) = b[j..].iter().position(|&c| c == b')') else {
                            break;
                        };
                        let mut end = j + close;
                        while end > j && b[end - 1].is_ascii_whitespace() {
                            end -= 1;
                        }
                        push(j, end);
                        i = j + close + 1;
                    }
                    None => break,
                }
            }
            _ => i += 1,
        }
    }
    out
}

/// `css` with every [`css_urls`] value for which `f` returns `Some` replaced.
/// An unquoted `url(…)` gets quotes when the new text needs them.
pub fn rewrite_css(css: &str, mut f: impl FnMut(&str) -> Option<String>) -> String {
    let mut edits = Vec::new();
    for spec in css_urls(css) {
        let Some(new) = f(&spec.value) else { continue };
        let before = css.as_bytes()[spec.start - 1];
        if before == b'"' || before == b'\'' {
            let quote = before as char;
            let new = new
                .replace('\\', "\\\\")
                .replace(quote, &format!("\\{quote}"))
                .replace('\n', "\\a ");
            edits.push((spec.start, spec.end, new));
        } else if new.contains(|c: char| c.is_whitespace() || "()'\"\\".contains(c)) {
            let new = new
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\a ");
            edits.push((spec.start, spec.end, format!("\"{new}\"")));
        } else {
            edits.push((spec.start, spec.end, new));
        }
    }
    splice(css, edits)
}

// -------------------------------------------------------------------- URLs

/// Collapse `.` and `..` segments of an absolute path (starting with `/`).
fn remove_dot_segments(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    let mut segs: Vec<&str> = Vec::new();
    for (n, seg) in parts.iter().enumerate().skip(1) {
        let last = n == parts.len() - 1;
        match *seg {
            "." => {
                if last {
                    segs.push("");
                }
            }
            ".." => {
                segs.pop();
                if last {
                    segs.push("");
                }
            }
            s => segs.push(s),
        }
    }
    format!("/{}", segs.join("/"))
}

/// `reference` as found in a resource fetched from `base`, made absolute.
/// Only https results are returned: other schemes (`http:`, `data:`, …) and a
/// non-https `base` give `None`. The fragment is dropped, the query kept.
pub fn resolve_url(base: &str, reference: &str) -> Option<String> {
    let r = reference.trim().split('#').next().unwrap_or("");
    if r.is_empty() {
        return None;
    }
    let first = r.split(['/', '?']).next().unwrap_or("");
    if first.contains(':') {
        return has_https_prefix(r).then(|| r.to_string());
    }
    if r.starts_with("//") {
        return to_https(r);
    }
    let base = base.trim().split('#').next().unwrap_or("");
    if !has_https_prefix(base) {
        return None;
    }
    let rest = &base[8..];
    let host_end = rest.find(['/', '?']).unwrap_or(rest.len());
    if host_end == 0 {
        return None;
    }
    let origin = format!("https://{}", &rest[..host_end]);
    let base_path = rest[host_end..].split('?').next().unwrap_or("");
    let base_path = if base_path.is_empty() { "/" } else { base_path };

    if r.starts_with('?') {
        return Some(format!("{origin}{base_path}{r}"));
    }
    let (ref_path, query) = match r.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (r, None),
    };
    let merged = if ref_path.starts_with('/') {
        ref_path.to_string()
    } else {
        let dir = &base_path[..=base_path.rfind('/').unwrap_or(0)];
        format!("{dir}{ref_path}")
    };
    let mut url = format!("{origin}{}", remove_dot_segments(&merged));
    if let Some(q) = query {
        url.push('?');
        url.push_str(q);
    }
    Some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(specs: &[Specifier]) -> Vec<&str> {
        specs.iter().map(|s| s.value.as_str()).collect()
    }

    fn temp_vault(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "herbarium-deps-{tag}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn key_for_is_fnv1a_64() {
        // Reference vectors of FNV-1a 64.
        assert_eq!(key_for(""), "cbf29ce484222325");
        assert_eq!(key_for("a"), "af63dc4c8601ec8c");
        assert_eq!(key_for("foobar"), "85944171f73967e8");
        let k = key_for("https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs");
        assert!(is_key(&k));
        assert_ne!(
            k,
            key_for("https://cdn.jsdelivr.net/npm/mermaid@10/dist/mermaid.esm.min.mjs")
        );
        assert!(!is_key("ABCDEF0123456789"));
        assert!(!is_key("abc"));
    }

    #[test]
    fn file_names_are_safe_and_match_the_mime() {
        let js = "text/javascript; charset=utf-8";
        assert_eq!(
            file_name_for(
                "https://cdn.x/npm/mermaid@11/dist/mermaid.esm.min.mjs?v=1#h",
                js
            ),
            "mermaid.esm.min.mjs"
        );
        assert_eq!(file_name_for("https://cdn.x/d3", js), "d3.js");
        assert_eq!(file_name_for("https://cdn.x/", js), "index.js");
        assert_eq!(file_name_for("https://cdn.x", "text/css"), "index.css");
        assert_eq!(
            file_name_for("https://fonts.x/css2?family=Inter", "text/css"),
            "css2.css"
        );
        assert_eq!(file_name_for("https://cdn.x/a/b.v2", js), "b.v2.js");
        assert_eq!(
            file_name_for("https://cdn.x/blob", "application/x-foo"),
            "blob.bin"
        );
        assert_eq!(
            file_name_for("https://cdn.x/x.dat", "application/x-foo"),
            "x.dat"
        );
        // Percent-encoded dots and slashes cannot climb out of the folder.
        assert_eq!(
            file_name_for("https://cdn.x/%2e%2e/..%2Fevil", js),
            "_evil.js"
        );
        assert_eq!(
            file_name_for("https://cdn.x/dep.json", "application/json"),
            "_dep.json.data"
        );
        assert_eq!(
            file_name_for("https://cdn.x/manifest.json", "application/json"),
            "manifest.json.data"
        );
        assert_eq!(
            file_name_for("https://cdn.x/api/data", "application/json"),
            "data.json.data"
        );
        let long = format!("https://cdn.x/{}.js", "n".repeat(200));
        let name = file_name_for(&long, js);
        assert_eq!(name.len(), 80);
        assert!(name.ends_with(".js"));
        assert!(!name.contains('/'));
    }

    #[test]
    fn store_and_load_round_trip() {
        let vault = temp_vault("store");
        let url = "https://cdn.x/lib.mjs";
        let meta = DepMeta {
            url: url.into(),
            file: "lib.mjs".into(),
            mime: "text/javascript".into(),
            bytes: 9,
            module: true,
            has_imports: true,
            fetched_at: 42,
        };
        let rel = store(&vault, &meta, b"export 1;").unwrap();
        let key = key_for(url);
        assert_eq!(rel, rel_path(&key, "lib.mjs"));
        assert_eq!(load(&vault, &key), Some(meta.clone()));
        assert_eq!(
            fs::read(vault.join(".herbarium/deps").join(&key).join("lib.mjs")).unwrap(),
            b"export 1;"
        );
        // camelCase on disk.
        let json =
            fs::read_to_string(vault.join(".herbarium/deps").join(&key).join("dep.json")).unwrap();
        assert!(json.contains("\"hasImports\": true") && json.contains("\"fetchedAt\": 42"));
        assert_eq!(load(&vault, "0123456789abcdef"), None);
        assert_eq!(load(&vault, "../../etc"), None);

        let bad = DepMeta {
            file: "../x.js".into(),
            ..meta.clone()
        };
        assert!(store(&vault, &bad, b"").is_err());
        let bad = DepMeta {
            file: "dep.json".into(),
            ..meta
        };
        assert!(store(&vault, &bad, b"").is_err());
        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn module_specifiers_in_minified_code() {
        let js = r#"import a from"https://cdn.x/a.js";import{b as c}from'./b.js';export*from"../c.js";import"/d.js";const e=await import("https://cdn.x/e.js");
// import z from "./comment.js"
/* import y from "./block.js" */
const s="import q from './string.js'";const t=`import r from "./tpl.js" ${import("./inner.js")}`;
const re=/"import x from '.\/re.js'"/;x.import("./prop.js");import.meta.url;import(variable);import bare from"bare-pkg";export{k}from"./k.js";export * as ns from "./ns.js";export const w=1;export{w as v};
import
  def ,
  { x as y } from
  'http://old.x/f.js';
import("//proto.x/p.js", { with: {} });"#;
        let specs = module_specifiers(js);
        assert_eq!(
            values(&specs),
            [
                "https://cdn.x/a.js",
                "./b.js",
                "../c.js",
                "/d.js",
                "https://cdn.x/e.js",
                "./k.js",
                "./ns.js",
                "http://old.x/f.js",
                "//proto.x/p.js",
            ]
        );
        for s in &specs {
            assert_eq!(&js[s.start..s.end], s.value);
            assert!(matches!(js.as_bytes()[s.start - 1], b'"' | b'\''));
        }
    }

    #[test]
    fn module_specifiers_ignores_lookalikes() {
        for js in [
            "const reimport = \"./a.js\"; foo.export('./a.js')",
            "var o = {import: './a.js', export: './b.js'};",
            "// import './a.js'\n/* export * from './b.js' */",
            "x = `import './a.js'`; y = \"import './b.js'\";",
            "import('./a\\u0062.js')",
            "import(dynamicName + './a.js')",
            "import x from 'bare'; import 'also-bare';",
        ] {
            assert!(module_specifiers(js).is_empty(), "{js}");
        }
    }

    #[test]
    fn rewrite_specifiers_escapes_for_the_quote() {
        let js = "import a from\"https://cdn.x/a.js\";import('./b.js');const s='./c.js';";
        let out = rewrite_specifiers(js, |s| match s {
            "https://cdn.x/a.js" => Some("../k1/a.js".into()),
            "./b.js" => Some("it's.js".into()),
            _ => None,
        });
        assert_eq!(
            out,
            "import a from\"../k1/a.js\";import('it\\'s.js');const s='./c.js';"
        );
        assert_eq!(rewrite_specifiers(js, |_| None), js);
    }

    #[test]
    fn css_urls_and_rewrite() {
        let css = "@import \"https://f.x/a.css\"; @import url(https://f.x/b.css);\n\
/* url(comment.png) */\n\
a{background:url( 'img/a.png' ) , url(data:image/png;base64,AAA), url(\"../b.svg#f\"), url(#frag); content:\"url(no.png)\";}\n\
@font-face{src:url(https://f.x/f.woff2)format(\"woff2\");mask:URL( x.svg )}\n\
.my-url(z){}";
        let specs = css_urls(css);
        assert_eq!(
            values(&specs),
            [
                "https://f.x/a.css",
                "https://f.x/b.css",
                "img/a.png",
                "../b.svg#f",
                "https://f.x/f.woff2",
                "x.svg",
            ]
        );
        for s in &specs {
            assert_eq!(&css[s.start..s.end], s.value);
        }
        let out = rewrite_css(css, |u| match u {
            "https://f.x/f.woff2" => Some("data:font/woff2;base64,AAAA".into()),
            "x.svg" => Some("a b.svg".into()),
            "img/a.png" => Some("q\"r.png".into()),
            _ => None,
        });
        assert!(out.contains("url(data:font/woff2;base64,AAAA)format"));
        assert!(out.contains("URL( \"a b.svg\" )"));
        assert!(out.contains("url( 'q\"r.png' )"));
        assert!(out.contains("url(data:image/png;base64,AAA)"));
        assert!(out.contains("content:\"url(no.png)\""));
    }

    #[test]
    fn resolve_url_cases() {
        let base = "https://cdn.x/a/b/c.js?v=1#top";
        let cases = [
            ("https://other.x/p.js#f", Some("https://other.x/p.js")),
            ("//h.x/p.js", Some("https://h.x/p.js")),
            ("/f.js", Some("https://cdn.x/f.js")),
            ("/a/./b/../f.js?q=1", Some("https://cdn.x/a/f.js?q=1")),
            ("./e.js", Some("https://cdn.x/a/b/e.js")),
            ("e.js", Some("https://cdn.x/a/b/e.js")),
            ("../d.js", Some("https://cdn.x/a/d.js")),
            ("../../../../x.js", Some("https://cdn.x/x.js")),
            ("d.js?x=1&y=2#frag", Some("https://cdn.x/a/b/d.js?x=1&y=2")),
            ("?only=query", Some("https://cdn.x/a/b/c.js?only=query")),
            ("sub/", Some("https://cdn.x/a/b/sub/")),
            ("..", Some("https://cdn.x/a/")),
            ("http://insecure.x/a.js", None),
            ("data:text/css,a{}", None),
            ("javascript:void(0)", None),
            ("#frag", None),
            ("", None),
        ];
        for (reference, want) in cases {
            assert_eq!(resolve_url(base, reference).as_deref(), want, "{reference}");
        }
        assert_eq!(
            resolve_url("https://cdn.x", "a.js").as_deref(),
            Some("https://cdn.x/a.js")
        );
        assert_eq!(resolve_url("http://cdn.x/a.js", "b.js"), None);
        assert_eq!(
            resolve_url("http://cdn.x/a.js", "https://y.x/b.js").as_deref(),
            Some("https://y.x/b.js")
        );
    }

    const PAGE: &str = r#"<html><head>
<link rel="stylesheet" href="https://fonts.x/s.css?a=1&amp;b=2">
<SCRIPT src=//cdn.x/classic.js></SCRIPT>
<script type="module" src='https://cdn.x/m.mjs'></script>
<script type="importmap">{"imports":{"d3":"https://cdn.x/d3.js","local":"./l.js"},"scopes":{"https://cdn.x/":{"x":"https://cdn.x/x.js"}},"integrity":{"https://cdn.x/d3.js":"sha384-abc"}}</script>
<script type=MODULE>import m from "https://cdn.x/inner.mjs"; import l from "./l.js"; const s = '<img src="https://no.x/a.png">';</script>
<script>var s = "<img src='https://nope.x/a.png'>"; import("https://nope.x/b.js");</script>
<!-- <script src="https://commented.x/a.js"></script> -->
<style>@import url(https://css.x/a.css);</style>
</head><body><img src="https://img.x/a.png"><img src="data:image/png;base64,AA"><img src="a.png"><img src="http://insecure.x/a.png"><img src="_herbarium/lib/x.js"></body></html>"#;

    #[test]
    fn page_refs_finds_every_remote_place() {
        let refs = page_refs(PAGE);
        let get = |url: &str| {
            refs.iter()
                .find(|r| r.url == url)
                .unwrap_or_else(|| panic!("{url}"))
        };
        let mut urls: Vec<&str> = refs.iter().map(|r| r.url.as_str()).collect();
        urls.sort();
        assert_eq!(
            urls,
            [
                "https://cdn.x/classic.js",
                "https://cdn.x/d3.js",
                "https://cdn.x/inner.mjs",
                "https://cdn.x/m.mjs",
                "https://cdn.x/x.js",
                "https://fonts.x/s.css?a=1&b=2",
                "https://img.x/a.png",
            ]
        );
        assert_eq!(
            get("https://fonts.x/s.css?a=1&b=2").kind,
            RefKind::Asset {
                tag: "link".into(),
                attr: "href".into()
            }
        );
        assert_eq!(
            get("https://fonts.x/s.css?a=1&b=2").raw,
            "https://fonts.x/s.css?a=1&b=2"
        );
        assert_eq!(
            get("https://cdn.x/classic.js").kind,
            RefKind::Asset {
                tag: "script".into(),
                attr: "src".into()
            }
        );
        assert_eq!(get("https://cdn.x/classic.js").raw, "//cdn.x/classic.js");
        assert_eq!(get("https://cdn.x/m.mjs").kind, RefKind::ModuleImport);
        assert_eq!(get("https://cdn.x/inner.mjs").kind, RefKind::ModuleImport);
        assert_eq!(get("https://cdn.x/d3.js").kind, RefKind::ImportMap);
        assert_eq!(get("https://cdn.x/x.js").kind, RefKind::ImportMap);
    }

    #[test]
    fn rewrite_page_touches_only_those_places() {
        let map: HashMap<String, String> = [
            ("https://fonts.x/s.css?a=1&b=2", "_herbarium/deps/k1/s.css"),
            ("//cdn.x/classic.js", "_herbarium/deps/k2/classic.js"),
            ("https://cdn.x/m.mjs", "_herbarium/deps/k3/m.mjs"),
            ("https://cdn.x/d3.js", "./_herbarium/lib/d3.js"),
            ("https://cdn.x/inner.mjs", "./_herbarium/lib/mermaid.mjs"),
            ("https://nope.x/a.png", "SHOULD-NOT-APPEAR"),
            ("https://nope.x/b.js", "SHOULD-NOT-APPEAR"),
            ("https://commented.x/a.js", "SHOULD-NOT-APPEAR"),
            ("https://css.x/a.css", "SHOULD-NOT-APPEAR"),
            ("https://no.x/a.png", "SHOULD-NOT-APPEAR"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let out = rewrite_page(PAGE, &map);
        assert!(!out.contains("SHOULD-NOT-APPEAR"));
        assert!(out.contains(r#"href="_herbarium/deps/k1/s.css""#));
        assert!(out.contains(r#"src="_herbarium/deps/k2/classic.js""#));
        assert!(out.contains("src='_herbarium/deps/k3/m.mjs'"));
        assert!(out.contains(r#""d3":"./_herbarium/lib/d3.js""#));
        // Import-map values only: the unmapped scope entry and the integrity key stay.
        assert!(out.contains(r#""x":"https://cdn.x/x.js""#));
        assert!(out.contains(r#""integrity":{"https://cdn.x/d3.js":"sha384-abc"}"#));
        assert!(out.contains(r#"import m from "./_herbarium/lib/mermaid.mjs";"#));
        assert!(out.contains(r#"import l from "./l.js""#));
        // Untouched surroundings.
        assert!(out.contains(r#"<img src="https://img.x/a.png">"#));
        assert!(out.contains("<!-- <script src=\"https://commented.x/a.js\"></script> -->"));
        assert_eq!(rewrite_page(PAGE, &HashMap::new()), PAGE);
        // After rewriting, the mapped places no longer refer to the internet.
        let left: Vec<String> = page_refs(&out).into_iter().map(|r| r.url).collect();
        assert!(left.contains(&"https://cdn.x/x.js".to_string()));
        assert!(!left.contains(&"https://cdn.x/m.mjs".to_string()));
    }

    #[test]
    fn rewrite_page_removes_integrity_on_mapped_tags_only() {
        let map: HashMap<String, String> = [
            ("https://cdn.x/a.js", "_herbarium/deps/k1/a.js"),
            ("https://cdn.x/s.css", "_herbarium/deps/k2/s.css"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

        let html = r#"<script src="https://cdn.x/a.js" integrity="sha384-x" crossorigin></script>
<script src="https://cdn.x/unmapped.js" integrity="sha384-keep" crossorigin></script>
<link rel="stylesheet" href="https://cdn.x/s.css" integrity="sha384-css" crossorigin="anonymous">
<link rel="stylesheet" href="https://cdn.x/unmapped.css" integrity="sha384-keepcss">"#;

        let out = rewrite_page(html, &map);
        assert_eq!(
            out,
            r#"<script src="_herbarium/deps/k1/a.js" crossorigin></script>
<script src="https://cdn.x/unmapped.js" integrity="sha384-keep" crossorigin></script>
<link rel="stylesheet" href="_herbarium/deps/k2/s.css" crossorigin="anonymous">
<link rel="stylesheet" href="https://cdn.x/unmapped.css" integrity="sha384-keepcss">"#
        );
    }

    #[test]
    fn rewrite_page_removes_integrity_when_first_attribute() {
        let map: HashMap<String, String> = [("https://cdn.x/a.js", "_herbarium/deps/k1/a.js")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        let html = r#"<script integrity="sha384-x" src="https://cdn.x/a.js" crossorigin></script>"#;
        let out = rewrite_page(html, &map);
        assert_eq!(
            out,
            r#"<script src="_herbarium/deps/k1/a.js" crossorigin></script>"#
        );
    }

    #[test]
    fn page_refs_skips_import_map_prefix_entries() {
        let html = r#"<script type="importmap">{
            "imports": {
                "three": "https://cdn.jsdelivr.net/npm/three@0.160/build/three.module.js",
                "three/addons/": "https://cdn.jsdelivr.net/npm/three@0.160/examples/jsm/"
            }
        }</script>"#;
        let refs = page_refs(html);
        assert_eq!(refs.len(), 1);
        assert_eq!(
            refs[0].url,
            "https://cdn.jsdelivr.net/npm/three@0.160/build/three.module.js"
        );
        assert_eq!(refs[0].kind, RefKind::ImportMap);
    }

    #[test]
    fn page_refs_ignores_non_resource_links() {
        let html = r#"<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Roboto&display=swap" rel="stylesheet">"#;
        let refs = page_refs(html);
        assert_eq!(refs.len(), 1);
        assert_eq!(
            refs[0].url,
            "https://fonts.googleapis.com/css2?family=Roboto&display=swap"
        );
        assert_eq!(
            refs[0].kind,
            RefKind::Asset {
                tag: "link".into(),
                attr: "href".into()
            }
        );
    }
}
