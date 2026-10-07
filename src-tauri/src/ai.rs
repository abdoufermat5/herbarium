// The AI services a remix can use. Anthropic is reached through its own
// Messages API and Claude Code through its command line (see remix.rs); every
// other service here speaks the OpenAI-compatible chat completions protocol,
// so one streaming client serves them all: OpenAI, Google Gemini, DeepSeek,
// Mistral, OpenRouter, a local Ollama, or any compatible server at a custom
// address.

use std::io::{BufRead, BufReader, Read};
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: &'static str,
    pub label: &'static str,
    /// The model filled in when the provider is picked; any model works.
    pub default_model: &'static str,
    /// Where the OpenAI-compatible API lives (None: not that protocol).
    pub base_url: Option<&'static str>,
    pub needs_key: bool,
    /// Where to get a key.
    pub key_url: Option<&'static str>,
    /// Environment variable the command line reads the key from.
    pub key_env: Option<&'static str>,
}

pub const PROVIDERS: &[Provider] = &[
    Provider {
        id: "anthropic",
        label: "Anthropic (Claude)",
        default_model: "claude-opus-5-5",
        base_url: None,
        needs_key: true,
        key_url: Some("https://console.anthropic.com/settings/keys"),
        key_env: Some("ANTHROPIC_API_KEY"),
    },
    Provider {
        id: "claude-code",
        label: "Claude Code",
        default_model: "claude-opus-5-5",
        base_url: None,
        needs_key: false,
        key_url: None,
        key_env: None,
    },
    Provider {
        id: "openai",
        label: "OpenAI",
        default_model: "gpt-4o",
        base_url: Some("https://api.openai.com/v1"),
        needs_key: true,
        key_url: Some("https://platform.openai.com/api-keys"),
        key_env: Some("OPENAI_API_KEY"),
    },
    Provider {
        id: "gemini",
        label: "Google Gemini",
        default_model: "gemini-2.5-pro",
        base_url: Some("https://generativelanguage.googleapis.com/v1beta/openai"),
        needs_key: true,
        key_url: Some("https://aistudio.google.com/apikey"),
        key_env: Some("GEMINI_API_KEY"),
    },
    Provider {
        id: "deepseek",
        label: "DeepSeek",
        default_model: "deepseek-chat",
        base_url: Some("https://api.deepseek.com/v1"),
        needs_key: true,
        key_url: Some("https://platform.deepseek.com/api_keys"),
        key_env: Some("DEEPSEEK_API_KEY"),
    },
    Provider {
        id: "mistral",
        label: "Mistral",
        default_model: "mistral-large-latest",
        base_url: Some("https://api.mistral.ai/v1"),
        needs_key: true,
        key_url: Some("https://console.mistral.ai/api-keys"),
        key_env: Some("MISTRAL_API_KEY"),
    },
    Provider {
        id: "openrouter",
        label: "OpenRouter",
        default_model: "deepseek/deepseek-chat",
        base_url: Some("https://openrouter.ai/api/v1"),
        needs_key: true,
        key_url: Some("https://openrouter.ai/keys"),
        key_env: Some("OPENROUTER_API_KEY"),
    },
    Provider {
        id: "ollama",
        label: "Ollama (local)",
        default_model: "llama3.1",
        base_url: Some("http://localhost:11434/v1"),
        needs_key: false,
        key_url: None,
        key_env: None,
    },
    Provider {
        id: "custom",
        label: "Other (OpenAI-compatible)",
        default_model: "",
        base_url: None,
        needs_key: false,
        key_url: None,
        key_env: Some("HERBARIUM_AI_KEY"),
    },
];

pub fn provider(id: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.id == id)
}

/// The API address for `provider`: the custom one when given (required for
/// `custom`, an override for the others, e.g. Ollama on another machine).
pub fn base_url(provider: &Provider, custom: Option<&str>) -> Result<String, String> {
    let custom = custom.map(str::trim).filter(|u| !u.is_empty());
    let url = match (custom, provider.base_url) {
        (Some(u), _) => u,
        (None, Some(u)) => u,
        (None, None) => {
            return Err("enter the address of the API in Settings → AI & sharing".into());
        }
    };
    check_url(url)?;
    Ok(url.trim_end_matches('/').to_string())
}

/// An http(s) address; plain http only for this computer or the local network.
pub fn check_url(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| format!("“{url}” is not a valid address"))?;
    match parsed.scheme() {
        "https" => Ok(()),
        "http" => {
            let host = parsed.host_str().unwrap_or_default();
            let local = host == "localhost"
                || host.ends_with(".local")
                || host.parse::<std::net::IpAddr>().is_ok_and(|ip| match ip {
                    std::net::IpAddr::V4(v4) => {
                        v4.is_loopback() || v4.is_private() || v4.is_link_local()
                    }
                    std::net::IpAddr::V6(v6) => v6.is_loopback(),
                });
            if local {
                Ok(())
            } else {
                Err("use https for an AI service on the internet (http is only for this computer or your network)".into())
            }
        }
        _ => Err(format!("“{url}” is not an http(s) address")),
    }
}

fn error_message(status: u16, raw: &str) -> String {
    let body: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
    // OpenAI style `{error:{message}}`; Gemini sometimes wraps it in a list.
    let err = if body.is_array() {
        &body[0]["error"]
    } else {
        &body["error"]
    };
    let message = err["message"]
        .as_str()
        .or_else(|| err.as_str())
        .or_else(|| body["message"].as_str())
        .unwrap_or("")
        .to_string();
    match status {
        401 | 403 => format!(
            "the API key was refused{}",
            if message.is_empty() {
                String::new()
            } else {
                format!(": {message}")
            }
        ),
        404 => format!(
            "not found — check the model name and the address{}",
            if message.is_empty() {
                String::new()
            } else {
                format!(" ({message})")
            }
        ),
        429 => format!(
            "rate limited or out of credit{}",
            if message.is_empty() {
                String::new()
            } else {
                format!(": {message}")
            }
        ),
        _ if message.is_empty() => format!("the AI service answered HTTP {status}"),
        _ => message,
    }
}

/// Read an OpenAI-compatible event stream, collecting the answer's text.
pub fn read_chat_stream(
    reader: impl Read,
    mut progress: impl FnMut(usize) -> bool,
) -> Result<String, String> {
    let mut text = String::new();
    let mut finish: Option<String> = None;
    let mut reported = 0;
    let mut done = false;
    for line in BufReader::new(reader).lines() {
        let line = line.map_err(|e| format!("the connection broke: {e}"))?;
        let Some(data) = line.strip_prefix("data:").map(str::trim) else {
            continue;
        };
        if data == "[DONE]" {
            done = true;
            break;
        }
        let event: Value = serde_json::from_str(data).map_err(|e| format!("bad event: {e}"))?;
        if !event["error"].is_null() {
            return Err(error_message(500, data));
        }
        let choice = &event["choices"][0];
        if let Some(piece) = choice["delta"]["content"].as_str() {
            text.push_str(piece);
        }
        if let Some(reason) = choice["finish_reason"].as_str() {
            finish = Some(reason.to_string());
        }
        if text.len() - reported >= 512 || text.is_empty() {
            reported = text.len();
            if !progress(text.chars().count()) {
                return Err("cancelled".into());
            }
        }
    }
    match finish.as_deref() {
        Some("length") => Err("the page was too long for the model to finish".into()),
        Some("content_filter") => Err("the model declined to remix this page".into()),
        Some(_) => Ok(text),
        None if done && !text.is_empty() => Ok(text),
        None => Err("the answer was cut off".into()),
    }
}

/// Ask an OpenAI-compatible chat API, streaming the answer.
pub fn ask_chat(
    base: &str,
    key: Option<&str>,
    model: &str,
    system: &str,
    prompt: &str,
    progress: impl FnMut(usize) -> bool,
) -> Result<String, String> {
    if model.trim().is_empty() {
        return Err("enter a model name in Settings → AI & sharing".into());
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20 * 60))
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("could not create HTTP client: {e}"))?;
    let body = json!({
        "model": model,
        "stream": true,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": prompt },
        ],
    });
    let mut req = client
        .post(format!("{base}/chat/completions"))
        .header("content-type", "application/json")
        .body(body.to_string());
    if let Some(key) = key {
        req = req.bearer_auth(key);
    }
    let resp = req.send().map_err(|e| {
        if e.is_connect() {
            format!("could not reach {base} — is the service running?")
        } else {
            format!("could not reach the AI service: {e}")
        }
    })?;
    let status = resp.status();
    if !status.is_success() {
        return Err(error_message(
            status.as_u16(),
            &resp.text().unwrap_or_default(),
        ));
    }
    read_chat_stream(resp, progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sse(events: &[Value]) -> String {
        let mut out: String = events.iter().map(|e| format!("data: {e}\n\n")).collect();
        out.push_str("data: [DONE]\n\n");
        out
    }

    #[test]
    fn chat_streams_are_read() {
        let ok = sse(&[
            json!({ "choices": [{ "delta": { "role": "assistant" } }] }),
            json!({ "choices": [{ "delta": { "reasoning_content": "thinking" } }] }),
            json!({ "choices": [{ "delta": { "content": "```html\n<p>" } }] }),
            json!({ "choices": [{ "delta": { "content": "x</p>\n```" }, "finish_reason": "stop" }] }),
        ]);
        assert_eq!(
            read_chat_stream(ok.as_bytes(), |_| true).unwrap(),
            "```html\n<p>x</p>\n```"
        );

        let long = sse(&[
            json!({ "choices": [{ "delta": { "content": "a" }, "finish_reason": "length" }] }),
        ]);
        assert!(
            read_chat_stream(long.as_bytes(), |_| true)
                .unwrap_err()
                .contains("too long")
        );
        let filtered =
            sse(&[json!({ "choices": [{ "delta": {}, "finish_reason": "content_filter" }] })]);
        assert!(
            read_chat_stream(filtered.as_bytes(), |_| true)
                .unwrap_err()
                .contains("declined")
        );
        let cut = "data: {\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\n\n";
        assert!(read_chat_stream(cut.as_bytes(), |_| true).is_err());
        let err = "data: {\"error\":{\"message\":\"model overloaded\"}}\n\n";
        assert!(
            read_chat_stream(err.as_bytes(), |_| true)
                .unwrap_err()
                .contains("overloaded")
        );
    }

    #[test]
    fn errors_and_addresses() {
        assert!(
            error_message(401, r#"{"error":{"message":"Incorrect API key"}}"#)
                .contains("refused: Incorrect API key")
        );
        assert!(
            error_message(
                400,
                r#"[{"error":{"code":400,"message":"API key not valid"}}]"#
            )
            .contains("API key not valid")
        );
        assert!(error_message(404, "").contains("model name"));
        assert!(check_url("https://api.example.com/v1").is_ok());
        assert!(check_url("http://localhost:11434/v1").is_ok());
        assert!(check_url("http://192.168.1.20:8080/v1").is_ok());
        assert!(check_url("http://api.example.com/v1").is_err());
        assert!(check_url("file:///etc/passwd").is_err());
        let custom = provider("custom").unwrap();
        assert!(base_url(custom, None).is_err());
        assert_eq!(
            base_url(custom, Some("https://x.dev/v1/")).unwrap(),
            "https://x.dev/v1"
        );
        assert_eq!(
            base_url(provider("deepseek").unwrap(), None).unwrap(),
            "https://api.deepseek.com/v1"
        );
        for p in PROVIDERS {
            assert!(p.base_url.is_none_or(|u| check_url(u).is_ok()), "{}", p.id);
        }
    }

    #[test]
    fn a_compatible_server_gets_the_prompt() {
        use std::io::Write;
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
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
            let mut body = vec![0; len];
            reader.read_exact(&mut body).unwrap();
            let answer = sse(&[
                json!({ "choices": [{ "delta": { "content": "<p>ok</p>" }, "finish_reason": "stop" }] }),
            ]);
            write!(sock, "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\r\n{answer}", answer.len()).unwrap();
            (head, String::from_utf8(body).unwrap())
        });
        let text = ask_chat(
            &base,
            Some("sk-x"),
            "deepseek-chat",
            "sys",
            "remix me",
            |_| true,
        )
        .unwrap();
        assert_eq!(text, "<p>ok</p>");
        let (head, body) = server.join().unwrap();
        assert!(head.starts_with("POST /v1/chat/completions "));
        assert!(
            head.to_ascii_lowercase()
                .contains("authorization: bearer sk-x")
        );
        let body: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["model"], "deepseek-chat");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "remix me");
    }
}
