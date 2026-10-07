// Import from AI chat data exports: every HTML artifact a person made in
// Claude or ChatGPT, with the prompt that asked for it, its date and a link
// back to the conversation. The formats are not documented and change over
// time, so parsing is lenient: anything unrecognised is skipped, never fatal.
//
// Claude (`conversations.json`): conversations with `chat_messages`; artifacts
// are `tool_use` items named `artifacts` (`create`, `update`, `rewrite`) or,
// in older exports, `<antArtifact …>` tags in the assistant's text.
// ChatGPT (`conversations.json`): conversations with a `mapping` tree; canvas
// documents are messages to `canmore.create_textdoc` / `canmore.update_textdoc`.
// Both: a complete HTML document in a fenced ```html block counts too.

use std::collections::{BTreeMap, HashSet};

use chrono::DateTime;
use serde::Serialize;
use serde_json::Value;

use crate::content::{extract_title, looks_like_html};
use crate::store::Store;

/// Longest prompt kept with a page.
const MAX_PROMPT: usize = 2000;

/// One page found in an export.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    /// Stable across imports of the same export: `<tool>:<conversation>:<artifact>`.
    pub key: String,
    pub title: String,
    #[serde(skip_serializing)]
    pub html: String,
    pub bytes: usize,
    /// unix ms
    pub created_at: i64,
    /// `Claude` or `ChatGPT`.
    pub tool: String,
    /// Link back to the conversation.
    pub url: String,
    pub prompt: String,
    pub conversation: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scan {
    pub candidates: Vec<Candidate>,
    pub conversations: usize,
    /// Artifacts of a kind that is not a standalone page (React, code, …).
    pub unsupported: usize,
}

/// Parse an export's `conversations.json`, whichever tool it came from.
pub fn scan(json: &str) -> Result<Scan, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| format!("not a conversations export: {e}"))?;
    let conversations = match &value {
        Value::Array(list) => list.as_slice(),
        Value::Object(o) => o
            .get("conversations")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .ok_or("not a conversations export: expected a list of conversations")?,
        _ => return Err("not a conversations export: expected a list of conversations".into()),
    };
    let mut scan = Scan {
        conversations: conversations.len(),
        ..Scan::default()
    };
    for conv in conversations {
        if conv.get("chat_messages").is_some() {
            claude_conversation(conv, &mut scan);
        } else if conv.get("mapping").is_some() {
            chatgpt_conversation(conv, &mut scan);
        }
    }
    scan.candidates.sort_by_key(|c| c.created_at);
    Ok(scan)
}

/// Keys of every page already imported into this vault.
pub fn imported_keys(store: &Store) -> Result<HashSet<String>, String> {
    Ok(store
        .all()
        .map_err(|e| e.to_string())?
        .iter()
        .filter_map(|m| crate::import_key(m).map(str::to_string))
        .collect())
}

fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn iso_ms(text: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|d| d.timestamp_millis())
}

fn clip(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(max).collect();
        out.push('…');
        out
    }
}

/// A complete HTML page for an artifact body of `kind`, or None when the kind
/// is not a standalone page.
fn page_html(kind: &str, body: &str, title: &str) -> Option<String> {
    let kind = kind.to_ascii_lowercase();
    if kind == "text/html" || kind == "code/html" || kind == "html" {
        return Some(body.to_string());
    }
    if kind == "image/svg+xml" || kind == "code/svg" || kind == "svg" {
        let title = title
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        return Some(format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title>\
             <style>body{{margin:0;min-height:100vh;display:grid;place-items:center}}svg{{max-width:100%;height:auto}}</style>\
             </head><body>{body}</body></html>"
        ));
    }
    None
}

/// Full HTML documents in fenced ```html blocks of `text`.
fn fenced_documents(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("```") {
        let after = &rest[start + 3..];
        let Some(nl) = after.find('\n') else { break };
        let lang = after[..nl].trim().to_ascii_lowercase();
        let body = &after[nl + 1..];
        let Some(end) = body.find("```") else { break };
        let code = &body[..end];
        if (lang == "html" || lang.is_empty()) && is_document(code) {
            out.push(code.trim().to_string());
        }
        rest = &body[end + 3..];
    }
    out
}

/// A whole page rather than a fragment: it has a doctype or an `<html>` element.
fn is_document(code: &str) -> bool {
    let head: String = code
        .trim_start()
        .chars()
        .take(512)
        .collect::<String>()
        .to_ascii_lowercase();
    (head.starts_with("<!doctype html") || head.contains("<html")) && looks_like_html(code)
}

fn push(scan: &mut Scan, mut c: Candidate) {
    if !looks_like_html(&c.html) {
        return;
    }
    if c.title.trim().is_empty() {
        c.title = extract_title(&c.html);
    }
    if c.title.trim().is_empty() {
        c.title = if c.conversation.is_empty() {
            "Untitled artifact".into()
        } else {
            c.conversation.clone()
        };
    }
    c.bytes = c.html.len();
    if let Some(existing) = scan.candidates.iter_mut().find(|e| e.key == c.key) {
        *existing = c;
    } else {
        scan.candidates.push(c);
    }
}

/* ------------------------------------------------------------------ Claude */

/// The visible text of a Claude message (its `text`, else its text items).
fn claude_text(msg: &Value) -> String {
    let text = str_of(msg, "text");
    if !text.trim().is_empty() {
        return text.to_string();
    }
    msg.get("content")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|i| str_of(i, "type") == "text")
                .map(|i| str_of(i, "text"))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// Index of the `>` closing the opening tag at the start of `text`, skipping
/// any `>` inside a quoted attribute value.
fn tag_end(text: &str) -> Option<usize> {
    let mut quoted = false;
    for (i, c) in text.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '>' if !quoted => return Some(i),
            _ => {}
        }
    }
    None
}

/// Attribute `name` of an `<antArtifact …>` opening tag.
fn tag_attr(tag: &str, name: &str) -> String {
    let pat = format!("{name}=\"");
    tag.find(&pat)
        .and_then(|i| {
            let rest = &tag[i + pat.len()..];
            rest.find('"').map(|e| rest[..e].to_string())
        })
        .unwrap_or_default()
}

struct ArtifactState {
    kind: String,
    title: String,
    content: String,
    created_at: i64,
    prompt: String,
}

fn claude_conversation(conv: &Value, scan: &mut Scan) {
    let conv_id = str_of(conv, "uuid").to_string();
    let conv_name = str_of(conv, "name").to_string();
    let url = format!("https://claude.ai/chat/{conv_id}");
    let conv_time = iso_ms(str_of(conv, "created_at")).unwrap_or(0);
    let mut artifacts: BTreeMap<String, ArtifactState> = BTreeMap::new();
    let mut prompt = String::new();
    let Some(messages) = conv.get("chat_messages").and_then(Value::as_array) else {
        return;
    };
    for msg in messages {
        let at = iso_ms(str_of(msg, "created_at")).unwrap_or(conv_time);
        let msg_id = str_of(msg, "uuid");
        if str_of(msg, "sender") == "human" {
            prompt = clip(&claude_text(msg), MAX_PROMPT);
            continue;
        }
        // Tool-use artifacts (current exports).
        for item in msg
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if str_of(item, "type") != "tool_use" || str_of(item, "name") != "artifacts" {
                continue;
            }
            let input = item.get("input").cloned().unwrap_or(Value::Null);
            let id = str_of(&input, "id").to_string();
            if id.is_empty() {
                continue;
            }
            match str_of(&input, "command") {
                "create" | "rewrite" => {
                    let entry = artifacts.entry(id).or_insert_with(|| ArtifactState {
                        kind: String::new(),
                        title: String::new(),
                        content: String::new(),
                        created_at: at,
                        prompt: prompt.clone(),
                    });
                    let kind = str_of(&input, "type");
                    if !kind.is_empty() {
                        entry.kind = kind.to_string();
                    }
                    let title = str_of(&input, "title");
                    if !title.is_empty() {
                        entry.title = title.to_string();
                    }
                    entry.content = str_of(&input, "content").to_string();
                }
                "update" => {
                    if let Some(entry) = artifacts.get_mut(&id) {
                        let old = str_of(&input, "old_str");
                        if !old.is_empty() && entry.content.contains(old) {
                            entry.content =
                                entry.content.replacen(old, str_of(&input, "new_str"), 1);
                        }
                    }
                }
                _ => {}
            }
        }
        // Inline artifacts and fenced documents in the text (older exports).
        let text = claude_text(msg);
        let mut rest = text.as_str();
        while let Some(start) = rest.find("<antArtifact") {
            let after = &rest[start..];
            let Some(open_end) = tag_end(after) else {
                break;
            };
            let tag = &after[..open_end];
            let body_start = open_end + 1;
            let Some(close) = after[body_start..].find("</antArtifact>") else {
                break;
            };
            let body = &after[body_start..body_start + close];
            let id = tag_attr(tag, "identifier");
            let id = if id.is_empty() {
                format!("{msg_id}-{start}")
            } else {
                id
            };
            let state = artifacts.entry(id).or_insert_with(|| ArtifactState {
                kind: String::new(),
                title: String::new(),
                content: String::new(),
                created_at: at,
                prompt: prompt.clone(),
            });
            state.kind = tag_attr(tag, "type");
            let title = tag_attr(tag, "title");
            if !title.is_empty() {
                state.title = title;
            }
            state.content = body.trim().to_string();
            rest = &after[body_start + close + "</antArtifact>".len()..];
        }
        for (n, doc) in fenced_documents(&text).into_iter().enumerate() {
            push(
                scan,
                Candidate {
                    key: format!("claude:{conv_id}:{msg_id}:{n}"),
                    title: String::new(),
                    html: doc,
                    bytes: 0,
                    created_at: at,
                    tool: "Claude".into(),
                    url: url.clone(),
                    prompt: prompt.clone(),
                    conversation: conv_name.clone(),
                },
            );
        }
    }
    for (id, state) in artifacts {
        match page_html(&state.kind, &state.content, &state.title) {
            Some(html) => push(
                scan,
                Candidate {
                    key: format!("claude:{conv_id}:{id}"),
                    title: state.title,
                    html,
                    bytes: 0,
                    created_at: state.created_at,
                    tool: "Claude".into(),
                    url: url.clone(),
                    prompt: state.prompt,
                    conversation: conv_name.clone(),
                },
            ),
            None => scan.unsupported += 1,
        }
    }
}

/* ----------------------------------------------------------------- ChatGPT */

/// Messages of the conversation's current branch, oldest first.
fn chatgpt_branch(conv: &Value) -> Vec<(String, Value)> {
    let Some(mapping) = conv.get("mapping").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut node_id = str_of(conv, "current_node").to_string();
    if node_id.is_empty() {
        // No current node: take the latest leaf.
        node_id = mapping
            .iter()
            .filter(|(_, n)| {
                n.get("children")
                    .and_then(Value::as_array)
                    .is_none_or(|c| c.is_empty())
            })
            .max_by(|a, b| {
                let t = |n: &Value| {
                    n.pointer("/message/create_time")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0)
                };
                t(a.1).total_cmp(&t(b.1))
            })
            .map(|(k, _)| k.clone())
            .unwrap_or_default();
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    while let Some(node) = mapping.get(&node_id) {
        if !seen.insert(node_id.clone()) {
            break;
        }
        if let Some(message) = node.get("message").filter(|m| !m.is_null()) {
            out.push((node_id.clone(), message.clone()));
        }
        match node.get("parent").and_then(Value::as_str) {
            Some(parent) => node_id = parent.to_string(),
            None => break,
        }
    }
    out.reverse();
    out
}

fn chatgpt_text(message: &Value) -> String {
    message
        .pointer("/content/parts")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .or_else(|| {
            message
                .pointer("/content/text")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default()
}

fn chatgpt_conversation(conv: &Value, scan: &mut Scan) {
    let conv_id = conv
        .get("conversation_id")
        .or_else(|| conv.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let conv_name = str_of(conv, "title").to_string();
    let url = format!("https://chatgpt.com/c/{conv_id}");
    let conv_time = conv
        .get("create_time")
        .and_then(Value::as_f64)
        .map(|s| (s * 1000.0) as i64)
        .unwrap_or(0);
    let mut prompt = String::new();
    // Canvas documents by name, updated in place.
    let mut canvas: BTreeMap<String, ArtifactState> = BTreeMap::new();
    let mut last_canvas: Option<String> = None;
    for (node_id, message) in chatgpt_branch(conv) {
        let role = message
            .pointer("/author/role")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let at = message
            .get("create_time")
            .and_then(Value::as_f64)
            .map(|s| (s * 1000.0) as i64)
            .unwrap_or(conv_time);
        let text = chatgpt_text(&message);
        match role {
            "user" => {
                prompt = clip(&text, MAX_PROMPT);
                continue;
            }
            "assistant" => {}
            _ => continue,
        }
        let recipient = str_of(&message, "recipient");
        if recipient == "canmore.create_textdoc" {
            if let Ok(doc) = serde_json::from_str::<Value>(&text) {
                let name = str_of(&doc, "name").to_string();
                let key = if name.is_empty() {
                    node_id.clone()
                } else {
                    name.clone()
                };
                canvas.insert(
                    key.clone(),
                    ArtifactState {
                        kind: str_of(&doc, "type").to_string(),
                        title: name,
                        content: str_of(&doc, "content").to_string(),
                        created_at: at,
                        prompt: prompt.clone(),
                    },
                );
                last_canvas = Some(key);
            }
            continue;
        }
        if recipient == "canmore.update_textdoc" {
            if let (Ok(update), Some(key)) = (serde_json::from_str::<Value>(&text), &last_canvas)
                && let Some(state) = canvas.get_mut(key)
            {
                for u in update
                    .get("updates")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let pattern = str_of(u, "pattern");
                    let replacement = str_of(u, "replacement");
                    // A full rewrite is `.*`; a literal pattern is replaced as text.
                    if pattern == ".*" || pattern == "(?s).*" || pattern == "^.*$" {
                        state.content = replacement.to_string();
                    } else if !pattern.is_empty() && state.content.contains(pattern) {
                        state.content = state.content.replacen(pattern, replacement, 1);
                    }
                }
            }
            continue;
        }
        for (n, doc) in fenced_documents(&text).into_iter().enumerate() {
            push(
                scan,
                Candidate {
                    key: format!("chatgpt:{conv_id}:{node_id}:{n}"),
                    title: String::new(),
                    html: doc,
                    bytes: 0,
                    created_at: at,
                    tool: "ChatGPT".into(),
                    url: url.clone(),
                    prompt: prompt.clone(),
                    conversation: conv_name.clone(),
                },
            );
        }
    }
    for (name, state) in canvas {
        match page_html(&state.kind, &state.content, &state.title) {
            Some(html) => push(
                scan,
                Candidate {
                    key: format!("chatgpt:{conv_id}:canvas:{name}"),
                    title: state.title,
                    html,
                    bytes: 0,
                    created_at: state.created_at,
                    tool: "ChatGPT".into(),
                    url: url.clone(),
                    prompt: state.prompt,
                    conversation: conv_name.clone(),
                },
            ),
            None => scan.unsupported += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PAGE: &str =
        "<!DOCTYPE html><html><head><title>Solar</title></head><body><p>Sun</p></body></html>";

    #[test]
    fn claude_tool_use_artifacts_keep_their_final_version() {
        let export = json!([{
            "uuid": "c1", "name": "Space chat", "created_at": "2025-01-02T10:00:00Z",
            "chat_messages": [
                { "uuid": "m1", "sender": "human", "created_at": "2025-01-02T10:00:00Z", "text": "Make a solar system page" },
                { "uuid": "m2", "sender": "assistant", "created_at": "2025-01-02T10:01:00Z", "text": "", "content": [
                    { "type": "text", "text": "Here it is" },
                    { "type": "tool_use", "name": "artifacts", "input": { "id": "solar", "command": "create", "type": "text/html", "title": "Solar system", "content": PAGE } },
                    { "type": "tool_use", "name": "artifacts", "input": { "id": "comp", "command": "create", "type": "application/vnd.ant.react", "content": "export default () => null" } }
                ] },
                { "uuid": "m3", "sender": "human", "created_at": "2025-01-02T10:02:00Z", "text": "Say Moon instead" },
                { "uuid": "m4", "sender": "assistant", "created_at": "2025-01-02T10:03:00Z", "content": [
                    { "type": "tool_use", "name": "artifacts", "input": { "id": "solar", "command": "update", "old_str": "<p>Sun</p>", "new_str": "<p>Moon</p>" } }
                ] }
            ]
        }]);
        let scan = scan(&export.to_string()).unwrap();
        assert_eq!(scan.conversations, 1);
        assert_eq!(scan.unsupported, 1, "the React component is not a page");
        assert_eq!(scan.candidates.len(), 1);
        let c = &scan.candidates[0];
        assert_eq!(c.key, "claude:c1:solar");
        assert_eq!(c.title, "Solar system");
        assert!(c.html.contains("<p>Moon</p>"), "updates are applied");
        assert_eq!(
            c.prompt, "Make a solar system page",
            "the prompt that created it"
        );
        assert_eq!(c.tool, "Claude");
        assert_eq!(c.url, "https://claude.ai/chat/c1");
        assert_eq!(c.created_at, 1735812060000);
    }

    #[test]
    fn claude_inline_artifacts_svg_and_fenced_documents() {
        let text = format!(
            "Sure.\n<antArtifact identifier=\"logo\" type=\"image/svg+xml\" title=\"Logo <1>\"><svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"4\"/></svg></antArtifact>\n\
             And as code:\n```html\n{PAGE}\n```\nA fragment: ```html\n<p>not a page</p>\n```"
        );
        let export = json!([{ "uuid": "c2", "name": "Old", "created_at": "2024-06-01T00:00:00Z", "chat_messages": [
            { "uuid": "h", "sender": "human", "text": "Draw" },
            { "uuid": "a", "sender": "assistant", "text": text }
        ] }]);
        let scan = scan(&export.to_string()).unwrap();
        let keys: Vec<_> = scan.candidates.iter().map(|c| c.key.as_str()).collect();
        assert!(
            keys.contains(&"claude:c2:logo") && keys.contains(&"claude:c2:a:0"),
            "{keys:?}"
        );
        assert_eq!(scan.candidates.len(), 2, "the fragment is not a page");
        let logo = scan
            .candidates
            .iter()
            .find(|c| c.key == "claude:c2:logo")
            .unwrap();
        assert!(logo.html.contains("<svg") && logo.html.contains("<title>Logo &lt;1&gt;</title>"));
        let fenced = scan
            .candidates
            .iter()
            .find(|c| c.key == "claude:c2:a:0")
            .unwrap();
        assert_eq!(fenced.title, "Solar", "the title comes from the document");
    }

    #[test]
    fn chatgpt_canvas_and_code_blocks_on_the_current_branch() {
        let canvas = json!({ "name": "quiz", "type": "code/html", "content": "<!doctype html><title>Quiz</title><p>v1</p>" }).to_string();
        let rewrite = json!({ "updates": [{ "pattern": ".*", "replacement": "<!doctype html><title>Quiz</title><p>v2</p>" }] }).to_string();
        let export = json!([{
            "id": "g1", "title": "Quiz time", "create_time": 1700000000.0, "current_node": "n5",
            "mapping": {
                "root": { "id": "root", "message": null, "parent": null, "children": ["n1"] },
                "n1": { "id": "n1", "parent": "root", "children": ["n2", "x"], "message": { "author": { "role": "user" }, "create_time": 1700000001.0, "content": { "content_type": "text", "parts": ["Make a quiz"] } } },
                "x": { "id": "x", "parent": "n1", "children": [], "message": { "author": { "role": "assistant" }, "content": { "parts": [format!("```html\n{PAGE}\n```")] } } },
                "n2": { "id": "n2", "parent": "n1", "children": ["n3"], "message": { "author": { "role": "assistant" }, "recipient": "canmore.create_textdoc", "create_time": 1700000002.0, "content": { "parts": [canvas] } } },
                "n3": { "id": "n3", "parent": "n2", "children": ["n4"], "message": { "author": { "role": "user" }, "content": { "parts": ["v2 please"] } } },
                "n4": { "id": "n4", "parent": "n3", "children": ["n5"], "message": { "author": { "role": "assistant" }, "recipient": "canmore.update_textdoc", "content": { "parts": [rewrite] } } },
                "n5": { "id": "n5", "parent": "n4", "children": [], "message": { "author": { "role": "assistant" }, "create_time": 1700000005.0, "content": { "parts": [format!("Also:\n```html\n{PAGE}\n```")] } } }
            }
        }]);
        let scan = scan(&export.to_string()).unwrap();
        let keys: Vec<_> = scan.candidates.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(
            keys,
            ["chatgpt:g1:canvas:quiz", "chatgpt:g1:n5:0"],
            "the abandoned branch is left out"
        );
        let quiz = &scan.candidates[0];
        assert!(quiz.html.contains("v2"));
        assert_eq!(quiz.prompt, "Make a quiz");
        assert_eq!(quiz.url, "https://chatgpt.com/c/g1");
        assert_eq!(scan.candidates[1].prompt, "v2 please");
    }

    #[test]
    fn rejects_what_is_not_an_export() {
        assert!(scan("{}").is_err());
        assert!(scan("not json").is_err());
        let empty = scan("[]").unwrap();
        assert!(empty.candidates.is_empty());
        assert_eq!(
            scan(&json!([{ "odd": true }]).to_string())
                .unwrap()
                .candidates
                .len(),
            0
        );
    }
}
