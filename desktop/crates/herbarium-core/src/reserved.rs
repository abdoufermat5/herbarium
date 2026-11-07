// The reserved `_herbarium/` URL namespace: bundled libraries, vendored
// dependencies and diagram renders, all addressed relative to the page so the
// page's own origin (and `<base href>`) covers them. This module only parses
// and resolves those paths; serving them is the protocol handler's job and
// fetching renders belongs to the desktop crate.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::{deps, libs};

/// Every reserved path starts with this, relative to the page.
pub const PREFIX: &str = "_herbarium/";

/// Largest diagram source carried in a URL (and the cache key input).
const MAX_SOURCE: usize = 32 * 1024;

/// Diagram kinds the diagram server (Kroki) is asked for.
const KINDS: [&str; 27] = [
    "plantuml",
    "c4plantuml",
    "d2",
    "graphviz",
    "ditaa",
    "erd",
    "nomnoml",
    "pikchr",
    "structurizr",
    "svgbob",
    "vega",
    "vegalite",
    "wavedrom",
    "bpmn",
    "bytefield",
    "excalidraw",
    "blockdiag",
    "seqdiag",
    "actdiag",
    "nwdiag",
    "packetdiag",
    "rackdiag",
    "umlet",
    "tikz",
    "symbolator",
    "dbml",
    "wireviz",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reserved {
    /// `_herbarium/lib/<file>`
    Lib(String),
    /// `_herbarium/deps/<key>/<file>`
    Dep { key: String, file: String },
    /// `_herbarium/render/<kind>/<payload>.svg`, with the decoded source.
    Render { kind: &'static str, source: String },
}

/// A single path segment that cannot step out of its directory.
fn single_segment(s: &str) -> Option<&str> {
    let ok = !s.is_empty() && s != "." && s != ".." && !s.contains(['/', '\\', '\0']);
    ok.then_some(s)
}

/// Parse a decoded path relative to the page, e.g. `_herbarium/lib/mermaid.js`.
pub fn parse(rel: &str) -> Option<Reserved> {
    let rest = rel.strip_prefix(PREFIX)?;
    if let Some(file) = rest.strip_prefix("lib/") {
        return Some(Reserved::Lib(single_segment(file)?.to_string()));
    }
    if let Some(rest) = rest.strip_prefix("deps/") {
        let (key, file) = rest.split_once('/')?;
        if !deps::is_key(key) {
            return None;
        }
        return Some(Reserved::Dep {
            key: key.to_string(),
            file: single_segment(file)?.to_string(),
        });
    }
    if let Some(rest) = rest.strip_prefix("render/") {
        let (kind, file) = rest.split_once('/')?;
        let kind = render_kind(kind)?;
        let payload = file.strip_suffix(".svg")?;
        // Reject before decoding what could never be a valid source.
        if payload.len() > MAX_SOURCE.div_ceil(3) * 4 {
            return None;
        }
        let bytes = b64url_decode(payload)?;
        if bytes.len() > MAX_SOURCE {
            return None;
        }
        let source = String::from_utf8(bytes).ok()?;
        return Some(Reserved::Render { kind, source });
    }
    None
}

/// The diagram-server kind for a fence language or kind name.
pub fn render_kind(lang: &str) -> Option<&'static str> {
    let lower = lang.trim().to_ascii_lowercase();
    let canonical = match lower.as_str() {
        "dot" | "gv" => "graphviz",
        "puml" | "uml" => "plantuml",
        "c4" => "c4plantuml",
        "vega-lite" => "vegalite",
        other => other,
    };
    KINDS.iter().copied().find(|k| *k == canonical)
}

/// The `_herbarium/render/…` path that renders `source` as `kind`; `None` for
/// an unknown kind or a source too long to carry in a URL.
pub fn render_src(kind: &str, source: &str) -> Option<String> {
    let kind = render_kind(kind)?;
    if source.len() > MAX_SOURCE {
        return None;
    }
    Some(format!(
        "{PREFIX}render/{kind}/{}.svg",
        b64url_encode(source.as_bytes())
    ))
}

fn render_cache_file(kind: &str, source: &str) -> String {
    format!("{}.svg", deps::key_for(&format!("{kind}\0{source}")))
}

/// Where a rendered diagram is cached.
pub fn render_cache_path(vault: &Path, kind: &str, source: &str) -> PathBuf {
    vault
        .join(".herbarium")
        .join("cache")
        .join("render")
        .join(render_cache_file(kind, source))
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// What a diagram shows when it cannot be rendered. The grey reads on both
/// light and dark pages.
pub fn placeholder_svg(message: &str) -> String {
    let message = escape_xml(message);
    format!(
        concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="360" height="80" viewBox="0 0 360 80" role="img" aria-label="{m}">"##,
            r##"<title>{m}</title>"##,
            r##"<rect x="1" y="1" width="358" height="78" rx="8" fill="none" stroke="#7a7f89" stroke-width="1.5" stroke-dasharray="5 4"/>"##,
            r##"<text x="180" y="45" text-anchor="middle" font-family="system-ui, sans-serif" font-size="14" fill="#7a7f89">{m}</text>"##,
            r##"</svg>"##
        ),
        m = message
    )
}

/// Bytes for a Lib, a stored Dep, or a *cached* Render; `None` when missing.
/// Nothing under the vault is read through a symlink.
pub fn resolve(vault: &Path, r: &Reserved) -> Option<(String, Cow<'static, [u8]>)> {
    match r {
        Reserved::Lib(file) => {
            let lib = libs::get(file)?;
            Some((lib.mime.to_string(), lib.bytes))
        }
        Reserved::Dep { key, file } => {
            let meta = deps::load(vault, key)?;
            if meta.file != *file || file == deps::META_FILE {
                return None;
            }
            let bytes =
                deps::read_plain(vault, &[".herbarium", "deps", key.as_str(), file.as_str()])?;
            Some((meta.mime, Cow::Owned(bytes)))
        }
        Reserved::Render { kind, source } => {
            let name = render_cache_file(kind, source);
            let bytes = deps::read_plain(vault, &[".herbarium", "cache", "render", name.as_str()])?;
            Some(("image/svg+xml".to_string(), Cow::Owned(bytes)))
        }
    }
}

const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Base64url (RFC 4648 §5) without padding.
fn b64url_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        out.push(B64URL[(n >> 18) as usize & 63] as char);
        out.push(B64URL[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(B64URL[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(B64URL[n as usize & 63] as char);
        }
    }
    out
}

/// Unpadded base64url; padding or other alphabets are rejected.
fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deps::DepMeta;
    use std::fs;

    fn temp_vault(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "herbarium-reserved-{tag}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn base64url_matches_known_vectors() {
        for (input, want) in [
            ("", ""),
            ("f", "Zg"),
            ("fo", "Zm8"),
            ("foo", "Zm9v"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(b64url_encode(input.as_bytes()), want);
            assert_eq!(b64url_decode(want).unwrap(), input.as_bytes());
        }
        // `+` and `/` of the standard alphabet become `-` and `_`.
        assert_eq!(b64url_encode(&[0xfb, 0xff, 0xfe]), "-__-");
        assert!(b64url_decode("Zg==").is_none());
        assert!(b64url_decode("Z").is_none());
        assert!(b64url_decode("a+b/").is_none());
    }

    #[test]
    fn render_path_round_trips_unicode_and_newlines() {
        let source = "digraph G {\n  a -> b [label=\"héllo → 世界 🌿\"];\n}";
        let path = render_src("dot", source).unwrap();
        assert!(path.starts_with("_herbarium/render/graphviz/"));
        assert!(path.ends_with(".svg"));
        assert!(!path.contains('='));
        assert_eq!(
            parse(&path),
            Some(Reserved::Render {
                kind: "graphviz",
                source: source.to_string()
            })
        );
    }

    #[test]
    fn render_src_refuses_oversized_sources_and_unknown_kinds() {
        let at_limit = "x".repeat(MAX_SOURCE);
        let path = render_src("plantuml", &at_limit).unwrap();
        assert!(
            matches!(parse(&path), Some(Reserved::Render { source, .. }) if source.len() == MAX_SOURCE)
        );
        assert!(render_src("plantuml", &"x".repeat(MAX_SOURCE + 1)).is_none());
        assert!(render_src("mermaid", "graph TD").is_none());
        // A hand-written oversized payload is not served either.
        let payload = b64url_encode("y".repeat(MAX_SOURCE + 1).as_bytes());
        assert!(parse(&format!("_herbarium/render/d2/{payload}.svg")).is_none());
    }

    #[test]
    fn render_kind_aliases() {
        assert_eq!(render_kind("dot"), Some("graphviz"));
        assert_eq!(render_kind("GV"), Some("graphviz"));
        assert_eq!(render_kind("puml"), Some("plantuml"));
        assert_eq!(render_kind("uml"), Some("plantuml"));
        assert_eq!(render_kind("c4"), Some("c4plantuml"));
        assert_eq!(render_kind("vega-lite"), Some("vegalite"));
        assert_eq!(render_kind("d2"), Some("d2"));
        assert_eq!(render_kind("mermaid"), None);
        assert_eq!(render_kind(""), None);
        assert_eq!(render_kind("wireviz"), Some("wireviz"));
    }

    #[test]
    fn parse_validates_each_namespace() {
        assert_eq!(
            parse("_herbarium/lib/mermaid.js"),
            Some(Reserved::Lib("mermaid.js".into()))
        );
        assert_eq!(parse("_herbarium/lib/"), None);
        assert_eq!(parse("_herbarium/lib/a/b.js"), None);
        assert_eq!(parse("_herbarium/lib/.."), None);
        assert_eq!(parse("_herbarium/lib/a\\b.js"), None);
        assert_eq!(parse("lib/mermaid.js"), None);
        assert_eq!(parse("_herbarium/other/x"), None);

        let key = "0123456789abcdef";
        assert_eq!(
            parse(&format!("_herbarium/deps/{key}/x.js")),
            Some(Reserved::Dep {
                key: key.into(),
                file: "x.js".into()
            })
        );
        assert_eq!(parse("_herbarium/deps/0123456789ABCDEF/x.js"), None);
        assert_eq!(parse("_herbarium/deps/0123456789abcde/x.js"), None);
        assert_eq!(parse(&format!("_herbarium/deps/{key}/a/b.js")), None);
        assert_eq!(parse(&format!("_herbarium/deps/{key}/..")), None);
        assert_eq!(parse(&format!("_herbarium/deps/{key}")), None);

        assert_eq!(parse("_herbarium/render/nope/Zm9v.svg"), None);
        assert_eq!(parse("_herbarium/render/d2/Zm9v.png"), None);
        assert_eq!(parse("_herbarium/render/d2/Zm9v=.svg"), None);
        // 0xff is not UTF-8.
        assert_eq!(parse("_herbarium/render/d2/_w.svg"), None);
        assert_eq!(
            parse("_herbarium/render/d2/Zm9v.svg"),
            Some(Reserved::Render {
                kind: "d2",
                source: "foo".into()
            })
        );
    }

    #[test]
    fn placeholder_escapes_its_message() {
        let svg = placeholder_svg("a <b> & \"c\"");
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("a &lt;b&gt; &amp; &quot;c&quot;"));
        assert!(!svg.contains("<b>"));
        assert!(svg.contains("role=\"img\""));
    }

    #[test]
    fn cache_path_is_stable_and_depends_on_kind_and_source() {
        let v = Path::new("/vault");
        let a = render_cache_path(v, "d2", "x -> y");
        assert_eq!(a, render_cache_path(v, "d2", "x -> y"));
        assert_ne!(a, render_cache_path(v, "graphviz", "x -> y"));
        assert_ne!(a, render_cache_path(v, "d2", "x -> z"));
        assert!(a.starts_with("/vault/.herbarium/cache/render"));
        assert_eq!(a.extension().and_then(|e| e.to_str()), Some("svg"));
    }

    #[test]
    fn resolve_serves_deps_only_when_dep_json_names_the_file() {
        let vault = temp_vault("dep");
        let url = "https://cdn.example/lib.js";
        let meta = DepMeta {
            url: url.into(),
            file: "lib.js".into(),
            mime: "text/javascript".into(),
            bytes: 3,
            module: false,
            has_imports: false,
            fetched_at: 1,
        };
        let rel = deps::store(&vault, &meta, b"1+1").unwrap();
        let key = deps::key_for(url);
        assert_eq!(rel, format!("_herbarium/deps/{key}/lib.js"));
        let r = parse(&rel).unwrap();
        let (mime, bytes) = resolve(&vault, &r).unwrap();
        assert_eq!(mime, "text/javascript");
        assert_eq!(&*bytes, b"1+1");

        // A stray file next to dep.json is not served, nor is dep.json itself.
        fs::write(
            vault.join(".herbarium/deps").join(&key).join("other.js"),
            b"x",
        )
        .unwrap();
        for file in ["other.js", "dep.json"] {
            let r = Reserved::Dep {
                key: key.clone(),
                file: file.into(),
            };
            assert!(resolve(&vault, &r).is_none(), "{file}");
        }
        // A dep.json whose URL does not hash to its folder is rejected.
        let other = "0000000000000000";
        fs::create_dir_all(vault.join(".herbarium/deps").join(other)).unwrap();
        fs::copy(
            vault.join(".herbarium/deps").join(&key).join("dep.json"),
            vault.join(".herbarium/deps").join(other).join("dep.json"),
        )
        .unwrap();
        fs::write(
            vault.join(".herbarium/deps").join(other).join("lib.js"),
            b"x",
        )
        .unwrap();
        assert!(
            resolve(
                &vault,
                &Reserved::Dep {
                    key: other.into(),
                    file: "lib.js".into()
                }
            )
            .is_none()
        );
        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn resolve_serves_cached_renders_only() {
        let vault = temp_vault("render");
        let r = Reserved::Render {
            kind: "d2",
            source: "a -> b".into(),
        };
        assert!(resolve(&vault, &r).is_none());
        let path = render_cache_path(&vault, "d2", "a -> b");
        crate::vault::write_atomic(&path, b"<svg/>").unwrap();
        let (mime, bytes) = resolve(&vault, &r).unwrap();
        assert_eq!(mime, "image/svg+xml");
        assert_eq!(&*bytes, b"<svg/>");
        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn resolve_serves_bundled_libs() {
        let vault = temp_vault("lib");
        let (mime, bytes) = resolve(&vault, &Reserved::Lib("mermaid.js".into())).unwrap();
        assert!(mime.contains("javascript"));
        assert!(!bytes.is_empty());
        assert!(resolve(&vault, &Reserved::Lib("nope.js".into())).is_none());
        let _ = fs::remove_dir_all(&vault);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_never_follows_symlinks() {
        use std::os::unix::fs::symlink;
        let vault = temp_vault("symlink");
        let outside = temp_vault("symlink-outside");
        fs::write(outside.join("secret.svg"), b"secret").unwrap();

        // The cached file itself is a link.
        let path = render_cache_path(&vault, "d2", "a");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        symlink(outside.join("secret.svg"), &path).unwrap();
        let r = Reserved::Render {
            kind: "d2",
            source: "a".into(),
        };
        assert!(resolve(&vault, &r).is_none());

        // A folder on the way is a link.
        let linked = temp_vault("symlink-vault");
        fs::create_dir_all(linked.join(".herbarium")).unwrap();
        symlink(&outside, linked.join(".herbarium/cache")).unwrap();
        let name = render_cache_file("d2", "b");
        fs::create_dir_all(outside.join("render")).unwrap();
        fs::write(outside.join("render").join(&name), b"secret").unwrap();
        let r = Reserved::Render {
            kind: "d2",
            source: "b".into(),
        };
        assert!(resolve(&linked, &r).is_none());
        for d in [&vault, &outside, &linked] {
            let _ = fs::remove_dir_all(d);
        }
    }
}
