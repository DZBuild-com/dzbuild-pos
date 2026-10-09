//! The TypeScript shape of free-form JSON crossing the IPC boundary.

use serde::Serialize;
use specta::Type;
use std::collections::BTreeMap;

/// Any JSON value, exported to TypeScript as the named recursive type `JsonValue`
/// (`null | boolean | number | string | JsonValue[] | { [key in string]: JsonValue }`).
///
/// specta rc.25 describes `serde_json::Value` as an *inline* recursive type, which its
/// TypeScript exporter refuses to expand. So a field of an exported type that holds a
/// `serde_json::Value` keeps that Rust type and is annotated `#[specta(type = JsonValue)]`
/// (`Option<JsonValue>` for an optional one). Type-level only: never built or serialised.
#[derive(Serialize, Type)]
#[serde(untagged)]
pub enum JsonValue {
    // `()` exports as `null`; a unit variant would export as the string `"Null"`.
    Null(()),
    Bool(bool),
    Number(i64),
    String(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}
