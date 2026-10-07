// Built-in features, written against the same `Extension` API a plugin uses.

mod fsrs;
mod history;
mod library;
mod links;
mod network;
mod pages;
pub use pages::import_key;
mod paths;
mod proposals;
mod review;
mod searches;
mod storage;
mod today;

use serde_json::{Map, Value, json};

use crate::extension::Extension;

pub(crate) fn all() -> Vec<Box<dyn Extension>> {
    vec![
        Box::new(pages::Pages),
        Box::new(library::Library),
        Box::new(review::Review),
        Box::new(storage::Storage),
        Box::new(network::Network),
        Box::new(history::History),
        Box::new(proposals::Proposals),
        Box::new(searches::Searches),
        Box::new(links::Links),
        Box::new(paths::Paths),
        Box::new(today::Today),
    ]
}

/// JSON Schema for an arguments object.
fn object(properties: Value, required: &[&str]) -> Value {
    let mut schema = Map::new();
    schema.insert("type".into(), json!("object"));
    schema.insert("properties".into(), properties);
    if !required.is_empty() {
        schema.insert("required".into(), json!(required));
    }
    schema.insert("additionalProperties".into(), json!(false));
    Value::Object(schema)
}

fn id_prop() -> Value {
    json!({ "type": "string", "description": "Page id." })
}
