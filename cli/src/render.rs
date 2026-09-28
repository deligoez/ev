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
    out.push_str(&place_marks(n));
    out
}

fn place_marks(n: &Value) -> String {
    let mut out = String::new();
    if n["unknown"] == true {
        out.push_str("  (contents unknown)");
    }
    if let Some(x) = n["to"].as_str() {
        let _ = write!(out, "  (to: {x})");
    }
    if let Some(x) = n["owner"].as_str() {
        let _ = write!(out, "  (owner: {x})");
    }
    if let Some(x) = n["with"].as_str() {
        let _ = write!(out, "  (with: {x})");
    }
    out
}

fn place_head(p: &Value) -> String {
    let aliases: Vec<&str> = p["aliases"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    format!("{}  ({})", s(p, "name"), aliases.join(", "))
}

fn errands(out: &mut String, e: &Value) {
    let _ = writeln!(out, "{}", place_head(&e["place"]));
    for (key, title) in [
        ("take", "take"),
        ("return", "return (theirs)"),
        ("collect", "collect (lent)"),
    ] {
        let list = e[key].as_array().cloned().unwrap_or_default();
        if list.is_empty() {
            continue;
        }
        let _ = writeln!(out, "  {title}:");
        for n in &list {
            let _ = writeln!(out, "    {}", s(n, "path_text"));
        }
    }
}

fn tree(out: &mut String, n: &Value, indent: usize) {
    let label = match n["code"].as_str() {
        Some(c) => format!("{c}  {}", s(n, "name")),
        None => s(n, "name"),
    };
    let mut extra = String::new();
    if let Some(q) = n["qty"].as_i64() {
        let _ = write!(extra, "  ×{q}");
    }
    if s(n, "state") == "candidate" {
        let _ = write!(extra, "  (candidate: {})", s(n, "disposition"));
    }
    if n["lost"].as_bool() == Some(true) {
        extra.push_str("  (lost)");
    }
    extra.push_str(&place_marks(n));
    let total = n["items"].as_i64().unwrap_or(0);
    if total > 0 {
        let _ = write!(extra, "  [{total} items]");
    }
    let _ = writeln!(
        out,
        "{}{label}  #{} ({}){extra}",
        "  ".repeat(indent),
        n["id"],
        s(n, "kind")
    );
    for c in n["children"].as_array().into_iter().flatten() {
        tree(out, c, indent + 1);
    }
}

pub fn human(v: &Value) -> String {
    let mut out = String::new();
    if let Some(node) = v.get("node").filter(|_| v.get("children").is_some()) {
        let _ = writeln!(out, "{}", line(node));
        for key in ["kind", "note", "theme", "address", "to", "owner", "with"] {
            if let Some(x) = node[key].as_str() {
                let _ = writeln!(out, "  {key}: {x}");
            }
        }
        if let Some(f) = node["fill"].as_i64() {
            let _ = writeln!(out, "  fill: {f}%");
        }
        if let Some(tags) = node["tags"].as_array().filter(|t| !t.is_empty()) {
            let tags: Vec<_> = tags.iter().filter_map(Value::as_str).collect();
            let _ = writeln!(out, "  tags: {}", tags.join(", "));
        }
        for p in node["photos"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "  photo: {}", p.as_str().unwrap_or_default());
        }
        if v["pending"].is_object() {
            let _ = writeln!(out, "  pending move → {}", s(&v["pending"], "path_text"));
        }
        if v["last_seen"].is_object() {
            let _ = writeln!(out, "  last seen: {}", s(&v["last_seen"], "path_text"));
        }
        for c in v["children"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "  └ {}", line(c));
        }
        return out;
    }
    if let Some(rules) = v.get("rules").and_then(Value::as_array) {
        if !rules.is_empty() || v.get("containers").is_none() {
            let _ = writeln!(out, "Rules:");
            for r in rules {
                let _ = writeln!(out, "  {}. {}", r["id"], s(r, "text"));
            }
        }
        if v.get("containers").is_none() {
            return out;
        }
        let _ = writeln!(out, "Similar things are in:");
        for x in v["similar"].as_array().into_iter().flatten() {
            let _ = writeln!(
                out,
                "  {}  ({}: {})",
                s(&x["container"], "path_text"),
                x["count"],
                x["matches"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        let _ = writeln!(
            out,
            "All {} places that can hold something:",
            v["complete"]["containers"]
        );
        for c in v["containers"].as_array().into_iter().flatten() {
            let theme = c["theme"]
                .as_str()
                .map(|t| format!("  [{t}]"))
                .unwrap_or_default();
            let _ = writeln!(out, "  {}{theme}  {} items", s(c, "path_text"), c["items"]);
        }
        return out;
    }
    if v.get("spread").is_some() && v.get("no_theme").is_some() {
        let _ = writeln!(out, "Alike things in several places:");
        for x in v["spread"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "  {}", s(x, "word"));
            for p in x["places"].as_array().into_iter().flatten() {
                let _ = writeln!(out, "    {}", s(p, "path_text"));
            }
        }
        let _ = writeln!(out, "Holders without a theme:");
        for n in v["no_theme"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "  {}", s(n, "path_text"));
        }
        let _ = writeln!(out, "Items lying loose in a room or on furniture:");
        for n in v["loose"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "  {}", s(n, "path_text"));
        }
        return out;
    }
    if v.get("take").is_some() && v.get("place").is_some() {
        errands(&mut out, v);
        return out;
    }
    if let Some(list) = v.get("errands").and_then(Value::as_array) {
        if list.is_empty() {
            out.push_str("(nothing to take, return or collect)\n");
        }
        for e in list {
            errands(&mut out, e);
        }
        return out;
    }
    if let Some(list) = v.get("places").and_then(Value::as_array) {
        for p in list {
            let _ = writeln!(
                out,
                "{}  take {} · return {} · collect {}",
                place_head(p),
                p["take"],
                p["return"],
                p["collect"]
            );
        }
        return out;
    }
    if v.get("aliases").is_some() && v.get("name").is_some() {
        let _ = writeln!(out, "{}", place_head(v));
        return out;
    }
    if let Some(t) = v.get("tree").and_then(Value::as_array) {
        for n in t {
            tree(&mut out, n, 0);
        }
        for n in v["unplaced"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "(place unknown) {}", line(n));
        }
        return out;
    }
    if let Some(events) = v.get("events").and_then(Value::as_array) {
        let _ = writeln!(out, "{}", line(&v["node"]));
        for e in events {
            let _ = writeln!(out, "  {}  {:<8} {}", s(e, "at"), s(e, "type"), e["data"]);
        }
        return out;
    }
    for key in ["results", "created"] {
        if let Some(list) = v.get(key).and_then(Value::as_array) {
            if list.is_empty() {
                out.push_str("(none)\n");
            }
            for n in list {
                let _ = writeln!(out, "{}", line(n));
            }
            return out;
        }
    }
    if let Some(list) = v.get("pending").and_then(Value::as_array) {
        if list.is_empty() {
            out.push_str("(no pending moves)\n");
        }
        for m in list {
            let _ = writeln!(
                out,
                "{}\n    → {}",
                line(&m["node"]),
                s(&m["to"], "path_text")
            );
        }
        return out;
    }
    if let Some(list) = v.get("lost").and_then(Value::as_array) {
        if list.is_empty() {
            out.push_str("(nothing lost)\n");
        }
        for m in list {
            let seen = m["last_seen"]
                .get("path_text")
                .and_then(Value::as_str)
                .unwrap_or("never known");
            let _ = writeln!(out, "{}\n    last seen: {seen}", line(&m["node"]));
        }
        return out;
    }
    if let Some(groups) = v.get("disposals").and_then(Value::as_object) {
        for (d, list) in groups {
            let _ = writeln!(out, "{d}:");
            for n in list.as_array().into_iter().flatten() {
                let _ = writeln!(out, "  {}", line(n));
                for p in n["parts"].as_array().into_iter().flatten() {
                    let _ = writeln!(out, "      + {}", s(p, "name"));
                }
            }
        }
        return out;
    }
    let _ = writeln!(out, "{v}");
    out
}

pub fn error(e: &Error) -> String {
    let mut out = format!("error: {e}\n");
    let v = e.to_json();
    for c in v["error"]["candidates"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  {}", line(c));
    }
    for c in v["error"]["details"]["children"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let _ = writeln!(out, "  holds {}", line(c));
    }
    out
}
