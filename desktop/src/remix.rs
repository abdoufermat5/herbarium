// Remix a page with an AI model: simplify it, explain it deeper, turn it into
// a quiz, translate it... The model gets the page's HTML and an instruction
// and answers with a new HTML document, which is kept as a proposal: the user
// compares it with the page and accepts or rejects it, as with an agent's
// edit. Two ways to reach a model: the Anthropic API with the user's key
// (streamed, so long pages don't time out), or the Claude Code command line
// with the user's own sign-in.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use crate::publish::Vault;

/// The API's address; `HERBARIUM_ANTHROPIC_API` points tests at a fake one.
fn api_base() -> String {
    crate::ai::anthropic_base()
}

/// Pages larger than this are not sent: the answer would not fit.
const MAX_INPUT: usize = 400 * 1024;
const MAX_TOKENS: u32 = 64_000;

/// Remixes running in the app, by page id, each with its own stop flag:
/// several pages can be remixed at once, and stopping one leaves the others.
static RUNNING: Mutex<BTreeMap<String, Arc<AtomicBool>>> = Mutex::new(BTreeMap::new());

/// A running remix of one page; it stops counting as running when dropped.
pub struct Running {
    page: String,
    stop: Arc<AtomicBool>,
}

impl Running {
    /// The stop flag, for the thread doing the work.
    pub fn flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let mut running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
        // Only forget our own entry.
        if running
            .get(&self.page)
            .is_some_and(|s| Arc::ptr_eq(s, &self.stop))
        {
            running.remove(&self.page);
        }
    }
}

/// Register a remix of `page`; refused while one of that page is running,
/// since the later answer would silently replace the earlier proposal.
pub fn start(page: &str) -> Result<Running, String> {
    let mut running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    if running.contains_key(page) {
        return Err("this page is already being remixed".into());
    }
    let stop = Arc::new(AtomicBool::new(false));
    running.insert(page.to_string(), stop.clone());
    Ok(Running {
        page: page.to_string(),
        stop,
    })
}

/// The ready-made instructions.
pub fn preset(name: &str) -> Option<&'static str> {
    Some(match name {
        "simplify" => {
            "Rewrite this page so a newcomer understands it on a first read: plainer words, shorter sentences, one idea at a time, a short summary at the top. Keep everything that is correct and important; drop jargon or explain it where it first appears."
        }
        "deeper" => {
            "Explain this page in more depth: add the reasoning behind each idea, worked examples, common mistakes and how the ideas connect. Keep the page's structure and add to it rather than replacing it."
        }
        "quiz" => {
            "Add active-recall questions to this page so the reader can test themselves. After each important section, add a short question with its answer in an element marked with the `data-herbarium-recall` attribute holding the question, e.g. `<p data-herbarium-recall=\"What does X do?\">It does Y.</p>`. Herbarium hides those answers until the reader reveals them. Leave the rest of the page as it is."
        }
        "translate" => {
            "Translate this page into the language named below, keeping the HTML structure, styles and scripts exactly as they are; only the human-readable text changes (including the <title>, alt text and labels)."
        }
        "cheatsheet" => {
            "Turn this page into a one-screen cheat sheet: the key facts, commands, formulas or steps, in a dense, well-organized layout (a grid of small cards works well) that reads at a glance and prints on one page."
        }
        "modernize" => {
            "Fix and modernize this page: repair anything broken (layout, scripts, contrast, overflowing content), make it responsive down to phone width, give it clean modern styling with a light and dark theme, and make it accessible (headings in order, labels, focus styles). Keep its content and behaviour."
        }
        "custom" => "",
        _ => return None,
    })
}

const SYSTEM: &str = "You rework single-file HTML pages kept in Herbarium, the user's personal library of pages they read and review. You receive a page and an instruction, and you answer with the complete new page: one self-contained HTML document (CSS and JavaScript inline) in a single ```html fenced code block, with nothing after the block. Keep the <title> meaningful. Keep the page working offline: do not add external scripts, stylesheets or fonts unless the original page already loads them. Keep the original's language unless the instruction asks for another. Links of the form herbarium://page/<id> point to other pages in the library: keep them.";

/// The user turn: the instruction, the reader's highlights, then the page.
pub fn user_prompt(
    preset_name: &str,
    instructions: &str,
    title: &str,
    highlights: &[(String, String)],
    html: &str,
) -> Result<String, String> {
    let base = preset(preset_name).ok_or_else(|| format!("unknown remix `{preset_name}`"))?;
    let extra = instructions.trim();
    if base.is_empty() && extra.is_empty() {
        return Err("say how to remix the page".into());
    }
    let mut out = String::new();
    out.push_str("<instruction>\n");
    out.push_str(base);
    if !extra.is_empty() {
        if !base.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(extra);
    }
    out.push_str("\n</instruction>\n\n");
    if !highlights.is_empty() {
        out.push_str("<highlights>\nPassages the reader highlighted, with their notes; they mattered to them, so keep them and give them weight.\n");
        for (quote, note) in highlights {
            out.push_str("- \"");
            out.push_str(quote);
            out.push('"');
            if !note.is_empty() {
                out.push_str(" (note: ");
                out.push_str(note);
                out.push(')');
            }
            out.push('\n');
        }
        out.push_str("</highlights>\n\n");
    }
    out.push_str(&format!("<page title=\"{}\">\n", title.replace('"', "'")));
    out.push_str(html);
    out.push_str("\n</page>");
    Ok(out)
}

/// The HTML document in a model's answer: the ```html block, or the
/// document itself when the answer is bare HTML.
pub fn extract_html(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    if let Some(start) = lower.find("```html") {
        let body_start = text[start..].find('\n').map(|i| start + i + 1)?;
        // The last fence closes the block: the page itself may contain ```.
        let end = text[body_start..]
            .rfind("```")
            .map(|i| body_start + i)
            .unwrap_or(text.len());
        let html = text[body_start..end].trim();
        if !html.is_empty() {
            return Some(html.to_string());
        }
    }
    let start = lower.find("<!doctype").or_else(|| lower.find("<html"))?;
    let end = lower
        .rfind("</html>")
        .map(|i| i + "</html>".len())
        .unwrap_or(text.len());
    (end > start).then(|| text[start..end].trim().to_string())
}

/// Read a Messages API event stream, collecting the answer's text.
/// `progress` gets the number of characters so far and returns false to stop.
pub fn read_stream(
    reader: impl Read,
    mut progress: impl FnMut(usize) -> bool,
) -> Result<String, String> {
    let mut text = String::new();
    let mut stop_reason: Option<String> = None;
    let mut data = String::new();
    let mut chars = 0;
    let mut reported = 0;
    for line in BufReader::new(reader).lines() {
        let line = line.map_err(|e| format!("the connection broke: {e}"))?;
        if let Some(d) = line.strip_prefix("data:") {
            data.push_str(d.trim_start());
            continue;
        }
        if !line.is_empty() || data.is_empty() {
            continue;
        }
        let event: Value = serde_json::from_str(&data).map_err(|e| format!("bad event: {e}"))?;
        data.clear();
        match event["type"].as_str().unwrap_or_default() {
            "content_block_delta" if event["delta"]["type"] == "text_delta" => {
                let piece = event["delta"]["text"].as_str().unwrap_or_default();
                text.push_str(piece);
                chars += piece.chars().count();
                if text.len() - reported >= 512 {
                    reported = text.len();
                    if !progress(chars) {
                        return Err("cancelled".into());
                    }
                }
            }
            "message_delta" => {
                if let Some(r) = event["delta"]["stop_reason"].as_str() {
                    stop_reason = Some(r.to_string());
                }
            }
            "error" => {
                return Err(api_error(&event));
            }
            _ => {
                if !progress(chars) {
                    return Err("cancelled".into());
                }
            }
        }
    }
    match stop_reason.as_deref() {
        // A refusal can come after some output: what came before is not an answer.
        Some("refusal") => Err("the model declined to remix this page".into()),
        Some("max_tokens") => Err("the page was too long for the model to finish".into()),
        Some(_) => Ok(text),
        None => Err("the answer was cut off".into()),
    }
}

fn api_error(body: &Value) -> String {
    let message = body["error"]["message"].as_str().unwrap_or("unknown error");
    match body["error"]["type"].as_str() {
        Some("authentication_error") => "the Anthropic API key was refused".into(),
        Some("overloaded_error") => "the model is overloaded; try again in a moment".into(),
        Some("rate_limit_error") => format!("rate limited: {message}"),
        _ => message.to_string(),
    }
}

/// Ask the Anthropic API, streaming the answer.
pub fn ask_api(
    key: &str,
    model: &str,
    system: &str,
    prompt: &str,
    progress: impl FnMut(usize) -> bool,
) -> Result<String, String> {
    ask_api_at(&api_base(), key, model, system, prompt, progress)
}

fn ask_api_at(
    base: &str,
    key: &str,
    model: &str,
    system: &str,
    prompt: &str,
    progress: impl FnMut(usize) -> bool,
) -> Result<String, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20 * 60))
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("could not create HTTP client: {e}"))?;
    let body = json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "stream": true,
        "thinking": { "type": "adaptive" },
        // If the model declines, the API tries Anthropic's recommended
        // fallback model in the same call.
        "fallbacks": "default",
        "system": system,
        "messages": [{ "role": "user", "content": prompt }],
    });
    let resp = client
        .post(format!("{base}/v1/messages"))
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", "server-side-fallback-2026-07-01")
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .map_err(|e| format!("could not reach the Anthropic API: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let raw = resp.text().unwrap_or_default();
        let parsed: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        return Err(if parsed.is_null() {
            format!("the Anthropic API answered HTTP {status}")
        } else {
            api_error(&parsed)
        });
    }
    read_stream(resp, progress)
}

/// Claude Code's tools, none of which a remix needs.
const NO_TOOLS: &str = "Bash,Edit,MultiEdit,Write,Read,Glob,Grep,LS,NotebookEdit,NotebookRead,WebFetch,WebSearch,Task,Agent,TodoWrite";

/// Ask Claude Code (`claude -p`), with the prompt on stdin. `chosen` is
/// where the user said it is, if they had to (see locate.rs).
pub fn ask_claude_code(
    chosen: Option<&str>,
    model: &str,
    system: &str,
    prompt: &str,
    progress: impl FnMut(usize) -> bool,
) -> Result<String, String> {
    let bin = crate::locate::claude(chosen)?;
    ask_claude_code_with(&bin, model, system, prompt, progress)
}

fn ask_claude_code_with(
    bin: &std::path::Path,
    model: &str,
    system: &str,
    prompt: &str,
    mut progress: impl FnMut(usize) -> bool,
) -> Result<String, String> {
    if model.starts_with('-') {
        return Err(format!("“{model}” is not a model name"));
    }
    let full = format!("{system}\n\n{prompt}");
    // The page may come from anywhere and could carry instructions of its
    // own: Claude Code gets no tools, and runs in an empty folder.
    let workdir = std::env::temp_dir().join(format!("herbarium-remix-{}", std::process::id()));
    std::fs::create_dir_all(&workdir).map_err(|e| format!("could not prepare Claude Code: {e}"))?;
    let mut child = crate::locate::command(bin)
        .args(["-p", "--output-format", "text", "--model", model])
        .args(["--disallowedTools", NO_TOOLS])
        .current_dir(&workdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!(
                    "Claude Code was not found at {}; check Settings → AI & sharing",
                    bin.display()
                )
            } else {
                format!("could not start Claude Code: {e}")
            }
        })?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let writer = std::thread::spawn(move || stdin.write_all(full.as_bytes()));
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        stdout.read_to_string(&mut out).map(|_| out)
    });
    let mut stderr = child.stderr.take().ok_or("no stderr")?;
    let errors = std::thread::spawn(move || {
        let mut out = String::new();
        let _ = stderr.read_to_string(&mut out);
        out
    });
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if !progress(0) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("cancelled".into());
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    let _ = writer.join();
    let out = reader
        .join()
        .map_err(|_| "Claude Code output was lost")?
        .map_err(|e| e.to_string())?;
    if !status.success() {
        let err = errors.join().unwrap_or_default();
        // Claude Code reports some problems (signing in) on stdout.
        let said = if err.trim().is_empty() {
            out.trim()
        } else {
            err.trim()
        };
        return Err(claude_code_error(said, &status.to_string()));
    }
    Ok(out)
}

/// What to tell the user when Claude Code fails, from what it said.
fn claude_code_error(said: &str, status: &str) -> String {
    let lower = said.to_lowercase();
    if [
        "/login",
        "not logged in",
        "log in",
        "invalid api key",
        "authentication",
        "oauth",
    ]
    .iter()
    .any(|w| lower.contains(w))
    {
        return "Claude Code is not signed in: open a terminal, run `claude`, sign in, then try again"
            .into();
    }
    let said: String = said.chars().take(400).collect();
    if said.is_empty() {
        format!("Claude Code failed ({status})")
    } else {
        format!("Claude Code failed: {said}")
    }
}

/// The prompt for remixing page `id`, and the page's `updatedAt` it was read at.
pub fn prompt_for(
    host: &dyn Vault,
    id: &str,
    preset_name: &str,
    instructions: &str,
) -> Result<(String, Value), String> {
    let page = host.op("pages.get", json!({ "id": id, "format": "html" }))?;
    let highlights = host.op("highlights.list", json!({ "page": id }))?;
    let html = page["html"].as_str().unwrap_or_default();
    check_size(html)?;
    let highlights: Vec<(String, String)> = highlights
        .as_array()
        .into_iter()
        .flatten()
        .map(|h| {
            (
                h["quote"].as_str().unwrap_or_default().to_string(),
                h["note"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    let prompt = user_prompt(
        preset_name,
        instructions,
        page["meta"]["title"].as_str().unwrap_or_default(),
        &highlights,
        html,
    )?;
    Ok((prompt, page["meta"]["updatedAt"].clone()))
}

/// Keep the remixed HTML as the page's proposal.
pub fn propose(
    host: &dyn Vault,
    id: &str,
    html: &str,
    base_updated_at: Value,
) -> Result<Value, String> {
    host.op(
        "proposals.create",
        json!({ "id": id, "html": html, "baseUpdatedAt": base_updated_at, "source": "remix" }),
    )
}

pub struct Job<'a> {
    pub provider: &'a str,
    pub model: &'a str,
    pub key: Option<&'a str>,
    /// For OpenAI-compatible services: a custom API address.
    pub base_url: Option<&'a str>,
    /// Where Claude Code is, when the user chose it.
    pub claude_path: Option<&'a str>,
    pub prompt: String,
}

/// Ask the configured model: `system` says what it is for, `job.prompt` is
/// the request. Returns the model's text.
pub fn run_text(
    job: Job,
    system: &str,
    progress: impl FnMut(usize) -> bool,
) -> Result<String, String> {
    match job.provider {
        "anthropic" => {
            let key = job
                .key
                .ok_or("add your Anthropic API key in Settings → AI & sharing first")?;
            ask_api(key, job.model, system, &job.prompt, progress)
        }
        "claude-code" => ask_claude_code(job.claude_path, job.model, system, &job.prompt, progress),
        other => {
            let provider = crate::ai::provider(other)
                .ok_or_else(|| format!("unknown AI provider `{other}`"))?;
            if provider.needs_key && job.key.is_none() {
                return Err(format!(
                    "add your {} API key in Settings → AI & sharing first",
                    provider.label
                ));
            }
            let base = crate::ai::base_url(provider, job.base_url)?;
            crate::ai::ask_chat(&base, job.key, job.model, system, &job.prompt, progress)
        }
    }
}

/// Run a remix and return the new page's HTML.
pub fn run(job: Job, progress: impl FnMut(usize) -> bool) -> Result<String, String> {
    let answer = run_text(job, SYSTEM, progress)?;
    let html = extract_html(&answer).ok_or("the answer held no HTML page")?;
    if !herbarium_core::content::looks_like_html(&html) {
        return Err("the answer held no HTML page".into());
    }
    Ok(html)
}

/// Refuse pages too large to send.
pub fn check_size(html: &str) -> Result<(), String> {
    if html.len() > MAX_INPUT {
        Err(format!(
            "this page is too large to remix ({} KB; the limit is {} KB)",
            html.len() / 1024,
            MAX_INPUT / 1024
        ))
    } else {
        Ok(())
    }
}

/// Stop the remix of `page`, if one is running.
pub fn cancel(page: &str) {
    let running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(stop) = running.get(page) {
        stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remixes_run_and_stop_per_page() {
        let a = start("remix-test-a").unwrap();
        let b = start("remix-test-b").unwrap();
        assert!(
            start("remix-test-a").is_err(),
            "one remix per page at a time"
        );
        cancel("remix-test-a");
        let stopped = |r: &Running| r.flag().load(Ordering::Relaxed);
        assert!(stopped(&a) && !stopped(&b), "stopping one leaves the other");
        drop(a);
        let again = start("remix-test-a").unwrap();
        assert!(!stopped(&again), "a new remix starts fresh");
    }

    #[test]
    fn html_is_taken_from_the_answer() {
        let fenced = "Here it is:\n```html\n<!doctype html><html><body><pre>```js\nx\n```</pre></body></html>\n```\n";
        assert_eq!(
            extract_html(fenced).unwrap(),
            "<!doctype html><html><body><pre>```js\nx\n```</pre></body></html>"
        );
        let bare = "Sure. <!DOCTYPE html><html><p>x</p></html> Enjoy";
        assert_eq!(
            extract_html(bare).unwrap(),
            "<!DOCTYPE html><html><p>x</p></html>"
        );
        assert_eq!(extract_html("no page here"), None);
    }

    #[test]
    fn prompts_carry_the_instruction_highlights_and_page() {
        let p = user_prompt(
            "translate",
            "French",
            "A \"page\"",
            &[("key idea".into(), "remember".into())],
            "<p>hi</p>",
        )
        .unwrap();
        assert!(p.contains("Translate this page"));
        assert!(p.contains("French"));
        assert!(p.contains("- \"key idea\" (note: remember)"));
        assert!(p.contains("<page title=\"A 'page'\">\n<p>hi</p>\n</page>"));
        assert!(user_prompt("custom", "  ", "t", &[], "x").is_err());
        assert!(user_prompt("nope", "", "t", &[], "x").is_err());
        assert!(user_prompt("custom", "make it pink", "t", &[], "x").is_ok());
    }

    fn sse(events: &[Value]) -> String {
        events
            .iter()
            .map(|e| format!("event: {}\ndata: {}\n\n", e["type"].as_str().unwrap(), e))
            .collect()
    }

    #[test]
    fn streams_are_read() {
        let ok = sse(&[
            json!({ "type": "message_start", "message": {} }),
            json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "thinking" } }),
            json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "thinking_delta", "thinking": "hmm" } }),
            json!({ "type": "content_block_delta", "index": 1, "delta": { "type": "text_delta", "text": "```html\n<p>" } }),
            json!({ "type": "content_block_delta", "index": 1, "delta": { "type": "text_delta", "text": "x</p>\n```" } }),
            json!({ "type": "message_delta", "delta": { "stop_reason": "end_turn" } }),
            json!({ "type": "message_stop" }),
        ]);
        assert_eq!(
            read_stream(ok.as_bytes(), |_| true).unwrap(),
            "```html\n<p>x</p>\n```"
        );

        let refused = sse(&[
            json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": "partial" } }),
            json!({ "type": "message_delta", "delta": { "stop_reason": "refusal" } }),
        ]);
        assert!(
            read_stream(refused.as_bytes(), |_| true)
                .unwrap_err()
                .contains("declined")
        );

        let error = sse(&[
            json!({ "type": "error", "error": { "type": "overloaded_error", "message": "Overloaded" } }),
        ]);
        assert!(
            read_stream(error.as_bytes(), |_| true)
                .unwrap_err()
                .contains("overloaded")
        );

        let cut = sse(&[json!({ "type": "message_start", "message": {} })]);
        assert!(read_stream(cut.as_bytes(), |_| true).is_err());
        assert_eq!(
            read_stream(ok.as_bytes(), |_| false).unwrap_err(),
            "cancelled"
        );
    }

    /// A one-shot HTTP server answering with `status` and `body`; hands back
    /// its address and the request it got.
    fn fake_api(status: &'static str, body: String) -> (String, std::thread::JoinHandle<String>) {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(sock.try_clone().unwrap());
            let mut head = String::new();
            let mut len = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
                head.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let mut req = vec![0; len];
            reader.read_exact(&mut req).unwrap();
            write!(
                sock,
                "HTTP/1.1 {status}\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            head + &String::from_utf8(req).unwrap()
        });
        (addr, handle)
    }

    #[test]
    fn the_api_is_asked_with_streaming_adaptive_thinking_and_fallbacks() {
        let answer = sse(&[
            json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": "```html\n<p>new</p>\n```" } }),
            json!({ "type": "message_delta", "delta": { "stop_reason": "end_turn" } }),
        ]);
        let (base, server) = fake_api("200 OK", answer);
        let text = ask_api_at(
            &base,
            "sk-test",
            "claude-opus-5-5",
            SYSTEM,
            "remix me",
            |_| true,
        )
        .unwrap();
        assert_eq!(extract_html(&text).unwrap(), "<p>new</p>");
        let request = server.join().unwrap().to_ascii_lowercase();
        assert!(request.starts_with("post /v1/messages "));
        assert!(request.contains("x-api-key: sk-test"));
        assert!(request.contains("anthropic-version: 2023-06-01"));
        assert!(request.contains("anthropic-beta: server-side-fallback-2026-07-01"));
        let body: Value =
            serde_json::from_str(&request[request.find("\r\n\r\n").unwrap() + 4..]).unwrap();
        assert_eq!(body["model"], "claude-opus-5-5");
        assert_eq!(body["stream"], true);
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert_eq!(body["fallbacks"], "default");
        assert_eq!(body["messages"][0]["content"], "remix me");

        let (base, server) = fake_api(
            "401 Unauthorized",
            json!({ "type": "error", "error": { "type": "authentication_error", "message": "invalid x-api-key" } }).to_string(),
        );
        let err = ask_api_at(&base, "bad", "m", SYSTEM, "p", |_| true).unwrap_err();
        assert!(err.contains("key was refused"), "{err}");
        server.join().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn claude_code_gets_the_prompt_on_stdin() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("herbarium-claude-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("claude");
        std::fs::write(
            &bin,
            "#!/bin/sh\necho \"$@\" > \"$0.args\"\ncat > \"$0.stdin\"\nprintf '```html\\n<p>cli</p>\\n```\\n'\n",
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let out =
            ask_claude_code_with(&bin, "claude-opus-5-5", SYSTEM, "remix me", |_| true).unwrap();
        assert_eq!(extract_html(&out).unwrap(), "<p>cli</p>");
        let args = std::fs::read_to_string(dir.join("claude.args")).unwrap();
        assert_eq!(
            args.trim(),
            format!("-p --output-format text --model claude-opus-5-5 --disallowedTools {NO_TOOLS}")
        );
        assert!(ask_claude_code_with(&bin, "--help", SYSTEM, "p", |_| true).is_err());
        let stdin = std::fs::read_to_string(dir.join("claude.stdin")).unwrap();
        assert!(stdin.starts_with(SYSTEM) && stdin.ends_with("remix me"));

        // Signing in is reported on stdout and gets a plain explanation.
        std::fs::write(
            dir.join("signed-out"),
            "#!/bin/sh\necho 'Invalid API key · Please run /login'\nexit 1\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("fail"),
            "#!/bin/sh\necho 'disk full' >&2\nexit 1\n",
        )
        .unwrap();
        for f in ["signed-out", "fail"] {
            std::fs::set_permissions(dir.join(f), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let err =
            ask_claude_code_with(&dir.join("signed-out"), "m", SYSTEM, "p", |_| true).unwrap_err();
        assert!(
            err.contains("not signed in") && err.contains("run `claude`"),
            "{err}"
        );
        let err = ask_claude_code_with(&dir.join("fail"), "m", SYSTEM, "p", |_| true).unwrap_err();
        assert!(err.contains("disk full"), "{err}");
        assert!(
            ask_claude_code_with(
                std::path::Path::new("/nonexistent/claude"),
                "m",
                SYSTEM,
                "p",
                |_| true
            )
            .unwrap_err()
            .contains("not found")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
