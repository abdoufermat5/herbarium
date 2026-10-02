// Title and plain-text extraction from HTML documents.
use scraper::{Html, Selector};

const MAX_TEXT: usize = 1_000_000;

/// Extract the `<title>` element; falls back to the first `<h1>`.
pub fn extract_title(html: &str) -> String {
    let doc = Html::parse_document(html);
    if let Ok(sel) = Selector::parse("title") {
        if let Some(el) = doc.select(&sel).next() {
            let t = el.text().collect::<String>().trim().to_string();
            if !t.is_empty() {
                return t;
            }
        }
    }
    if let Ok(sel) = Selector::parse("h1") {
        if let Some(el) = doc.select(&sel).next() {
            let t = el.text().collect::<String>().trim().to_string();
            if !t.is_empty() {
                return t;
            }
        }
    }
    String::new()
}

/// Strip markup and return the visible text of the page, whitespace-normalized.
pub fn extract_text(html: &str) -> String {
    let doc = Html::parse_document(html);
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

/// Content sniff so a renamed non-HTML file (image, PDF, plain text…) is not
/// imported just because it ends in `.html`. html5ever accepts any input, so
/// "parses" proves nothing; instead require a real element in `<head>`/`<body>`.
pub fn looks_like_html(content: &str) -> bool {
    let head: String = content.chars().take(64 * 1024).collect();
    if head.contains('\0') {
        return false;
    }
    let doc = Html::parse_document(&head);
    let sel = Selector::parse("head *, body *").expect("static selector");
    doc.select(&sel).next().is_some()
}

#[cfg(test)]
mod html_sniff_tests {
    use super::looks_like_html;

    #[test]
    fn accepts_html() {
        assert!(looks_like_html("<!DOCTYPE html><html><body><p>x</p></body></html>"));
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
