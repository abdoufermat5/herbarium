//! Model Context Protocol server over stdio (newline-delimited JSON-RPC 2.0).
//!
//! Every operation registered on the [`Host`] that is visible to agents
//! becomes an MCP tool; the operation name `area.verb` is exposed as
//! `area_verb` because MCP clients restrict tool names to `[A-Za-z0-9_-]`.

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
- Pages run in a sandbox: no fetch/XHR to arbitrary hosts, no forms, no \
  same-origin storage, no window.claude or window.storage APIs.
- Prefer existing folders and tags (folders_list, tags_list) over new ones.

Library behaviour:
- pages_list and pages_search return at most 50 results unless you pass `limit`.
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
    ("review.clear", "Clear review", Effect::Idempotent),
    ("review.due", "Pages due for review", Effect::ReadOnly),
    ("review.settings", "Review settings", Effect::ReadOnly),
    ("review.stats", "Review statistics", Effect::ReadOnly),
    ("review.configure", "Configure review", Effect::Idempotent),
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
        "capabilities": { "tools": { "listChanged": false } },
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
