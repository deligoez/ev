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

fn review_mark(r: &Value) -> &'static str {
    match r["status"].as_str() {
        Some("toured") if r["changed_since"] == true => "[toured, changed since]",
        Some("toured") => "[toured]",
        Some("kept") => "[kept as is]",
        _ => "[raw]",
    }
}

fn task_line(out: &mut String, t: &Value) {
    let pos = t["position"]
        .as_i64()
        .map(|p| format!("{p}."))
        .unwrap_or_else(|| "-".into());
    let state = match t["status"].as_str() {
        Some("doing") => " (in progress)",
        Some("done") => " (done)",
        Some("dropped") => " (dropped)",
        _ => "",
    };
    let _ = writeln!(out, "{pos} #{} {}{state}", t["id"], s(t, "title"));
    let _ = writeln!(out, "     why: {}", s(t, "why"));
    for n in t["nodes"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "     • {}", s(n, "path_text"));
    }
}

fn progress_line(p: &Value) -> String {
    format!(
        "{} places: {} toured, {} kept as is, {} raw; {} changed since their tour",
        p["units"], p["toured"], p["kept"], p["raw"], p["changed_since_tour"]
    )
}

pub fn human(v: &Value) -> String {
    let mut out = String::new();
    if v.get("open_tasks").is_some() {
        let _ = writeln!(out, "Goal: {}", v["goal"].as_str().unwrap_or("(not set)"));
        let _ = writeln!(out, "{}", progress_line(&v["progress"]));
        if v["task"].is_object() {
            let _ = writeln!(out, "\nNext task ({} open):", v["open_tasks"]);
            task_line(&mut out, &v["task"]);
            for p in v["task"]["places"].as_array().into_iter().flatten() {
                let _ = writeln!(
                    out,
                    "\n  {} {}",
                    s(&p["node"], "path_text"),
                    review_mark(&p["review"])
                );
                for o in p["observations"].as_array().into_iter().flatten() {
                    let _ = writeln!(out, "    observed: {}", s(o, "text"));
                }
                for c in p["children"].as_array().into_iter().flatten() {
                    let _ = writeln!(out, "    └ {}", line(c));
                }
                for a in p["arriving"].as_array().into_iter().flatten() {
                    let _ = writeln!(out, "    → arriving: {}", s(a, "path_text"));
                }
            }
        } else {
            out.push_str("\n(no open task)\n");
        }
        let un = v["unplanned"].as_array().map_or(0, Vec::len);
        if un > 0 {
            let _ = writeln!(out, "\nRaw places no task covers ({un}):");
            for p in v["unplanned"].as_array().into_iter().flatten() {
                let _ = writeln!(out, "  {}", s(p, "path_text"));
            }
        }
        return out;
    }
    if v.get("units").is_some() && v.get("places").is_some() {
        let _ = writeln!(out, "{}", progress_line(v));
        for p in v["places"].as_array().into_iter().flatten() {
            let unknown = if p["unknown"] == true {
                "  (contents unknown)"
            } else {
                ""
            };
            let _ = writeln!(
                out,
                "  {} {}{unknown}",
                review_mark(&p["review"]),
                s(p, "path_text")
            );
        }
        return out;
    }
    if let Some(list) = v.get("tasks").and_then(Value::as_array) {
        if list.is_empty() {
            out.push_str("(no tasks)\n");
        }
        for t in list {
            task_line(&mut out, t);
        }
        return out;
    }
    if v.get("why").is_some() && v.get("title").is_some() {
        task_line(&mut out, v);
        return out;
    }
    if v.get("goal").is_some() && v.as_object().is_some_and(|o| o.len() == 1) {
        let _ = writeln!(out, "Goal: {}", v["goal"].as_str().unwrap_or("(not set)"));
        return out;
    }
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
        if v["review"].is_object() {
            let _ = writeln!(
                out,
                "  review: {} ({})",
                s(&v["review"], "status"),
                s(&v["review"], "at")
            );
        }
        for o in v["observations"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "  observed #{}: {}", o["id"], s(o, "text"));
        }
        for t in v["tasks"].as_array().into_iter().flatten() {
            let via = if t["via"] == node["id"] {
                String::new()
            } else {
                format!("  (via #{})", t["via"])
            };
            let _ = writeln!(
                out,
                "  task {}. #{} {}{via}",
                t["position"],
                t["id"],
                s(t, "title")
            );
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
