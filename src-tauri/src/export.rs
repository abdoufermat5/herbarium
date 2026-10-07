// Exports that leave the app: one page as a self-contained HTML file, and a
// set of pages as a static website. Local assets (images, scripts, styles,
// media next to the page) are inlined as `data:` URIs so a file works on its
// own; links between exported pages point at each other's files.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use herbarium_core::assets::inline_local_assets;
use herbarium_core::content::PAGE_LINK_PREFIX;
use herbarium_core::rewrite::rewrite_attrs;
use herbarium_core::vault;
use serde::Serialize;

/// One page to export, read while the vault was locked.
pub struct ExportPage {
    pub id: String,
    pub title: String,
    pub folder: Option<String>,
    pub tags: Vec<String>,
    pub html: String,
}

/// The page's HTML with its local assets inlined (see
/// [`herbarium_core::assets::inline_local_assets`]).
pub fn standalone(vault_dir: &Path, page: &ExportPage) -> String {
    inline_local_assets(vault_dir, &page.id, page.folder.as_deref(), &page.html)
}

/// Point `herbarium-app://open/<id>` links at `<id>.html` for exported pages.
fn link_exported(html: &str, exported: &HashSet<String>) -> String {
    rewrite_attrs(html, |tag, attr, value| {
        if tag != "a" || attr != "href" {
            return None;
        }
        let rest = value
            .get(..PAGE_LINK_PREFIX.len())
            .filter(|p| p.eq_ignore_ascii_case(PAGE_LINK_PREFIX))
            .map(|_| &value[PAGE_LINK_PREFIX.len()..])?;
        let end = rest.find(['?', '#', '/']).unwrap_or(rest.len());
        let (id, tail) = rest.split_at(end);
        let fragment = tail.find('#').map(|i| &tail[i..]).unwrap_or("");
        exported
            .contains(id)
            .then(|| format!("{id}.html{fragment}"))
    })
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The site's front page: every page by folder, with its tags.
fn index_html(title: &str, pages: &[ExportPage]) -> String {
    let mut by_folder: BTreeMap<String, Vec<&ExportPage>> = BTreeMap::new();
    for page in pages {
        by_folder
            .entry(page.folder.clone().unwrap_or_default())
            .or_default()
            .push(page);
    }
    let mut body = String::new();
    for (folder, mut list) in by_folder {
        list.sort_by_key(|p| p.title.to_lowercase());
        if !folder.is_empty() {
            body.push_str(&format!("<h2>{}</h2>\n", escape(&folder)));
        }
        body.push_str("<ul>\n");
        for page in list {
            let tags: String = page
                .tags
                .iter()
                .map(|t| format!(" <span class=\"tag\">{}</span>", escape(t)))
                .collect();
            body.push_str(&format!(
                "<li><a href=\"{}.html\">{}</a>{tags}</li>\n",
                escape(&page.id),
                escape(&page.title)
            ));
        }
        body.push_str("</ul>\n");
    }
    format!(
        "<!doctype html>
<html lang=\"en\">
<head>
<meta charset=\"utf-8\">
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<title>{title}</title>
<style>
:root {{ color-scheme: light dark; --text: #2f3437; --muted: #6b6a68; --bg: #fff; --accent: #346538; --chip: #f1f0ec; }}
@media (prefers-color-scheme: dark) {{ :root {{ --text: #e6e5e3; --muted: #9b9a97; --bg: #202020; --accent: #8cbf91; --chip: #2a2a29; }} }}
body {{ margin: 0 auto; max-width: 720px; padding: 48px 16px; font: 16px/1.6 system-ui, sans-serif; color: var(--text); background: var(--bg); }}
h1 {{ font-size: 28px; margin: 0 0 4px; }}
.count {{ color: var(--muted); margin: 0 0 32px; }}
h2 {{ font-size: 14px; text-transform: uppercase; letter-spacing: .06em; color: var(--muted); margin: 32px 0 8px; }}
ul {{ list-style: none; padding: 0; margin: 0; }}
li {{ padding: 6px 0; }}
a {{ color: var(--accent); }}
.tag {{ font-size: 12px; background: var(--chip); color: var(--muted); border-radius: 4px; padding: 1px 6px; margin-left: 4px; }}
</style>
</head>
<body>
<h1>{title}</h1>
<p class=\"count\">{count} pages</p>
{body}</body>
</html>
",
        title = escape(title),
        count = pages.len(),
    )
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteReport {
    pub pages: usize,
    pub index: String,
}

/// Write `pages` as a static site into `dest`, which must be missing or an
/// empty directory (nothing of the user's is ever overwritten).
pub fn write_site(
    vault_dir: &Path,
    title: &str,
    pages: &[ExportPage],
    dest: &Path,
) -> Result<SiteReport, String> {
    if dest.exists() {
        let mut entries =
            std::fs::read_dir(dest).map_err(|e| format!("cannot read {}: {e}", dest.display()))?;
        if entries.next().is_some() {
            return Err(format!(
                "{} is not empty; choose a new or empty folder",
                dest.display()
            ));
        }
    } else {
        std::fs::create_dir_all(dest)
            .map_err(|e| format!("cannot create {}: {e}", dest.display()))?;
    }
    let exported: HashSet<String> = pages.iter().map(|p| p.id.clone()).collect();
    for page in pages {
        let html = link_exported(&standalone(vault_dir, page), &exported);
        vault::write_atomic(&dest.join(format!("{}.html", page.id)), html.as_bytes())?;
    }
    let index: PathBuf = dest.join("index.html");
    // A page with the id `index` keeps its file; the front page moves aside.
    let index = if exported.contains("index") {
        dest.join("contents.html")
    } else {
        index
    };
    vault::write_atomic(&index, index_html(title, pages).as_bytes())?;
    Ok(SiteReport {
        pages: pages.len(),
        index: index.to_string_lossy().into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("herbarium-export-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn page(id: &str, folder: Option<&str>, html: &str) -> ExportPage {
        ExportPage {
            id: id.into(),
            title: format!("Title <{id}>"),
            folder: folder.map(str::to_string),
            tags: vec!["t&t".into()],
            html: html.into(),
        }
    }

    #[test]
    fn standalone_inlines_local_assets_only() {
        let vault = temp("inline");
        std::fs::create_dir_all(vault.join("notes/img")).unwrap();
        std::fs::write(vault.join("notes/img/dot.png"), b"PNG").unwrap();
        std::fs::write(vault.join("notes/app.js"), b"go()").unwrap();
        std::fs::write(vault.join("secret.txt"), b"no").unwrap();
        let p = page(
            "p",
            Some("notes"),
            r#"<img src="img/dot.png"><script src="./app.js"></script><img src="../secret.txt"><img src="https://x/y.png"><img src="missing.png">"#,
        );
        let out = standalone(&vault, &p);
        assert!(
            out.contains(r#"<img src="data:image/png;base64,UE5H">"#),
            "{out}"
        );
        assert!(
            out.contains(r#"<script src="data:text/javascript;charset=utf-8;base64,Z28oKQ==">"#),
            "{out}"
        );
        assert!(
            out.contains(r#"src="../secret.txt""#),
            "nothing outside the page folder"
        );
        assert!(out.contains(r#"src="https://x/y.png""#) && out.contains(r#"src="missing.png""#));
        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn site_links_pages_and_refuses_a_non_empty_folder() {
        let vault = temp("site-vault");
        let dest = temp("site-dest").join("out");
        let pages = vec![
            page(
                "a",
                None,
                r##"<a href="herbarium-app://open/b#part">b</a><a href="herbarium-app://open/gone">x</a>"##,
            ),
            page("b", Some("f"), "<p>b</p>"),
        ];
        let report = write_site(&vault, "My <site>", &pages, &dest).unwrap();
        assert_eq!(report.pages, 2);
        let a = std::fs::read_to_string(dest.join("a.html")).unwrap();
        assert!(a.contains(r##"href="b.html#part""##), "{a}");
        assert!(
            a.contains("herbarium-app://open/gone"),
            "links to pages left out stay as they were"
        );
        let index = std::fs::read_to_string(dest.join("index.html")).unwrap();
        assert!(
            index.contains(r#"<a href="b.html">Title &lt;b&gt;</a>"#),
            "{index}"
        );
        assert!(index.contains("My &lt;site&gt;") && index.contains("t&amp;t"));

        let err = write_site(&vault, "again", &pages, &dest).unwrap_err();
        assert!(err.contains("not empty"), "{err}");
        let _ = std::fs::remove_dir_all(&vault);
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }
}
