// Attribute rewriting for exports: walk the start tags of an HTML document and
// let a callback replace attribute values (asset URLs, page links), leaving
// every other byte as it was. Comments and the bodies of <script> and <style>
// are skipped, so code that merely mentions `src="…"` is never touched.

/// Rewrite attribute values of `html`. `f(tag, attr, value)` gets lowercase
/// tag and attribute names and the value with `&amp;` decoded; returning
/// `Some(new)` replaces the value (written double-quoted and escaped).
pub fn rewrite_attrs(html: &str, mut f: impl FnMut(&str, &str, &str) -> Option<String>) -> String {
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len());
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if html[i..].starts_with("<!--") {
            i = html[i + 4..]
                .find("-->")
                .map_or(bytes.len(), |e| i + 4 + e + 3);
            continue;
        }
        let Some(first) = bytes.get(i + 1) else { break };
        if !first.is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        // Tag name.
        let mut j = i + 1;
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-') {
            j += 1;
        }
        let tag = html[i + 1..j].to_ascii_lowercase();
        // Attributes until the closing `>`.
        loop {
            while j < bytes.len() && (bytes[j].is_ascii_whitespace() || bytes[j] == b'/') {
                j += 1;
            }
            if j >= bytes.len() || bytes[j] == b'>' {
                break;
            }
            let name_start = j;
            while j < bytes.len()
                && !bytes[j].is_ascii_whitespace()
                && !matches!(bytes[j], b'=' | b'>' | b'/')
            {
                j += 1;
            }
            let name = html[name_start..j].to_ascii_lowercase();
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if bytes.get(j) != Some(&b'=') {
                continue;
            }
            j += 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            let (value_start, value_end, next) = match bytes.get(j) {
                Some(q @ (b'"' | b'\'')) => {
                    let end = html[j + 1..]
                        .find(*q as char)
                        .map_or(bytes.len(), |e| j + 1 + e);
                    (j, (end + 1).min(bytes.len()), end + 1)
                }
                _ => {
                    let start = j;
                    while j < bytes.len() && !bytes[j].is_ascii_whitespace() && bytes[j] != b'>' {
                        j += 1;
                    }
                    (start, j, j)
                }
            };
            let raw = &html[value_start..value_end];
            // Drop only the delimiting quotes, never quotes inside the value.
            let unquoted = match raw.as_bytes().first() {
                Some(q @ (b'"' | b'\'')) => {
                    let inner = &raw[1..];
                    inner.strip_suffix(*q as char).unwrap_or(inner)
                }
                _ => raw,
            };
            let value = unquoted.replace("&amp;", "&");
            if let Some(new) = f(&tag, &name, &value) {
                out.push_str(&html[copied..value_start]);
                out.push('"');
                out.push_str(&new.replace('&', "&amp;").replace('"', "&quot;"));
                out.push('"');
                copied = value_end;
            }
            j = next.min(bytes.len());
        }
        i = (j + 1).min(bytes.len());
        // Raw text: skip to the matching end tag.
        if tag == "script" || tag == "style" {
            let close = format!("</{tag}");
            i = find_ci(&html[i..], &close).map_or(bytes.len(), |e| i + e);
        }
    }
    out.push_str(&html[copied..]);
    out
}

fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    let hay = haystack.as_bytes();
    let n = needle.as_bytes();
    (0..=hay.len().checked_sub(n.len())?).find(|&k| hay[k..k + n.len()].eq_ignore_ascii_case(n))
}

/// A URL relative to the page: no scheme, not root- or protocol-relative and
/// not a fragment or query on its own. Returns the path part, without any
/// `?query` or `#fragment`.
pub fn relative_path(url: &str) -> Option<&str> {
    let url = url.trim();
    if url.is_empty() || url.starts_with(['/', '#', '?', '\\']) {
        return None;
    }
    let path_end = url.find(['?', '#']).unwrap_or(url.len());
    let path = &url[..path_end];
    let first_segment = path.split('/').next().unwrap_or("");
    if first_segment.contains(':') || path.is_empty() {
        return None;
    }
    Some(path.strip_prefix("./").unwrap_or(path))
}

/// Whether `(tag, attr)` loads a resource the page needs: images, scripts,
/// stylesheets and icons, media sources and posters.
pub fn is_asset_attr(tag: &str, attr: &str) -> bool {
    matches!(
        (tag, attr),
        (
            "img" | "script" | "source" | "audio" | "video" | "track" | "input" | "embed",
            "src"
        ) | ("link", "href")
            | ("video", "poster")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_only_matching_attributes() {
        let html = r#"<!doctype html><html><head><link rel=stylesheet href=style.css><script src='app.js?v=2'></script>
<script>const s = '<img src="code.png">';</script><style>a{background:url(x.png)}</style></head>
<body><!-- <img src="comment.png"> --><IMG SRC="a&amp;b.png" alt="x"><img src="https://cdn/x.png"><img src=data:image/png;base64,AA>
<a href="herbarium-app://open/next">n</a><p title='src="no.png"'>t</p></body></html>"#;
        let mut seen = Vec::new();
        let out = rewrite_attrs(html, |tag, attr, value| {
            seen.push(format!("{tag}.{attr}={value}"));
            match (tag, attr) {
                ("img", "src") if value == "a&b.png" => Some("data:new".into()),
                ("link", "href") => Some("data:text/css,a\"b".into()),
                ("a", "href") => Some("next.html".into()),
                _ => None,
            }
        });
        assert!(out.contains(r#"<IMG SRC="data:new" alt="x">"#), "{out}");
        assert!(
            out.contains(r#"<link rel=stylesheet href="data:text/css,a&quot;b">"#),
            "{out}"
        );
        assert!(out.contains(r#"<a href="next.html">n</a>"#));
        assert!(
            out.contains(r#"const s = '<img src="code.png">';"#),
            "script bodies are untouched"
        );
        assert!(out.contains(r#"<!-- <img src="comment.png"> -->"#));
        assert!(
            !seen
                .iter()
                .any(|s| s.contains("code.png") || s.contains("comment.png"))
        );
        assert!(seen.contains(&"script.src=app.js?v=2".to_string()));
        assert!(seen.contains(&"p.title=src=\"no.png\"".to_string()));
    }

    #[test]
    fn unclosed_markup_does_not_panic() {
        for html in [
            "<",
            "<img",
            "<img src=\"x",
            "<script>never closed",
            "<!-- open",
            "<a href=",
            "é<b>",
        ] {
            let _ = rewrite_attrs(html, |_, _, _| Some("x".into()));
        }
    }

    #[test]
    fn relative_paths() {
        assert_eq!(relative_path("img/a.png?x#y"), Some("img/a.png"));
        assert_eq!(relative_path("./a.png"), Some("a.png"));
        for absolute in [
            "https://x/a.png",
            "data:x",
            "/abs.png",
            "//cdn/x",
            "#top",
            "?q",
            "javascript:1",
            "",
        ] {
            assert_eq!(relative_path(absolute), None, "{absolute}");
        }
        assert!(is_asset_attr("img", "src") && is_asset_attr("link", "href"));
        assert!(!is_asset_attr("a", "href"));
    }
}
