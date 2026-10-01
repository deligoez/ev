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

/// Where a lost thing was last seen, or that it never was.
pub fn last_seen(n: &Value) -> String {
    match n["last_seen"].as_object() {
        Some(_) => tf("  (last seen in {})", &[&label(&n["last_seen"])]),
        None => t("  (never seen anywhere)").to_string(),
    }
}

/// How far a place has been counted, in words: `raw`, `counting`, `toured`, `kept`.
pub fn count_label(state: &str) -> &'static str {
    match state {
        "counting" => t("being counted"),
        "toured" => t("counted"),
        "kept" => t("left as is"),
        _ => t("not counted"),
    }
}

fn place_marks(n: &Value) -> String {
    let mut out = String::new();
    if let Some(c) = n["count"].as_str() {
        out.push_str(&format!("  ({})", count_label(c)));
    }
    if n["temporary"] == true {
        out.push_str(t("  (temporary place)"));
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
        Some("toured") if r["changed_since"] == true => t("[counted, changed since]"),
        Some("toured") => t("[counted]"),
        Some("kept") => t("[left as is]"),
        Some("counting") => t("[being counted]"),
        _ => t("[not counted]"),
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
        "{} places: {} counted, {} left as is, {} being counted, {} not counted; {} changed since counted",
        &[
            &p["units"],
            &p["toured"],
            &p["kept"],
            &p["counting"],
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

/// A document's kind in the reader's language.
fn doc_kind(k: &str) -> &str {
    match k {
        "invoice" => t("invoice"),
        "warranty" => t("warranty certificate"),
        "manual" => t("manual"),
        "service" => t("service form"),
        "appraisal" => t("appraisal"),
        "policy" => t("policy"),
        "other" => t("other document"),
        other => other,
    }
}

/// `#3 invoice  2024-05-03  Amazon  no 402-123  fatura.pdf — note`: what is known of it.
pub(crate) fn doc_line(d: &Value) -> String {
    let mut parts = vec![format!("#{} {}", d["id"], doc_kind(&s(d, "kind")))];
    for k in ["issued_at", "issuer"] {
        if let Some(x) = d[k].as_str() {
            parts.push(x.to_string());
        }
    }
    if let Some(n) = d["number"].as_str() {
        parts.push(tf("no {}", &[&n]));
    }
    if let Some(n) = d["original_name"].as_str() {
        parts.push(n.to_string());
    }
    let note = d["note"]
        .as_str()
        .map(|n| format!(" — {n}"))
        .unwrap_or_default();
    format!("{}{note}", parts.join("  "))
}

/// `#12 2024-05-03  Amazon  Bosch GSB 13 RE ×1  2479.00 TRY`: when, where, what, how many, paid.
pub(crate) fn purchase_line(p: &Value) -> String {
    let mut parts = vec![format!("#{}", p["id"])];
    if let Some(d) = p["delivered_at"].as_str().or(p["ordered_at"].as_str()) {
        parts.push(d.to_string());
    }
    if let Some(sh) = p["shop"].as_str() {
        parts.push(sh.to_string());
    }
    let qty = p["linked_qty"]
        .as_i64()
        .unwrap_or(p["qty"].as_i64().unwrap_or(1));
    parts.push(format!("{} ×{qty}", s(p, "name")));
    if let Some(paid) = p["paid"].as_str() {
        let cur = p["currency"].as_str().unwrap_or_default();
        parts.push(format!("{paid} {cur}").trim_end().to_string());
    }
    parts.join("  ")
}

/// Where a line stands: dismissed, returned, or how much of it is still to link.
fn purchase_state(p: &Value) -> String {
    if let Some(d) = p["dismissed"].as_str() {
        return tf("[dismissed: {}]", &[&d]);
    }
    let open = p["open_qty"].as_i64().unwrap_or(0);
    let mut st = if open > 0 {
        tf("[{} open]", &[&open])
    } else {
        t("[linked]").to_string()
    };
    if p["status"] == "returned" {
        st.push_str(&format!(" {}", t("[returned]")));
    }
    st
}

/// One purchase: its line, links, pages, order and documents.
fn purchase(out: &mut String, p: &Value) {
    let _ = writeln!(out, "{}  {}", purchase_line(p), purchase_state(p));
    for (k, label) in [
        ("merchant", t("seller")),
        ("brand", t("make")),
        ("order_no", t("order")),
        ("order_url", t("order page")),
        ("product_url", t("product page")),
        ("why", t("why")),
    ] {
        if let Some(x) = p[k].as_str() {
            let _ = writeln!(out, "  {label}: {x}");
        }
    }
    for l in p["linked"].as_array().into_iter().flatten() {
        let _ = writeln!(
            out,
            "  → #{} {} ×{}",
            l["node"]["id"],
            s(&l["node"], "path_text"),
            l["qty"]
        );
    }
    for d in p["documents"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  {}: {}", t("document"), doc_line(d));
    }
}

/// One document: its line, where its copy is, and what it belongs to.
fn document(out: &mut String, v: &Value) {
    let d = &v["document"];
    let _ = writeln!(out, "{}", doc_line(d));
    if v["existing"] == true {
        let _ = writeln!(
            out,
            "  {}",
            t("(already in the store; only new links were added)")
        );
    }
    let _ = writeln!(out, "  {}: {}", t("file"), s(d, "file"));
    if let Some(e) = d["ettn"].as_str() {
        let _ = writeln!(out, "  ETTN: {e}");
    }
    for n in d["nodes"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  → #{} {}", n["id"], s(n, "path_text"));
    }
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
        ("uncounted", t("Not counted yet")),
        ("parked", t("Waiting for a final place")),
        ("stale", t("Changed since counted")),
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
                    "parked" => Some(tf(
                        "  (parked in {})",
                        &[&n["in"]["code"]
                            .as_str()
                            .map_or_else(|| s(&n["in"], "path_text"), str::to_string)],
                    )),
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

/// A grid as text: a header of column letters, then one line per row from the back, each cell
/// showing the back-left cell of the box that covers it (its usual code suffix) or `·`.
pub fn grid_lines(grid: &Value) -> Vec<String> {
    let cols = grid["cols"].as_u64().unwrap_or(0) as usize;
    let anchor = |id: &Value| -> String {
        grid["boxes"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|b| &b["id"] == id)
            .and_then(|b| b["cells"].as_str())
            .map(|c| c.split('-').next().unwrap_or(c).to_string())
            .unwrap_or_else(|| "?".into())
    };
    let mut lines = Vec::new();
    let mut head = String::from("    ");
    for c in 0..cols {
        let _ = write!(head, "{:<5}", (b'A' + c as u8) as char);
    }
    lines.push(head.trim_end().to_string());
    for (r, row) in grid["map"].as_array().into_iter().flatten().enumerate() {
        let mut line = format!("{:>2}  ", r + 1);
        for cell in row.as_array().into_iter().flatten() {
            let label = if cell.is_null() {
                "·".to_string()
            } else {
                anchor(cell)
            };
            let _ = write!(line, "{label:<5}");
        }
        lines.push(line.trim_end().to_string());
    }
    lines
}

/// Centimetres as a person reads them: whole, or to a tenth when that is not a whole.
pub fn cm(v: &Value) -> String {
    match v.as_f64() {
        Some(n) => {
            let r = (n * 10.0).round() / 10.0;
            if r.fract() == 0.0 {
                format!("{}", r as i64)
            } else {
                format!("{r:.1}")
            }
        }
        None => v.to_string(),
    }
}

/// A grid's size and which way its rows run: from the back of a drawer, from the top of a
/// piece of furniture seen from the front.
pub fn grid_title(grid: &Value) -> String {
    let text = if grid["face"] == "front" {
        "{}×{} grid seen from the front, row 1 at the top"
    } else {
        "{}×{} grid, row 1 at the back"
    };
    tf(text, &[&grid["cols"], &grid["rows"]])
}

fn grid_block(out: &mut String, node: &Value, grid: &Value) {
    let _ = writeln!(out, "{}  {}", s(node, "path_text"), grid_title(grid));
    for l in grid_lines(grid) {
        let _ = writeln!(out, "  {l}");
    }
    let free: Vec<&str> = grid["free"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let _ = writeln!(
        out,
        "  {}",
        tf("free ({}): {}", &[&free.len(), &free.join(" ")])
    );
    for b in grid["boxes"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  {:<7} {}", s(b, "cells"), s(b, "name"));
    }
    let unplaced: Vec<String> = grid["unplaced"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|b| format!("#{} {}", b["id"], s(b, "name")))
        .collect();
    if !unplaced.is_empty() {
        let _ = writeln!(
            out,
            "  {}",
            tf("not placed in a cell: {}", &[&unplaced.join(", ")])
        );
    }
}

/// How much room a holder has, from the `room` object `ev suggest` and `ev regroup` attach.
/// `ev themes`' shared words with how many things name each: `anten (2), SMA (2)`.
pub fn theme_words(e: &Value) -> String {
    e["words"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|w| {
            format!(
                "{} ({})",
                w["word"].as_str().unwrap_or_default(),
                w["things"]
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn room_text(r: &Value) -> String {
    let fill = &r["fill"];
    let mut out = match r["room"].as_str() {
        Some("yes") => tf("room ({}% full)", &[fill]),
        Some("little") => tf("little room ({}% full)", &[fill]),
        Some("none") => tf("full ({}%)", &[fill]),
        _ => t("fill unknown").to_string(),
    };
    if r["stale"] == true {
        out.push_str(t(", changed since"));
    }
    out
}

fn percent(x: &Value) -> String {
    tf(
        "{}%",
        &[&format!("{:.0}", x.as_f64().unwrap_or(0.0) * 100.0)],
    )
}

fn list_str(v: &Value) -> String {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

/// A holder by its label when it has one (`K4x4-07-A-F2  Gridfinity 1x1x1 — …`), else by path.
fn head(n: &Value) -> String {
    match n["code"].as_str() {
        Some(c) => format!("{c}  {}", s(n, "name")),
        None => s(n, "path_text"),
    }
}

/// A holder by its label alone, for the short side of an arrow.
fn label(n: &Value) -> String {
    n["code"]
        .as_str()
        .map_or_else(|| s(n, "path_text"), str::to_string)
}

fn thing(n: &Value) -> String {
    format!("#{} {}", n["id"], s(n, "name"))
}

/// `ev suggest`: the ranked holders with why each scored, then every holder.
fn suggestion(out: &mut String, v: &Value) {
    if v["for"].is_object() {
        let _ = writeln!(out, "{}", tf("For: {}", &[&s(&v["for"], "path_text")]));
    }
    let _ = writeln!(out, "{}", tf("Words: {}", &[&list_str(&v["words"])]));
    let facet = list_str(&v["facet"]);
    if !facet.is_empty() {
        let _ = writeln!(out, "{}", tf("Facet: {}", &[&facet]));
    }
    let added = list_str(&v["synonyms_added"]);
    if !added.is_empty() {
        let _ = writeln!(out, "{}", tf("Synonyms added: {}", &[&added]));
    }
    if v["new_group_likely"] == true {
        let _ = writeln!(
            out,
            "{}",
            t("No holder matches what this thing is: it likely needs a new group.")
        );
    }
    let rules = v["rules"].as_array().cloned().unwrap_or_default();
    if !rules.is_empty() {
        let _ = writeln!(out, "\n{}", t("Rules:"));
        for r in &rules {
            let _ = writeln!(out, "  {}. {}", r["id"], s(r, "text"));
        }
    }
    let similar = v["similar"].as_array().cloned().unwrap_or_default();
    let _ = writeln!(out, "\n{}", t("Best matches:"));
    if similar.is_empty() {
        let _ = writeln!(out, "  {}", t("(none)"));
    }
    for (i, x) in similar.iter().enumerate() {
        let c = &x["container"];
        // A place not gone through yet is a guess to check before it is proposed.
        let untoured = if c["review"].is_null() || c["review"]["status"] == "raw" {
            t("  (not counted)")
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "  {}. {}{untoured}  {}",
            i + 1,
            head(c),
            tf(
                "score {} · covers {} · {}",
                &[
                    &x["score"],
                    &percent(&x["coverage"]),
                    &room_text(&c["room"])
                ]
            )
        );
        let matched: Vec<String> = x["matched"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|m| {
                let mark = if m["specific"] == true { "*" } else { "" };
                format!("{}{mark} {} ({})", s(m, "term"), m["points"], s(m, "from"))
            })
            .collect();
        let _ = writeln!(out, "     {}", tf("matched: {}", &[&matched.join("; ")]));
    }
    let other = v["other_facet"].as_array().cloned().unwrap_or_default();
    if !other.is_empty() {
        let _ = writeln!(out, "\n{}", t("Kept out, another facet:"));
        for x in &other {
            let _ = writeln!(
                out,
                "  {}  [{}]  {}",
                head(&x["container"]),
                list_str(&x["container"]["facet"]),
                tf("score {}", &[&x["score"]])
            );
        }
    }
    let parking = v["parking"].as_array().cloned().unwrap_or_default();
    if !parking.is_empty() {
        let _ = writeln!(
            out,
            "\n{}",
            t("Parking places, not offered as a final place:")
        );
        for x in &parking {
            let _ = writeln!(
                out,
                "  {}  {}",
                head(&x["container"]),
                tf("score {}", &[&x["score"]])
            );
        }
    }
    let _ = writeln!(
        out,
        "\n{}",
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
            "  {}{theme}  {} · {}",
            head(c),
            tf("{} items", &[&c["items"]]),
            room_text(&c["room"])
        );
    }
    let _ = writeln!(out, "\n{}", scored());
}

fn scored() -> &'static str {
    t(
        "Scoring: a holder's theme ×3, name ×2.5, note ×1; the things inside it: name ×2, tags ×1.5, note ×1. Rare words weigh more, repeats less; * marks a word rare enough to say what the thing is.",
    )
}

/// `ev regroup`: what could move, merge or grow, from the same score as `ev suggest`.
fn regroup(out: &mut String, v: &Value) {
    if v["scope"].is_object() {
        let _ = writeln!(out, "{}", head(&v["scope"]));
    }
    let _ = writeln!(
        out,
        "{}",
        tf(
            "{} of {} things are already in their best place.",
            &[&v["checked"]["best_where_they_are"], &v["checked"]["items"]]
        )
    );
    let section = |out: &mut String, key: &str, title: &str| -> Vec<Value> {
        let list = v[key].as_array().cloned().unwrap_or_default();
        if !list.is_empty() {
            let _ = writeln!(out, "\n{title}");
        }
        list
    };
    for e in section(out, "elsewhere", t("Would fit better elsewhere:")) {
        let _ = writeln!(out, "  {}", thing(&e["item"]));
        let _ = writeln!(
            out,
            "    {} ({}) → {} ({})",
            label(&e["now"]["holder"]),
            e["now"]["score"],
            label(&e["better"]["holder"]),
            e["better"]["score"]
        );
    }
    for e in section(
        out,
        "alone",
        t("Sharing no word with anything in its holder (a guess; look before moving):"),
    ) {
        let _ = writeln!(out, "  {}", thing(&e["item"]));
        let _ = writeln!(
            out,
            "    {} → {} ({})",
            label(&e["now"]["holder"]),
            label(&e["better"]["holder"]),
            e["better"]["score"]
        );
    }
    for e in section(
        out,
        "strays",
        t("Named for another box's theme (worth a look):"),
    ) {
        let _ = writeln!(out, "  {}", thing(&e["item"]));
        let _ = writeln!(out, "    \"{}\" → {}", s(&e, "term"), label(&e["home"]));
    }
    for e in section(out, "declined", t("Declined, left where they are:")) {
        let why = e["why"]
            .as_str()
            .map(|w| format!("  ({w})"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  {}  · {}{why}",
            thing(&e["item"]),
            label(&e["holder"])
        );
    }
    for e in section(out, "full", t("Full:")) {
        let _ = writeln!(
            out,
            "  {}  {}",
            head(&e["holder"]),
            tf("{}%", &[&e["fill"]])
        );
        let spares = e["bigger_spares"].as_array().cloned().unwrap_or_default();
        if spares.is_empty() {
            let _ = writeln!(out, "    {}", t("no bigger spare box recorded"));
        }
        for sp in &spares {
            let at = list_str(&sp["fits_at"]);
            let at = if at.is_empty() {
                t("no free cell for it").to_string()
            } else {
                tf("fits at {}", &[&at])
            };
            let _ = writeln!(
                out,
                "    {}",
                tf(
                    "bigger spare: {} {} · {}",
                    &[&s(sp, "name"), &s(sp, "size"), &at]
                )
            );
        }
    }
    for e in section(out, "sparse", t("Nearly empty:")) {
        let into = if e["merge_into"].is_object() {
            tf("could merge into {}", &[&label(&e["merge_into"]["holder"])])
        } else {
            t("no similar box with room").to_string()
        };
        let _ = writeln!(
            out,
            "  {}  {} · {into}",
            head(&e["holder"]),
            tf("{}%", &[&e["fill"]])
        );
    }
    for e in section(
        out,
        "mixed",
        t("Mixed (half or more fit better elsewhere):"),
    ) {
        let _ = writeln!(
            out,
            "  {}  {}",
            head(&e["holder"]),
            tf("{} items", &[&e["items"]])
        );
        for n in e["better_elsewhere"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "    - {}", n.as_str().unwrap_or_default());
        }
    }
    for e in section(out, "unknown_fill", t("Fill unknown or out of date:")) {
        let _ = writeln!(out, "  {}", head(&e["holder"]));
    }
    let _ = writeln!(out, "\n{}", scored());
}

#[expect(
    clippy::too_many_lines,
    reason = "one branch per output shape; the long shapes have their own functions"
)]
pub fn human(v: &Value) -> String {
    let mut out = String::new();
    if v.get("item").is_some()
        && v.get("declined").is_some()
        && v.as_object().is_some_and(|o| o.len() == 2)
    {
        declined(&mut out, v);
        return out;
    }
    if v.get("layout").is_some() && v.get("tiles").is_some() {
        map(&mut out, v);
        return out;
    }
    if v.get("sketch").is_some()
        && v.get("node").is_some()
        && v.as_object().is_some_and(|o| o.len() == 2)
    {
        sketch(&mut out, v);
        return out;
    }
    if v.get("placed").is_none()
        && let Some(list) = v.get("grids").and_then(Value::as_array)
    {
        for g in list {
            grid_block(&mut out, &g["node"], &g["grid"]);
            out.push('\n');
        }
        return out;
    }
    if let Some(list) = v.get("placed").and_then(Value::as_array) {
        for b in list {
            let cells = b["cells"].as_str().unwrap_or("—");
            let _ = writeln!(out, "{}  → {cells}", line(b));
        }
        for g in v["grids"].as_array().into_iter().flatten() {
            out.push('\n');
            grid_block(&mut out, &g["node"], &g["grid"]);
        }
        return out;
    }
    if v.get("grid").is_some() && v.get("node").is_some() && v.get("children").is_none() {
        if v["grid"].is_null() {
            let _ = writeln!(out, "{}  {}", s(&v["node"], "path_text"), t("(no grid)"));
        } else {
            grid_block(&mut out, &v["node"], &v["grid"]);
        }
        return out;
    }
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
    if v.get("document").is_some_and(Value::is_object) {
        document(&mut out, v);
        return out;
    }
    if let Some(i) = v.get("imported") {
        let _ = writeln!(
            out,
            "{}",
            tf(
                "{} new, {} updated, {} unchanged, {} skipped; {} document links, {} documents skipped",
                &[
                    &i["new"],
                    &i["updated"],
                    &i["unchanged"],
                    &i["skipped"],
                    &i["document_links"],
                    &i["documents_skipped"]
                ]
            )
        );
        return out;
    }
    if v.get("purchase").is_some_and(Value::is_object) {
        purchase(&mut out, &v["purchase"]);
        return out;
    }
    if let Some(list) = v.get("purchases").and_then(Value::as_array)
        && v.get("node").is_none()
    {
        if list.is_empty() {
            let _ = writeln!(out, "{}", t("(no purchases)"));
        }
        for p in list {
            let _ = writeln!(out, "{}  {}", purchase_line(p), purchase_state(p));
        }
        return out;
    }
    if let Some(list) = v.get("documents").and_then(Value::as_array)
        && v.get("node").is_none()
    {
        if list.is_empty() {
            let _ = writeln!(out, "{}", t("(no documents)"));
        }
        for d in list {
            let on = d["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|n| format!("#{}", n["id"]))
                .collect::<Vec<_>>();
            let on = if on.is_empty() {
                String::new()
            } else {
                format!("  → {}", on.join(" "))
            };
            let _ = writeln!(out, "{}{on}", doc_line(d));
        }
        return out;
    }
    if v.get("text").is_some() && v.get("make").is_some() {
        let _ = writeln!(out, "{}  [{}]", need_line(v), s(v, "status"));
        return out;
    }
    if v.get("open_tasks").is_some() {
        next(&mut out, v);
        return out;
    }
    if v.get("units").is_some() && v.get("places").is_some() {
        let _ = writeln!(out, "{}", progress_line(v));
        for p in v["places"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "  {} {}", review_mark(&p["review"]), s(p, "path_text"));
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
        show(&mut out, v, node);
        return out;
    }
    if let (Some(kit), Some(parts)) = (
        v.get("kit").filter(|k| k.is_object()),
        v.get("parts").and_then(Value::as_array),
    ) {
        kit_parts(&mut out, v, kit, parts);
        return out;
    }
    if let Some(list) = v.get("kits").and_then(Value::as_array) {
        if list.is_empty() {
            let _ = writeln!(out, "{}", t("(no kits)"));
        }
        for k in list {
            let c = &k["counts"];
            let _ = writeln!(
                out,
                "#{} {} ×{}  {}",
                k["id"],
                s(k, "name"),
                k["copies"],
                tf(
                    "{} of {} found · {} lost · {} still missing",
                    &[&c["found"], &c["expected"], &c["lost"], &c["open"]]
                )
            );
        }
        return out;
    }
    if let (Some(node), Some(into)) = (
        v.get("node").filter(|n| n.is_object()),
        v.get("into").and_then(Value::as_array),
    ) {
        let _ = writeln!(out, "{}", line(node));
        for p in into {
            let _ = writeln!(out, "  + {}", line(p));
        }
        return out;
    }
    if let Some(list) = v.get("facets").and_then(Value::as_array) {
        facet_list(&mut out, list);
        return out;
    }
    if let Some(list) = v.get("themes").and_then(Value::as_array) {
        theme_list(&mut out, list);
        return out;
    }
    if let Some(list) = v.get("synonyms").and_then(Value::as_array) {
        if list.is_empty() {
            let _ = writeln!(out, "{}", t("(no synonyms)"));
        }
        for g in list {
            let _ = writeln!(out, "{}. {}", g["id"], s(g, "words"));
        }
        return out;
    }
    if v.get("checked").is_some() && v.get("elsewhere").is_some() {
        regroup(&mut out, v);
        return out;
    }
    if let Some(rules) = v.get("rules").and_then(Value::as_array) {
        if v.get("containers").is_none() {
            let _ = writeln!(out, "{}", t("Rules:"));
            for r in rules {
                let _ = writeln!(out, "  {}. {}", r["id"], s(r, "text"));
            }
            return out;
        }
        suggestion(&mut out, v);
        return out;
    }
    if v.get("spread").is_some() && v.get("no_theme").is_some() {
        audit(&mut out, v);
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
        tree_and_lost(&mut out, v, tr);
        return out;
    }
    if let Some(events) = v.get("events").and_then(Value::as_array) {
        let _ = writeln!(out, "{}", line(&v["node"]));
        for e in events {
            // With `--contents`, an event of something that came, went or was added here.
            let item = if e["item"].is_object() {
                format!("  {} {}", s(e, "relation"), line(&e["item"]))
            } else {
                String::new()
            };
            let _ = writeln!(
                out,
                "  {}  {:<8} {}{item}",
                s(e, "at"),
                s(e, "type"),
                e["data"]
            );
        }
        return out;
    }
    for key in ["results", "created", "edited"] {
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
    if let Some(list) = v.get("attached").and_then(Value::as_array) {
        for n in list {
            let what = match n["crop"].as_str() {
                Some(c) => tf("photo {}, crop {}", &[&n["photo"], &c]),
                None => tf("photo {}, whole", &[&n["photo"]]),
            };
            let _ = writeln!(out, "{}  ({what})", line(n));
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
    // `ev photo add`, `list` and `remove`: the node, then its photos by number.
    if let (Some(node), Some(list)) = (
        v.get("node").filter(|n| n.is_object()),
        v.get("photos").and_then(Value::as_array),
    ) {
        photo_list(&mut out, node, list);
        return out;
    }
    let _ = writeln!(out, "{v}");
    out
}

/// A node's photos, one per line by the number other commands take: the file, the crop when
/// only part of the photo shows the node, and the note.
fn photo_list(out: &mut String, node: &Value, list: &[Value]) {
    let _ = writeln!(out, "{}", line(node));
    if list.is_empty() {
        let _ = writeln!(out, "{}", t("  (no photos)"));
    }
    for p in list {
        let mut extra = String::new();
        if let Some(c) = p["crop"].as_str() {
            extra.push_str(&tf("  crop {}", &[&c]));
        }
        if p["exists"] == false {
            extra.push_str(t("  (file missing)"));
        }
        if let Some(n) = p["note"].as_str().filter(|n| !n.is_empty()) {
            extra.push_str(&format!("  — {n}"));
        }
        let _ = writeln!(out, "  {}. {}{extra}", p["n"], s(p, "path"));
    }
}

/// A regroup decline set or taken back.
fn declined(out: &mut String, v: &Value) {
    let d = &v["declined"];
    let text = if d.is_null() {
        t("regroup may propose moving it again").to_string()
    } else {
        let why = d["why"]
            .as_str()
            .map(|w| format!("  ({w})"))
            .unwrap_or_default();
        format!("{}{why}", tf("stays in {}", &[&label(&d["holder"])]))
    };
    let _ = writeln!(out, "{}  {text}", thing(&v["item"]));
}

/// A map: where it is, how it is laid out, then its tiles in reading order.
fn map(out: &mut String, v: &Value) {
    let _ = writeln!(out, "{}", s(v, "path_text"));
    let layout = match v["layout"].as_str().unwrap_or_default() {
        "grid" => tf("grid {}×{}", &[&v["size"]["cols"], &v["size"]["rows"]]),
        "sketch" => tf(
            "sketch {}×{} cm",
            &[&cm(&v["size"]["w"]), &cm(&v["size"]["d"])],
        ),
        "stack" => t("stack, front on, top first").to_string(),
        _ => t("tiles (no layout yet)").to_string(),
    };
    let _ = writeln!(out, "  {layout}");
    let by_id: std::collections::HashMap<i64, &Value> = v["tiles"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(v["unplaced"].as_array().into_iter().flatten())
        .filter_map(|t| Some((t["id"].as_i64()?, t)))
        .collect();
    let mut band = Value::Null;
    for id in ev_core::reading_order(v) {
        let Some(t) = by_id.get(&id) else { continue };
        if t["band"] != Value::Null && t["band"] != band {
            band = t["band"].clone();
            if let Some(b) = v["bands"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|b| b["id"] == t["band"])
            {
                let _ = writeln!(out, "  {}", line(b));
            }
        }
        let at = t["cells"].as_str().map(str::to_string).unwrap_or_default();
        let stacked: Vec<String> = t["stacked"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|x| s(x, "code"))
            .collect();
        let stacked = if stacked.is_empty() {
            String::new()
        } else {
            tf("  (with {} on it)", &[&stacked.join(", ")])
        };
        let _ = writeln!(
            out,
            "    {:<6} #{:<4} {}  {}{stacked}",
            at,
            t["id"],
            match t["code"].as_str() {
                Some(c) => format!("{c}  {}", s(t, "name")),
                None => s(t, "name"),
            },
            tf("{} items", &[&t["items"]])
        );
    }
    if v["unplaced"].as_array().is_some_and(|u| !u.is_empty()) {
        let _ = writeln!(out, "  {}", t("(the unplaced ones are listed last)"));
    }
}

/// A node's sketch: where it lies, how big it is and what it stands on.
fn sketch(out: &mut String, v: &Value) {
    let p = &v["sketch"];
    let text = if p.is_null() {
        t("(no sketch)").to_string()
    } else {
        let mut parts = Vec::new();
        if !p["x"].is_null() {
            parts.push(tf("at {},{} cm", &[&cm(&p["x"]), &cm(&p["y"])]));
        }
        if !p["w"].is_null() {
            parts.push(tf("{}×{} cm", &[&cm(&p["w"]), &cm(&p["d"])]));
        }
        if !p["on"].is_null() {
            parts.push(tf("on #{}", &[&p["on"]]));
        }
        parts.join(" · ")
    };
    let _ = writeln!(out, "{}  {text}", line(&v["node"]));
}

/// `ev next`: the goal, how far the home is counted, the next task with its places, and the
/// raw places no task covers.
fn next(out: &mut String, v: &Value) {
    let _ = writeln!(out, "{}", tf("Goal: {}", &[&goal(v)]));
    let _ = writeln!(out, "{}", progress_line(&v["progress"]));
    if v["task"].is_object() {
        let _ = writeln!(out, "\n{}", tf("Next task ({} open):", &[&v["open_tasks"]]));
        task_line(out, &v["task"]);
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
}

/// `ev show`: a node's fields, marks, tasks, kits, contents and grid.
fn show(out: &mut String, v: &Value, node: &Value) {
    let _ = writeln!(out, "{}", line(node));
    if let Some(k) = node["kind"].as_str() {
        let _ = writeln!(out, "  {}: {}", t("kind"), kind(k));
    }
    for (key, label) in [
        ("make", t("make")),
        ("model", t("model")),
        ("serial", t("serial")),
        ("note", t("note")),
        ("theme", t("theme")),
        ("size", t("size")),
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
    for p in v["purchases"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  {}: {}", t("bought"), purchase_line(p));
    }
    for d in v["documents"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  {}: {}", t("document"), doc_line(d));
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
    for k in v["kits"].as_array().into_iter().flatten() {
        let _ = writeln!(
            out,
            "  {}: {} · {}. {}",
            t("kit"),
            s(k, "kit"),
            k["n"],
            s(k, "text")
        );
    }
    if let Some(c) = v["cells"].as_str() {
        let _ = writeln!(out, "  {}: {c}", t("cells"));
    }
    for c in v["children"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  └ {}", line(c));
    }
    if v["grid"].is_object() {
        out.push('\n');
        grid_block(out, node, &v["grid"]);
    }
}

/// `ev kit show`: the kit's counts, then each part with the records linked to it.
fn kit_parts(out: &mut String, v: &Value, kit: &Value, parts: &[Value]) {
    let c = &v["counts"];
    let _ = writeln!(
        out,
        "{} ×{}  {}",
        s(kit, "name"),
        kit["copies"],
        tf(
            "{} of {} found · {} lost · {} still missing",
            &[&c["found"], &c["expected"], &c["lost"], &c["open"]]
        )
    );
    for p in parts {
        let mark = match (p["open"].as_i64(), p["lost"].as_i64()) {
            (Some(0), Some(0)) => "✓",
            _ if p["found"] == 0 => "·",
            _ => "~",
        };
        let mut counts = format!("{}/{}", p["found"], p["expected"]);
        if p["lost"].as_i64().unwrap_or(0) > 0 {
            counts.push_str(&format!("  {}", tf("{} lost", &[&p["lost"]])));
        }
        let _ = writeln!(
            out,
            "{:>3}. {mark} {}  {counts}",
            p["n"].as_i64().unwrap_or_default(),
            s(p, "text")
        );
        for n in p["nodes"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "       {}", line(n));
        }
    }
}

/// `ev facet list`: each facet, its words and the holders tagged with it.
fn facet_list(out: &mut String, list: &[Value]) {
    if list.is_empty() {
        let _ = writeln!(out, "{}", t("(no facets)"));
    }
    for f in list {
        let words = s(f, "words");
        let words = if words.is_empty() {
            String::new()
        } else {
            format!("  ({words})")
        };
        let _ = writeln!(out, "{}{words}", s(f, "name"));
        let holders: Vec<String> = f["holders"]
            .as_array()
            .into_iter()
            .flatten()
            .map(label)
            .collect();
        if holders.is_empty() {
            let _ = writeln!(out, "  {}", t("(no holder tagged yet)"));
        } else {
            let _ = writeln!(out, "  {}", holders.join(", "));
        }
    }
}

/// `ev themes`: each place without a theme, the words its contents share, and what it reads
/// like.
fn theme_list(out: &mut String, list: &[Value]) {
    if list.is_empty() {
        let _ = writeln!(out, "{}", t("(every place with things in it has a theme)"));
    }
    for e in list {
        let _ = writeln!(
            out,
            "{}  ({})",
            head(&e["holder"]),
            tf("{} items", &[&e["things"]])
        );
        let _ = writeln!(out, "  {}", tf("words: {}", &[&theme_words(e)]));
        if e["like"].is_object() {
            let like = format!(
                "{} [{}]",
                label(&e["like"]),
                e["like"]["theme"].as_str().unwrap_or_default()
            );
            let _ = writeln!(out, "  {}", tf("reads like: {}", &[&like]));
        }
        let _ = writeln!(
            out,
            "  {}",
            // Names carry commas of their own, so the list is joined with semicolons.
            tf(
                "contents: {}",
                &[&e["contents"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("; ")]
            )
        );
    }
}

/// `ev audit`: alike things spread out, holders without a theme, loose items, and sizes a
/// name says that the field does not.
fn audit(out: &mut String, v: &Value) {
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
    if let Some(list) = v["size_drift"].as_array().filter(|l| !l.is_empty()) {
        let _ = writeln!(
            out,
            "{}",
            t("Boxes whose name says a size their size field does not:")
        );
        for n in list {
            let field = n["size"].as_str().unwrap_or("—");
            let _ = writeln!(
                out,
                "  {}  ({} → {field})",
                s(n, "path_text"),
                s(n, "name_size")
            );
        }
    }
}

/// `ev tree`: the tree, then what is lost listed apart, with where it was last seen.
fn tree_and_lost(out: &mut String, v: &Value, tr: &[Value]) {
    for n in tr {
        tree(out, n, 0);
    }
    // What is lost is listed apart, with where it was last seen.
    let lost = v["lost"].as_array().cloned().unwrap_or_default();
    if !lost.is_empty() {
        let _ = writeln!(out, "{}", t("Unknown place"));
        for n in &lost {
            let _ = writeln!(out, "  {}{}", thing(n), last_seen(n));
            for c in n["children"].as_array().into_iter().flatten() {
                tree(out, c, 2);
            }
        }
    }
}

fn settings(out: &mut String, v: &Value) {
    let lang = &v["language"];
    let setting = |x: &str| match x {
        "auto" => t("Automatic").to_string(),
        "en" => "English".to_string(),
        "tr" => "Türkçe".to_string(),
        "dark" => t("Dark").to_string(),
        "light" => t("Light").to_string(),
        other => other.to_string(),
    };
    let _ = writeln!(
        out,
        "{}: {}  ({})",
        t("Language"),
        setting(&s(lang, "setting")),
        tf(
            "shown in {}; the computer's language is {}",
            &[
                &setting(&s(lang, "effective")),
                &setting(&s(lang, "system"))
            ]
        )
    );
    let _ = writeln!(
        out,
        "{}: {}",
        t("Appearance"),
        setting(&s(&v["theme"], "setting"))
    );
    if let Some(f) = v["file"].as_str() {
        let _ = writeln!(out, "{}", tf("Saved in {}", &[&f]));
    }
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
        let _ = writeln!(out, "  {}", tf("holds {}", &[&line(c)]));
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::i18n::{Lang, set_lang};

    #[test]
    fn a_node_with_tasks_prints_the_node_not_only_its_tasks() {
        set_lang(Lang::En);
        let v = json!({
            "node": {"id": 5, "path_text": "Ev › Kutu", "name": "Kutu", "kind": "container"},
            "children": [],
            "tasks": [{"id": 1, "position": 2, "title": "Sort it", "via": 5}],
            "marks": {"label": {"value": "needed"}},
        });
        let out = super::human(&v);
        assert!(out.starts_with("#5"), "{out}");
        assert!(out.contains("task 2. #1 Sort it"), "{out}");
        assert!(out.contains("label: to print"), "{out}");
        set_lang(Lang::Tr);
        let out = super::human(&v);
        assert!(
            out.contains("tür: kap") && out.contains("etiket: basılacak"),
            "{out}"
        );
        set_lang(Lang::En);
    }

    #[test]
    fn centimetres_read_whole_or_to_a_tenth() {
        use serde_json::json;
        assert_eq!(super::cm(&json!(297.2558999999999)), "297.3");
        assert_eq!(super::cm(&json!(1842.0)), "1842");
        assert_eq!(super::cm(&json!(-300.04)), "-300");
        assert_eq!(super::cm(&json!(null)), "null");
    }
}
