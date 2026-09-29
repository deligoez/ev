//! Readable text for a terminal; JSON is the contract, this is a courtesy. The words follow the
//! language setting (`ev settings`); JSON and error messages stay English.

use std::fmt::Write;

use ev_core::Error;
use serde_json::Value;

use crate::i18n::{t, tf};

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn disposition(d: &str) -> String {
    match d {
        "trash" => t("trash"),
        "give" => t("give"),
        "sell" => t("sell"),
        "return" => t("return"),
        "mistake" => t("record error"),
        other => return other.to_string(),
    }
    .to_string()
}

fn kind(k: &str) -> String {
    match k {
        "home" => t("home"),
        "room" => t("room"),
        "furniture" => t("furniture"),
        "container" => t("container"),
        "item" => t("item"),
        other => return other.to_string(),
    }
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
            out.push_str(&tf(
                "  (candidate: {})",
                &[&disposition(&s(n, "disposition"))],
            ));
        }
        "gone" => {
            out.push_str(&tf("  (gone: {})", &[&disposition(&s(n, "disposition"))]));
        }
        _ => {}
    }
    if n["lost"].as_bool() == Some(true) {
        out.push_str(t("  (lost)"));
    }
    out.push_str(&place_marks(n));
    out
}

fn place_marks(n: &Value) -> String {
    let mut out = String::new();
    if n["unknown"] == true {
        out.push_str(t("  (contents unknown)"));
    }
    if let Some(x) = n["to"].as_str() {
        out.push_str(&tf("  (to: {})", &[&x]));
    }
    if let Some(x) = n["owner"].as_str() {
        out.push_str(&tf("  (owner: {})", &[&x]));
    }
    if let Some(x) = n["with"].as_str() {
        out.push_str(&tf("  (with: {})", &[&x]));
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
        ("take", t("take")),
        ("return", t("return (theirs)")),
        ("collect", t("collect (lent)")),
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
        extra.push_str(&tf(
            "  (candidate: {})",
            &[&disposition(&s(n, "disposition"))],
        ));
    }
    if n["lost"].as_bool() == Some(true) {
        extra.push_str(t("  (lost)"));
    }
    extra.push_str(&place_marks(n));
    let total = n["items"].as_i64().unwrap_or(0);
    if total > 0 {
        extra.push_str(&tf("  [{} items]", &[&total]));
    }
    let _ = writeln!(
        out,
        "{}{label}  #{} ({}){extra}",
        "  ".repeat(indent),
        n["id"],
        kind(&s(n, "kind"))
    );
    for c in n["children"].as_array().into_iter().flatten() {
        tree(out, c, indent + 1);
    }
}

fn review_mark(r: &Value) -> &'static str {
    match r["status"].as_str() {
        Some("toured") if r["changed_since"] == true => t("[toured, changed since]"),
        Some("toured") => t("[toured]"),
        Some("kept") => t("[kept as is]"),
        _ => t("[raw]"),
    }
}

fn task_line(out: &mut String, t_: &Value) {
    let pos = t_["position"]
        .as_i64()
        .map(|p| format!("{p}."))
        .unwrap_or_else(|| "-".into());
    let state = match t_["status"].as_str() {
        Some("doing") => t(" (in progress)"),
        Some("done") => t(" (done)"),
        Some("dropped") => t(" (dropped)"),
        _ => "",
    };
    let _ = writeln!(out, "{pos} #{} {}{state}", t_["id"], s(t_, "title"));
    let _ = writeln!(out, "     {}", tf("why: {}", &[&s(t_, "why")]));
    for n in t_["nodes"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "     • {}", s(n, "path_text"));
    }
}

fn progress_line(p: &Value) -> String {
    tf(
        "{} places: {} toured, {} kept as is, {} raw; {} changed since their tour",
        &[
            &p["units"],
            &p["toured"],
            &p["kept"],
            &p["raw"],
            &p["changed_since_tour"],
        ],
    )
}

fn goal(v: &Value) -> String {
    match v["goal"].as_str() {
        Some("organize") => t("organize").to_string(),
        Some("track") => t("track").to_string(),
        Some(other) => other.to_string(),
        None => t("(not set)").to_string(),
    }
}

fn need_line(n: &Value) -> String {
    let qty = n["qty"]
        .as_i64()
        .map(|q| format!("{q} × "))
        .unwrap_or_default();
    let make = if n["make"] == true { t(" (make)") } else { "" };
    let for_ = n["for"]
        .get("path_text")
        .and_then(Value::as_str)
        .map(|p| tf("  for {}", &[&p]))
        .unwrap_or_default();
    format!("#{} {qty}{}{make}{for_}", n["id"], s(n, "text"))
}

fn todo(out: &mut String, v: &Value) {
    let c = &v["counts"];
    let _ = writeln!(
        out,
        "{}  ·  {}",
        tf("Goal: {}", &[&goal(v)]),
        progress_line(&v["progress"])
    );
    let head = |out: &mut String, title: &str, n: &Value| {
        if n.as_u64().unwrap_or(0) > 0 {
            let _ = writeln!(out, "\n{title} ({n})");
            true
        } else {
            false
        }
    };
    if head(out, t("Tasks"), &c["tasks"]) {
        for t_ in v["tasks"].as_array().into_iter().flatten() {
            let _ = writeln!(
                out,
                "  {}. #{} {}",
                t_["position"],
                t_["id"],
                s(t_, "title")
            );
        }
    }
    if head(out, t("Moves"), &c["moves"]) {
        for m in v["moves"].as_array().into_iter().flatten() {
            let _ = writeln!(
                out,
                "  {}  → {}",
                line(&m["node"]),
                s(&m["to"], "path_text")
            );
        }
    }
    if head(out, t("Errands"), &c["errands"]) {
        for e in v["errands"].as_array().into_iter().flatten() {
            errands(out, e);
        }
    }
    if head(out, t("Leaving"), &c["disposals"]) {
        for (d, list) in v["disposals"].as_object().into_iter().flatten() {
            for n in list.as_array().into_iter().flatten() {
                let sale = match n["sale"]["value"].as_str() {
                    Some(st) => format!(
                        "  [{}{}{}]",
                        if st == "listed" {
                            t("listed")
                        } else {
                            t("reserved")
                        },
                        n["sale"]["amount"]
                            .as_i64()
                            .map(|a| format!(" {a}"))
                            .unwrap_or_default(),
                        n["sale"]["note"]
                            .as_str()
                            .map(|w| format!(" @ {w}"))
                            .unwrap_or_default()
                    ),
                    None => String::new(),
                };
                let _ = writeln!(out, "  {}: {}{sale}", disposition(d), line(n));
            }
        }
    }
    for (key, title) in [
        ("labels", t("Labels to print")),
        ("repairs", t("Broken")),
        ("expiring", t("Use-by soon")),
        ("unknown", t("Contents unknown")),
        ("stale", t("Changed since toured")),
        ("unclear", t("Unclear records")),
        ("photos", t("Photo of the current state needed")),
    ] {
        if head(out, title, &c[key]) {
            for n in v[key].as_array().into_iter().flatten() {
                let extra = match key {
                    "repairs" => n["note"].as_str().map(|x| format!("  ({x})")),
                    "expiring" => Some(tf("  {} ({} days)", &[&s(n, "expires"), &n["days_left"]])),
                    "photos" if n["photo_reason"] == "none" => Some(t("  (no photo)").into()),
                    "photos" => Some(tf("  (changed {})", &[&s(n, "changed_at")])),
                    _ => None,
                }
                .unwrap_or_default();
                let _ = writeln!(out, "  {}{extra}", s(n, "path_text"));
            }
        }
    }
    if head(
        out,
        t("Whole photo shared by several records"),
        &c["shared_photos"],
    ) {
        for group in v["shared_photos"].as_array().into_iter().flatten() {
            let names: Vec<String> = group["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|n| format!("#{} {}", n["id"], s(n, "name")))
                .collect();
            let _ = writeln!(out, "  {}", names.join(", "));
        }
    }
    if head(out, t("To get"), &c["needs"]) {
        for n in v["needs"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "  {}", need_line(n));
        }
    }
    if head(out, t("Lost"), &c["lost"]) {
        for m in v["lost"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "  {}", line(&m["node"]));
        }
    }
}

pub fn human(v: &Value) -> String {
    let mut out = String::new();
    if v.get("counts").is_some() && v.get("unclear").is_some() {
        todo(&mut out, v);
        return out;
    }
    if v.get("language").is_some() && v["language"].get("setting").is_some() {
        settings(&mut out, v);
        return out;
    }
    if let Some(list) = v.get("labels").and_then(Value::as_array) {
        if list.is_empty() {
            let _ = writeln!(out, "{}", t("(no labels to print)"));
        }
        for n in list {
            let theme = n["theme"]
                .as_str()
                .map(|t| format!("  {t}"))
                .unwrap_or_default();
            let _ = writeln!(out, "{}{theme}", s(n, "code"));
        }
        return out;
    }
    if let Some(list) = v.get("needs").and_then(Value::as_array)
        && v.get("node").is_none()
    {
        if list.is_empty() {
            let _ = writeln!(out, "{}", t("(nothing to get)"));
        }
        for n in list {
            let _ = writeln!(out, "{}", need_line(n));
        }
        return out;
    }
    if v.get("text").is_some() && v.get("make").is_some() {
        let _ = writeln!(out, "{}  [{}]", need_line(v), s(v, "status"));
        return out;
    }
    if v.get("open_tasks").is_some() {
        let _ = writeln!(out, "{}", tf("Goal: {}", &[&goal(v)]));
        let _ = writeln!(out, "{}", progress_line(&v["progress"]));
        if v["task"].is_object() {
            let _ = writeln!(out, "\n{}", tf("Next task ({} open):", &[&v["open_tasks"]]));
            task_line(&mut out, &v["task"]);
            for p in v["task"]["places"].as_array().into_iter().flatten() {
                let _ = writeln!(
                    out,
                    "\n  {} {}",
                    s(&p["node"], "path_text"),
                    review_mark(&p["review"])
                );
                for o in p["observations"].as_array().into_iter().flatten() {
                    let _ = writeln!(out, "    {}", tf("observed: {}", &[&s(o, "text")]));
                }
                for c in p["children"].as_array().into_iter().flatten() {
                    let _ = writeln!(out, "    └ {}", line(c));
                }
                for a in p["arriving"].as_array().into_iter().flatten() {
                    let _ = writeln!(out, "    → {}", tf("arriving: {}", &[&s(a, "path_text")]));
                }
            }
        } else {
            let _ = writeln!(out, "\n{}", t("(no open task)"));
        }
        let un = v["unplanned"].as_array().map_or(0, Vec::len);
        if un > 0 {
            let _ = writeln!(out, "\n{}", tf("Raw places no task covers ({}):", &[&un]));
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
                t("  (contents unknown)")
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
    // `ev show` also carries `tasks` (the node's own); only a bare task list lands here.
    if let Some(list) = v.get("tasks").and_then(Value::as_array)
        && v.get("node").is_none()
    {
        if list.is_empty() {
            let _ = writeln!(out, "{}", t("(no tasks)"));
        }
        for t_ in list {
            task_line(&mut out, t_);
        }
        return out;
    }
    if v.get("why").is_some() && v.get("title").is_some() {
        task_line(&mut out, v);
        return out;
    }
    if v.get("goal").is_some() && v.as_object().is_some_and(|o| o.len() == 1) {
        let _ = writeln!(out, "{}", tf("Goal: {}", &[&goal(v)]));
        return out;
    }
    if let Some(node) = v.get("node").filter(|_| v.get("children").is_some()) {
        let _ = writeln!(out, "{}", line(node));
        if let Some(k) = node["kind"].as_str() {
            let _ = writeln!(out, "  {}: {}", t("kind"), kind(k));
        }
        for (key, label) in [
            ("note", t("note")),
            ("theme", t("theme")),
            ("address", t("address")),
            ("to", t("to take to")),
            ("owner", t("owner")),
            ("with", t("lent to")),
        ] {
            if let Some(x) = node[key].as_str() {
                let _ = writeln!(out, "  {label}: {x}");
            }
        }
        if let Some(f) = node["fill"].as_i64() {
            let _ = writeln!(out, "  {}: {}", t("fill"), tf("{}%", &[&f]));
        }
        if let Some(tags) = node["tags"].as_array().filter(|t| !t.is_empty()) {
            let tags: Vec<_> = tags.iter().filter_map(Value::as_str).collect();
            let _ = writeln!(out, "  {}: {}", t("tags"), tags.join(", "));
        }
        for p in node["photos"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "  {}: {}", t("photo"), p.as_str().unwrap_or_default());
        }
        if v["pending"].is_object() {
            let _ = writeln!(
                out,
                "  {}",
                tf("pending move → {}", &[&s(&v["pending"], "path_text")])
            );
        }
        if v["last_seen"].is_object() {
            let _ = writeln!(
                out,
                "  {}",
                tf("last seen: {}", &[&s(&v["last_seen"], "path_text")])
            );
        }
        if v["review"].is_object() {
            let _ = writeln!(
                out,
                "  {}",
                tf(
                    "review: {} ({})",
                    &[&s(&v["review"], "status"), &s(&v["review"], "at")]
                )
            );
        }
        for o in v["observations"].as_array().into_iter().flatten() {
            let _ = writeln!(
                out,
                "  {}",
                tf("observed #{}: {}", &[&o["id"], &s(o, "text")])
            );
        }
        for (kind, m) in v["marks"].as_object().into_iter().flatten() {
            let kind = match kind.as_str() {
                "label" => t("label"),
                "broken" => t("broken"),
                "expires" => t("use-by"),
                "sale" => t("sale"),
                other => other,
            };
            let what = [
                m["value"].as_str().map(|x| {
                    match x {
                        "needed" => t("to print"),
                        "printed" => t("printed"),
                        "listed" => t("listed"),
                        "reserved" => t("reserved"),
                        other => other,
                    }
                    .to_string()
                }),
                m["amount"].as_i64().map(|a| a.to_string()),
                m["note"].as_str().map(str::to_string),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
            let _ = writeln!(out, "  {kind}: {what}");
        }
        for n in v["needs"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "  {}: {}", t("to get"), need_line(n));
        }
        for t_ in v["tasks"].as_array().into_iter().flatten() {
            let via = if t_["via"] == node["id"] {
                String::new()
            } else {
                tf("  (via #{})", &[&t_["via"]])
            };
            let _ = writeln!(
                out,
                "  {} {}. #{} {}{via}",
                t("task"),
                t_["position"],
                t_["id"],
                s(t_, "title")
            );
        }
        for c in v["children"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "  └ {}", line(c));
        }
        return out;
    }
    if let Some(rules) = v.get("rules").and_then(Value::as_array) {
        if !rules.is_empty() || v.get("containers").is_none() {
            let _ = writeln!(out, "{}", t("Rules:"));
            for r in rules {
                let _ = writeln!(out, "  {}. {}", r["id"], s(r, "text"));
            }
        }
        if v.get("containers").is_none() {
            return out;
        }
        let _ = writeln!(out, "{}", t("Similar things are in:"));
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
            "{}",
            tf(
                "All {} places that can hold something:",
                &[&v["complete"]["containers"]]
            )
        );
        for c in v["containers"].as_array().into_iter().flatten() {
            let theme = c["theme"]
                .as_str()
                .map(|t| format!("  [{t}]"))
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "  {}{theme}  {}",
                s(c, "path_text"),
                tf("{} items", &[&c["items"]])
            );
        }
        return out;
    }
    if v.get("spread").is_some() && v.get("no_theme").is_some() {
        let _ = writeln!(out, "{}", t("Alike things in several places:"));
        for x in v["spread"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "  {}", s(x, "word"));
            for p in x["places"].as_array().into_iter().flatten() {
                let _ = writeln!(out, "    {}", s(p, "path_text"));
            }
        }
        let _ = writeln!(out, "{}", t("Holders without a theme:"));
        for n in v["no_theme"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "  {}", s(n, "path_text"));
        }
        let _ = writeln!(out, "{}", t("Items lying loose in a room or on furniture:"));
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
            let _ = writeln!(out, "{}", t("(nothing to take, return or collect)"));
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
                "{}  {}",
                place_head(p),
                tf(
                    "take {} · return {} · collect {}",
                    &[&p["take"], &p["return"], &p["collect"]]
                )
            );
        }
        return out;
    }
    if v.get("aliases").is_some() && v.get("name").is_some() {
        let _ = writeln!(out, "{}", place_head(v));
        return out;
    }
    if let Some(tr) = v.get("tree").and_then(Value::as_array) {
        for n in tr {
            tree(&mut out, n, 0);
        }
        for n in v["unplaced"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "{}{}", t("(place unknown) "), line(n));
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
                let _ = writeln!(out, "{}", t("(none)"));
            }
            for n in list {
                let _ = writeln!(out, "{}", line(n));
            }
            return out;
        }
    }
    if let Some(list) = v.get("pending").and_then(Value::as_array) {
        if list.is_empty() {
            let _ = writeln!(out, "{}", t("(no pending moves)"));
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
    if let Some(list) = v.get("recoded").and_then(Value::as_array) {
        for r in list {
            let code = |k: &str| r[k].as_str().unwrap_or("—").to_string();
            let _ = writeln!(
                out,
                "#{} {}  {} → {}",
                r["id"],
                s(r, "name"),
                code("before"),
                code("after")
            );
        }
        return out;
    }
    if let Some(list) = v.get("lost").and_then(Value::as_array) {
        if list.is_empty() {
            let _ = writeln!(out, "{}", t("(nothing lost)"));
        }
        for m in list {
            let seen = m["last_seen"]
                .get("path_text")
                .and_then(Value::as_str)
                .unwrap_or(t("never known"));
            let _ = writeln!(
                out,
                "{}\n    {}",
                line(&m["node"]),
                tf("last seen: {}", &[&seen])
            );
        }
        return out;
    }
    if let Some(groups) = v.get("disposals").and_then(Value::as_object) {
        for (d, list) in groups {
            let _ = writeln!(out, "{}:", disposition(d));
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
