// Agent edits that wait for approval. When the vault asks for it, an agent's
// rewrite of a page's HTML (`pages.set_html`, `history.restore`) is kept as a
// proposal under `.herbarium/proposals/` instead of replacing the page; the
// user compares it with the page and accepts or rejects it. One proposal per
// page: a newer one replaces the older.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::pages::{check_base, ensure_html, overwrite_html_by};
use super::{id_prop, object};
use crate::content::extract_title;
use crate::extension::{Caller, Ctx, Extension, OpResult, Operation, Registry, events};
use crate::models::PageMeta;
use crate::time::now_ms;
use crate::vault;

pub(crate) struct Proposals;

/// Vault-wide agent settings, stored in `.herbarium/agents.json`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentSettings {
    /// Agent rewrites of a page's HTML wait for the user's approval.
    #[serde(default)]
    review_edits: bool,
}

fn settings_path(vault: &Path) -> PathBuf {
    vault.join(".herbarium").join("agents.json")
}

fn load_settings(vault: &Path) -> AgentSettings {
    let path = settings_path(vault);
    let Ok(raw) = fs::read_to_string(&path) else {
        return AgentSettings::default();
    };
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        eprintln!("herbarium: ignoring invalid {}: {e}", path.display());
        AgentSettings::default()
    })
}

fn save_settings(vault: &Path, settings: &AgentSettings) -> OpResult<()> {
    let path = settings_path(vault);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create settings folder: {e}"))?;
    }
    let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    vault::write_atomic(&path, &bytes)
}

/// A pending agent edit, stored as `<id>.json` next to `<id>.html`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Proposal {
    pub id: String,
    /// When the agent proposed it, in unix ms.
    pub at: i64,
    /// The page's `updatedAt` the agent worked from; accepting fails with a
    /// conflict if the page changed since, unless forced.
    pub base_updated_at: i64,
    /// The `<title>` of the proposed HTML.
    pub title: String,
    pub bytes: usize,
}

fn proposals_dir(vault: &Path) -> PathBuf {
    vault.join(".herbarium").join("proposals")
}

/// The `.html` and `.json` paths of a page's proposal, refusing ids that are
/// not a single path component.
fn proposal_paths(vault: &Path, id: &str) -> OpResult<(PathBuf, PathBuf)> {
    let id = vault::checked_page_id(id)?;
    let dir = proposals_dir(vault);
    Ok((
        dir.join(format!("{id}.html")),
        dir.join(format!("{id}.json")),
    ))
}

/// Refuse to touch a proposals directory that resolves outside the vault.
fn ensure_contained(vault: &Path) -> OpResult<()> {
    let dir = proposals_dir(vault);
    if !dir.exists() {
        return Ok(());
    }
    let base = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
    if vault::is_within_vault(&dir, &base) {
        Ok(())
    } else {
        Err("proposals directory escapes vault".into())
    }
}

fn read_proposal(vault: &Path, id: &str) -> OpResult<Option<Proposal>> {
    ensure_contained(vault)?;
    let (_, json_path) = proposal_paths(vault, id)?;
    let Ok(raw) = fs::read_to_string(&json_path) else {
        return Ok(None);
    };
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|e| format!("invalid proposal for {id}: {e}"))
}

fn read_proposal_html(vault: &Path, id: &str) -> OpResult<String> {
    ensure_contained(vault)?;
    let (html_path, _) = proposal_paths(vault, id)?;
    fs::read_to_string(&html_path).map_err(|e| format!("cannot read proposal: {e}"))
}

/// Remove a page's proposal; a missing one is fine.
pub(crate) fn delete_proposal(vault: &Path, id: &str) -> OpResult<()> {
    ensure_contained(vault)?;
    let (html_path, json_path) = proposal_paths(vault, id)?;
    for path in [json_path, html_path] {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("cannot remove proposal: {e}")),
        }
    }
    Ok(())
}

fn write_proposal(vault: &Path, proposal: &Proposal, html: &str) -> OpResult<()> {
    fs::create_dir_all(proposals_dir(vault))
        .map_err(|e| format!("cannot create proposals folder: {e}"))?;
    ensure_contained(vault)?;
    let (html_path, json_path) = proposal_paths(vault, &proposal.id)?;
    // HTML first: a `.json` always describes a complete `.html`.
    vault::write_atomic(&html_path, html.as_bytes())?;
    let meta = serde_json::to_vec_pretty(proposal).map_err(|e| e.to_string())?;
    vault::write_atomic(&json_path, &meta)
}

/// Every pending proposal for a page that still exists, newest first.
fn list(ctx: &Ctx) -> OpResult<Vec<Value>> {
    let vault = &ctx.store.vault;
    ensure_contained(vault)?;
    let Ok(entries) = fs::read_dir(proposals_dir(vault)) else {
        return Ok(Vec::new());
    };
    let mut out: Vec<(Proposal, PageMeta)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        let Ok(Some(proposal)) = read_proposal(vault, id) else {
            continue;
        };
        if proposal.id != id {
            continue;
        }
        if let Ok(Some(meta)) = ctx.store.get_meta(id) {
            out.push((proposal, meta));
        }
    }
    out.sort_by_key(|(p, _)| std::cmp::Reverse(p.at));
    Ok(out
        .into_iter()
        .map(|(p, meta)| {
            json!({
                "id": p.id,
                "at": p.at,
                "baseUpdatedAt": p.base_updated_at,
                "title": p.title,
                "bytes": p.bytes,
                "pageTitle": meta.title,
                "stale": meta.updated_at != p.base_updated_at,
            })
        })
        .collect())
}

/// What `pages.set_html` and `history.restore` do: overwrite the page, or,
/// for an agent when the vault asks for review, keep the HTML as a proposal.
pub(crate) fn overwrite_or_propose(
    ctx: &mut Ctx,
    id: &str,
    html: &str,
    base_updated_at: Option<i64>,
) -> OpResult<Value> {
    let tag = ctx.caller.tag();
    if ctx.caller != Caller::Agent || !load_settings(&ctx.store.vault).review_edits {
        let meta = overwrite_html_by(ctx, id, html, base_updated_at, tag)?;
        return serde_json::to_value(meta).map_err(|e| e.to_string());
    }
    ensure_html(html)?;
    let meta = ctx.page(id)?;
    check_base(&ctx.store.vault, &meta, base_updated_at)?;
    let proposal = Proposal {
        id: meta.id.clone(),
        at: now_ms(),
        base_updated_at: base_updated_at.unwrap_or(meta.updated_at),
        title: extract_title(html),
        bytes: html.len(),
    };
    write_proposal(&ctx.store.vault, &proposal, html)?;
    ctx.emit(events::PROPOSAL_CREATED, json!({ "proposal": proposal }));
    Ok(json!({
        "pendingApproval": true,
        "message": "The user reviews agent edits in this vault: the new HTML is waiting for their approval in Herbarium and the page is unchanged until they accept it. A later edit to the same page replaces this proposal.",
        "proposal": proposal,
        "page": meta,
    }))
}

#[derive(Deserialize)]
struct IdArgs {
    id: String,
}

#[derive(Deserialize)]
struct AcceptArgs {
    id: String,
    /// Apply even if the page changed since the agent read it.
    #[serde(default)]
    force: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateArgs {
    id: String,
    html: String,
    base_updated_at: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigureArgs {
    review_edits: bool,
}

#[derive(Deserialize)]
struct NoArgs {}

impl Extension for Proposals {
    fn id(&self) -> &str {
        "core.proposals"
    }

    fn register(&self, r: &mut Registry) -> OpResult<()> {
        r.on(events::PAGE_DELETED, |store, event| {
            if let Some(id) = event.payload["page"]["id"].as_str() {
                let _ = delete_proposal(&store.vault, id);
            }
        });

        r.add(Operation::new(
            "agents.settings",
            "Vault-wide agent settings. `reviewEdits`: when true, `pages_set_html` and `history_restore` from an agent do not change the page; they leave a proposal the user accepts or rejects in Herbarium.",
            object(json!({}), &[]),
            |ctx: &mut Ctx, _: NoArgs| Ok(load_settings(&ctx.store.vault)),
        ))?;

        r.add(
            Operation::new(
                "agents.configure",
                "Set whether agent rewrites of a page's HTML wait for approval.",
                object(
                    json!({ "reviewEdits": { "type": "boolean" } }),
                    &["reviewEdits"],
                ),
                |ctx: &mut Ctx, a: ConfigureArgs| {
                    let settings = AgentSettings {
                        review_edits: a.review_edits,
                    };
                    save_settings(&ctx.store.vault, &settings)?;
                    Ok(settings)
                },
            )
            .ui_only(),
        )?;

        r.add(Operation::new(
            "proposals.list",
            "Agent edits waiting for the user's approval, newest first. Each entry has the page `id`, when it was proposed (`at`), the proposed `title`, the page's current `pageTitle`, and `stale` when the page changed after the agent read it.",
            object(json!({}), &[]),
            |ctx: &mut Ctx, _: NoArgs| list(ctx),
        ))?;

        r.add(Operation::new(
            "proposals.get",
            "Read the HTML an agent proposed for a page, with the proposal's details.",
            object(json!({ "id": id_prop() }), &["id"]),
            |ctx: &mut Ctx, a: IdArgs| {
                let vault = &ctx.store.vault;
                let proposal = read_proposal(vault, &a.id)?
                    .ok_or_else(|| format!("no proposal for page {}", a.id))?;
                let html = read_proposal_html(vault, &a.id)?;
                Ok(json!({ "proposal": proposal, "html": html }))
            },
        ))?;

        r.add(
            Operation::new(
                "proposals.create",
                "Keep HTML as a proposal for a page, to compare and accept or reject (what a remix by an AI model produces). Replaces the page's earlier proposal.",
                object(
                    json!({
                        "id": id_prop(),
                        "html": { "type": "string" },
                        "baseUpdatedAt": { "type": "integer" }
                    }),
                    &["id", "html"],
                ),
                |ctx: &mut Ctx, a: CreateArgs| {
                    ensure_html(&a.html)?;
                    let meta = ctx.page(&a.id)?;
                    let proposal = Proposal {
                        id: meta.id.clone(),
                        at: now_ms(),
                        base_updated_at: a.base_updated_at.unwrap_or(meta.updated_at),
                        title: extract_title(&a.html),
                        bytes: a.html.len(),
                    };
                    write_proposal(&ctx.store.vault, &proposal, &a.html)?;
                    ctx.emit(events::PROPOSAL_CREATED, json!({ "proposal": proposal }));
                    Ok(proposal)
                },
            )
            .ui_only(),
        )?;

        r.add(
            Operation::new(
                "proposals.accept",
                "Apply an agent's proposed HTML to its page (the replaced HTML is kept in the page history). Fails with a `conflict:` error if the page changed after the agent read it, unless `force` is true.",
                object(
                    json!({ "id": id_prop(), "force": { "type": "boolean" } }),
                    &["id"],
                ),
                |ctx: &mut Ctx, a: AcceptArgs| {
                    let vault = ctx.store.vault.clone();
                    let proposal = read_proposal(&vault, &a.id)?
                        .ok_or_else(|| format!("no proposal for page {}", a.id))?;
                    let html = read_proposal_html(&vault, &a.id)?;
                    let base = (!a.force).then_some(proposal.base_updated_at);
                    let meta = overwrite_html_by(ctx, &a.id, &html, base, Caller::Agent.tag())?;
                    delete_proposal(&vault, &a.id)?;
                    ctx.emit(events::PROPOSAL_RESOLVED, json!({ "id": a.id, "accepted": true }));
                    Ok(meta)
                },
            )
            .ui_only(),
        )?;

        r.add(
            Operation::new(
                "proposals.reject",
                "Discard an agent's proposed HTML; the page is left as it is.",
                object(json!({ "id": id_prop() }), &["id"]),
                |ctx: &mut Ctx, a: IdArgs| {
                    if read_proposal(&ctx.store.vault, &a.id)?.is_none() {
                        return Err(format!("no proposal for page {}", a.id));
                    }
                    delete_proposal(&ctx.store.vault, &a.id)?;
                    ctx.emit(
                        events::PROPOSAL_RESOLVED,
                        json!({ "id": a.id, "accepted": false }),
                    );
                    Ok(json!({ "rejected": a.id }))
                },
            )
            .ui_only(),
        )
    }
}
