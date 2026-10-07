//! Herbarium core: the vault kernel (plain `.html` + `.json` files indexed in
//! SQLite), the extension API, and the built-in features. Front ends (the
//! desktop app, the MCP server) run everything through [`Host::call`].

mod builtin;
pub mod content;
pub mod extension;
mod host;
pub mod models;
pub mod query;
pub mod store;
pub mod time;
pub mod vault;

pub use extension::{Caller, Ctx, Event, Extension, OpResult, Operation, Registry, events};
pub use host::Host;
