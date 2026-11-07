//! Bundled third-party libraries served directly by the app.
//!
//! Provides offline copies of Mermaid, KaTeX, highlight.js, and Chart.js so
//! that diagram, formula, code, and chart rendering continue working with
//! network access off. Files are compiled into the binary via `include_bytes!`.
//!
//! ES module imports of bundled libraries (Mermaid, KaTeX, highlight.js) are
//! served as virtual `.mjs` modules. Note that Chart.js is only available in its
//! classic UMD form as `chart.js` (Chart.js ESM uses named imports not satisfied
//! by a virtual module).

use std::borrow::Cow;

pub const MERMAID_VERSION: &str = "11.17.2";
pub const KATEX_VERSION: &str = "0.16.47";
pub const HIGHLIGHT_JS_VERSION: &str = "11.12.0";
pub const CHART_JS_VERSION: &str = "4.6.1";

const MERMAID_JS: &[u8] = include_bytes!("../lib/mermaid.js");
const KATEX_JS: &[u8] = include_bytes!("../lib/katex.js");
const KATEX_AUTO_RENDER_JS: &[u8] = include_bytes!("../lib/katex-auto-render.js");
const KATEX_CSS: &[u8] = include_bytes!("../lib/katex.css");
const HIGHLIGHT_JS: &[u8] = include_bytes!("../lib/highlight.js");
const HIGHLIGHT_CSS: &[u8] = include_bytes!("../lib/highlight.css");
const CHART_JS: &[u8] = include_bytes!("../lib/chart.js");
const LICENSES_MD: &[u8] = include_bytes!("../lib/LICENSES.md");

pub struct LibFile {
    pub mime: &'static str,
    pub bytes: Cow<'static, [u8]>,
}

/// Lists the bundled packages and their exact versions matching `package.json`.
pub fn names() -> &'static [(&'static str, &'static str)] {
    &[
        ("chart.js", CHART_JS_VERSION),
        ("highlight.js", HIGHLIGHT_JS_VERSION),
        ("katex", KATEX_VERSION),
        ("mermaid", MERMAID_VERSION),
    ]
}

fn virtual_module(bytes: &'static [u8], global: &str) -> Vec<u8> {
    let suffix = format!("\n;export default {global};\n");
    let mut out = Vec::with_capacity(bytes.len() + suffix.len());
    out.extend_from_slice(bytes);
    out.extend_from_slice(suffix.as_bytes());
    out
}

/// Retrieve a bundled library file. `file` is the path after `_herbarium/lib/`.
///
/// Virtual `.mjs` modules return the minified IIFE with an appended
/// `export default <Global>;` (and for `katex-auto-render.mjs`, bundles KaTeX
/// first). Returns `None` for unknown names or paths containing directory
/// separators.
pub fn get(file: &str) -> Option<LibFile> {
    if file.contains('/') || file.contains('\\') {
        return None;
    }

    match file {
        "mermaid.js" => Some(LibFile {
            mime: "text/javascript; charset=utf-8",
            bytes: Cow::Borrowed(MERMAID_JS),
        }),
        "mermaid.mjs" => Some(LibFile {
            mime: "text/javascript; charset=utf-8",
            bytes: Cow::Owned(virtual_module(MERMAID_JS, "mermaid")),
        }),
        "katex.js" => Some(LibFile {
            mime: "text/javascript; charset=utf-8",
            bytes: Cow::Borrowed(KATEX_JS),
        }),
        "katex.mjs" => Some(LibFile {
            mime: "text/javascript; charset=utf-8",
            bytes: Cow::Owned(virtual_module(KATEX_JS, "katex")),
        }),
        "katex.css" => Some(LibFile {
            mime: "text/css; charset=utf-8",
            bytes: Cow::Borrowed(KATEX_CSS),
        }),
        "katex-auto-render.js" => Some(LibFile {
            mime: "text/javascript; charset=utf-8",
            bytes: Cow::Borrowed(KATEX_AUTO_RENDER_JS),
        }),
        "katex-auto-render.mjs" => {
            let suffix = b"\n;export default renderMathInElement;\n";
            let mut bytes =
                Vec::with_capacity(KATEX_JS.len() + 2 + KATEX_AUTO_RENDER_JS.len() + suffix.len());
            bytes.extend_from_slice(KATEX_JS);
            bytes.extend_from_slice(b"\n;");
            bytes.extend_from_slice(KATEX_AUTO_RENDER_JS);
            bytes.extend_from_slice(suffix);
            Some(LibFile {
                mime: "text/javascript; charset=utf-8",
                bytes: Cow::Owned(bytes),
            })
        }
        "highlight.js" => Some(LibFile {
            mime: "text/javascript; charset=utf-8",
            bytes: Cow::Borrowed(HIGHLIGHT_JS),
        }),
        "highlight.mjs" => Some(LibFile {
            mime: "text/javascript; charset=utf-8",
            bytes: Cow::Owned(virtual_module(HIGHLIGHT_JS, "hljs")),
        }),
        "highlight.css" => Some(LibFile {
            mime: "text/css; charset=utf-8",
            bytes: Cow::Borrowed(HIGHLIGHT_CSS),
        }),
        "chart.js" => Some(LibFile {
            mime: "text/javascript; charset=utf-8",
            bytes: Cow::Borrowed(CHART_JS),
        }),
        "LICENSES.md" => Some(LibFile {
            mime: "text/markdown; charset=utf-8",
            bytes: Cow::Borrowed(LICENSES_MD),
        }),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Package {
    Mermaid,
    Katex,
    HighlightJs,
    ChartJs,
}

fn is_version_compatible(req: &str, bundled: &str) -> bool {
    let req = req.trim();
    if req.is_empty() || req.eq_ignore_ascii_case("latest") {
        return true;
    }
    let req = req.trim_start_matches(['^', '~', 'v']);
    let req = req.split(['-', '+']).next().unwrap_or(req);

    let mut parts = req.split('.');
    let req_major: u64 = match parts.next().and_then(|s| s.parse().ok()) {
        Some(m) => m,
        None => return false,
    };
    let req_minor: Option<u64> = parts.next().and_then(|s| s.parse().ok());

    let mut b_parts = bundled.split('.');
    let b_major: u64 = b_parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let b_minor: u64 = b_parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);

    if b_major == 0 {
        if req_major != 0 {
            return false;
        }
        match req_minor {
            Some(m) => m == b_minor,
            None => false,
        }
    } else {
        req_major == b_major
    }
}

fn is_highlight_match(path: &str) -> bool {
    path.is_empty()
        || path == "+esm"
        || path == "highlight.js"
        || path == "highlight.min.js"
        || path == "es/highlight.js"
        || path == "es/highlight.min.js"
        || path == "lib/highlight.js"
        || path == "lib/highlight.min.js"
        || path == "lib/common"
        || path == "lib/common.js"
        || path == "es/common"
        || path == "es/common.js"
        || path.ends_with("/highlight.js")
        || path.ends_with("/highlight.min.js")
}

fn is_chart_umd_match(path: &str) -> bool {
    path == "chart.umd.js"
        || path == "chart.umd.min.js"
        || path == "dist/chart.umd.js"
        || path == "dist/chart.umd.min.js"
        || path == "chart.min.js"
        || path == "dist/chart.min.js"
        || path.ends_with("/chart.umd.js")
        || path.ends_with("/chart.umd.min.js")
        || path.ends_with("/chart.min.js")
}

/// Returns the `_herbarium/lib/<file>` path a CDN URL can be served from, when
/// it refers to a bundled library on the same compatibility line.
///
/// Supported CDN forms:
/// - jsDelivr: `https://cdn.jsdelivr.net/npm/<pkg>@<ver>/<path>`
/// - unpkg: `https://unpkg.com/<pkg>@<ver>/<path>`
/// - cdnjs: `https://cdnjs.cloudflare.com/ajax/libs/<pkg>/<ver>/<path>`
/// - esm.sh: `https://esm.sh/<pkg>@<ver>[/path]`
pub fn map_cdn_url(url: &str, as_module: bool) -> Option<&'static str> {
    let has_plus_esm = url.contains("+esm");
    // Strip query string and fragment
    let clean = url.split(['?', '#']).next().unwrap_or(url);
    let raw = clean
        .strip_prefix("https://")
        .or_else(|| clean.strip_prefix("http://"))
        .or_else(|| clean.strip_prefix("//"))
        .unwrap_or(clean);

    let (rest, is_esm_sh, is_cdnjs) = if let Some(r) = raw.strip_prefix("cdn.jsdelivr.net/npm/") {
        (r, false, false)
    } else if let Some(r) = raw.strip_prefix("unpkg.com/") {
        (r, false, false)
    } else if let Some(r) = raw.strip_prefix("cdnjs.cloudflare.com/ajax/libs/") {
        (r, false, true)
    } else {
        (raw.strip_prefix("esm.sh/")?, true, false)
    };

    let (pkg_name, version, path) = if is_cdnjs {
        let mut parts = rest.splitn(3, '/');
        let p = parts.next().unwrap_or("");
        let v = parts.next().unwrap_or("");
        let path = parts.next().unwrap_or("");
        (p, v, path)
    } else {
        let (pkg_and_ver, path) = if rest.starts_with('@') {
            if let Some(first_slash) = rest.find('/') {
                let after_scope = &rest[first_slash + 1..];
                if let Some(second_slash) = after_scope.find('/') {
                    (
                        &rest[..first_slash + 1 + second_slash],
                        &after_scope[second_slash + 1..],
                    )
                } else {
                    (rest, "")
                }
            } else {
                (rest, "")
            }
        } else if let Some(slash) = rest.find('/') {
            (&rest[..slash], &rest[slash + 1..])
        } else {
            (rest, "")
        };

        let (pkg, ver) = if let Some(last_at) = pkg_and_ver.rfind('@') {
            if last_at > 0 {
                (&pkg_and_ver[..last_at], &pkg_and_ver[last_at + 1..])
            } else {
                (pkg_and_ver, "")
            }
        } else {
            (pkg_and_ver, "")
        };
        (pkg, ver, path)
    };

    let package = match pkg_name.to_ascii_lowercase().as_str() {
        "mermaid" => Package::Mermaid,
        "katex" => Package::Katex,
        "highlight.js" | "highlightjs" | "@highlightjs/cdn-assets" => Package::HighlightJs,
        "chart.js" => Package::ChartJs,
        _ => return None,
    };

    let bundled_version = match package {
        Package::Mermaid => MERMAID_VERSION,
        Package::Katex => KATEX_VERSION,
        Package::HighlightJs => HIGHLIGHT_JS_VERSION,
        Package::ChartJs => CHART_JS_VERSION,
    };

    if !is_version_compatible(version, bundled_version) {
        return None;
    }

    let is_mod = as_module || is_esm_sh || path.ends_with(".mjs") || has_plus_esm;
    let clean_path = path.trim_matches('/');

    match package {
        Package::Mermaid => {
            if clean_path.is_empty()
                || clean_path == "+esm"
                || clean_path.ends_with(".js")
                || clean_path.ends_with(".mjs")
            {
                if is_mod {
                    Some("_herbarium/lib/mermaid.mjs")
                } else {
                    Some("_herbarium/lib/mermaid.js")
                }
            } else {
                None
            }
        }
        Package::Katex => {
            if clean_path.contains("fonts")
                || clean_path.ends_with(".woff2")
                || clean_path.ends_with(".woff")
                || clean_path.ends_with(".ttf")
            {
                None
            } else if clean_path.contains("auto-render") {
                if is_mod {
                    Some("_herbarium/lib/katex-auto-render.mjs")
                } else {
                    Some("_herbarium/lib/katex-auto-render.js")
                }
            } else if clean_path.ends_with(".css") {
                Some("_herbarium/lib/katex.css")
            } else if clean_path.is_empty()
                || clean_path == "+esm"
                || clean_path.ends_with(".js")
                || clean_path.ends_with(".mjs")
            {
                if is_mod {
                    Some("_herbarium/lib/katex.mjs")
                } else {
                    Some("_herbarium/lib/katex.js")
                }
            } else {
                None
            }
        }
        Package::HighlightJs => {
            if clean_path.contains("languages") || clean_path.contains("styles") {
                None
            } else if is_highlight_match(clean_path) {
                if is_mod {
                    Some("_herbarium/lib/highlight.mjs")
                } else {
                    Some("_herbarium/lib/highlight.js")
                }
            } else {
                None
            }
        }
        Package::ChartJs => {
            if !is_mod && is_chart_umd_match(clean_path) {
                Some("_herbarium/lib/chart.js")
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtual_modules_end_with_export_default() {
        let modules = [
            ("mermaid.mjs", "export default mermaid;\n"),
            ("katex.mjs", "export default katex;\n"),
            (
                "katex-auto-render.mjs",
                "export default renderMathInElement;\n",
            ),
            ("highlight.mjs", "export default hljs;\n"),
        ];

        for (file, expected_suffix) in modules {
            let lib = get(file).unwrap_or_else(|| panic!("failed to get {file}"));
            assert_eq!(lib.mime, "text/javascript; charset=utf-8");
            let text = std::str::from_utf8(&lib.bytes).expect("valid utf-8");
            assert!(
                text.ends_with(expected_suffix),
                "{file} should end with {expected_suffix:?}, got tail: {:?}",
                &text[text.len().saturating_sub(60)..]
            );
        }
    }

    #[test]
    fn test_get_regular_files() {
        assert_eq!(get("katex.css").unwrap().mime, "text/css; charset=utf-8");
        assert_eq!(
            get("highlight.css").unwrap().mime,
            "text/css; charset=utf-8"
        );
        assert_eq!(
            get("mermaid.js").unwrap().mime,
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            get("katex.js").unwrap().mime,
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            get("LICENSES.md").unwrap().mime,
            "text/markdown; charset=utf-8"
        );

        // Paths with directory separators or unknown names must return None
        assert!(get("../lib/mermaid.js").is_none());
        assert!(get("sub/mermaid.js").is_none());
        assert!(get("nonexistent.js").is_none());
    }

    #[test]
    fn test_names() {
        let pkgs = names();
        assert_eq!(pkgs.len(), 4);
        assert_eq!(pkgs[0], ("chart.js", CHART_JS_VERSION));
        assert_eq!(pkgs[1], ("highlight.js", HIGHLIGHT_JS_VERSION));
        assert_eq!(pkgs[2], ("katex", KATEX_VERSION));
        assert_eq!(pkgs[3], ("mermaid", MERMAID_VERSION));
    }

    #[test]
    fn test_map_cdn_url_all_cdn_forms() {
        // jsDelivr
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/mermaid@11.17.2/dist/mermaid.min.js",
                false
            ),
            Some("_herbarium/lib/mermaid.js")
        );
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs",
                false
            ),
            Some("_herbarium/lib/mermaid.mjs")
        );
        assert_eq!(
            map_cdn_url("https://cdn.jsdelivr.net/npm/mermaid@11/+esm", false),
            Some("_herbarium/lib/mermaid.mjs")
        );
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/@highlightjs/cdn-assets@11.12.0/highlight.min.js",
                false
            ),
            Some("_herbarium/lib/highlight.js")
        );
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/chart.js@4.6.1/dist/chart.umd.js",
                false
            ),
            Some("_herbarium/lib/chart.js")
        );
        assert_eq!(
            map_cdn_url("https://cdn.jsdelivr.net/npm/chart.js@4/auto", false),
            None
        );

        // unpkg
        assert_eq!(
            map_cdn_url("https://unpkg.com/katex@0.16.47/dist/katex.min.js", false),
            Some("_herbarium/lib/katex.js")
        );
        assert_eq!(
            map_cdn_url(
                "https://unpkg.com/katex@0.16.47/dist/contrib/auto-render.min.js",
                false
            ),
            Some("_herbarium/lib/katex-auto-render.js")
        );
        assert_eq!(
            map_cdn_url("https://unpkg.com/katex@0.16.47/dist/katex.min.css", false),
            Some("_herbarium/lib/katex.css")
        );

        // cdnjs (case-insensitive names)
        assert_eq!(
            map_cdn_url(
                "https://cdnjs.cloudflare.com/ajax/libs/KaTeX/0.16.47/katex.min.css",
                false
            ),
            Some("_herbarium/lib/katex.css")
        );
        assert_eq!(
            map_cdn_url(
                "https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.6.1/chart.umd.min.js",
                false
            ),
            Some("_herbarium/lib/chart.js")
        );
        assert_eq!(
            map_cdn_url(
                "https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.12.0/highlight.min.js",
                false
            ),
            Some("_herbarium/lib/highlight.js")
        );
        assert_eq!(
            map_cdn_url(
                "https://cdnjs.cloudflare.com/ajax/libs/mermaid/11.17.2/mermaid.min.js",
                false
            ),
            Some("_herbarium/lib/mermaid.js")
        );

        // esm.sh (always module)
        assert_eq!(
            map_cdn_url("https://esm.sh/mermaid@11.17.2", false),
            Some("_herbarium/lib/mermaid.mjs")
        );
        assert_eq!(
            map_cdn_url("https://esm.sh/highlight.js@11/lib/common", false),
            Some("_herbarium/lib/highlight.mjs")
        );
        assert_eq!(map_cdn_url("https://esm.sh/chart.js@4", false), None);
    }

    #[test]
    fn test_map_cdn_url_as_module_switch() {
        let url = "https://cdnjs.cloudflare.com/ajax/libs/mermaid/11.17.2/mermaid.min.js";
        assert_eq!(map_cdn_url(url, false), Some("_herbarium/lib/mermaid.js"));
        assert_eq!(map_cdn_url(url, true), Some("_herbarium/lib/mermaid.mjs"));

        let katex_url = "https://cdnjs.cloudflare.com/ajax/libs/KaTeX/0.16.47/katex.min.js";
        assert_eq!(
            map_cdn_url(katex_url, false),
            Some("_herbarium/lib/katex.js")
        );
        assert_eq!(
            map_cdn_url(katex_url, true),
            Some("_herbarium/lib/katex.mjs")
        );

        let hljs_url =
            "https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.12.0/highlight.min.js";
        assert_eq!(
            map_cdn_url(hljs_url, false),
            Some("_herbarium/lib/highlight.js")
        );
        assert_eq!(
            map_cdn_url(hljs_url, true),
            Some("_herbarium/lib/highlight.mjs")
        );

        let chart_url = "https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.6.1/chart.umd.min.js";
        assert_eq!(
            map_cdn_url(chart_url, false),
            Some("_herbarium/lib/chart.js")
        );
        assert_eq!(map_cdn_url(chart_url, true), None);
    }

    #[test]
    fn test_map_cdn_url_incompatible_major() {
        // mermaid 10.x vs 11.x
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/mermaid@10.9.0/dist/mermaid.min.js",
                false
            ),
            None
        );
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/mermaid@12.0.0/dist/mermaid.min.js",
                false
            ),
            None
        );

        // katex 0.15.x or 0.17.x vs 0.16.x
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/katex@0.15.0/dist/katex.min.js",
                false
            ),
            None
        );
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/katex@0.17.0/dist/katex.min.js",
                false
            ),
            None
        );
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/katex@1.0.0/dist/katex.min.js",
                false
            ),
            None
        );

        // chart.js 3.x vs 4.x
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/chart.js@3.9.0/dist/chart.min.js",
                false
            ),
            None
        );

        // highlight.js 10.x vs 11.x
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/highlight.js@10.7.3/lib/common.js",
                false
            ),
            None
        );
    }

    #[test]
    fn test_map_cdn_url_unknown_package_or_domain() {
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/lodash@4.17.21/lodash.min.js",
                false
            ),
            None
        );
        assert_eq!(
            map_cdn_url(
                "https://cdnjs.cloudflare.com/ajax/libs/jquery/3.6.0/jquery.min.js",
                false
            ),
            None
        );
        assert_eq!(map_cdn_url("https://esm.sh/react@18.2.0", false), None);
        assert_eq!(map_cdn_url("https://example.com/mermaid.js", false), None);
    }

    #[test]
    fn test_map_cdn_url_katex_fonts_and_highlight_styles_dropped() {
        // KaTeX fonts must return None
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/katex@0.16.47/dist/fonts/KaTeX_AMS-Regular.woff2",
                false
            ),
            None
        );
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/katex@0.16.47/dist/fonts/KaTeX_Main-Regular.ttf",
                false
            ),
            None
        );

        // highlight.js styles and languages must return None
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/highlight.js@11.12.0/styles/github.css",
                false
            ),
            None
        );
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/highlight.js@11.12.0/languages/rust.min.js",
                false
            ),
            None
        );
    }

    #[test]
    fn test_map_cdn_url_query_and_fragment_ignored() {
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/mermaid@11.17.2/dist/mermaid.min.js?v=1#hash",
                false
            ),
            Some("_herbarium/lib/mermaid.js")
        );
    }

    #[test]
    fn test_katex_auto_render_mjs_bundles_katex() {
        let lib = get("katex-auto-render.mjs").expect("katex-auto-render.mjs should exist");
        assert_eq!(lib.mime, "text/javascript; charset=utf-8");
        let text = std::str::from_utf8(&lib.bytes).expect("valid utf-8");

        let katex_text = std::str::from_utf8(KATEX_JS).expect("valid utf-8");
        let auto_text = std::str::from_utf8(KATEX_AUTO_RENDER_JS).expect("valid utf-8");

        let katex_pos = text.find(katex_text).expect("contains katex bundle");
        let auto_pos = text.find(auto_text).expect("contains auto-render bundle");

        assert!(
            katex_pos < auto_pos,
            "katex bundle must precede auto-render bundle"
        );
        assert!(text.ends_with("\n;export default renderMathInElement;\n"));
    }

    #[test]
    fn test_chart_module_mapping_returns_none() {
        // get("chart.mjs") returns None
        assert!(get("chart.mjs").is_none());

        // as_module true returns None for any chart URL
        let umd_jsdelivr = "https://cdn.jsdelivr.net/npm/chart.js@4.6.1/dist/chart.umd.js";
        assert_eq!(map_cdn_url(umd_jsdelivr, true), None);

        let umd_cdnjs = "https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.6.1/chart.umd.min.js";
        assert_eq!(map_cdn_url(umd_cdnjs, true), None);

        // .mjs path
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/chart.js@4.6.1/dist/chart.mjs",
                false
            ),
            None
        );
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/chart.js@4.6.1/auto.mjs",
                false
            ),
            None
        );

        // +esm
        assert_eq!(
            map_cdn_url("https://cdn.jsdelivr.net/npm/chart.js@4.6.1/+esm", false),
            None
        );
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/chart.js@4.6.1/dist/chart.umd.js/+esm",
                false
            ),
            None
        );

        // esm.sh
        assert_eq!(map_cdn_url("https://esm.sh/chart.js@4.6.1", false), None);
        assert_eq!(map_cdn_url("https://esm.sh/chart.js@4", false), None);
        assert_eq!(
            map_cdn_url("https://esm.sh/chart.js@4.6.1/dist/chart.umd.js", false),
            None
        );

        // dist/chart.js ESM build
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/chart.js@4.6.1/dist/chart.js",
                false
            ),
            None
        );

        // auto variants
        assert_eq!(
            map_cdn_url("https://cdn.jsdelivr.net/npm/chart.js@4/auto", false),
            None
        );
        assert_eq!(
            map_cdn_url("https://cdn.jsdelivr.net/npm/chart.js@4.6.1/auto.js", false),
            None
        );
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/chart.js@4.6.1/dist/chart.auto.js",
                false
            ),
            None
        );
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/chart.js@4.6.1/auto/auto.js",
                false
            ),
            None
        );
    }

    #[test]
    fn test_chart_classic_mapping_still_works() {
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/chart.js@4.6.1/dist/chart.umd.js",
                false
            ),
            Some("_herbarium/lib/chart.js")
        );
        assert_eq!(
            map_cdn_url(
                "https://cdn.jsdelivr.net/npm/chart.js@4.6.1/dist/chart.umd.min.js",
                false
            ),
            Some("_herbarium/lib/chart.js")
        );
        assert_eq!(
            map_cdn_url(
                "https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.6.1/chart.umd.min.js",
                false
            ),
            Some("_herbarium/lib/chart.js")
        );
        assert_eq!(
            map_cdn_url(
                "https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.6.1/chart.min.js",
                false
            ),
            Some("_herbarium/lib/chart.js")
        );
        assert_eq!(
            map_cdn_url("https://unpkg.com/chart.js@4.6.1/dist/chart.umd.js", false),
            Some("_herbarium/lib/chart.js")
        );
        assert_eq!(
            map_cdn_url("https://unpkg.com/chart.js@4.6.1/dist/chart.min.js", false),
            Some("_herbarium/lib/chart.js")
        );
    }
}
