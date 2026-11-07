// The host ties a set of extensions to an open vault and is the single entry
// point every front end uses to run operations.

use std::path::Path;

use serde_json::{Value, json};

use crate::builtin;
use crate::extension::{Caller, Ctx, Extension, OpResult, Operation, Registry, events};
use crate::lock::VaultLock;
use crate::models::IndexReport;
use crate::store::Store;
use crate::vault;

/// Operations that never write to the vault or the index; everything else
/// takes the vault lock. Unknown or new operations therefore default to
/// locked, which is always safe.
const READ_ONLY_OPS: &[&str] = &[
    "agents.settings",
    "appearance.get",
    "folders.list",
    "highlights.list",
    "history.get",
    "history.list",
    "network.settings",
    "pages.find_by_url",
    "pages.graph",
    "pages.links",
    "pages.list",
    "pages.search",
    "paths.get",
    "paths.list",
    "paths.progress",
    "previews.list",
    "proposals.get",
    "proposals.list",
    "published.list",
    "review.due",
    "review.settings",
    "review.stats",
    "searches.list",
    "tags.list",
];

pub struct Host {
    registry: Registry,
    extensions: Vec<String>,
    store: Option<Store>,
}

impl Host {
    /// A host with the built-in extensions only.
    pub fn new() -> Self {
        Self::with_extensions(Vec::new()).expect("built-in extensions register cleanly")
    }

    /// A host with the built-in extensions followed by `extra`.
    pub fn with_extensions(extra: Vec<Box<dyn Extension>>) -> OpResult<Self> {
        let mut registry = Registry::default();
        let mut extensions = Vec::new();
        for ext in builtin::all().into_iter().chain(extra) {
            if extensions.iter().any(|id| id == ext.id()) {
                return Err(format!("extension {} is already registered", ext.id()));
            }
            ext.register(&mut registry)
                .map_err(|e| format!("extension {}: {e}", ext.id()))?;
            extensions.push(ext.id().to_string());
        }
        Ok(Host {
            registry,
            extensions,
            store: None,
        })
    }

    /// Open (creating if needed) a vault and re-index it from disk.
    pub fn open_vault(&mut self, path: &str) -> OpResult<IndexReport> {
        let canonical = vault::normalize(path)?;
        let store = Store::open(canonical).map_err(|e| e.to_string())?;
        let report = {
            let _lock = VaultLock::acquire(&store.vault)?;
            reindex(&store)?
        };
        self.registry.dispatch(&store, &[indexed_event(&report)]);
        self.store = Some(store);
        Ok(report)
    }

    /// Attach a read-only handle to the index of a vault that another host
    /// (this process or another) has already opened and indexed. It never
    /// indexes, migrates, locks or writes, so it answers read operations
    /// (see `READ_ONLY_OPS`) while a writer is busy; calling a mutating
    /// operation on it fails at the first write.
    pub fn open_vault_read_only(&mut self, vault: &Path) -> OpResult<()> {
        let store = Store::open_read_only(vault.to_path_buf()).map_err(|e| e.to_string())?;
        self.store = Some(store);
        Ok(())
    }

    pub fn vault_path(&self) -> Option<&Path> {
        self.store.as_ref().map(|s| s.vault.as_path())
    }

    pub fn store(&self) -> Option<&Store> {
        self.store.as_ref()
    }

    pub fn extensions(&self) -> &[String] {
        &self.extensions
    }

    /// Operations visible to `caller`, sorted by name.
    pub fn operations(&self, caller: Caller) -> impl Iterator<Item = &Operation> {
        self.registry
            .operations()
            .filter(move |op| op.allows(caller))
    }

    pub fn call(&self, caller: Caller, name: &str, args: Value) -> OpResult<Value> {
        let _span = tracing::debug_span!("host.call", op = name, caller = caller.tag()).entered();
        let started = std::time::Instant::now();
        let result = self.call_inner(caller, name, args);
        tracing::debug!(
            elapsed_ms = started.elapsed().as_secs_f64() * 1e3,
            ok = result.is_ok()
        );
        result
    }

    fn call_inner(&self, caller: Caller, name: &str, args: Value) -> OpResult<Value> {
        let op = self
            .registry
            .get(name)
            .filter(|op| op.allows(caller))
            .ok_or_else(|| format!("unknown operation: {name}"))?;
        let store = self.store.as_ref().ok_or("no vault open")?;
        let mut ctx = Ctx::new(store, caller);
        // Mutating operations are serialised across processes; the lock is
        // released before subscribers run (they only touch the index).
        let lock = if READ_ONLY_OPS.contains(&name) {
            None
        } else {
            Some(VaultLock::acquire(&store.vault)?)
        };
        let out = op.run(&mut ctx, args)?;
        drop(lock);
        self.registry.dispatch(store, &ctx.into_events());
        Ok(out)
    }
}

impl Default for Host {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn reindex(store: &Store) -> OpResult<IndexReport> {
    let started = std::time::Instant::now();
    let outcome = vault::index_vault(store)?;
    let total = store.count().map_err(|e| e.to_string())?;
    tracing::info!(
        indexed = outcome.indexed,
        removed = outcome.removed,
        skipped = outcome.skipped.len(),
        total,
        elapsed_ms = started.elapsed().as_secs_f64() * 1e3,
        "index_vault"
    );
    Ok(IndexReport {
        indexed: outcome.indexed,
        removed: outcome.removed,
        total,
        skipped: outcome.skipped,
    })
}

pub(crate) fn indexed_event(report: &IndexReport) -> crate::extension::Event {
    crate::extension::Event {
        name: events::VAULT_INDEXED.into(),
        payload: json!(report),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_list_names_registered_operations() {
        let host = Host::new();
        for name in READ_ONLY_OPS {
            assert!(
                host.registry.get(name).is_some(),
                "{name} is not registered"
            );
        }
    }
}
