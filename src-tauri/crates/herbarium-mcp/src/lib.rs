//! Model Context Protocol server over stdio (newline-delimited JSON-RPC 2.0).
//!
//! Every operation registered on the [`Host`] that is visible to agents
//! becomes an MCP tool; the operation name `area.verb` is exposed as
//! `area_verb` because MCP clients restrict tool names to `[A-Za-z0-9_-]`.
//!
//! Pages are also MCP resources (`herbarium://page/<id>` for the HTML,
//! `herbarium://page/<id>/text` for the visible text), so a client can attach
//! them as context, and a few prompts package common requests.

use std::io::{self, BufRead, Write};

use herbarium_core::{Caller, Host};
use serde_json::{Value, json};

/// Newest first; the first entry is offered when the client asks for an
/// unknown version.
const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "\
Herbarium is the user's library of HTML pages (explanations, references, \
interactive demos), stored as plain files in a local vault. Use these tools \
to save, find, read, organize and schedule review of pages.

When writing a page for Herbarium (pages_create / pages_set_html):
- Produce ONE complete, self-contained HTML document with a meaningful <title>; \
  it becomes the page title.
- Inline CSS and JS. Pages have NO network access by default: all external \
  scripts, styles, fonts, remote images, and fetch/XHR requests are blocked. \
  If a page needs a CDN, pass `allowCdn: true` to pages_create; external \
  scripts, styles, fonts, and network connections may then come only from \
  allowlisted CDNs: cdnjs.cloudflare.com, cdn.jsdelivr.net, unpkg.com, \
  code.jquery.com, esm.sh and Google Fonts (fonts.googleapis.com, \
  fonts.gstatic.com); all other hosts are blocked.
- Agents may only turn network access OFF with network_set; only the user \
  can turn it back on in Herbarium.
- eval, WebAssembly and Web Workers are allowed only on pages with \
  `allowCdn: true`. Files stored next to the page in its vault folder (images, \
  styles, scripts) load by relative path. Remote images require `allowCdn: true`; \
  on default pages without network, use data: URLs or vault-relative files.
- Links open in the user's browser only after they confirm; pages cannot \
  navigate their own frame elsewhere.
- Pages run in a sandbox: no fetch/XHR to arbitrary hosts, no forms, and no \
  window.claude API. Pages may keep state between sessions: `localStorage` and \
  the Claude-artifact `window.storage` API (async `get`/`set`/`delete`/`list`, \
  with an optional `shared` flag) are persisted per page in the vault, capped \
  at 1 MiB per page. Use them for progress, preferences and small saved inputs.
- Make pages reviewable: mark the answer to a recall question with the \
  `data-herbarium-recall` attribute, optionally holding the question, e.g. \
  `<p data-herbarium-recall=\"What does Cargo.lock pin?\">Exact versions…</p>`. \
  During review sessions (or when the user turns on \"Quiz me\") those elements \
  are hidden until the user reveals them; mark answers, not whole sections.
- Pass `source` to pages_create: `tool` (what you are, e.g. \"Claude Code\"), \
  `prompt` (the user's request, briefly) and `url` when the page came from the \
  web. The user sees it with the page and can search it.
- The user may review agent edits (agents_settings): pages_set_html and \
  history_restore then return `pendingApproval: true` and leave the page \
  unchanged until the user accepts the proposal in Herbarium. Say so to the user \
  instead of reporting the page as updated.
- Prefer existing folders and tags (folders_list, tags_list) over new ones.

Library behaviour:
- pages_list and pages_search return at most 50 results unless you pass `limit`.
- To point the user at a saved page, give them the clickable link \
  `herbarium-app://open/<id>`; opening it focuses Herbarium on that page.
- pages_delete, pages_bulk_delete and folders_delete with `withPages` move pages \
  to the trash; pages_trash lists it and pages_restore brings a page back.";

fn tool_name(op: &str) -> String {
    op.replace('.', "_")
}

/// How a tool affects the vault, for MCP tool annotations.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Effect {
    /// Only reads; modifies nothing.
    ReadOnly,
    /// Adds data or records an event; repeating the call has a further effect.
    Additive,
    /// Adds or changes data; repeating the call with the same arguments leaves the vault in the same state.
    Idempotent,
    /// Modifies or restores data, but repeating the call fails (e.g. restore, rename).
    Mutating,
    /// Removes or trashes data, and repeating the call fails because the target is gone.
    Destructive,
    /// Removes data, but repeating the call safely succeeds (changes nothing more).
    DestructiveIdempotent,
}

/// Title and effect of every built-in operation, keyed by operation name.
const ANNOTATIONS: &[(&str, &str, Effect)] = &[
    ("pages.create", "Create page", Effect::Additive),
    ("pages.import", "Import pages", Effect::Additive),
    ("pages.get", "Read page", Effect::ReadOnly),
    ("pages.list", "List pages", Effect::ReadOnly),
    ("pages.search", "Search pages", Effect::ReadOnly),
    ("pages.update", "Update page metadata", Effect::Idempotent),
    ("pages.set_html", "Replace page HTML", Effect::Mutating),
    ("pages.delete", "Move page to trash", Effect::Destructive),
    ("pages.trash", "List trash", Effect::ReadOnly),
    ("pages.restore", "Restore page from trash", Effect::Mutating),
    ("pages.purge", "Empty trash", Effect::Destructive),
    ("pages.duplicate", "Duplicate page", Effect::Additive),
    (
        "pages.bulk_update",
        "Update several pages",
        Effect::Idempotent,
    ),
    (
        "pages.bulk_delete",
        "Move several pages to trash",
        Effect::Destructive,
    ),
    ("folders.list", "List folders", Effect::ReadOnly),
    ("folders.create", "Create folder", Effect::Idempotent),
    ("folders.rename", "Rename or move folder", Effect::Mutating),
    ("folders.delete", "Delete folder", Effect::Destructive),
    ("tags.list", "List tags", Effect::ReadOnly),
    ("tags.rename", "Rename tag", Effect::Idempotent),
    ("tags.delete", "Delete tag", Effect::DestructiveIdempotent),
    ("vault.info", "Vault overview", Effect::ReadOnly),
    ("vault.rescan", "Rescan vault", Effect::Idempotent),
    ("network.set", "Set page network access", Effect::Idempotent),
    ("network.settings", "Network settings", Effect::ReadOnly),
    (
        "network.configure",
        "Configure network defaults",
        Effect::Idempotent,
    ),
    ("review.schedule", "Schedule review", Effect::Additive),
    ("review.complete", "Complete review", Effect::Additive),
    (
        "review.preview",
        "Preview review intervals",
        Effect::ReadOnly,
    ),
    ("review.clear", "Clear review", Effect::Idempotent),
    ("review.due", "Pages due for review", Effect::ReadOnly),
    ("review.settings", "Review settings", Effect::ReadOnly),
    ("review.stats", "Review statistics", Effect::ReadOnly),
    ("review.configure", "Configure review", Effect::Idempotent),
    ("history.list", "List page versions", Effect::ReadOnly),
    ("history.get", "Read page version", Effect::ReadOnly),
    ("history.restore", "Restore page version", Effect::Mutating),
    ("agents.settings", "Agent settings", Effect::ReadOnly),
    (
        "proposals.list",
        "Edits awaiting approval",
        Effect::ReadOnly,
    ),
    ("proposals.get", "Read proposed edit", Effect::ReadOnly),
];

/// MCP tool annotations for an operation. Operations missing from
/// [`ANNOTATIONS`] (e.g. from plugins) get only a title.
fn annotations(op: &str) -> Value {
    let Some(&(_, title, effect)) = ANNOTATIONS.iter().find(|(name, ..)| *name == op) else {
        return json!({ "title": op });
    };
    let mut out = json!({ "title": title, "openWorldHint": false });
    let hints = match effect {
        Effect::ReadOnly => json!({ "readOnlyHint": true }),
        Effect::Additive => {
            json!({ "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false })
        }
        Effect::Idempotent => {
            json!({ "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true })
        }
        Effect::Mutating => {
            json!({ "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false })
        }
        Effect::Destructive => {
            json!({ "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false })
        }
        Effect::DestructiveIdempotent => {
            json!({ "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true })
        }
    };
    if let (Some(out), Value::Object(hints)) = (out.as_object_mut(), hints) {
        out.extend(hints);
    }
    out
}
/// `structuredContent` must be an object; other results are wrapped.
fn structured(out: Value) -> Value {
    match out {
        Value::Object(_) => out,
        Value::Array(_) => json!({ "items": out }),
        other => json!({ "value": other }),
    }
}

/// Serve requests from `input` until EOF, writing responses to `output`.
pub fn serve(host: &Host, mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    let mut line = Vec::new();
    loop {
        line.clear();
        if input.read_until(b'\n', &mut line)? == 0 {
            return Ok(());
        }
        if line.trim_ascii().is_empty() {
            continue;
        }
        // Bytes, not `str`: a malformed (e.g. non-UTF-8) line is a parse error
        // for that message only, never the end of the session.
        let reply = match serde_json::from_slice::<Value>(&line) {
            Ok(Value::Array(batch)) if !batch.is_empty() => {
                let replies: Vec<Value> =
                    batch.iter().filter_map(|msg| handle(host, msg)).collect();
                (!replies.is_empty()).then_some(Value::Array(replies))
            }
            Ok(msg) => handle(host, &msg),
            Err(e) => Some(error(Value::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(reply) = reply {
            serde_json::to_writer(&mut output, &reply)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
}

/// Handle one message; notifications and client responses get no reply.
fn handle(host: &Host, msg: &Value) -> Option<Value> {
    let Some(obj) = msg.as_object() else {
        return Some(error(Value::Null, -32600, "invalid request"));
    };
    let id = obj.get("id")?.clone();
    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        if obj.contains_key("result") || obj.contains_key("error") {
            // A response to a server request; this server sends none.
            return None;
        }
        return Some(error(
            id,
            -32600,
            "invalid request: method must be a string",
        ));
    };
    let params = obj.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => success(id, initialize(&params)),
        "ping" => success(id, json!({})),
        "tools/list" => success(id, json!({ "tools": tools(host) })),
        "tools/call" => call(host, id, &params),
        "resources/list" => resources_list(host, id, &params),
        "resources/templates/list" => {
            success(id, json!({ "resourceTemplates": resource_templates() }))
        }
        "resources/read" => resources_read(host, id, &params),
        "prompts/list" => success(id, json!({ "prompts": prompts_list() })),
        "prompts/get" => prompts_get(id, &params),
        _ => error(id, -32601, &format!("method not found: {method}")),
    })
}

fn initialize(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = PROTOCOL_VERSIONS
        .iter()
        .find(|v| Some(**v) == requested)
        .unwrap_or(&PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": {
            "tools": { "listChanged": false },
            "resources": { "listChanged": false },
            "prompts": { "listChanged": false },
        },
        "serverInfo": { "name": "herbarium", "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS,
    })
}

fn tools(host: &Host) -> Vec<Value> {
    host.operations(Caller::Agent)
        .map(|op| {
            json!({
                "name": tool_name(&op.name),
                "description": op.description,
                "inputSchema": op.input_schema,
                "annotations": annotations(&op.name),
            })
        })
        .collect()
}

fn call(host: &Host, id: Value, params: &Value) -> Value {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(op) = host
        .operations(Caller::Agent)
        .find(|op| tool_name(&op.name) == name)
    else {
        return error(id, -32602, &format!("unknown tool: {name}"));
    };
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
    match host.call(Caller::Agent, &op.name, args) {
        Ok(out) => {
            let text = serde_json::to_string_pretty(&out).unwrap_or_default();
            success(
                id,
                json!({
                    "content": [{ "type": "text", "text": text }],
                    "structuredContent": structured(out),
                    "isError": false,
                }),
            )
        }
        Err(e) => success(
            id,
            json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
        ),
    }
}

const PAGE_URI: &str = "herbarium://page/";
const TEXT_SUFFIX: &str = "/text";
/// Pages per `resources/list` reply; the rest follow through `nextCursor`.
const RESOURCES_PAGE: usize = 100;
/// JSON-RPC error code MCP uses for an unknown resource.
const RESOURCE_NOT_FOUND: i64 = -32002;

fn resources_list(host: &Host, id: Value, params: &Value) -> Value {
    let Some(store) = host.store() else {
        return error(id, -32603, "no vault open");
    };
    let start = match params.get("cursor") {
        None | Some(Value::Null) => 0,
        Some(cursor) => match cursor.as_str().and_then(|c| c.parse::<usize>().ok()) {
            Some(n) => n,
            None => return error(id, -32602, "invalid cursor"),
        },
    };
    let pages = match store.all() {
        Ok(pages) => pages,
        Err(e) => return error(id, -32603, &e.to_string()),
    };
    let resources: Vec<Value> = pages
        .iter()
        .skip(start)
        .take(RESOURCES_PAGE)
        .map(|meta| {
            let mut place = meta.folder.clone().unwrap_or_default();
            if !meta.tags.is_empty() {
                if !place.is_empty() {
                    place.push_str(" · ");
                }
                place.push_str(&meta.tags.join(", "));
            }
            let mut resource = json!({
                "uri": format!("{PAGE_URI}{}", meta.id),
                "name": meta.id,
                "title": meta.title,
                "mimeType": "text/html",
            });
            if !place.is_empty() {
                resource["description"] = Value::String(place);
            }
            resource
        })
        .collect();
    let mut result = json!({ "resources": resources });
    let next = start + RESOURCES_PAGE;
    if next < pages.len() {
        result["nextCursor"] = Value::String(next.to_string());
    }
    success(id, result)
}

fn resource_templates() -> Value {
    json!([
        {
            "uriTemplate": "herbarium://page/{id}",
            "name": "page",
            "title": "Herbarium page",
            "description": "A saved page's full HTML source.",
            "mimeType": "text/html",
        },
        {
            "uriTemplate": "herbarium://page/{id}/text",
            "name": "page-text",
            "title": "Herbarium page text",
            "description": "A saved page's visible text, much smaller than its HTML.",
            "mimeType": "text/plain",
        },
    ])
}

fn resources_read(host: &Host, id: Value, params: &Value) -> Value {
    let Some(uri) = params.get("uri").and_then(Value::as_str) else {
        return error(id, -32602, "resources/read needs a `uri`");
    };
    let not_found = |id| {
        error(
            id,
            RESOURCE_NOT_FOUND,
            &format!("resource not found: {uri}"),
        )
    };
    let Some(rest) = uri.strip_prefix(PAGE_URI) else {
        return not_found(id);
    };
    let (page_id, text) = match rest.strip_suffix(TEXT_SUFFIX) {
        Some(page_id) => (page_id, true),
        None => (rest, false),
    };
    if page_id.is_empty() || page_id.contains('/') {
        return not_found(id);
    }
    let format = if text { "text" } else { "html" };
    let page = match host.call(
        Caller::Agent,
        "pages.get",
        json!({ "id": page_id, "format": format }),
    ) {
        Ok(page) => page,
        Err(_) => return not_found(id),
    };
    let body = page.get(format).and_then(Value::as_str).unwrap_or_default();
    success(
        id,
        json!({
            "contents": [{
                "uri": uri,
                "mimeType": if text { "text/plain" } else { "text/html" },
                "text": body,
            }]
        }),
    )
}

struct PromptArg {
    name: &'static str,
    description: &'static str,
    required: bool,
}

struct Prompt {
    name: &'static str,
    title: &'static str,
    description: &'static str,
    arguments: &'static [PromptArg],
    /// The message, with `{argument}` placeholders.
    template: &'static str,
}

const PROMPTS: &[Prompt] = &[
    Prompt {
        name: "save_page",
        title: "Write a page",
        description: "Write a self-contained HTML page about a topic and save it to Herbarium.",
        arguments: &[
            PromptArg {
                name: "topic",
                description: "What the page should explain.",
                required: true,
            },
            PromptArg {
                name: "folder",
                description: "Folder to save it in, e.g. `rust/cargo`.",
                required: false,
            },
        ],
        template: "Write a clear, self-contained HTML page explaining: {topic}\n\n\
Make it worth coming back to: a short summary first, worked examples, and an \
interactive element where it helps understanding. Mark a few recall questions \
with `data-herbarium-recall` so review sessions can quiz me.\n\n\
Check folders_list and tags_list first and reuse what fits. Save it with \
pages_create{folder_clause}, passing `source` with your tool name and this \
request, then give me the herbarium-app://open/<id> link.",
    },
    Prompt {
        name: "ask_vault",
        title: "Ask my pages",
        description: "Answer a question from the pages saved in Herbarium.",
        arguments: &[PromptArg {
            name: "question",
            description: "What you want to know.",
            required: true,
        }],
        template: "Answer this question using the pages saved in my Herbarium vault: {question}\n\n\
Search with pages_search (try a few phrasings), read the most relevant pages with \
pages_get and `format: \"text\"`, and answer from what they say. Cite each page you \
used as a herbarium-app://open/<id> link. If the vault does not cover the question, \
say so before adding anything from your own knowledge.",
    },
    Prompt {
        name: "review_session",
        title: "Review with me",
        description: "Walk through the pages due for review, quizzing you on each.",
        arguments: &[],
        template: "Run a review session with me over my Herbarium pages that are due.\n\n\
Get the queue with review_due. For each page, read it with pages_get \
(`format: \"text\"`), ask me two or three questions about it one at a time, and \
tell me how I did. Then record the review with review_complete: grade `again` if \
I struggled, `hard`, `good` or `easy` otherwise. Stop when the queue is empty or I \
ask to stop, and finish with a short summary.",
    },
];

fn prompts_list() -> Vec<Value> {
    PROMPTS
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "title": p.title,
                "description": p.description,
                "arguments": p.arguments.iter().map(|a| json!({
                    "name": a.name,
                    "description": a.description,
                    "required": a.required,
                })).collect::<Vec<_>>(),
            })
        })
        .collect()
}

fn prompts_get(id: Value, params: &Value) -> Value {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(prompt) = PROMPTS.iter().find(|p| p.name == name) else {
        return error(id, -32602, &format!("unknown prompt: {name}"));
    };
    let args = params.get("arguments");
    let arg = |key: &str| {
        args.and_then(|a| a.get(key))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
    };
    let mut text = prompt.template.to_string();
    for a in prompt.arguments {
        match arg(a.name) {
            Some(value) => text = text.replace(&format!("{{{}}}", a.name), value),
            None if a.required => {
                return error(
                    id,
                    -32602,
                    &format!("missing required argument: {}", a.name),
                );
            }
            None => {}
        }
    }
    let folder_clause = arg("folder")
        .map(|f| format!(" in the folder `{f}`"))
        .unwrap_or_default();
    let text = text.replace("{folder_clause}", &folder_clause);
    success(
        id,
        json!({
            "description": prompt.description,
            "messages": [{ "role": "user", "content": { "type": "text", "text": text } }],
        }),
    )
}

fn success(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exchange(host: &Host, requests: &[Value]) -> Vec<Value> {
        let input: String = requests.iter().map(|r| format!("{r}\n")).collect();
        let mut out = Vec::new();
        serve(host, input.as_bytes(), &mut out).unwrap();
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn session_negotiates_lists_and_calls_tools() {
        let vault = std::env::temp_dir().join(format!("herbarium-mcp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&vault);
        let mut host = Host::new();
        host.open_vault(vault.to_str().unwrap()).unwrap();

        let replies = exchange(
            &host,
            &[
                json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2024-11-05" } }),
                json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
                json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
                json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "pages_create", "arguments": { "html": "<title>T</title><p>x</p>" } } }),
                json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "pages_get", "arguments": { "id": "missing" } } }),
                json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": { "name": "pages_import", "arguments": {} } }),
                json!({ "jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": { "name": "pages_list", "arguments": {} } }),
            ],
        );

        assert_eq!(replies.len(), 6, "the notification gets no reply");
        assert_eq!(replies[0]["result"]["protocolVersion"], "2024-11-05");
        let instructions = replies[0]["result"]["instructions"].as_str().unwrap();
        assert!(instructions.contains("allowCdn: true"));
        assert!(instructions.contains("50 results"));
        assert!(instructions.contains("trash"));
        assert!(instructions.contains("cdnjs.cloudflare.com"));
        assert!(
            instructions.contains("agents may only turn network access off")
                || instructions.contains("Agents may only turn network access OFF")
        );
        let tools = replies[1]["result"]["tools"].as_array().unwrap();
        let names: Vec<_> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"pages_create") && names.contains(&"review_schedule"));
        assert!(
            !names.contains(&"pages_import"),
            "UI-only operations are not tools"
        );
        assert!(
            !names.contains(&"pages_purge"),
            "UI-only operations are not tools"
        );
        assert!(
            !names.contains(&"network_configure"),
            "UI-only operations are not tools"
        );
        assert!(
            !names.contains(&"review_configure"),
            "UI-only operations are not tools"
        );
        assert!(
            tools.iter().all(|t| t["annotations"]["title"].is_string()),
            "every tool has a title"
        );
        assert_eq!(replies[2]["result"]["isError"], false);
        let created: Value =
            serde_json::from_str(replies[2]["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(created["title"], "T");
        assert_eq!(
            replies[2]["result"]["structuredContent"], created,
            "objects are returned as-is"
        );
        assert_eq!(
            replies[3]["result"]["isError"], true,
            "operation failures are tool errors"
        );
        assert!(replies[3]["result"].get("structuredContent").is_none());
        assert_eq!(replies[4]["error"]["code"], -32602);
        let items = &replies[5]["result"]["structuredContent"]["items"];
        assert_eq!(
            items.as_array().map(Vec::len),
            Some(1),
            "arrays are wrapped in `items`"
        );
        assert_eq!(items[0]["id"], created["id"]);

        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn tools_carry_truthful_annotations() {
        let vault =
            std::env::temp_dir().join(format!("herbarium-mcp-annotations-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&vault);
        let mut host = Host::new();
        host.open_vault(vault.to_str().unwrap()).unwrap();

        let replies = exchange(
            &host,
            &[json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })],
        );
        let tools = replies[0]["result"]["tools"].as_array().unwrap();
        let annotations = |name: &str| {
            tools
                .iter()
                .find(|t| t["name"] == name)
                .unwrap_or_else(|| panic!("{name} is a tool"))["annotations"]
                .clone()
        };

        // Read-only tools must have readOnlyHint: true and no destructiveHint or idempotentHint.
        for read_only_tool in &[
            "pages_get",
            "pages_list",
            "pages_search",
            "pages_trash",
            "folders_list",
            "tags_list",
            "vault_info",
            "network_settings",
            "review_due",
            "review_settings",
            "review_stats",
        ] {
            let ann = annotations(read_only_tool);
            assert_eq!(
                ann["readOnlyHint"], true,
                "{read_only_tool} should be readOnlyHint: true"
            );
            assert!(
                ann.get("destructiveHint").is_none(),
                "{read_only_tool} should omit destructiveHint"
            );
            assert!(
                ann.get("idempotentHint").is_none(),
                "{read_only_tool} should omit idempotentHint"
            );
        }

        // Destructive tools must have destructiveHint: true and readOnlyHint: false.
        for destructive_tool in &[
            "pages_delete",
            "pages_bulk_delete",
            "folders_delete",
            "tags_delete",
        ] {
            let ann = annotations(destructive_tool);
            assert_eq!(
                ann["destructiveHint"], true,
                "{destructive_tool} should be destructiveHint: true"
            );
            assert_eq!(
                ann["readOnlyHint"], false,
                "{destructive_tool} should not be read-only"
            );
        }

        // Destructive ops that fail repeat calls must NOT be marked idempotent.
        assert_eq!(annotations("pages_delete")["idempotentHint"], false);
        assert_eq!(annotations("pages_bulk_delete")["idempotentHint"], false);
        assert_eq!(annotations("folders_delete")["idempotentHint"], false);

        // Tags delete is destructive but safely idempotent on repeat calls.
        assert_eq!(annotations("tags_delete")["idempotentHint"], true);

        // Restore and rename fail on repeat calls: not idempotent.
        assert_eq!(annotations("pages_restore")["destructiveHint"], false);
        assert_eq!(annotations("pages_restore")["idempotentHint"], false);
        assert_eq!(annotations("folders_rename")["destructiveHint"], false);
        assert_eq!(annotations("folders_rename")["idempotentHint"], false);
        assert_eq!(annotations("pages_set_html")["destructiveHint"], false);
        assert_eq!(annotations("pages_set_html")["idempotentHint"], false);

        // Additive ops.
        assert_eq!(annotations("pages_create")["destructiveHint"], false);
        assert_eq!(annotations("pages_create")["idempotentHint"], false);

        // Idempotent mutating ops.
        assert_eq!(annotations("pages_update")["idempotentHint"], true);
        assert_eq!(annotations("pages_bulk_update")["idempotentHint"], true);
        assert_eq!(annotations("folders_create")["idempotentHint"], true);
        assert_eq!(annotations("tags_rename")["idempotentHint"], true);
        assert_eq!(annotations("vault_rescan")["idempotentHint"], true);
        assert_eq!(annotations("network_set")["idempotentHint"], true);
        assert_eq!(annotations("review_clear")["idempotentHint"], true);

        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn tool_rpc_response_behavior_and_safety_rules() {
        let vault = std::env::temp_dir().join(format!("herbarium-mcp-rpc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&vault);
        let mut host = Host::new();
        host.open_vault(vault.to_str().unwrap()).unwrap();

        // 1. Create a page.
        let replies = exchange(
            &host,
            &[
                json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "pages_create", "arguments": { "html": "<title>Page 1</title><p>Hello</p>" } } }),
            ],
        );
        assert_eq!(replies[0]["result"]["isError"], false);
        let page_id = replies[0]["result"]["structuredContent"]["id"]
            .as_str()
            .unwrap()
            .to_string();

        // 2. Safe network rules: Agent calling network_set with allowCdn: true must fail.
        let replies = exchange(
            &host,
            &[
                json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "network_set", "arguments": { "id": &page_id, "allowCdn": true } } }),
            ],
        );
        assert_eq!(
            replies[0]["result"]["isError"], true,
            "agents cannot enable CDN"
        );
        assert!(
            replies[0]["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("agents may only turn network access off")
        );
        assert!(
            replies[0]["result"].get("structuredContent").is_none(),
            "errors must not return fake structured content"
        );

        // 3. Destructive op: Delete page moves to trash. First call succeeds.
        let replies = exchange(
            &host,
            &[
                json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "pages_delete", "arguments": { "id": &page_id } } }),
            ],
        );
        assert_eq!(replies[0]["result"]["isError"], false);
        assert_eq!(
            replies[0]["result"]["structuredContent"]["deleted"],
            page_id
        );

        // Repeat call fails because page is already deleted: proves pages_delete is not idempotent.
        let replies = exchange(
            &host,
            &[
                json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "pages_delete", "arguments": { "id": &page_id } } }),
            ],
        );
        assert_eq!(
            replies[0]["result"]["isError"], true,
            "repeated delete must fail"
        );
        assert!(
            replies[0]["result"].get("structuredContent").is_none(),
            "errors omit structuredContent"
        );

        // 4. Restore page from trash. First call succeeds.
        let replies = exchange(
            &host,
            &[
                json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": { "name": "pages_restore", "arguments": { "id": &page_id } } }),
            ],
        );
        assert_eq!(replies[0]["result"]["isError"], false);
        assert_eq!(replies[0]["result"]["structuredContent"]["id"], page_id);

        // Repeat restore fails because page is no longer in trash: proves pages_restore is not idempotent.
        let replies = exchange(
            &host,
            &[
                json!({ "jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": { "name": "pages_restore", "arguments": { "id": &page_id } } }),
            ],
        );
        assert_eq!(
            replies[0]["result"]["isError"], true,
            "repeated restore must fail"
        );
        assert!(
            replies[0]["result"].get("structuredContent").is_none(),
            "errors omit structuredContent"
        );

        // 5. Structure verification: primitive and null wrapping.
        assert_eq!(structured(json!(null)), json!({ "value": null }));
        assert_eq!(structured(json!(123)), json!({ "value": 123 }));
        assert_eq!(structured(json!("abc")), json!({ "value": "abc" }));

        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn pages_are_resources() {
        let vault =
            std::env::temp_dir().join(format!("herbarium-mcp-resources-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&vault);
        let mut host = Host::new();
        host.open_vault(vault.to_str().unwrap()).unwrap();
        for i in 0..(RESOURCES_PAGE + 1) {
            host.call(
                Caller::Agent,
                "pages.create",
                json!({ "html": format!("<title>P{i:03}</title><p>body {i}</p>"), "folder": "f", "tags": ["t"] }),
            )
            .unwrap();
        }

        let replies = exchange(
            &host,
            &[
                json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
                json!({ "jsonrpc": "2.0", "id": 2, "method": "resources/list" }),
                json!({ "jsonrpc": "2.0", "id": 3, "method": "resources/list", "params": { "cursor": "100" } }),
                json!({ "jsonrpc": "2.0", "id": 4, "method": "resources/read", "params": { "uri": "herbarium://page/p000" } }),
                json!({ "jsonrpc": "2.0", "id": 5, "method": "resources/read", "params": { "uri": "herbarium://page/p000/text" } }),
                json!({ "jsonrpc": "2.0", "id": 6, "method": "resources/read", "params": { "uri": "herbarium://page/missing" } }),
                json!({ "jsonrpc": "2.0", "id": 7, "method": "resources/read", "params": { "uri": "herbarium://page/../p000" } }),
                json!({ "jsonrpc": "2.0", "id": 8, "method": "resources/templates/list" }),
                json!({ "jsonrpc": "2.0", "id": 9, "method": "resources/list", "params": { "cursor": "x" } }),
            ],
        );

        let caps = &replies[0]["result"]["capabilities"];
        assert!(caps["resources"].is_object() && caps["prompts"].is_object());

        let first = &replies[1]["result"];
        assert_eq!(first["resources"].as_array().unwrap().len(), RESOURCES_PAGE);
        assert_eq!(first["nextCursor"], "100");
        let r0 = &first["resources"][0];
        assert_eq!(r0["uri"], "herbarium://page/p000");
        assert_eq!(r0["title"], "P000");
        assert_eq!(r0["description"], "f · t");
        let second = &replies[2]["result"];
        assert_eq!(second["resources"].as_array().unwrap().len(), 1);
        assert!(
            second.get("nextCursor").is_none(),
            "the last page has no cursor"
        );

        let html = &replies[3]["result"]["contents"][0];
        assert_eq!(html["mimeType"], "text/html");
        assert!(html["text"].as_str().unwrap().contains("<p>body 0</p>"));
        let text = &replies[4]["result"]["contents"][0];
        assert_eq!(text["mimeType"], "text/plain");
        assert!(text["text"].as_str().unwrap().contains("body 0"));
        assert!(!text["text"].as_str().unwrap().contains("<p>"));

        assert_eq!(replies[5]["error"]["code"], RESOURCE_NOT_FOUND);
        assert_eq!(replies[6]["error"]["code"], RESOURCE_NOT_FOUND);
        assert_eq!(
            replies[7]["result"]["resourceTemplates"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(replies[8]["error"]["code"], -32602);

        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn prompts_fill_their_arguments() {
        let host = Host::new();
        let replies = exchange(
            &host,
            &[
                json!({ "jsonrpc": "2.0", "id": 1, "method": "prompts/list" }),
                json!({ "jsonrpc": "2.0", "id": 2, "method": "prompts/get", "params": { "name": "save_page", "arguments": { "topic": "Cargo workspaces", "folder": "rust" } } }),
                json!({ "jsonrpc": "2.0", "id": 3, "method": "prompts/get", "params": { "name": "save_page", "arguments": { "topic": "Lifetimes" } } }),
                json!({ "jsonrpc": "2.0", "id": 4, "method": "prompts/get", "params": { "name": "ask_vault", "arguments": {} } }),
                json!({ "jsonrpc": "2.0", "id": 5, "method": "prompts/get", "params": { "name": "nope" } }),
                json!({ "jsonrpc": "2.0", "id": 6, "method": "prompts/get", "params": { "name": "review_session" } }),
            ],
        );

        let names: Vec<_> = replies[0]["result"]["prompts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["save_page", "ask_vault", "review_session"]);

        let text = |i: usize| {
            replies[i]["result"]["messages"][0]["content"]["text"]
                .as_str()
                .unwrap()
                .to_string()
        };
        let with_folder = text(1);
        assert!(with_folder.contains("Cargo workspaces"));
        assert!(with_folder.contains("in the folder `rust`"));
        assert!(!with_folder.contains('{'), "every placeholder is filled");
        let without_folder = text(2);
        assert!(!without_folder.contains("folder `") && !without_folder.contains('{'));
        assert_eq!(
            replies[3]["error"]["code"], -32602,
            "a required argument is missing"
        );
        assert_eq!(replies[4]["error"]["code"], -32602);
        assert!(text(5).contains("review_due"));
    }

    #[test]
    fn malformed_messages_get_errors_and_the_session_continues() {
        let host = Host::new();
        let mut input = b"\xff\xfe not utf-8\n".to_vec();
        input.extend_from_slice(br#"{"jsonrpc":"2.0","id":1,"method":5}"#);
        input.extend_from_slice(b"\n");
        input.extend_from_slice(br#"[{"jsonrpc":"2.0","id":2,"method":"ping"},{"jsonrpc":"2.0","method":"notifications/x"},{"jsonrpc":"2.0","id":3,"method":"nope"}]"#);
        input.extend_from_slice(b"\n");
        input.extend_from_slice(br#"{"jsonrpc":"2.0","id":4,"method":"ping"}"#);
        let mut out = Vec::new();
        serve(&host, input.as_slice(), &mut out).unwrap();
        let replies: Vec<Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();

        assert_eq!(replies.len(), 4);
        assert_eq!(replies[0]["error"]["code"], -32700);
        assert_eq!(replies[1]["error"]["code"], -32600);
        assert_eq!(replies[1]["id"], 1);
        let batch = replies[2].as_array().expect("a batch gets an array reply");
        assert_eq!(
            batch.len(),
            2,
            "the notification inside the batch gets no reply"
        );
        assert_eq!(batch[0]["id"], 2);
        assert_eq!(batch[1]["error"]["code"], -32601);
        assert_eq!(replies[3]["id"], 4, "the last line has no trailing newline");
    }
}
