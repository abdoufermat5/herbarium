// The extension API. Every feature, built-in or third-party, is an
// `Extension` that registers named `Operation`s (JSON in, JSON out) and may
// subscribe to `Event`s emitted by other operations. Front ends (desktop UI,
// MCP, …) only ever talk to the registry, so a new operation is reachable from
// all of them without extra wiring. The JSON-only boundary is deliberate: it is
// the shape a sandboxed (WASM) plugin runtime can implement later.

use std::collections::BTreeMap;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::models::PageMeta;
use crate::store::Store;
use crate::vault;

pub type OpResult<T> = Result<T, String>;

type Handler = Box<dyn Fn(&mut Ctx, Value) -> OpResult<Value> + Send + Sync>;
type Subscriber = Box<dyn Fn(&Store, &Event) + Send + Sync>;

/// Who is invoking an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caller {
    /// The desktop interface.
    Ui,
    /// An AI agent or other automation (MCP, CLI).
    Agent,
}

impl Caller {
    /// Stable tag recorded with a history snapshot: `ui` or `agent`.
    pub fn tag(self) -> &'static str {
        match self {
            Caller::Ui => "ui",
            Caller::Agent => "agent",
        }
    }
}

pub struct Operation {
    /// Namespaced `area.verb`, e.g. `pages.create`.
    pub name: String,
    /// Shown to agents as the tool description.
    pub description: String,
    /// JSON Schema of the arguments object.
    pub input_schema: Value,
    /// False for operations that only make sense inside the desktop UI.
    pub for_agents: bool,
    handler: Handler,
}

impl Operation {
    /// Build an operation from a typed handler; arguments are deserialized
    /// from, and the result serialized to, JSON.
    pub fn new<A, R, F>(name: &str, description: &str, input_schema: Value, handler: F) -> Self
    where
        A: DeserializeOwned,
        R: Serialize,
        F: Fn(&mut Ctx, A) -> OpResult<R> + Send + Sync + 'static,
    {
        let op_name = name.to_string();
        Operation {
            name: name.to_string(),
            description: description.to_string(),
            input_schema,
            for_agents: true,
            handler: Box::new(move |ctx, args| {
                let args = if args.is_null() {
                    Value::Object(Default::default())
                } else {
                    args
                };
                let args = serde_json::from_value(args)
                    .map_err(|e| format!("{op_name}: invalid arguments: {e}"))?;
                let out = handler(ctx, args)?;
                serde_json::to_value(out).map_err(|e| e.to_string())
            }),
        }
    }

    /// Hide the operation from agents.
    pub fn ui_only(mut self) -> Self {
        self.for_agents = false;
        self
    }

    pub fn allows(&self, caller: Caller) -> bool {
        caller == Caller::Ui || self.for_agents
    }

    pub(crate) fn run(&self, ctx: &mut Ctx, args: Value) -> OpResult<Value> {
        (self.handler)(ctx, args)
    }
}

/// Something that happened in the vault, broadcast after the operation that
/// caused it succeeded. Names are `area.past-tense`, e.g. `page.created`.
#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub name: String,
    pub payload: Value,
}

pub mod events {
    pub const PAGE_CREATED: &str = "page.created";
    pub const PAGE_UPDATED: &str = "page.updated";
    pub const PAGE_DELETED: &str = "page.deleted";
    pub const VAULT_INDEXED: &str = "vault.indexed";
}

pub trait Extension: Send + Sync {
    /// Stable, unique id, also the key of this extension's `PageMeta::ext` data.
    fn id(&self) -> &str;
    fn register(&self, registry: &mut Registry) -> OpResult<()>;
}

#[derive(Default)]
pub struct Registry {
    ops: BTreeMap<String, Operation>,
    subscribers: Vec<(String, Subscriber)>,
}

impl Registry {
    pub fn add(&mut self, op: Operation) -> OpResult<()> {
        if self.ops.contains_key(&op.name) {
            return Err(format!("operation {} is already registered", op.name));
        }
        self.ops.insert(op.name.clone(), op);
        Ok(())
    }

    /// Run `handler` after every successful operation that emits `event`.
    pub fn on(&mut self, event: &str, handler: impl Fn(&Store, &Event) + Send + Sync + 'static) {
        self.subscribers
            .push((event.to_string(), Box::new(handler)));
    }

    pub fn get(&self, name: &str) -> Option<&Operation> {
        self.ops.get(name)
    }

    pub fn operations(&self) -> impl Iterator<Item = &Operation> {
        self.ops.values()
    }

    pub(crate) fn dispatch(&self, store: &Store, events: &[Event]) {
        for event in events {
            for (name, sub) in &self.subscribers {
                if *name == event.name {
                    sub(store, event);
                }
            }
        }
    }
}

/// What an operation handler can touch: the open vault, plus an outbox of
/// events delivered once the handler returns successfully.
pub struct Ctx<'a> {
    pub caller: Caller,
    pub store: &'a Store,
    events: Vec<Event>,
}

impl<'a> Ctx<'a> {
    pub(crate) fn new(store: &'a Store, caller: Caller) -> Self {
        Ctx {
            caller,
            store,
            events: Vec::new(),
        }
    }

    pub(crate) fn into_events(self) -> Vec<Event> {
        self.events
    }

    pub fn emit(&mut self, name: &str, payload: impl Serialize) {
        let payload = serde_json::to_value(payload).unwrap_or(Value::Null);
        self.events.push(Event {
            name: name.to_string(),
            payload,
        });
    }

    pub fn page(&self, id: &str) -> OpResult<PageMeta> {
        self.store
            .get_meta(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("page not found: {id}"))
    }

    /// Persist metadata (sidecar + index), keeping the indexed text, and emit
    /// `page.updated`.
    pub fn save(&mut self, meta: &PageMeta) -> OpResult<()> {
        vault::write_meta(&self.store.vault, meta)?;
        let text = self
            .store
            .text_for(&meta.id)
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let mtime = vault::page_mtime(&self.store.vault, meta);
        self.store
            .upsert(meta, &text, mtime)
            .map_err(|e| e.to_string())?;
        self.emit(events::PAGE_UPDATED, serde_json::json!({ "page": meta }));
        Ok(())
    }
}
