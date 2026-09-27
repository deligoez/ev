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

fn line(n: &Value) -> String {
    let mut out = format!("#{:<4} {}", n["id"], s(n, "path_text"));
    if n["code"].is_string() {
        let _ = write!(out, "  [{}]", s(n, "name"));
    }
    if let Some(q) = n.get("qty").and_then(Value::as_i64) {
        let _ = write!(out, "  x{q}");
    }
    match s(n, "state").as_str() {
        "candidate" => {
            let _ = write!(out, "  (candidate: {})", s(n, "disposition"));
        }
        "gone" => {
            let _ = write!(out, "  (gone: {})", s(n, "disposition"));
        }
        _ => {}
    }
    if n["lost"].as_bool() == Some(true) {
        out.push_str("  (lost)");
    }
    out
}

