//! Model Context Protocol server over stdio (newline-delimited JSON-RPC 2.0).
//!
//! Every operation registered on the [`Host`] that is visible to agents
//! becomes an MCP tool; the operation name `area.verb` is exposed as
//! `area_verb` because MCP clients restrict tool names to `[A-Za-z0-9_-]`.

use std::io::{self, BufRead, Write};

use herbarium_core::{Caller, Host};
use serde_json::{json, Value};

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
- Inline CSS and JS. External scripts, styles and fonts may only come from \
  cdnjs.cloudflare.com, cdn.jsdelivr.net, unpkg.com, code.jquery.com and \
  Google Fonts; anything else is blocked. Images may use https: or data: URLs; \
  prefer data: or CDN-hosted images, since any image host sees when the page is \
  opened.
- Links open in the user's browser only after they confirm; pages cannot \
  navigate their own frame elsewhere.
- Pages run in a sandbox: no fetch/XHR to arbitrary hosts, no forms, no \
  same-origin storage, no window.claude or window.storage APIs.
- Prefer existing folders and tags (folders_list, tags_list) over new ones.";

fn tool_name(op: &str) -> String {
    op.replace('.', "_")
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
                let replies: Vec<Value> = batch.iter().filter_map(|msg| handle(host, msg)).collect();
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
        return Some(error(id, -32600, "invalid request: method must be a string"));
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
            })
        })
        .collect()
}

fn call(host: &Host, id: Value, params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
    let Some(op) = host.operations(Caller::Agent).find(|op| tool_name(&op.name) == name) else {
        return error(id, -32602, &format!("unknown tool: {name}"));
    };
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
    let (text, is_error) = match host.call(Caller::Agent, &op.name, args) {
        Ok(out) => (serde_json::to_string_pretty(&out).unwrap_or_default(), false),
        Err(e) => (e, true),
    };
    success(id, json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }))
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
        String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    #[test]
    fn session_negotiates_lists_and_calls_tools() {
        let vault = std::env::temp_dir().join(format!("herbarium-mcp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&vault);
        let mut host = Host::new();
        host.open_vault(vault.to_str().unwrap()).unwrap();

        let replies = exchange(&host, &[
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2024-11-05" } }),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "pages_create", "arguments": { "html": "<title>T</title><p>x</p>" } } }),
            json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "pages_get", "arguments": { "id": "missing" } } }),
            json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": { "name": "pages_import", "arguments": {} } }),
        ]);

        assert_eq!(replies.len(), 5, "the notification gets no reply");
        assert_eq!(replies[0]["result"]["protocolVersion"], "2024-11-05");
        let names: Vec<_> = replies[1]["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"pages_create") && names.contains(&"review_schedule"));
        assert!(!names.contains(&"pages_import"), "UI-only operations are not tools");
        assert_eq!(replies[2]["result"]["isError"], false);
        let created: Value = serde_json::from_str(replies[2]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(created["title"], "T");
        assert_eq!(replies[3]["result"]["isError"], true, "operation failures are tool errors");
        assert_eq!(replies[4]["error"]["code"], -32602);

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
        let replies: Vec<Value> =
            String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();

        assert_eq!(replies.len(), 4);
        assert_eq!(replies[0]["error"]["code"], -32700);
        assert_eq!(replies[1]["error"]["code"], -32600);
        assert_eq!(replies[1]["id"], 1);
        let batch = replies[2].as_array().expect("a batch gets an array reply");
        assert_eq!(batch.len(), 2, "the notification inside the batch gets no reply");
        assert_eq!(batch[0]["id"], 2);
        assert_eq!(batch[1]["error"]["code"], -32601);
        assert_eq!(replies[3]["id"], 4, "the last line has no trailing newline");
    }
}
