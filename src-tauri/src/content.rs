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