// `herbarium add` and deep-link parsing.
//
// `add` runs in-process against a vault, like `herbarium mcp` (design 0.2,
// decision 9), and writes each input through the `pages.create` operation —
// the same `create` path `pages.import` uses — so the CLI rejects exactly the
// HTML the app's importer would reject, with the same reason. It is an
// operation call rather than a direct vault write so validation, id slugging,
// indexing and events all stay in core.
//
// Deep links are parsed here too: `herbarium-app://open/<percent-encoded id>`
// and `herbarium-app://review`; anything else is ignored.

use std::io::Read;
use std::time::Duration;

use serde::Serialize;
use serde_json::{Map, Value, json};

use herbarium_core::{Caller, Host};

/// Event the desktop app emits for the frontend to act on a deep link.
pub const DEEP_LINK_EVENT: &str = "deep-link";

/// Prefix of the clickable link printed for every saved page.
pub const LINK_PREFIX: &str = "herbarium-app://open/";

/// Largest response `herbarium add` will fetch from a URL.
const FETCH_LIMIT: u64 = 20 * 1024 * 1024;

/// A parsed deep link. Serializes to `{kind:"open", id}` / `{kind:"review"}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum DeepLink {
    Open { id: String },
    Review,
}

/// Arguments of a `herbarium add` invocation.
#[derive(Debug, Default, PartialEq)]
pub struct AddArgs {
    pub vault: Option<String>,
    pub folder: Option<String>,
    pub tags: Vec<String>,
    pub title: Option<String>,
    /// `--network` / `--no-network`; absent follows the vault default.
    pub network: Option<bool>,
    /// `--tool`: what generated the pages, recorded as their source.
    pub tool: Option<String>,
    /// `--prompt`: the request that produced the pages.
    pub prompt: Option<String>,
    pub inputs: Vec<String>,
}

pub const USAGE: &str = "Herbarium — keep generated HTML pages in a local vault.

usage:
  herbarium                              open the desktop app
  herbarium add [options] <file|-|url>…  save pages into the vault
  herbarium import [options] <export>    import artifacts from a Claude or ChatGPT data export
  herbarium publish [options] <page-id>  share a page as a gist or on your GitHub Pages site
  herbarium remix [options] <page-id>    rework a page with Claude; the result waits for approval
  herbarium mcp [--vault <path>]         serve the MCP protocol on stdin/stdout
  herbarium native-host install          connect the browser extension
  herbarium help                         show this help

Run `herbarium add --help` for the add options.";

pub const ADD_USAGE: &str = "usage: herbarium add [options] <file|-|url>…

Save one or more pages into the vault and print `<id>\\t<link>` for each.
Accepts local HTML files, `-` for stdin, and http(s) URLs.

options:
  --vault <path>    vault to write to (default: HERBARIUM_VAULT, then the app's last vault)
  --folder <path>   destination folder such as rust/cargo (default: the vault root)
  --tag <tag>       add a tag; repeat for several
  --title <title>   page title; only with a single input (default: the page's <title>)
  --network         allow the page to load from allowlisted CDNs
  --no-network      block all network access for the page (default: the vault setting)
  --tool <name>     record the tool that generated the page, e.g. \"Claude Code\"
  --prompt <text>   record the request that produced the page
  -h, --help        show this help

A page fetched from a URL records that URL as its source.
Fetched URLs time out after 30 s and are capped at 20 MiB.
Exit status: 0 on success, 1 when any input failed, 2 on a usage error.";

/// Resolve the vault the way every front end does: `--vault`, then
/// `HERBARIUM_VAULT`, then the vault last opened in the Herbarium app.
pub fn resolve_vault(explicit: Option<String>) -> Result<String, String> {
    explicit
        .or_else(|| std::env::var("HERBARIUM_VAULT").ok().filter(|v| !v.is_empty()))
        .or(crate::config::load()?.vault_path)
        .ok_or_else(|| {
            "no vault: pass --vault <path>, set HERBARIUM_VAULT, or open a vault in the Herbarium app first"
                .to_string()
        })
}

/// Parse the arguments after `add`. `Ok(None)` means `--help` was asked for.
pub fn parse_add(args: &[String]) -> Result<Option<AddArgs>, String> {
    let mut parsed = AddArgs::default();
    let mut only_inputs = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if only_inputs {
            parsed.inputs.push(arg.clone());
            i += 1;
            continue;
        }
        match arg.as_str() {
            "--help" | "-h" => return Ok(None),
            "--" => only_inputs = true,
            "--vault" => {
                parsed.vault = Some(take_value(args, &mut i, "--vault")?);
                continue;
            }
            "--folder" => {
                parsed.folder = Some(take_value(args, &mut i, "--folder")?);
                continue;
            }
            "--tag" => {
                parsed.tags.push(take_value(args, &mut i, "--tag")?);
                continue;
            }
            "--title" => {
                parsed.title = Some(take_value(args, &mut i, "--title")?);
                continue;
            }
            "--tool" => {
                parsed.tool = Some(take_value(args, &mut i, "--tool")?);
                continue;
            }
            "--prompt" => {
                parsed.prompt = Some(take_value(args, &mut i, "--prompt")?);
                continue;
            }
            "--network" => parsed.network = Some(true),
            "--no-network" => parsed.network = Some(false),
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option `{other}`"));
            }
            other => parsed.inputs.push(other.to_string()),
        }
        i += 1;
    }

    if parsed.inputs.is_empty() {
        return Err("no input: pass one or more files, `-` for stdin, or an http(s) URL".into());
    }
    if parsed.title.is_some() && parsed.inputs.len() != 1 {
        return Err("--title is only valid with a single input".into());
    }
    Ok(Some(parsed))
}

/// Consume the value of `flag` and advance past it.
fn take_value(args: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
    *i += 1;
    let value = args
        .get(*i)
        .ok_or_else(|| format!("{flag} needs a value"))?
        .clone();
    *i += 1;
    Ok(value)
}

/// Parse a deep link; `None` for anything we do not handle.
pub fn parse_deep_link(url: &str) -> Option<DeepLink> {
    const SCHEME: &str = "herbarium-app://";
    let rest = url.strip_prefix(SCHEME)?;
    // Drop any query/fragment, then a single trailing slash.
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    if rest == "review" {
        return Some(DeepLink::Review);
    }
    let raw_id = rest.strip_prefix("open/")?;
    if raw_id.is_empty() {
        return None;
    }
    let id = percent_encoding::percent_decode_str(raw_id)
        .decode_utf8()
        .ok()?
        .into_owned();
    // Ids are single path segments; a decoded slash is not one.
    if id.is_empty() || id.contains('/') {
        return None;
    }
    Some(DeepLink::Open { id })
}

/// Process exit code for `herbarium add …`.
pub const IMPORT_USAGE: &str = "usage: herbarium import [options] <export.zip|conversations.json>

Import every HTML artifact from a Claude or ChatGPT data export into the
vault, each with its prompt, its original date and a link to the
conversation. Artifacts imported before are skipped.

options:
  --vault <path>    vault to write to (default: HERBARIUM_VAULT, then the app's last vault)
  --folder <path>   destination folder (default: the vault root)
  --dry-run         list what would be imported without saving anything
  -h, --help        show this help

Exit status: 0 on success, 1 when any artifact failed, 2 on a usage error.";

/// `herbarium import …`: returns the exit status.
pub const PUBLISH_USAGE: &str = "usage: herbarium publish [options] <page-id>

Publish a page, as a self-contained file, with your GitHub account and print
its address. Publishing again updates it.

options:
  --vault <path>   vault to read from (default: HERBARIUM_VAULT, then the app's last vault)
  --gist           as a secret gist: anyone with the link can see it (default)
  --site           on your GitHub Pages site, a public website (repository: --repo)
  --repo <name>    the site's repository (default: the app setting, herbarium-pages)
  --unpublish      take the page down instead
  -h, --help       show this help

The GitHub token is GITHUB_TOKEN, or the one saved in the app.";

pub fn run_publish(args: &[String]) -> i32 {
    let mut vault = None;
    let mut target = "gist";
    let mut repo = None;
    let mut unpublish = false;
    let mut ids = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let parsed = match args[i].as_str() {
            "--help" | "-h" => {
                println!("{PUBLISH_USAGE}");
                return 0;
            }
            "--vault" => take_value(args, &mut i, "--vault").map(|v| vault = Some(v)),
            "--repo" => take_value(args, &mut i, "--repo").map(|v| repo = Some(v)),
            flag @ ("--gist" | "--site" | "--unpublish") => {
                match flag {
                    "--gist" => target = "gist",
                    "--site" => target = "site",
                    _ => unpublish = true,
                }
                i += 1;
                Ok(())
            }
            other if other.starts_with("--") => Err(format!("unknown option `{other}`")),
            other => {
                ids.push(other.to_string());
                i += 1;
                Ok(())
            }
        };
        if let Err(e) = parsed {
            eprintln!("herbarium publish: {e}\n\n{PUBLISH_USAGE}");
            return 2;
        }
    }
    let [id] = ids.as_slice() else {
        eprintln!("herbarium publish: pass exactly one page id\n\n{PUBLISH_USAGE}");
        return 2;
    };
    let run = || -> Result<Value, String> {
        let token = std::env::var("GITHUB_TOKEN")
            .ok()
            .filter(|t| !t.trim().is_empty())
            .or(crate::secrets::load().github_token)
            .ok_or("no GitHub token: set GITHUB_TOKEN or connect GitHub in the app")?;
        let repo = match repo.clone() {
            Some(r) => r,
            None => crate::config::load()?.publish_repo,
        };
        let mut host = Host::new();
        host.open_vault(&resolve_vault(vault.clone())?)?;
        crate::publish::run(&host, token.trim(), id, target, &repo, unpublish)
    };
    match run() {
        Ok(info) => {
            match info["url"].as_str() {
                Some(url) => println!("{url}"),
                None => println!("unpublished {id}"),
            }
            0
        }
        Err(e) => {
            eprintln!("herbarium publish: {e}");
            1
        }
    }
}

pub const REMIX_USAGE: &str = "usage: herbarium remix [options] <page-id>

Rework a page with Claude. The new version is kept as a proposal: compare it
with the page and accept or reject it in the app.

options:
  --vault <path>          vault to use (default: HERBARIUM_VAULT, then the app's last vault)
  --preset <name>         simplify, deeper, quiz, translate, cheatsheet, modernize or custom
                          (default: custom when --instructions is given, else simplify)
  --instructions <text>   what to change (for translate: the language)
  --provider <id>         anthropic, claude-code, openai, gemini, deepseek, mistral,
                          openrouter, ollama or custom (default: the app setting)
  --claude-code           same as --provider claude-code
  --model <model>         default: the app setting, or the service's recommended model
  --base-url <url>        the API address (custom, or Ollama elsewhere)
  -h, --help              show this help

The key comes from the provider's variable (ANTHROPIC_API_KEY, OPENAI_API_KEY,
GEMINI_API_KEY, DEEPSEEK_API_KEY, MISTRAL_API_KEY, OPENROUTER_API_KEY,
HERBARIUM_AI_KEY for custom), or the one saved in the app.";

pub fn run_remix(args: &[String]) -> i32 {
    let mut vault = None;
    let mut preset = None;
    let mut instructions = String::new();
    let mut model = None;
    let mut provider = None;
    let mut base_url = None;
    let mut ids = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let parsed = match args[i].as_str() {
            "--help" | "-h" => {
                println!("{REMIX_USAGE}");
                return 0;
            }
            "--vault" => take_value(args, &mut i, "--vault").map(|v| vault = Some(v)),
            "--preset" => take_value(args, &mut i, "--preset").map(|v| preset = Some(v)),
            "--instructions" => {
                take_value(args, &mut i, "--instructions").map(|v| instructions = v)
            }
            "--model" => take_value(args, &mut i, "--model").map(|v| model = Some(v)),
            "--provider" => take_value(args, &mut i, "--provider").map(|v| provider = Some(v)),
            "--base-url" => take_value(args, &mut i, "--base-url").map(|v| base_url = Some(v)),
            "--claude-code" => {
                provider = Some("claude-code".to_string());
                i += 1;
                Ok(())
            }
            other if other.starts_with("--") => Err(format!("unknown option `{other}`")),
            other => {
                ids.push(other.to_string());
                i += 1;
                Ok(())
            }
        };
        if let Err(e) = parsed {
            eprintln!("herbarium remix: {e}\n\n{REMIX_USAGE}");
            return 2;
        }
    }
    let [id] = ids.as_slice() else {
        eprintln!("herbarium remix: pass exactly one page id\n\n{REMIX_USAGE}");
        return 2;
    };
    let preset = preset.unwrap_or_else(|| {
        if instructions.trim().is_empty() {
            "simplify"
        } else {
            "custom"
        }
        .to_string()
    });
    let run = || -> Result<Value, String> {
        let cfg = crate::config::load()?;
        let provider_id = provider.clone().unwrap_or_else(|| cfg.ai_provider.clone());
        let info = crate::ai::provider(&provider_id)
            .ok_or_else(|| format!("unknown provider `{provider_id}`"))?;
        let key = info
            .key_env
            .and_then(|var| std::env::var(var).ok())
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .or_else(|| crate::secrets::load().ai_key(&provider_id));
        let base_url = base_url.clone().or_else(|| {
            (provider_id == cfg.ai_provider)
                .then(|| cfg.ai_base_url.clone())
                .flatten()
        });
        // No model named: the app's choice for its own service, else the
        // service's recommended model.
        let model = match model.clone() {
            Some(m) => m,
            None if provider_id == cfg.ai_provider && !cfg.ai_model.trim().is_empty() => {
                cfg.ai_model.clone()
            }
            None => crate::ai::resolve_model(info, key.as_deref(), base_url.as_deref()),
        };
        let mut host = Host::new();
        host.open_vault(&resolve_vault(vault.clone())?)?;
        let (prompt, base) = crate::remix::prompt_for(&host, id, &preset, &instructions)?;
        let job = crate::remix::Job {
            provider: &provider_id,
            model: &model,
            key: key.as_deref(),
            base_url: base_url.as_deref(),
            prompt,
        };
        let html = crate::remix::run(job, |_| true)?;
        crate::remix::propose(&host, id, &html, base)
    };
    match run() {
        Ok(proposal) => {
            println!(
                "proposal ready for {id}: {}",
                proposal["title"].as_str().unwrap_or_default()
            );
            0
        }
        Err(e) => {
            eprintln!("herbarium remix: {e}");
            1
        }
    }
}

pub fn run_import(args: &[String]) -> i32 {
    let mut vault = None;
    let mut folder = None;
    let mut dry_run = false;
    let mut inputs = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let parsed = match args[i].as_str() {
            "--help" | "-h" => {
                println!("{IMPORT_USAGE}");
                return 0;
            }
            "--vault" => take_value(args, &mut i, "--vault").map(|v| vault = Some(v)),
            "--folder" => take_value(args, &mut i, "--folder").map(|v| folder = Some(v)),
            "--dry-run" => {
                dry_run = true;
                i += 1;
                Ok(())
            }
            other if other.starts_with("--") => Err(format!("unknown option `{other}`")),
            other => {
                inputs.push(other.to_string());
                i += 1;
                Ok(())
            }
        };
        if let Err(e) = parsed {
            eprintln!("herbarium import: {e}\n\n{IMPORT_USAGE}");
            return 2;
        }
    }
    let [input] = inputs.as_slice() else {
        eprintln!("herbarium import: pass exactly one export file\n\n{IMPORT_USAGE}");
        return 2;
    };
    let run = || -> Result<i32, String> {
        let mut host = Host::new();
        host.open_vault(&resolve_vault(vault.clone())?)?;
        let (scan, listing) = crate::ai_import::scan_file(&host, std::path::Path::new(input))?;
        let fresh = listing
            .candidates
            .iter()
            .filter(|c| !c.already_imported)
            .count();
        println!(
            "{} artifacts in {} conversations ({} new, {} not standalone pages)",
            listing.candidates.len(),
            listing.conversations,
            fresh,
            listing.unsupported
        );
        if dry_run {
            for c in listing.candidates.iter().filter(|c| !c.already_imported) {
                println!(
                    "{}\t{}\t{}",
                    c.candidate.tool, c.candidate.title, c.candidate.key
                );
            }
            return Ok(0);
        }
        let report = crate::ai_import::import(&host, &scan.candidates, None, folder.as_deref())?;
        println!("imported {}, skipped {}", report.imported, report.skipped);
        for e in &report.errors {
            eprintln!("herbarium import: {e}");
        }
        Ok(if report.errors.is_empty() { 0 } else { 1 })
    };
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("herbarium import: {e}");
            1
        }
    }
}

pub fn run_add(args: &[String]) -> i32 {
    let parsed = match parse_add(args) {
        Ok(Some(parsed)) => parsed,
        Ok(None) => {
            println!("{ADD_USAGE}");
            return 0;
        }
        Err(e) => {
            eprintln!("herbarium add: {e}\n");
            eprintln!("{ADD_USAGE}");
            return 2;
        }
    };
    match add(&parsed) {
        Ok(failed) => {
            if failed {
                1
            } else {
                0
            }
        }
        Err(e) => {
            eprintln!("herbarium add: {e}");
            1
        }
    }
}

/// Resolve the vault, then import every input. Returns true if any failed.
fn add(args: &AddArgs) -> Result<bool, String> {
    let vault = resolve_vault(args.vault.clone())?;
    let mut host = Host::new();
    host.open_vault(&vault)?;

    let mut failed = false;
    for input in &args.inputs {
        match import_one(&host, args, input) {
            Ok(id) => println!("{id}\t{LINK_PREFIX}{id}"),
            Err(reason) => {
                eprintln!("herbarium add: {input}: {reason}");
                failed = true;
            }
        }
    }
    Ok(failed)
}

/// Import one input (file, `-`, or URL) and return the new page id.
fn import_one(host: &Host, args: &AddArgs, input: &str) -> Result<String, String> {
    let is_url = input.starts_with("http://") || input.starts_with("https://");
    let html = if input == "-" {
        let body = read_capped(std::io::stdin().lock(), "input")?;
        String::from_utf8(body).map_err(|_| "input is not valid UTF-8".to_string())?
    } else if is_url {
        fetch(input)?
    } else {
        std::fs::read_to_string(input).map_err(|e| e.to_string())?
    };

    let mut op = Map::new();
    op.insert("html".into(), Value::String(html));
    if let Some(folder) = &args.folder {
        op.insert("folder".into(), Value::String(folder.clone()));
    }
    if !args.tags.is_empty() {
        op.insert("tags".into(), json!(args.tags));
    }
    if let Some(title) = &args.title {
        op.insert("title".into(), Value::String(title.clone()));
    }
    if let Some(network) = args.network {
        op.insert("allowCdn".into(), Value::Bool(network));
    }
    let mut source = Map::new();
    if is_url {
        source.insert("url".into(), Value::String(input.to_string()));
    }
    if let Some(tool) = &args.tool {
        source.insert("tool".into(), Value::String(tool.clone()));
    }
    if let Some(prompt) = &args.prompt {
        source.insert("prompt".into(), Value::String(prompt.clone()));
    }
    if !source.is_empty() {
        op.insert("source".into(), Value::Object(source));
    }

    // The user is invoking this directly, so the CLI counts as the UI; the
    // operation is the same `create` path `pages.import` runs per file.
    let page = host.call(Caller::Ui, "pages.create", Value::Object(op))?;
    page.get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "page created without an id".to_string())
}

/// Read at most [`FETCH_LIMIT`] bytes from `reader`, erroring if there is more.
/// `what` names the source in messages ("response", "input").
fn read_capped<R: Read>(reader: R, what: &str) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    reader
        .take(FETCH_LIMIT + 1)
        .read_to_end(&mut body)
        .map_err(|e| format!("could not read {what}: {e}"))?;
    if body.len() as u64 > FETCH_LIMIT {
        return Err(format!("{what} larger than 20 MiB"));
    }
    Ok(body)
}

/// Fetch a page over HTTP(S) with a 30 s timeout and a 20 MiB cap.
fn fetch(url: &str) -> Result<String, String> {
    // `rustls-no-provider` hands provider setup to us; the app's updater also
    // uses the ring provider, so nothing new is pulled in.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("could not create HTTP client: {e}"))?;
    let resp = client
        .get(url)
        .send()
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    if resp.content_length().is_some_and(|len| len > FETCH_LIMIT) {
        return Err("response larger than 20 MiB".into());
    }
    let body = read_capped(resp, "response")?;
    Ok(String::from_utf8_lossy(&body).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_every_option() {
        let parsed = parse_add(&args(&[
            "--vault",
            "/v",
            "--folder",
            "rust/cargo",
            "--tag",
            "rust",
            "--tag",
            "cargo",
            "--title",
            "T",
            "--network",
            "--tool",
            "Claude Code",
            "--prompt",
            "explain cargo",
            "page.html",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(
            parsed,
            AddArgs {
                vault: Some("/v".into()),
                folder: Some("rust/cargo".into()),
                tags: vec!["rust".into(), "cargo".into()],
                title: Some("T".into()),
                network: Some(true),
                tool: Some("Claude Code".into()),
                prompt: Some("explain cargo".into()),
                inputs: vec!["page.html".into()],
            }
        );
    }

    #[test]
    fn no_network_and_multiple_inputs() {
        let parsed = parse_add(&args(&["--no-network", "a.html", "-"]))
            .unwrap()
            .unwrap();
        assert_eq!(parsed.network, Some(false));
        assert_eq!(parsed.inputs, vec!["a.html".to_string(), "-".to_string()]);
    }

    #[test]
    fn title_requires_a_single_input() {
        let err = parse_add(&args(&["--title", "T", "a.html", "b.html"])).unwrap_err();
        assert!(err.contains("--title"), "{err}");
    }

    #[test]
    fn rejects_unknown_option_and_missing_value() {
        assert!(parse_add(&args(&["--nope", "a.html"])).is_err());
        assert!(parse_add(&args(&["--folder"])).is_err());
    }

    #[test]
    fn requires_an_input() {
        assert!(parse_add(&args(&["--folder", "x"])).is_err());
    }

    #[test]
    fn double_dash_treats_the_rest_as_inputs() {
        let parsed = parse_add(&args(&["--", "--network", "-"]))
            .unwrap()
            .unwrap();
        assert_eq!(parsed.network, None);
        assert_eq!(
            parsed.inputs,
            vec!["--network".to_string(), "-".to_string()]
        );
    }

    #[test]
    fn help_is_not_a_run() {
        assert_eq!(parse_add(&args(&["--help"])).unwrap(), None);
        assert_eq!(parse_add(&args(&["-h"])).unwrap(), None);
    }

    #[test]
    fn parses_open_deep_link() {
        assert_eq!(
            parse_deep_link("herbarium-app://open/rust-cargo"),
            Some(DeepLink::Open {
                id: "rust-cargo".into()
            })
        );
        assert_eq!(
            parse_deep_link("herbarium-app://open/a%20b"),
            Some(DeepLink::Open { id: "a b".into() })
        );
        // A trailing slash and query are tolerated.
        assert_eq!(
            parse_deep_link("herbarium-app://open/x/?a=1"),
            Some(DeepLink::Open { id: "x".into() })
        );
        // A decoded slash would escape the id segment.
        assert_eq!(parse_deep_link("herbarium-app://open/a%2Fb"), None);
        assert_eq!(parse_deep_link("herbarium-app://open/"), None);
    }

    #[test]
    fn parses_review_deep_link() {
        assert_eq!(
            parse_deep_link("herbarium-app://review"),
            Some(DeepLink::Review)
        );
        assert_eq!(
            parse_deep_link("herbarium-app://review/"),
            Some(DeepLink::Review)
        );
    }

    #[test]
    fn ignores_other_urls() {
        assert_eq!(parse_deep_link("herbarium-app://other/x"), None);
        assert_eq!(parse_deep_link("herbarium://page/x"), None);
        assert_eq!(parse_deep_link("https://example.com/open/x"), None);
        assert_eq!(parse_deep_link(""), None);
    }

    #[test]
    fn deep_link_serializes_for_the_frontend() {
        assert_eq!(
            serde_json::to_value(DeepLink::Open { id: "x".into() }).unwrap(),
            serde_json::json!({ "kind": "open", "id": "x" })
        );
        assert_eq!(
            serde_json::to_value(DeepLink::Review).unwrap(),
            serde_json::json!({ "kind": "review" })
        );
    }
}
