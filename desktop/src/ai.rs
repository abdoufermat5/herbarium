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
        default_model: "opus",
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

/// A model a service offers, with a readable name when it gives one.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Model {
    pub id: String,
    pub name: String,
}

/// Anthropic's API address; `HERBARIUM_ANTHROPIC_API` points tests elsewhere.
pub fn anthropic_base() -> String {
    std::env::var("HERBARIUM_ANTHROPIC_API")
        .unwrap_or_else(|_| "https://api.anthropic.com".into())
        .trim_end_matches('/')
        .to_string()
}

/// Words in a model id that mean it does not write text (embeddings,
/// speech, images, moderation…): never offered.
const NOT_CHAT: &[&str] = &[
    "embed",
    "whisper",
    "tts",
    "dall-e",
    "davinci",
    "babbage",
    "moderation",
    "audio",
    "realtime",
    "transcribe",
    "image",
    "imagen",
    "search",
    "computer-use",
    "ocr",
    "aqa",
    "veo",
    "guard",
    "rerank",
    "vision-preview",
];
/// Words that mark a smaller, older or experimental variant: offered, but
/// not recommended.
const LESSER: &[&str] = &[
    "mini",
    "nano",
    "lite",
    "tiny",
    "small",
    "preview",
    "exp",
    "experimental",
    "instruct",
    "codex",
    "deep-research",
    "0301",
    "0314",
    "0613",
    "16k",
];

/// Whether a word of `id` starts with one of `words` (`gpt-4o-mini` has
/// "mini", `gemini` does not); words with a dash match anywhere.
fn has_any(id: &str, words: &[&str]) -> bool {
    let id = id.to_ascii_lowercase();
    let tokens: Vec<&str> = id
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    words.iter().any(|w| {
        if w.contains('-') {
            id.contains(w)
        } else {
            tokens.iter().any(|t| t.starts_with(w))
        }
    })
}

/// Version numbers in an id, up to the first date-like number:
/// `gpt-4.1` → [4, 1], `claude-sonnet-4-5-20250929` → [4, 5], `gpt-5-2025-08-07` → [5].
fn version(id: &str) -> Vec<u32> {
    let mut out = Vec::new();
    for part in id
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
    {
        match part.parse::<u32>() {
            Ok(n) if n < 1000 && part.len() <= 3 => out.push(n),
            _ => break,
        }
    }
    out
}

/// The model to use when the user has not picked one: the best current
/// model of the service's main family.
pub fn recommend(provider: &Provider, models: &[Model]) -> Option<String> {
    if models.is_empty() {
        return None;
    }
    let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
    // Services whose usual model is a moving alias (deepseek-chat,
    // mistral-large-latest…): keep it when offered.
    let alias_default = matches!(provider.id, "deepseek" | "mistral" | "openrouter");
    if alias_default && ids.contains(&provider.default_model) {
        return Some(provider.default_model.to_string());
    }
    let families: &[&str] = match provider.id {
        "anthropic" | "claude-code" => &["opus", "sonnet"],
        "openai" => &["gpt", "o"],
        "gemini" => &["pro", "flash"],
        "deepseek" => &["chat", "reasoner"],
        "mistral" => &["large", "medium"],
        _ => &[],
    };
    let main: Vec<&str> = ids
        .iter()
        .copied()
        .filter(|id| !has_any(id, LESSER))
        .collect();
    let pool = if main.is_empty() { ids.clone() } else { main };
    for family in families {
        let matches: Vec<&str> = pool
            .iter()
            .copied()
            .filter(|id| {
                let lower = id.to_ascii_lowercase();
                if *family == "o" {
                    lower.starts_with('o') && lower[1..].starts_with(|c: char| c.is_ascii_digit())
                } else {
                    lower.contains(family)
                }
            })
            .collect();
        if matches.is_empty() {
            continue;
        }
        // Anthropic lists newest first; elsewhere the highest version wins,
        // and among equals the plainest id (`gpt-5` over `gpt-5-2025-08-07`).
        if provider.id == "anthropic" {
            return Some(matches[0].to_string());
        }
        let best = matches
            .iter()
            .max_by(|a, b| version(a).cmp(&version(b)).then(b.len().cmp(&a.len())))
            .copied();
        return best.map(str::to_string);
    }
    if ids.contains(&provider.default_model) {
        return Some(provider.default_model.to_string());
    }
    Some(pool[0].to_string())
}

/// Make a readable name from an id: `gpt-4.1` → `GPT 4.1`, `llama3.1:8b` → `Llama3.1 8b`.
fn pretty(id: &str) -> String {
    let id = id.rsplit('/').next().unwrap_or(id);
    id.split(['-', '_', ':'])
        .filter(|w| !w.is_empty())
        .map(|w| match w {
            "gpt" => "GPT".to_string(),
            w if w.chars().all(|c| c.is_ascii_digit() || c == '.') => w.to_string(),
            w => {
                let mut c = w.chars();
                c.next()
                    .map(|f| f.to_uppercase().chain(c).collect())
                    .unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn http() -> Result<reqwest::blocking::Client, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("could not create HTTP client: {e}"))
}

/// The text models `provider` offers to this key, best first by the service's
/// own order. Claude Code has no list: its aliases always mean the newest.
pub fn list_models(
    provider: &Provider,
    key: Option<&str>,
    custom_base: Option<&str>,
) -> Result<Vec<Model>, String> {
    if provider.id == "claude-code" {
        return Ok([
            ("opus", "Claude Opus (newest)"),
            ("sonnet", "Claude Sonnet (newest)"),
            ("haiku", "Claude Haiku (newest)"),
        ]
        .iter()
        .map(|(id, name)| Model {
            id: id.to_string(),
            name: name.to_string(),
        })
        .collect());
    }
    if provider.needs_key && key.is_none() {
        return Err(format!("add your {} API key first", provider.label));
    }
    let client = http()?;
    let (url, req) = if provider.id == "anthropic" {
        let url = format!("{}/v1/models?limit=1000", anthropic_base());
        let req = client
            .get(&url)
            .header("x-api-key", key.unwrap_or_default())
            .header("anthropic-version", "2023-06-01");
        (url, req)
    } else {
        let url = format!("{}/models", base_url(provider, custom_base)?);
        let mut req = client.get(&url);
        if let Some(key) = key {
            req = req.bearer_auth(key);
        }
        (url, req)
    };
    let resp = req.send().map_err(|e| {
        if e.is_connect() {
            format!("could not reach {url} — is the service running?")
        } else {
            format!("could not reach the AI service: {e}")
        }
    })?;
    let status = resp.status();
    let raw = resp.text().unwrap_or_default();
    if !status.is_success() {
        return Err(error_message(status.as_u16(), &raw));
    }
    let body: Value = serde_json::from_str(&raw)
        .map_err(|_| "the service sent an unreadable model list".to_string())?;
    // OpenAI style `{data:[…]}`; Ollama's native `{models:[…]}` too.
    let list = body["data"]
        .as_array()
        .or_else(|| body["models"].as_array())
        .cloned()
        .unwrap_or_default();
    let mut models = Vec::new();
    for m in list {
        let Some(raw_id) = m["id"]
            .as_str()
            .or_else(|| m["name"].as_str())
            .or_else(|| m["model"].as_str())
        else {
            continue;
        };
        // Gemini names models `models/gemini-…`; the chat API takes the bare id.
        let id = raw_id.strip_prefix("models/").unwrap_or(raw_id).to_string();
        if has_any(&id, NOT_CHAT) {
            continue;
        }
        let name = m["display_name"]
            .as_str()
            .or_else(|| m["name"].as_str().filter(|n| *n != raw_id))
            .map(str::to_string)
            .unwrap_or_else(|| pretty(&id));
        if !models.iter().any(|x: &Model| x.id == id) {
            models.push(Model { id, name });
        }
    }
    if provider.id != "anthropic" && provider.id != "openrouter" {
        // Newest versions first, main models before small ones.
        models.sort_by(|a, b| {
            has_any(&a.id, LESSER)
                .cmp(&has_any(&b.id, LESSER))
                .then(version(&b.id).cmp(&version(&a.id)))
                .then(a.id.cmp(&b.id))
        });
    }
    if models.is_empty() {
        return Err("the service offers no text models to this key".into());
    }
    Ok(models)
}

/// The model to use for `provider` when none was picked: the recommended
/// one from its list, or its usual model when the list cannot be read.
pub fn resolve_model(provider: &Provider, key: Option<&str>, custom_base: Option<&str>) -> String {
    list_models(provider, key, custom_base)
        .ok()
        .and_then(|models| recommend(provider, &models))
        .unwrap_or_else(|| provider.default_model.to_string())
}

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
    let mut chars = 0;
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
            chars += piece.chars().count();
        }
        if let Some(reason) = choice["finish_reason"].as_str() {
            finish = Some(reason.to_string());
        }
        if text.len() - reported >= 512 || text.is_empty() {
            reported = text.len();
            if !progress(chars) {
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

    fn ids(list: &[&str]) -> Vec<Model> {
        list.iter()
            .map(|id| Model {
                id: id.to_string(),
                name: pretty(id),
            })
            .collect()
    }

    #[test]
    fn the_best_current_model_is_recommended() {
        let openai = ids(&[
            "gpt-4o",
            "gpt-4o-mini",
            "gpt-4.1",
            "gpt-4.1-mini",
            "gpt-5",
            "gpt-5-2025-08-07",
            "gpt-5-mini",
            "o3",
            "o4-mini",
        ]);
        assert_eq!(
            recommend(provider("openai").unwrap(), &openai).as_deref(),
            Some("gpt-5")
        );
        let gemini = ids(&[
            "gemini-2.5-flash",
            "gemini-2.0-flash",
            "gemini-1.5-pro",
            "gemini-2.5-pro",
            "gemini-2.5-pro-preview-06-05",
        ]);
        assert_eq!(
            recommend(provider("gemini").unwrap(), &gemini).as_deref(),
            Some("gemini-2.5-pro")
        );
        // Anthropic lists newest first.
        let claude = ids(&[
            "claude-opus-5-5",
            "claude-sonnet-5-5",
            "claude-haiku-5-5",
            "claude-opus-4-8",
        ]);
        assert_eq!(
            recommend(provider("anthropic").unwrap(), &claude).as_deref(),
            Some("claude-opus-5-5")
        );
        let deepseek = ids(&["deepseek-reasoner", "deepseek-chat"]);
        assert_eq!(
            recommend(provider("deepseek").unwrap(), &deepseek).as_deref(),
            Some("deepseek-chat")
        );
        let mistral = ids(&[
            "mistral-small-latest",
            "codestral-latest",
            "mistral-large-latest",
            "mistral-large-2411",
        ]);
        assert_eq!(
            recommend(provider("mistral").unwrap(), &mistral).as_deref(),
            Some("mistral-large-latest")
        );
        let ollama = ids(&["qwen2.5:14b", "llama3.1:8b"]);
        assert_eq!(
            recommend(provider("ollama").unwrap(), &ollama).as_deref(),
            Some("qwen2.5:14b")
        );
        assert_eq!(recommend(provider("openai").unwrap(), &[]), None);
        assert_eq!(version("claude-sonnet-4-5-20250929"), vec![4, 5]);
        assert_eq!(version("gpt-5-2025-08-07"), vec![5]);
        assert_eq!(pretty("gpt-4.1-mini"), "GPT 4.1 Mini");
        assert_eq!(pretty("llama3.1:8b"), "Llama3.1 8b");
    }

    /// A one-request HTTP server answering `body` (JSON) with `status`.
    fn serve(status: &'static str, body: Value) -> (String, std::thread::JoinHandle<String>) {
        use std::io::Write;
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(sock.try_clone().unwrap());
            let mut head = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                head.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let body = body.to_string();
            write!(sock, "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}", body.len()).unwrap();
            head
        });
        (base, handle)
    }

    #[test]
    fn model_lists_are_read_and_filtered() {
        let (base, server) = serve(
            "200 OK",
            json!({ "object": "list", "data": [
                { "id": "gpt-4o", "object": "model" },
                { "id": "text-embedding-3-large" },
                { "id": "whisper-1" },
                { "id": "gpt-5-mini" },
                { "id": "dall-e-3" },
                { "id": "gpt-5" },
                { "id": "gpt-4o-realtime-preview" },
                { "id": "omni-moderation-latest" }
            ] }),
        );
        let models = list_models(provider("custom").unwrap(), Some("k"), Some(&base)).unwrap();
        let got: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(
            got,
            ["gpt-5", "gpt-4o", "gpt-5-mini"],
            "text models, newest and main first"
        );
        assert_eq!(models[0].name, "GPT 5");
        let head = server.join().unwrap().to_ascii_lowercase();
        assert!(head.starts_with("get /v1/models "));
        assert!(head.contains("authorization: bearer k"));

        // Gemini names models `models/…`; the list keeps the bare id.
        let (base, server) = serve(
            "200 OK",
            json!({ "data": [{ "id": "models/gemini-2.5-pro" }, { "id": "models/text-embedding-004" }] }),
        );
        let models = list_models(provider("gemini").unwrap(), Some("k"), Some(&base)).unwrap();
        assert_eq!(
            models,
            vec![Model {
                id: "gemini-2.5-pro".into(),
                name: "Gemini 2.5 Pro".into()
            }]
        );
        server.join().unwrap();

        let (base, server) = serve(
            "401 Unauthorized",
            json!({ "error": { "message": "Incorrect API key provided" } }),
        );
        let err = list_models(provider("openai").unwrap(), Some("bad"), Some(&base)).unwrap_err();
        assert!(err.contains("refused"), "{err}");
        server.join().unwrap();

        assert!(
            list_models(provider("deepseek").unwrap(), None, None)
                .unwrap_err()
                .contains("API key")
        );
        let cli = list_models(provider("claude-code").unwrap(), None, None).unwrap();
        assert_eq!(cli[0].id, "opus");
        // Nothing reachable: the usual model.
        assert_eq!(
            resolve_model(
                provider("custom").unwrap(),
                None,
                Some("http://127.0.0.1:9/v1")
            ),
            ""
        );
        assert_eq!(
            resolve_model(provider("claude-code").unwrap(), None, None),
            "opus"
        );
    }
}
