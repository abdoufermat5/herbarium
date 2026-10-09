//! Herbarium core: the vault kernel (plain `.html` + `.json` files indexed in
//! SQLite), the extension API, and the built-in features. Front ends (the
//! desktop app, the MCP server) run everything through [`Host::call`].

pub mod assets;
pub(crate) mod builtin;
pub mod content;
pub mod extension;
mod host;
pub mod importer;
pub mod models;
pub mod query;
pub mod rewrite;
pub mod store;
pub mod time;
pub mod vault;

pub use builtin::{SCRIPT_HOSTS, STYLE_HOSTS, import_key};
pub use extension::{Caller, Ctx, Event, Extension, OpResult, Operation, Registry, events};
pub use host::Host;
