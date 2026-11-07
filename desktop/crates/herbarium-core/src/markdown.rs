// Offline Markdown and Mermaid rendering to static HTML documents.
// Converts CommonMark, GitHub Flavored Markdown (tables, task lists, strikethrough,
// footnotes, heading attributes), LaTeX math, Mermaid diagrams, and diagram fences
// into self-contained HTML pages that work offline using local `_herbarium/` assets.

use pulldown_cmark::{CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd, html};

/// Escape characters reserved in HTML: `&`, `<`, `>`, `"`, and `'`.
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

const STYLESHEET: &str = r#":root {
  --bg: #ffffff;
  --fg: #1f2328;
  --muted: #656d76;
  --border: #d0d7de;
  --code-bg: #f6f8fa;
  --link: #0969da;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #0d1117;
    --fg: #e6edf3;
    --muted: #848d97;
    --border: #30363d;
    --code-bg: #161b22;
    --link: #2f81f7;
  }
}
html {
  color-scheme: light dark;
}
body {
  box-sizing: border-box;
  margin: 0;
  padding: 2rem 1rem;
  background: var(--bg);
  color: var(--fg);
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  line-height: 1.6;
}
.markdown {
  max-width: 72ch;
  margin: 0 auto;
  word-break: break-word;
}
.markdown > :first-child {
  margin-top: 0;
}
h1, h2, h3, h4, h5, h6 {
  line-height: 1.25;
  margin-top: 1.5em;
  margin-bottom: 0.5em;
  font-weight: 600;
}
p, ul, ol, dl, table, blockquote, pre, figure {
  margin-top: 0;
  margin-bottom: 1em;
}
a {
  color: var(--link);
  text-decoration: none;
}
a:hover {
  text-decoration: underline;
}
code, kbd, samp, pre {
  font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace;
  font-size: 0.9em;
}
code {
  background: var(--code-bg);
  padding: 0.2em 0.4em;
  border-radius: 6px;
}
pre {
  background: var(--code-bg);
  padding: 1em;
  border-radius: 6px;
  overflow-x: auto;
  line-height: 1.45;
}
pre code {
  background: transparent;
  padding: 0;
  border-radius: 0;
  font-size: inherit;
}
blockquote {
  margin: 1em 0;
  padding: 0 1em;
  color: var(--muted);
  border-left: 0.25em solid var(--border);
}
table {
  border-collapse: collapse;
  width: 100%;
  margin: 1em 0;
  display: block;
  overflow-x: auto;
}
th, td {
  border: 1px solid var(--border);
  padding: 6px 13px;
}
th {
  background: var(--code-bg);
  font-weight: 600;
}
hr {
  height: 0.25em;
  padding: 0;
  margin: 24px 0;
  background-color: var(--border);
  border: 0;
}
img {
  max-width: 100%;
  height: auto;
}
figure.diagram {
  margin: 1.5em 0;
  text-align: center;
}
figure.diagram img {
  max-width: 100%;
  height: auto;
}
pre.mermaid {
  background: transparent;
  text-align: center;
  border: none;
  padding: 0;
  display: flex;
  justify-content: center;
  align-items: center;
}
.math-inline {
  display: inline;
}
.math-display {
  overflow-x: auto;
  margin: 1em 0;
  text-align: center;
}
ul.contains-task-list {
  list-style-type: none;
  padding-left: 0;
}
li.task-list-item {
  list-style-type: none;
}
li.task-list-item input {
  margin: 0 0.35em 0.25em -1.4em;
  vertical-align: middle;
}
body.mermaid-body {
  min-height: 100vh;
  display: grid;
  place-items: center;
  margin: 0;
  padding: 1rem;
  box-sizing: border-box;
}"#;

const MERMAID_SCRIPT: &str = r#"<script src="_herbarium/lib/mermaid.js"></script><script>mermaid.initialize({startOnLoad:true,theme:matchMedia('(prefers-color-scheme: dark)').matches?'dark':'default'})</script>"#;

const KATEX_CSS: &str = r#"<link rel="stylesheet" href="_herbarium/lib/katex.css">"#;
const KATEX_SCRIPT: &str = r#"<script src="_herbarium/lib/katex.js"></script><script src="_herbarium/lib/katex-auto-render.js"></script><script>document.addEventListener('DOMContentLoaded',function(){renderMathInElement(document.body,{delimiters:[{left:'\\[',right:'\\]',display:true},{left:'\\(',right:'\\)',display:false}]})});</script>"#;

const HIGHLIGHT_CSS: &str = r#"<link rel="stylesheet" href="_herbarium/lib/highlight.css">"#;
const HIGHLIGHT_SCRIPT: &str =
    r#"<script src="_herbarium/lib/highlight.js"></script><script>hljs.highlightAll()</script>"#;

fn process_code_block(
    kind: CodeBlockKind,
    source: String,
    has_mermaid: &mut bool,
    has_highlight: &mut bool,
) -> Event<'static> {
    match kind {
        CodeBlockKind::Fenced(info) => {
            let info = info.trim();
            let lang = info.split_whitespace().next().unwrap_or("");
            let lang_lower = lang.to_ascii_lowercase();
            if lang_lower == "mermaid" {
                *has_mermaid = true;
                let escaped = escape_html(&source);
                Event::Html(CowStr::from(format!(
                    r#"<pre class="mermaid">{escaped}</pre>"#
                )))
            } else if let Some(render_kind) = crate::reserved::render_kind(&lang_lower) {
                if let Some(src) = crate::reserved::render_src(render_kind, &source) {
                    let figure = format!(
                        r#"<figure class="diagram"><img alt="{render_kind} diagram" src="{src}"></figure>"#,
                        render_kind = escape_html(render_kind),
                        src = escape_html(&src),
                    );
                    Event::Html(CowStr::from(figure))
                } else {
                    // Fall back to a code block when render_src returns None
                    let escaped_source = escape_html(&source);
                    let html = if lang.is_empty() {
                        format!("<pre><code>{escaped_source}</code></pre>")
                    } else {
                        *has_highlight = true;
                        let escaped_lang = escape_html(lang);
                        format!(
                            r#"<pre><code class="language-{escaped_lang}">{escaped_source}</code></pre>"#
                        )
                    };
                    Event::Html(CowStr::from(html))
                }
            } else {
                let escaped_source = escape_html(&source);
                let html = if lang.is_empty() {
                    format!("<pre><code>{escaped_source}</code></pre>")
                } else {
                    *has_highlight = true;
                    let escaped_lang = escape_html(lang);
                    format!(
                        r#"<pre><code class="language-{escaped_lang}">{escaped_source}</code></pre>"#
                    )
                };
                Event::Html(CowStr::from(html))
            }
        }
        CodeBlockKind::Indented => {
            let escaped_source = escape_html(&source);
            Event::Html(CowStr::from(format!(
                "<pre><code>{escaped_source}</code></pre>"
            )))
        }
    }
}

/// Render a Markdown document as a complete, self-contained HTML page.
pub fn markdown_page(title: &str, markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_MATH);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);

    let mut events = Vec::new();
    let mut in_code_block: Option<(CodeBlockKind, String)> = None;
    let mut first_heading = String::new();
    let mut in_first_heading = false;
    let mut found_first_heading = false;
    let mut has_mermaid = false;
    let mut has_math = false;
    let mut has_highlight = false;

    for event in Parser::new_ext(markdown, options) {
        if let Some((_, source)) = &mut in_code_block {
            match event {
                Event::End(TagEnd::CodeBlock) => {
                    let (kind, source) = in_code_block.take().unwrap();
                    let html_event =
                        process_code_block(kind, source, &mut has_mermaid, &mut has_highlight);
                    events.push(html_event);
                }
                Event::Text(t) => {
                    source.push_str(&t);
                }
                _ => {}
            }
            continue;
        }

        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = Some((kind, String::new()));
            }
            Event::Start(Tag::Heading { .. }) => {
                if !found_first_heading {
                    in_first_heading = true;
                }
                events.push(event);
            }
            Event::End(TagEnd::Heading(..)) => {
                if in_first_heading {
                    in_first_heading = false;
                    found_first_heading = true;
                }
                events.push(event);
            }
            Event::Text(t) => {
                if in_first_heading {
                    first_heading.push_str(&t);
                }
                events.push(Event::Text(t));
            }
            Event::Code(c) => {
                if in_first_heading {
                    first_heading.push_str(&c);
                }
                events.push(Event::Code(c));
            }
            Event::InlineMath(content) => {
                has_math = true;
                let escaped = escape_html(&content);
                events.push(Event::Html(CowStr::from(format!(
                    r#"<span class="math-inline">\({escaped}\)</span>"#
                ))));
            }
            Event::DisplayMath(content) => {
                has_math = true;
                let escaped = escape_html(&content);
                events.push(Event::Html(CowStr::from(format!(
                    r#"<div class="math-display">\[{escaped}\]</div>"#
                ))));
            }
            other => {
                events.push(other);
            }
        }
    }

    if let Some((kind, source)) = in_code_block.take() {
        let html_event = process_code_block(kind, source, &mut has_mermaid, &mut has_highlight);
        events.push(html_event);
    }

    let mut body_html = String::new();
    html::push_html(&mut body_html, events.into_iter());

    let effective_title = if !title.trim().is_empty() {
        title.trim()
    } else if !first_heading.trim().is_empty() {
        first_heading.trim()
    } else {
        "Untitled"
    };
    let escaped_title = escape_html(effective_title);

    let mut out = String::with_capacity(body_html.len() + STYLESHEET.len() + 1024);
    out.push_str("<!doctype html>\n<html>\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"color-scheme\" content=\"light dark\">\n");
    out.push_str("<title>");
    out.push_str(&escaped_title);
    out.push_str("</title>\n");
    out.push_str("<style>\n");
    out.push_str(STYLESHEET);
    out.push_str("\n</style>\n");
    if has_math {
        out.push_str(KATEX_CSS);
        out.push('\n');
    }
    if has_highlight {
        out.push_str(HIGHLIGHT_CSS);
        out.push('\n');
    }
    out.push_str("</head>\n<body>\n");
    out.push_str("<article class=\"markdown\">\n");
    out.push_str(&body_html);
    out.push_str("\n</article>\n");
    if has_mermaid {
        out.push_str(MERMAID_SCRIPT);
        out.push('\n');
    }
    if has_math {
        out.push_str(KATEX_SCRIPT);
        out.push('\n');
    }
    if has_highlight {
        out.push_str(HIGHLIGHT_SCRIPT);
        out.push('\n');
    }
    out.push_str("</body>\n</html>");
    out
}

/// Render a standalone Mermaid diagram as a centered, self-contained HTML page.
pub fn mermaid_page(title: &str, source: &str) -> String {
    let title = title.trim();
    let title = if title.is_empty() { "Untitled" } else { title };
    let escaped_title = escape_html(title);
    let escaped_source = escape_html(source);

    let mut out = String::with_capacity(source.len() + STYLESHEET.len() + 512);
    out.push_str("<!doctype html>\n<html>\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"color-scheme\" content=\"light dark\">\n");
    out.push_str("<title>");
    out.push_str(&escaped_title);
    out.push_str("</title>\n");
    out.push_str("<style>\n");
    out.push_str(STYLESHEET);
    out.push_str("\n</style>\n");
    out.push_str("</head>\n<body class=\"mermaid-body\">\n");
    out.push_str("<pre class=\"mermaid\">");
    out.push_str(&escaped_source);
    out.push_str("</pre>\n");
    out.push_str(MERMAID_SCRIPT);
    out.push_str("\n</body>\n</html>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_page_renders_elements_and_only_needed_lib_tags() {
        let md = r#"# Architecture Overview

Here is an architectural document.

| Component | Status |
|-----------|--------|
| Vault     | Online |
| Sync      | Active |

```mermaid
graph TD
    Client --> Server
```

```plantuml
@startuml
Bob -> Alice : hello
@enduml
```

Inline math $E = mc^2$ and display math:
$$\int_0^\infty e^{-x^2} dx = \frac{\sqrt{\pi}}{2}$$

And code:
```rust
fn main() {
    println!("Hello, world!");
}
```
"#;

        let html = markdown_page("Test Page", md);

        // Required doctype, head elements and body structure
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains(r#"<meta charset="utf-8">"#));
        assert!(html.contains(r#"<meta name="color-scheme" content="light dark">"#));
        assert!(html.contains("<title>Test Page</title>"));
        assert!(html.contains("<article class=\"markdown\">"));

        // Elements rendered
        assert!(html.contains("<h1"));
        assert!(html.contains("Architecture Overview</h1>"));
        assert!(html.contains("<table>"));
        assert!(html.contains("<th>Component</th>"));
        assert!(html.contains("<td>Vault</td>"));
        assert!(html.contains(r#"<pre class="mermaid">graph TD"#));
        assert!(html.contains(r#"<figure class="diagram"><img alt="plantuml diagram" src="_herbarium/render/plantuml/"#));
        assert!(html.contains(r#"<span class="math-inline">\(E = mc^2\)</span>"#));
        assert!(html.contains(r#"<div class="math-display">\[\int_0^\infty e^{-x^2} dx = \frac{\sqrt{\pi}}{2}\]</div>"#));
        assert!(html.contains(r#"<pre><code class="language-rust">fn main() {"#));

        // Required libraries are included
        assert!(html.contains(r#"<script src="_herbarium/lib/mermaid.js"></script>"#));
        assert!(html.contains("mermaid.initialize({startOnLoad:true,theme:matchMedia('(prefers-color-scheme: dark)').matches?'dark':'default'})"));
        assert!(html.contains(r#"<link rel="stylesheet" href="_herbarium/lib/katex.css">"#));
        assert!(html.contains(r#"<script src="_herbarium/lib/katex.js"></script>"#));
        assert!(html.contains(r#"<script src="_herbarium/lib/katex-auto-render.js"></script>"#));
        assert!(html.contains(r"renderMathInElement(document.body,{delimiters:[{left:'\\[',right:'\\]',display:true},{left:'\\(',right:'\\)',display:false}]})"));
        assert!(html.contains(r#"<link rel="stylesheet" href="_herbarium/lib/highlight.css">"#));
        assert!(html.contains(r#"<script src="_herbarium/lib/highlight.js"></script>"#));
        assert!(html.contains("<script>hljs.highlightAll()</script>"));
    }

    #[test]
    fn markdown_page_selective_lib_inclusion() {
        // Plain markdown: no library scripts or stylesheets
        let plain = markdown_page(
            "Plain",
            "# Just text\n\nParagraph with **bold** text.\n\n| A | B |\n|---|---|\n| 1 | 2 |\n",
        );
        assert!(!plain.contains("_herbarium/lib/mermaid.js"));
        assert!(!plain.contains("_herbarium/lib/katex"));
        assert!(!plain.contains("_herbarium/lib/highlight"));

        // Only mermaid
        let mermaid_only = markdown_page("", "```mermaid\ngraph LR; A --> B\n```");
        assert!(mermaid_only.contains("_herbarium/lib/mermaid.js"));
        assert!(!mermaid_only.contains("_herbarium/lib/katex"));
        assert!(!mermaid_only.contains("_herbarium/lib/highlight"));

        // Only math
        let math_only = markdown_page("", "Calculate $x + y$ here.");
        assert!(!math_only.contains("_herbarium/lib/mermaid.js"));
        assert!(math_only.contains("_herbarium/lib/katex.css"));
        assert!(math_only.contains("_herbarium/lib/katex.js"));
        assert!(math_only.contains("_herbarium/lib/katex-auto-render.js"));
        assert!(!math_only.contains("_herbarium/lib/highlight"));

        // Only code with a language
        let code_only = markdown_page("", "```python\nprint('hello')\n```");
        assert!(!code_only.contains("_herbarium/lib/mermaid.js"));
        assert!(!code_only.contains("_herbarium/lib/katex"));
        assert!(code_only.contains("_herbarium/lib/highlight.css"));
        assert!(code_only.contains("_herbarium/lib/highlight.js"));

        // Indented code block or code block without language does not pull highlight.js
        let plain_code = markdown_page("", "```\nplain text\n```\n\n    four spaces code\n");
        assert!(!plain_code.contains("_herbarium/lib/highlight"));

        // PlantUML diagram alone does not pull Mermaid, KaTeX or highlight
        let diag_only = markdown_page("", "```plantuml\n@startuml\nA -> B\n@enduml\n```");
        assert!(diag_only.contains(r#"<figure class="diagram"><img alt="plantuml diagram""#));
        assert!(!diag_only.contains("_herbarium/lib/mermaid.js"));
        assert!(!diag_only.contains("_herbarium/lib/katex"));
        assert!(!diag_only.contains("_herbarium/lib/highlight"));
    }

    #[test]
    fn markdown_page_title_fallback() {
        // Fallback to first heading
        let h1 = markdown_page("", "# Primary Title\n\nSome body text.");
        assert!(h1.contains("<title>Primary Title</title>"));

        // Fallback to first heading even if h2 with formatting
        let h2 = markdown_page("   ", "## Secondary *formatted* `code`\n\nBody.");
        assert!(h2.contains("<title>Secondary formatted code</title>"));

        // Fallback to Untitled when no headings exist
        let no_h = markdown_page("", "Just a lonely paragraph without headings.");
        assert!(no_h.contains("<title>Untitled</title>"));

        // Explicit title takes precedence over first heading
        let explicit = markdown_page("Explicit Page Title", "# Heading in Markdown\n\nText.");
        assert!(explicit.contains("<title>Explicit Page Title</title>"));

        // Special characters in title are escaped (`<tags>` would be raw HTML,
        // not heading text, so the test uses a bare `<`).
        let special = markdown_page("", "# 1 < 2 & \"quotes\" 'single'");
        assert!(
            special.contains("<title>1 &lt; 2 &amp; &quot;quotes&quot; &#39;single&#39;</title>")
        );
    }

    #[test]
    fn code_fence_escaping_prevents_script_execution() {
        let md = r#"```html
<script>alert("pwned");</script>
```"#;
        let html = markdown_page("Escape Test", md);
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("&lt;script&gt;alert(&quot;pwned&quot;);&lt;/script&gt;"));

        let mermaid_md = r#"```mermaid
graph TD;
    A[<script>evil()</script>] --> B
```"#;
        let mermaid_html = markdown_page("Mermaid Escape", mermaid_md);
        assert!(!mermaid_html.contains("<script>evil()</script>"));
        assert!(mermaid_html.contains("&lt;script&gt;evil()&lt;/script&gt;"));
    }

    #[test]
    fn raw_html_passes_through_in_sandboxed_markdown() {
        let md = "Text with <span class=\"custom-tag\">inline HTML</span> and <details><summary>More</summary>Hidden</details>";
        let html = markdown_page("Raw HTML", md);
        assert!(html.contains("<span class=\"custom-tag\">inline HTML</span>"));
        assert!(html.contains("<details><summary>More</summary>Hidden</details>"));
    }

    #[test]
    fn mermaid_page_renders_centered_layout() {
        let source = "graph LR\n    Start --> End";
        let html = mermaid_page("Flow", source);

        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("<title>Flow</title>"));
        assert!(html.contains(r#"<body class="mermaid-body">"#));
        assert!(html.contains("<pre class=\"mermaid\">graph LR\n    Start --&gt; End</pre>"));
        assert!(html.contains(r#"<script src="_herbarium/lib/mermaid.js"></script>"#));
        assert!(html.contains("mermaid.initialize"));
        assert!(!html.contains("_herbarium/lib/katex"));
        assert!(!html.contains("_herbarium/lib/highlight"));

        // Title fallback
        let untitled = mermaid_page("", source);
        assert!(untitled.contains("<title>Untitled</title>"));
    }

    #[test]
    fn diagram_fence_fallback_to_code_block_on_oversized_source() {
        // Source exceeding MAX_SOURCE (32 KiB) causes render_src to return None
        let large_source = "A -> B\n".repeat(6000); // ~42 KB
        let md = format!("```plantuml\n{large_source}```");
        let html = markdown_page("Large Diagram", &md);

        // Fallback to code block with language-plantuml
        assert!(html.contains(r#"<pre><code class="language-plantuml">"#));
        assert!(!html.contains("<figure class=\"diagram\">"));
        // Code with language triggers highlight.js
        assert!(html.contains("_herbarium/lib/highlight.css"));
        assert!(html.contains("_herbarium/lib/highlight.js"));
    }
}
