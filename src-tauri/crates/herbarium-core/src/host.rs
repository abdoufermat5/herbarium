// The host ties a set of extensions to an open vault and is the single entry
// point every front end uses to run operations.

use std::path::Path;

use serde_json::{json, Value};

use crate::builtin;
use crate::extension::{events, Caller, Ctx, Extension, OpResult, Operation, Registry};
use crate::models::IndexReport;
use crate::store::Store;
use crate::vault;

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
        Ok(Host { registry, extensions, store: None })
    }

    /// Open (creating if needed) a vault and re-index it from disk.
    pub fn open_vault(&mut self, path: &str) -> OpResult<IndexReport> {
        let canonical = vault::normalize(path)?;
        let store = Store::open(canonical).map_err(|e| e.to_string())?;
        let report = reindex(&store)?;
        self.registry.dispatch(&store, &[indexed_event(&report)]);
        self.store = Some(store);
        Ok(report)
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
        self.registry.operations().filter(move |op| op.allows(caller))
    }

    pub fn call(&self, caller: Caller, name: &str, args: Value) -> OpResult<Value> {
        let op = self
            .registry
            .get(name)
            .filter(|op| op.allows(caller))
            .ok_or_else(|| format!("unknown operation: {name}"))?;
        let store = self.store.as_ref().ok_or("no vault open")?;
        let mut ctx = Ctx::new(store);
        let out = op.run(&mut ctx, args)?;
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
    let (indexed, removed) = vault::index_vault(store)?;
    let total = store.count().map_err(|e| e.to_string())?;
    Ok(IndexReport { indexed, removed, total })
}

pub(crate) fn indexed_event(report: &IndexReport) -> crate::extension::Event {
    crate::extension::Event { name: events::VAULT_INDEXED.into(), payload: json!(report) }
}
