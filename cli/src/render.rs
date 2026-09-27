//! Readable text for a terminal; JSON is the contract, this is a courtesy.

use std::fmt::Write;

use ev_core::Error;
use serde_json::Value;

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

