// Title and plain-text extraction from HTML documents.
use scraper::{Html, Selector};

use crate::models::PageMeta;

const MAX_TEXT: usize = 1_000_000;
const SNIFF_CHARS: usize = 64 * 1024;

/// Everything the indexer needs to know about a page's HTML, derived from a
/// single parse. The index/create/set_html paths build one of these instead of
/// re-parsing the document for title, text, links and sniffing separately.
pub struct Analysis {
    pub title: String,
    pub text: String,
    pub links: Vec<String>,
    pub looks_like_html: bool,
}

/// Parse `html` once and derive the title, plain text, outgoing page links and
/// the HTML content-sniff verdict.
pub fn analyze(html: &str) -> Analysis {
    let has_nul = first_n_chars(html, SNIFF_CHARS).contains('\0');
    let doc = Html::parse_document(html);
    Analysis {
        title: title_from_doc(&doc),
        text: text_from_doc(&doc),
        links: links_from_doc(&doc),
        looks_like_html: !has_nul && has_real_element(&doc),
    }
}

/// Resolve the title after the document changes. This is the `set_html`
/// semantics shared by rescan, create and set_html: the title follows the
/// document unless the user renamed it themselves (source title differs from
/// the current title) — and it never becomes empty.
pub fn follow_document_title(meta: &PageMeta, extracted: &str) -> String {
    if extracted.is_empty() {
        return meta.title.clone();
    }
    let user_never_renamed = meta.source_title.as_deref() == Some(meta.title.as_str());
    if user_never_renamed || meta.title.is_empty() {
        extracted.to_string()
    } else {
        meta.title.clone()
    }
}

/// Extract the `<title>` element; falls back to the first `<h1>`.
pub fn extract_title(html: &str) -> String {
    title_from_doc(&Html::parse_document(html))
}

/// Strip markup and return the visible text of the page, whitespace-normalized.
pub fn extract_text(html: &str) -> String {
    text_from_doc(&Html::parse_document(html))
}

/// Content sniff so a renamed non-HTML file (image, PDF, plain text…) is not
/// imported just because it ends in `.html`. html5ever accepts any input, so
/// "parses" proves nothing; instead require a real element in `<head>`/`<body>`.
pub fn looks_like_html(content: &str) -> bool {
    if first_n_chars(content, SNIFF_CHARS).contains('\0') {
        return false;
    }
    let doc = Html::parse_document(content);
    has_real_element(&doc)
}

/// Link prefix that opens a page in Herbarium; pages link to each other with it.
pub const PAGE_LINK_PREFIX: &str = "herbarium-app://open/";

/// Ids of the pages `html` links to with `herbarium-app://open/<id>` (`<a href>`
/// only), deduplicated in document order. Anything after the id (`?`, `#`, `/`)
/// is ignored.
pub fn extract_page_links(html: &str) -> Vec<String> {
    links_from_doc(&Html::parse_document(html))
}

fn title_from_doc(doc: &Html) -> String {
    if let Ok(sel) = Selector::parse("title")
        && let Some(el) = doc.select(&sel).next()
    {
        let t = el.text().collect::<String>().trim().to_string();
        if !t.is_empty() {
            return t;
        }
    }
    if let Ok(sel) = Selector::parse("h1")
        && let Some(el) = doc.select(&sel).next()
    {
        let t = el.text().collect::<String>().trim().to_string();
        if !t.is_empty() {
            return t;
        }
    }
    String::new()
}

fn text_from_doc(doc: &Html) -> String {
    let raw = if let Ok(sel) = Selector::parse("body") {
        match doc.select(&sel).next() {
            Some(body) => body.text().collect::<Vec<_>>().join(" "),
            None => doc.root_element().text().collect::<Vec<_>>().join(" "),
        }
    } else {
        doc.root_element().text().collect::<Vec<_>>().join(" ")
    };
    let cleaned: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    cleaned.chars().take(MAX_TEXT).collect()
}

fn links_from_doc(doc: &Html) -> Vec<String> {
    let Ok(sel) = Selector::parse("a[href]") else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for el in doc.select(&sel) {
        let Some(href) = el.value().attr("href") else {
            continue;
        };
        let href = href.trim();
        let Some(rest) = href
            .get(..PAGE_LINK_PREFIX.len())
            .filter(|p| p.eq_ignore_ascii_case(PAGE_LINK_PREFIX))
            .map(|_| &href[PAGE_LINK_PREFIX.len()..])
        else {
            continue;
        };
        let id: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            .collect();
        if !id.is_empty() && !id.starts_with('.') && !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

fn has_real_element(doc: &Html) -> bool {
    let sel = Selector::parse("head *, body *").expect("static selector");
    doc.select(&sel).next().is_some()
}

/// Borrow the first `n` chars of `content` without allocating: the byte offset
/// of the `n`-th char (or the whole string when it is shorter) is a valid UTF-8
/// boundary.
fn first_n_chars(content: &str, n: usize) -> &str {
    match content.char_indices().nth(n) {
        Some((idx, _)) => &content[..idx],
        None => content,
    }
}

#[cfg(test)]
mod html_sniff_tests {
    use super::looks_like_html;

    #[test]
    fn accepts_html() {
        assert!(looks_like_html(
            "<!DOCTYPE html><html><body><p>x</p></body></html>"
        ));
        assert!(looks_like_html("\u{feff}  <div class=\"a\">hi</div>"));
    }

    #[test]
    fn rejects_non_html() {
        assert!(!looks_like_html("just some notes"));
        assert!(!looks_like_html("%PDF-1.7\0\0binary"));
        assert!(!looks_like_html("a < b and c > d"));
        assert!(!looks_like_html(""));
    }
}
