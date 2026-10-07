// Publish a page online with the user's GitHub account: as a secret gist (a
// link to share), or as a page of their GitHub Pages site (a public website,
// `https://<login>.github.io/<repo>/<page>.html`, with an index of every page
// published there). What is published is the page as a self-contained file,
// local assets inlined. Publishing again updates the same gist or file; the
// records live in the vault (`published.*` operations).
//
// The site repository is created on first use, with a `.herbarium` marker;
// an existing repository without the marker is never written to.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use herbarium_core::{Caller, Host};
use reqwest::Method;
use reqwest::blocking::Client;
use serde_json::{Value, json};

use crate::export::{ExportPage, index_html, standalone};

/// GitHub's API address; `HERBARIUM_GITHUB_API` points tests at a fake one.
fn api_base() -> String {
    std::env::var("HERBARIUM_GITHUB_API")
        .unwrap_or_else(|_| "https://api.github.com".into())
        .trim_end_matches('/')
        .to_string()
}

/// The site's address; `HERBARIUM_GITHUB_PAGES` overrides `https://<login>.github.io` for tests.
fn pages_origin(login: &str) -> String {
    std::env::var("HERBARIUM_GITHUB_PAGES")
        .unwrap_or_else(|_| format!("https://{}.github.io", login.to_ascii_lowercase()))
        .trim_end_matches('/')
        .to_string()
}

const MARKER: &str = ".herbarium";

/// The vault, as publishing uses it. The app locks it for each call only, so
/// the vault stays usable while GitHub answers.
pub trait Vault {
    fn op(&self, name: &str, args: Value) -> Result<Value, String>;
    fn vault(&self) -> Result<PathBuf, String>;
}

impl Vault for Host {
    fn op(&self, name: &str, args: Value) -> Result<Value, String> {
        self.call(Caller::Ui, name, args)
    }
    fn vault(&self) -> Result<PathBuf, String> {
        self.vault_path()
            .map(Path::to_path_buf)
            .ok_or_else(|| "no vault is open".into())
    }
}

impl Vault for Mutex<Host> {
    fn op(&self, name: &str, args: Value) -> Result<Value, String> {
        self.lock().map_err(|e| e.to_string())?.op(name, args)
    }
    fn vault(&self) -> Result<PathBuf, String> {
        self.lock().map_err(|e| e.to_string())?.vault()
    }
}

pub struct GitHub {
    client: Client,
    base: String,
    token: String,
}

impl GitHub {
    pub fn new(token: &str) -> Result<Self, String> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent(concat!("Herbarium/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| format!("could not create HTTP client: {e}"))?;
        Ok(GitHub {
            client,
            base: api_base(),
            token: token.to_string(),
        })
    }

    /// Call the API; hands back the status and the JSON body (Null when empty).
    fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<(u16, Value), String> {
        let mut req = self
            .client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .header("accept", "application/vnd.github+json")
            .header("x-github-api-version", "2022-11-28");
        if let Some(body) = body {
            req = req
                .header("content-type", "application/json")
                .body(body.to_string());
        }
        let resp = req
            .send()
            .map_err(|e| format!("could not reach GitHub: {e}"))?;
        let status = resp.status().as_u16();
        let text = resp.text().unwrap_or_default();
        let value = serde_json::from_str(&text).unwrap_or(Value::Null);
        Ok((status, value))
    }

    /// Like `call`, but any status outside 2xx (and `ok_also`) is an error.
    fn expect(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        ok_also: &[u16],
    ) -> Result<(u16, Value), String> {
        let (status, value) = self.call(method, path, body)?;
        if (200..300).contains(&status) || ok_also.contains(&status) {
            Ok((status, value))
        } else {
            Err(github_error(status, &value))
        }
    }

    /// The signed-in user's login.
    pub fn login(&self) -> Result<String, String> {
        let (_, user) = self.expect(Method::GET, "/user", None, &[])?;
        user["login"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| "GitHub did not say who you are".into())
    }

    fn file_sha(&self, repo: &str, path: &str) -> Result<Option<String>, String> {
        let (status, file) = self.expect(
            Method::GET,
            &format!("/repos/{repo}/contents/{path}"),
            None,
            &[404],
        )?;
        Ok((status != 404)
            .then(|| file["sha"].as_str().map(str::to_string))
            .flatten())
    }

    fn put_file(
        &self,
        repo: &str,
        path: &str,
        content: &[u8],
        message: &str,
    ) -> Result<(), String> {
        let mut body = json!({
            "message": message,
            "content": herbarium_core::assets::base64(content),
        });
        if let Some(sha) = self.file_sha(repo, path)? {
            body["sha"] = json!(sha);
        }
        self.expect(
            Method::PUT,
            &format!("/repos/{repo}/contents/{path}"),
            Some(body),
            &[],
        )?;
        Ok(())
    }

    fn delete_file(&self, repo: &str, path: &str, message: &str) -> Result<(), String> {
        if let Some(sha) = self.file_sha(repo, path)? {
            self.expect(
                Method::DELETE,
                &format!("/repos/{repo}/contents/{path}"),
                Some(json!({ "message": message, "sha": sha })),
                &[404],
            )?;
        }
        Ok(())
    }
}

fn github_error(status: u16, body: &Value) -> String {
    let message = body["message"].as_str().unwrap_or("unknown error");
    match status {
        401 => "GitHub refused the token; check it in Settings → Publishing".into(),
        403 if message.to_lowercase().contains("rate limit") => {
            "GitHub's rate limit was reached; try again later".into()
        }
        403 | 404 => {
            format!("GitHub said “{message}”; the token may lack the gist or repository permission")
        }
        _ => format!("GitHub answered {status}: {message}"),
    }
}

/// A page as it will be published.
pub struct Item {
    pub id: String,
    pub title: String,
    pub html: String,
}

/// Read a page for publishing, with its local assets inlined.
pub fn item(host: &dyn Vault, id: &str) -> Result<Item, String> {
    let page = host.op("pages.get", json!({ "id": id, "format": "html" }))?;
    let meta = &page["meta"];
    let export = ExportPage {
        id: meta["id"].as_str().unwrap_or(id).to_string(),
        title: meta["title"].as_str().unwrap_or(id).to_string(),
        folder: meta["folder"].as_str().map(str::to_string),
        tags: Vec::new(),
        html: page["html"].as_str().unwrap_or_default().to_string(),
    };
    let vault = host.vault()?;
    Ok(Item {
        html: standalone(&vault, &export),
        id: export.id,
        title: export.title,
    })
}

fn now_ms() -> i64 {
    herbarium_core::time::now_ms()
}

/// Create or update the page's secret gist; `existing` is its gist id.
pub fn gist(gh: &GitHub, item: &Item, existing: Option<&str>) -> Result<Value, String> {
    let body = json!({
        "description": format!("{} — shared from Herbarium", item.title),
        "public": false,
        "files": { format!("{}.html", item.id): { "content": item.html } },
    });
    let mut answer = None;
    if let Some(id) = existing {
        let (status, gist) = gh.expect(
            Method::PATCH,
            &format!("/gists/{id}"),
            Some(body.clone()),
            &[404],
        )?;
        if status != 404 {
            answer = Some(gist);
        }
    }
    let gist = match answer {
        Some(g) => g,
        None => gh.expect(Method::POST, "/gists", Some(body), &[])?.1,
    };
    let id = gist["id"]
        .as_str()
        .ok_or("GitHub did not return the gist")?;
    let url = gist["html_url"]
        .as_str()
        .ok_or("GitHub did not return the gist")?;
    Ok(json!({ "id": id, "url": url, "title": item.title, "at": now_ms() }))
}

pub fn delete_gist(gh: &GitHub, id: &str) -> Result<(), String> {
    gh.expect(Method::DELETE, &format!("/gists/{id}"), None, &[404])?;
    Ok(())
}

/// Make sure `login/repo` exists and is Herbarium's; returns its default branch.
fn ensure_repo(gh: &GitHub, login: &str, repo: &str) -> Result<String, String> {
    let full = format!("{login}/{repo}");
    let (status, info) = gh.expect(Method::GET, &format!("/repos/{full}"), None, &[404])?;
    if status == 404 {
        let (_, info) = gh.expect(
            Method::POST,
            "/user/repos",
            Some(json!({
                "name": repo,
                "description": "Pages shared from Herbarium",
                "auto_init": true,
                "has_issues": false,
                "has_wiki": false,
                "has_projects": false,
            })),
            &[],
        )?;
        gh.put_file(
            &full,
            MARKER,
            b"This repository is managed by Herbarium: pages you publish from the app land here.\n",
            "Herbarium site",
        )?;
        // Serve the HTML as is, without a Jekyll build.
        gh.put_file(&full, ".nojekyll", b"", "Serve pages as they are")?;
        return Ok(info["default_branch"]
            .as_str()
            .unwrap_or("main")
            .to_string());
    }
    if gh.file_sha(&full, MARKER)?.is_none() {
        return Err(format!(
            "github.com/{full} already exists and was not made by Herbarium; choose another repository name in Settings → Publishing"
        ));
    }
    Ok(info["default_branch"]
        .as_str()
        .unwrap_or("main")
        .to_string())
}

fn ensure_pages(gh: &GitHub, full: &str, branch: &str) -> Result<(), String> {
    let (status, _) = gh.expect(Method::GET, &format!("/repos/{full}/pages"), None, &[404])?;
    if status == 404 {
        // 409: already enabled meanwhile.
        gh.expect(
            Method::POST,
            &format!("/repos/{full}/pages"),
            Some(json!({ "build_type": "legacy", "source": { "branch": branch, "path": "/" } })),
            &[409],
        )?;
    }
    Ok(())
}

fn site_url(login: &str, repo: &str, path: &str) -> String {
    let origin = pages_origin(login);
    if repo.eq_ignore_ascii_case(&format!("{login}.github.io")) {
        format!("{origin}/{path}")
    } else {
        format!("{origin}/{repo}/{path}")
    }
}

/// Rewrite the site's index from the vault's records for `full`.
fn write_index(gh: &GitHub, host: &dyn Vault, full: &str) -> Result<(), String> {
    let records = host.op("published.list", json!({}))?;
    let mut pages: Vec<ExportPage> = records
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, r)| {
            let site = &r["site"];
            (site["repo"] == full).then(|| ExportPage {
                id: id.clone(),
                title: site["title"].as_str().unwrap_or(id).to_string(),
                folder: None,
                tags: Vec::new(),
                html: String::new(),
            })
        })
        .collect();
    pages.sort_by_key(|p| p.title.to_lowercase());
    let html = index_html("Pages shared from Herbarium", &pages);
    gh.put_file(full, "index.html", html.as_bytes(), "Update the index")
}

/// Publish the page to the GitHub Pages site in `repo`, then record it and
/// refresh the index. Returns the record.
pub fn site(gh: &GitHub, host: &dyn Vault, item: &Item, repo: &str) -> Result<Value, String> {
    check_repo_name(repo)?;
    let login = gh.login()?;
    let branch = ensure_repo(gh, &login, repo)?;
    let full = format!("{login}/{repo}");
    let path = format!("{}.html", item.id);
    gh.put_file(
        &full,
        &path,
        item.html.as_bytes(),
        &format!("Publish {}", item.title),
    )?;
    ensure_pages(gh, &full, &branch)?;
    let info = json!({
        "repo": full,
        "path": path,
        "url": site_url(&login, repo, &path),
        "title": item.title,
        "at": now_ms(),
    });
    host.op(
        "published.record",
        json!({ "id": item.id, "target": "site", "info": info }),
    )?;
    write_index(gh, host, &full)?;
    Ok(info)
}

/// Take the page off its site and forget it.
pub fn delete_site_page(
    gh: &GitHub,
    host: &dyn Vault,
    id: &str,
    record: &Value,
) -> Result<(), String> {
    let full = record["repo"].as_str().ok_or("no site recorded")?;
    let path = record["path"].as_str().ok_or("no site recorded")?;
    gh.delete_file(full, path, &format!("Unpublish {path}"))?;
    host.op("published.forget", json!({ "id": id, "target": "site" }))?;
    write_index(gh, host, full)
}

pub fn check_repo_name(repo: &str) -> Result<(), String> {
    let ok = !repo.is_empty()
        && repo.len() <= 100
        && repo != "."
        && repo != ".."
        && repo
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if ok {
        Ok(())
    } else {
        Err(format!("“{repo}” is not a valid repository name"))
    }
}

/// Publish (`unpublish` false) or unpublish a page to `target`, recording the
/// result. Shared by the app and `herbarium publish`.
pub fn run(
    host: &dyn Vault,
    token: &str,
    id: &str,
    target: &str,
    repo: &str,
    unpublish: bool,
) -> Result<Value, String> {
    let gh = GitHub::new(token)?;
    let records = host.op("published.list", json!({}))?;
    let record = records[id][target].clone();
    match (target, unpublish) {
        ("gist", false) => {
            let item = item(host, id)?;
            let info = gist(&gh, &item, record["id"].as_str())?;
            host.op(
                "published.record",
                json!({ "id": item.id, "target": "gist", "info": info }),
            )?;
            Ok(info)
        }
        ("gist", true) => {
            if let Some(gist_id) = record["id"].as_str() {
                delete_gist(&gh, gist_id)?;
            }
            host.op("published.forget", json!({ "id": id, "target": "gist" }))?;
            Ok(json!({ "unpublished": id }))
        }
        ("site", false) => site(&gh, host, &item(host, id)?, repo),
        ("site", true) => {
            if record.is_null() {
                return Ok(json!({ "unpublished": id }));
            }
            delete_site_page(&gh, host, id, &record)?;
            Ok(json!({ "unpublished": id }))
        }
        _ => Err(format!(
            "unknown publish target `{target}`; use gist or site"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_names_and_urls() {
        assert!(check_repo_name("herbarium-pages").is_ok());
        assert!(check_repo_name("me.github.io").is_ok());
        for bad in ["", "..", "a/b", "a b", "x?y"] {
            assert!(check_repo_name(bad).is_err(), "{bad}");
        }
        assert_eq!(
            site_url("Me", "notes", "a.html"),
            "https://me.github.io/notes/a.html"
        );
        assert_eq!(
            site_url("Me", "me.github.io", "a.html"),
            "https://me.github.io/a.html"
        );
    }
}
