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
        "digitize" => t("photograph, then throw out"),
        "merged" => t("joined another portion"),
        "used" => t("used up"),
        "left" => t("left behind"),
        "stolen" => t("stolen"),
        "unknown" => t("left, how not known"),
        "trade" => t("trade"),
        other => return other.to_string(),
    }
    .to_string()
}

/// `  (shred)` after a thing that leaves shredded, so the pile shows it.
fn shred_mark(n: &Value) -> &'static str {
    if n["shred"].as_bool() == Some(true) {
        t("  (shred)")
    } else {
        ""
    }
}

pub(crate) fn kind(k: &str) -> String {
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
        "gone" if s(n, "disposition") == "digitize" => out.push_str(t("  (gone, copy kept)")),
        "gone" => {
            let how = crate::history::left_as(&s(n, "disposition"));
            out.push_str(&tf("  (gone: {})", &[&how]));
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

/// When a box was called empty and what was said: `2026-10-05 · <note>`.
pub fn empty_said(e: &Value) -> String {
    let day: String = e["at"]
        .as_str()
        .unwrap_or_default()
        .chars()
        .take(10)
        .collect();
    match e["note"].as_str().filter(|n| !n.is_empty()) {
        Some(note) => format!("{day} · {note}"),
        None => day,
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
    // A box known to be empty, as `ev ui` marks it.
    if n["empty"] == true {
        out.push_str(&format!("  [{}]", t("empty")));
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

/// One list of `ev past`: its years with what left and the money, then each thing.
fn past_list(out: &mut String, l: &Value) {
    let sums = |m: &Value| -> String {
        m.as_object()
            .into_iter()
            .flatten()
            .map(|(c, a)| amount(a.as_str().unwrap_or_default(), c))
            .collect::<Vec<_>>()
            .join(" + ")
    };
    let undated = Some(&l["undated"]).filter(|u| u.is_object());
    for y in l["years"].as_array().into_iter().flatten().chain(undated) {
        let mut parts = vec![tf("{} left", &[&y["left"]])];
        let (paid, got) = (sums(&y["paid"]), sums(&y["got"]));
        if !paid.is_empty() {
            parts.push(tf("paid {}", &[&paid]));
        }
        if !got.is_empty() {
            parts.push(tf("got {}", &[&got]));
        }
        let year = match y["year"].as_i64() {
            Some(n) => n.to_string(),
            None => t("when not known").to_string(),
        };
        let _ = writeln!(out, "  {year}  {}", parts.join(" · "));
    }
    for n in l["past"].as_array().into_iter().flatten() {
        let mut parts = Vec::new();
        if let Some(c) = n["came"].as_str() {
            parts.push(tf("came {}", &[&c]));
        }
        let left = n["left"].as_str().unwrap_or(t("when not known"));
        parts.push(tf("left {}", &[&left]));
        parts.push(crate::history::left_as(&s(n, "how")).to_string());
        if let Some(w) = n["where"].as_str() {
            parts.push(tf("was in {}", &[&w]));
        }
        let paid = sums(&n["paid"]);
        if !paid.is_empty() {
            parts.push(tf("paid {}", &[&paid]));
        }
        if let Some(g) = n["got"].as_object() {
            let price = amount(
                g["price"].as_str().unwrap_or_default(),
                g["currency"].as_str().unwrap_or("TRY"),
            );
            parts.push(tf("got {}", &[&price]));
        }
        let _ = writeln!(
            out,
            "  #{} {}\n      {}",
            n["id"],
            s(n, "name"),
            parts.join(" · ")
        );
    }
}

/// A place not counted yet, as `left_here` and `left_nearby` give it: its state, its path and
/// the tasks it is in, or that no task holds it.
fn left_place_line(p: &Value) -> String {
    let tasks: Vec<String> = p["tasks"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t_| format!("#{} {}", t_["id"], s(t_, "title")))
        .collect();
    let tasks = if tasks.is_empty() {
        t("in no task").to_string()
    } else {
        tasks.join(", ")
    };
    format!(
        "  {} {}  [{tasks}]",
        review_mark(&serde_json::json!({ "status": p["status"] })),
        s(&p["node"], "path_text")
    )
}

/// `due 2026-10-04 (in 2 days)`, `(today)` or `(3 days overdue)`.
fn due_text(due: &str, days: &Value) -> String {
    let when = match days.as_i64() {
        Some(0) => t("today").to_string(),
        Some(n) if n < 0 => tf("{} days overdue", &[&-n]),
        Some(n) => tf("in {} days", &[&n]),
        None => return tf("due {}", &[&due]),
    };
    tf("due {} ({})", &[&due, &when])
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
    if let Some(d) = t_["due"].as_str() {
        // A closed task has no days left: only the date it was due.
        let closed = matches!(t_["status"].as_str(), Some("done" | "dropped"));
        let days = if closed {
            &Value::Null
        } else {
            &t_["days_left"]
        };
        let _ = writeln!(out, "     {}", due_text(d, days));
    }
    let _ = writeln!(out, "     {}", tf("why: {}", &[&s(t_, "why")]));
    for n in t_["nodes"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "     • {}", s(n, "path_text"));
    }
}

/// Why a place needs a new photo, as a suffix.
fn photo_reason(n: &Value) -> String {
    match n["photo_reason"].as_str() {
        Some("none") => t("  (no photo)").into(),
        Some("marked") => t("  (photo marked out of date)").into(),
        _ => tf(
            "  (changed {})",
            &[&crate::history::local_day(&s(n, "changed_at"))],
        ),
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

/// How a gone record left, in one line (spec/past-belongings.md): when, how, where it was, and
/// what a sale brought through what. None for a record that has not left.
pub(crate) fn departure_text(v: &Value) -> Option<String> {
    let d = v["departure"].as_object()?;
    // Sold while still here: only what it brought, it has not left yet.
    if d.get("pending").is_some_and(|p| p == true) {
        return d.get("price").and_then(Value::as_str).map(|p| {
            let c = d.get("currency").and_then(Value::as_str).unwrap_or("TRY");
            let via = d
                .get("via")
                .and_then(Value::as_str)
                .map(|v| format!(" · {}", tf("via {}", &[&v])))
                .unwrap_or_default();
            format!("{}{via}", tf("for {}", &[&amount(p, c)]))
        });
    }
    let mut parts = vec![
        d.get("at")
            .and_then(Value::as_str)
            .unwrap_or(t("when not known"))
            .to_string(),
    ];
    if let Some(how) = v["node"]["disposition"].as_str() {
        parts.push(crate::history::left_as(how).to_string());
    }
    if let Some(w) = d.get("where").and_then(Value::as_str) {
        parts.push(tf("was in {}", &[&w]));
    }
    // What it brought, then through what: one way of saying each.
    if let Some(p) = d.get("price").and_then(Value::as_str) {
        let c = d.get("currency").and_then(Value::as_str).unwrap_or("TRY");
        parts.push(tf("for {}", &[&amount(p, c)]));
    }
    if let Some(v) = d.get("via").and_then(Value::as_str) {
        parts.push(tf("via {}", &[&v]));
    }
    if let Some(t_) = d.get("traded_for").filter(|t_| t_.is_object()) {
        parts.push(tf("for {}", &[&format!("#{} {}", t_["id"], s(t_, "name"))]));
    }
    parts.retain(|p| !p.is_empty());
    Some(parts.join(" · "))
}

/// An amount in the reader's way of writing it: `1234.56 TRY` in English, `1.234,56 TL` in
/// Turkish (thousands by dots, decimals by a comma, the lira as TL).
pub(crate) fn amount(a: &str, currency: &str) -> String {
    if crate::i18n::lang() == crate::i18n::Lang::En {
        return format!("{a} {currency}").trim_end().to_string();
    }
    let (sign, digits) = a.strip_prefix('-').map_or(("", a), |d| ("-", d));
    let (whole, cents) = digits.split_once('.').unwrap_or((digits, ""));
    let mut grouped = String::new();
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(c);
    }
    let cur = if currency == "TRY" { "TL" } else { currency };
    let cents = if cents.is_empty() {
        String::new()
    } else {
        format!(",{cents}")
    };
    format!("{sign}{grouped}{cents} {cur}")
        .trim_end()
        .to_string()
}

/// A number with a fraction as the reader writes it: `4,35` in Turkish, `4.35` in English.
pub(crate) fn decimal(v: &Value) -> String {
    let text = v.to_string();
    if crate::i18n::lang() == crate::i18n::Lang::En {
        text
    } else {
        text.replace('.', ",")
    }
}

/// Where a word was found in a holder, as `ev suggest` says it: the holder's own theme, name
/// or note, or a thing's (`item: <name>`), in the reader's language.
fn match_source(from: &str) -> String {
    let (field, thing) = from.split_once(": ").unwrap_or((from, ""));
    let field = match field {
        "item" => t("a thing's name").to_string(),
        "item note" => t("a thing's note").to_string(),
        "item tag" => t("a thing's tag").to_string(),
        f => crate::history::field_word(f),
    };
    if thing.is_empty() {
        field
    } else {
        format!("{field}: {thing}")
    }
}

/// A sale's condition in the reader's language.
pub(crate) fn condition(c: &str) -> &str {
    match c {
        "new" => t("new"),
        "like-new" => t("like new"),
        "used" => t("used"),
        other => other,
    }
}

/// A coverage's kind in the reader's language.
pub(crate) fn coverage_kind(k: &str) -> &str {
    match k {
        "statutory" => t("statutory warranty"),
        "manufacturer" => t("manufacturer warranty"),
        "extended" => t("extended warranty"),
        "store" => t("store warranty"),
        "insurance" => t("insurance"),
        other => other,
    }
}

/// A purchase's dates, each named: the order date is the one the shop's order page shows (and
/// the one a person means by "bought on"), the delivery date follows when it differs. A bare
/// date was read as the purchase day when it was the delivery.
pub(crate) fn purchase_dates(p: &Value) -> Option<String> {
    let (ordered, delivered) = (p["ordered_at"].as_str(), p["delivered_at"].as_str());
    match (ordered, delivered) {
        (Some(o), Some(d)) if o != d => Some(tf("ordered {} · delivered {}", &[&o, &d])),
        (Some(o), _) => Some(tf("ordered {}", &[&o])),
        (None, Some(d)) => Some(tf("delivered {}", &[&d])),
        (None, None) => None,
    }
}

/// `ordered 2024-05-03 · Shop · ×2 · 500.00 TRY · ≈ 940.14 TRY today`: a purchase in one
/// line without the shop's title, for the details pane, which shows the title apart.
pub(crate) fn purchase_brief(p: &Value) -> String {
    let mut parts = Vec::new();
    parts.extend(purchase_dates(p));
    if let Some(sh) = p["shop"].as_str() {
        parts.push(sh.to_string());
    }
    let qty = p["linked_qty"]
        .as_i64()
        .unwrap_or(p["qty"].as_i64().unwrap_or(1));
    parts.push(format!("×{qty}"));
    if let Some(paid) = p["paid"].as_str() {
        parts.push(amount(paid, p["currency"].as_str().unwrap_or_default()));
    }
    if let Some(today) = p["today"].as_object() {
        parts.push(tf(
            "≈ {} in {} money",
            &[
                &amount(
                    today["amount"].as_str().unwrap_or_default(),
                    today["currency"].as_str().unwrap_or_default(),
                ),
                &today["index_month"].as_str().unwrap_or_default(),
            ],
        ));
    }
    if let Some(k) = p["kit"].as_str() {
        parts.push(tf("the set: kit {}", &[&k]));
    }
    parts.join(" · ")
}

/// `2500.00 TRY · 2026-09-01 · listing`: the current value in one line.
pub(crate) fn valuation_brief(x: &Value) -> String {
    let at = s(x, "at");
    let at = if x["approximate"] == true {
        tf("about {}", &[&at])
    } else {
        at
    };
    let mut parts = vec![amount(&s(x, "amount"), &s(x, "currency")), at];
    if let Some(src) = x["source"].as_str() {
        parts.push(src.to_string());
    }
    parts.join(" · ")
}

/// A document's kind in the reader's language.
pub(crate) fn doc_kind(k: &str) -> &str {
    match k {
        "invoice" => t("invoice"),
        "warranty" => t("warranty certificate"),
        "manual" => t("manual"),
        "service" => t("service form"),
        "appraisal" => t("appraisal"),
        "policy" => t("policy"),
        "scan" => t("scan"),
        "image" => t("product image"),
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

/// `#3  2026-10-01  2500.00 TRY  second-hand listing — note`: when, how much, from where.
pub(crate) fn valuation_line(x: &Value) -> String {
    let mut parts = vec![format!("#{}", x["id"]), s(x, "at")];
    if x["approximate"] == true {
        parts[1] = tf("about {}", &[&parts[1]]);
    }
    parts.push(amount(&s(x, "amount"), &s(x, "currency")));
    if let Some(today) = x["today"].as_object().filter(|_| x["currency"].is_string()) {
        let month = today["index_month"].as_str().unwrap_or_default();
        if !s(x, "at").starts_with(month) {
            parts.push(tf(
                "≈ {} in {} money",
                &[
                    &amount(
                        today["amount"].as_str().unwrap_or_default(),
                        today["currency"].as_str().unwrap_or_default(),
                    ),
                    &month,
                ],
            ));
        }
    }
    if let Some(src) = x["source"].as_str() {
        parts.push(src.to_string());
    }
    let note = x["note"]
        .as_str()
        .map(|n| format!(" — {n}"))
        .unwrap_or_default();
    format!("{}{note}", parts.join("  "))
}

/// `#2 info  https://…  (archived)`.
pub(crate) fn link_line(l: &Value) -> String {
    let mut parts = vec![
        format!(
            "#{} {}",
            l["id"],
            match l["kind"].as_str() {
                Some("info") => t("info page"),
                Some("manual") => t("manual"),
                Some("support") => t("support"),
                Some("driver") => t("driver"),
                _ => t("other"),
            }
        ),
        s(l, "url"),
    ];
    if let Some(a) = l["archive"].as_str() {
        parts.push(tf("archive: {}", &[&a]));
    }
    let note = l["note"]
        .as_str()
        .map(|n| format!(" — {n}"))
        .unwrap_or_default();
    format!("{}{note}", parts.join("  "))
}

/// `#12 2024-05-03  Amazon  Bosch GSB 13 RE ×1  1999.00 TRY`: when, where, what, how many, paid.
pub(crate) fn purchase_line(p: &Value) -> String {
    let mut parts = vec![format!("#{}", p["id"])];
    parts.extend(purchase_dates(p));
    if let Some(sh) = p["shop"].as_str() {
        parts.push(sh.to_string());
    }
    let qty = p["linked_qty"]
        .as_i64()
        .unwrap_or(p["qty"].as_i64().unwrap_or(1));
    parts.push(format!("{} ×{qty}", s(p, "name")));
    if let Some(pack) = p["pack"].as_i64().filter(|n| *n > 1)
        && p["linked_qty"].is_null()
    {
        parts.push(tf("({} units each)", &[&pack]));
    }
    if let Some(paid) = p["paid"].as_str() {
        parts.push(amount(paid, p["currency"].as_str().unwrap_or_default()));
    }
    if let Some(today) = p["today"].as_object() {
        parts.push(tf(
            "≈ {} in {} money",
            &[
                &amount(
                    today["amount"].as_str().unwrap_or_default(),
                    today["currency"].as_str().unwrap_or_default(),
                ),
                &today["index_month"].as_str().unwrap_or_default(),
            ],
        ));
    }
    // Bought as a kit's line: the whole set, for every part of it.
    if let Some(k) = p["kit"].as_str() {
        parts.push(tf("the set: kit {}", &[&k]));
    }
    parts.join("  ")
}

/// Where a line stands: dismissed, returned, or how much of it is still to link.
fn purchase_state(p: &Value) -> String {
    if let Some(d) = p["dismissed"].as_str() {
        let why = match d {
            "consumed" => t("consumed"),
            "given" => t("given"),
            "returned" => t("returned to the shop"),
            "elsewhere" => t("elsewhere"),
            "not-mine" => t("not mine"),
            "duplicate" => t("a duplicate"),
            other => other,
        };
        return tf("[dismissed: {}]", &[&why]);
    }
    let open = p["open_qty"].as_i64().unwrap_or(0);
    let kits: Vec<&str> = p["kits"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let covers: Vec<String> = p["coverages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| format!("#{c}"))
        .collect();
    let mut st = if !kits.is_empty() {
        tf("[bought as kit {}]", &[&kits.join(", ")])
    } else if !covers.is_empty() {
        tf("[bought as coverage {}]", &[&covers.join(", ")])
    } else if p["bucket"] == "digital" {
        format!("[{}]", t("digital"))
    } else if p["bucket"] == "service" {
        format!("[{}]", t("services"))
    } else if open > 0 {
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
        // A thing that left is named where it was, marked as gone.
        let gone = if l["node"]["state"] == "gone" {
            format!("  {}", t("[gone]"))
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "  → #{} {} ×{}{gone}",
            l["node"]["id"],
            s(&l["node"], "path_text"),
            l["qty"]
        );
    }
    for d in p["documents"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  {}: {}", t("document"), doc_line(d));
    }
    if let Some(of) = p["same_as"].as_i64() {
        let _ = writeln!(out, "  {}", tf("the same purchase as #{}", &[&of]));
    }
    for d in p["declined"].as_array().into_iter().flatten() {
        let why = d["why"]
            .as_str()
            .map(|w| format!(" — {w}"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  {}: #{} {}{why}",
            t("not"),
            d["node"]["id"],
            s(&d["node"], "path_text")
        );
    }
    for j in p["joined"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  {}", tf("also seen as #{}", &[j]));
    }
    for a in p["attachments"].as_array().into_iter().flatten() {
        let what = match a["type"].as_str() {
            Some("link") => format!("{}  {}", t("link"), s(a, "url")),
            Some("image") => format!("{}  {}", t("product image"), s(a, "file")),
            Some("valuation") => {
                let at = a["at"].as_str().unwrap_or("-");
                let at = if a["approximate"] == true {
                    tf("about {}", &[&at])
                } else {
                    at.to_string()
                };
                format!(
                    "{}  {} {}  {at}",
                    t("value"),
                    s(a, "amount"),
                    a["currency"].as_str().unwrap_or_default()
                )
            }
            _ => format!(
                "{}  {} {}",
                t("coverage"),
                a["kind"].as_str().unwrap_or_default(),
                a["term"]
                    .as_str()
                    .or(a["ends"].as_str())
                    .unwrap_or_default()
            ),
        };
        let state = match a["brought_to"].as_i64() {
            Some(n) => tf("brought to #{}", &[&n]),
            None => t("to bring").to_string(),
        };
        let _ = writeln!(out, "  {} #{}: {what}  [{state}]", t("attachment"), a["id"]);
    }
}

fn type_counts(counts: &Value) -> String {
    counts
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, n)| format!("{} ×{n}", attachment_type(k)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `ev buy bring --all`: the total, each line it brought from, then each it left and why.
fn bring_all(out: &mut String, v: &Value) {
    let from = v["brought_from"].as_array().cloned().unwrap_or_default();
    if from.is_empty() {
        // Why nothing came: what was looked at and what it carried.
        let c = &v["checked"];
        let _ = writeln!(
            out,
            "{}",
            tf(
                "Nothing brought: {} linked lines checked, {} carry that type, {} of it brought already.",
                &[&c["lines"], &c["carrying"], &c["already"]]
            )
        );
    } else {
        let _ = writeln!(
            out,
            "{}",
            tf(
                "Brought from {} purchases: {}",
                &[&from.len(), &type_counts(&v["brought_types"])]
            )
        );
    }
    for f in &from {
        let _ = writeln!(
            out,
            "  #{} → #{} {}: {}",
            f["purchase"],
            f["node"],
            s(f, "name"),
            type_counts(&f["brought_types"])
        );
    }
    for l in v["left"].as_array().into_iter().flatten() {
        let why = if l["why"] == "gone" {
            tf("#{} {} is gone", &[&l["node"], &s(l, "name")])
        } else {
            t("linked to several things").to_string()
        };
        let _ = writeln!(
            out,
            "  {}",
            tf(
                "left #{}: {}, {} waiting; ev buy bring {} <ref> brings it",
                &[&l["purchase"], &why, &l["waiting"], &l["purchase"]]
            )
        );
    }
}

fn attachment_type(kind: &str) -> &'static str {
    match kind {
        "link" => t("link"),
        "valuation" => t("value"),
        "coverage" => t("coverage"),
        _ => t("product image"),
    }
}

/// What `ev buy bring` did, above the thing: what it brought by type, what it left and why.
fn bring_summary(out: &mut String, v: &Value) {
    let line = &v["from_purchase"];
    let brought: Vec<String> = v["brought_types"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, n)| format!("{} ×{n}", attachment_type(k)))
        .collect();
    let skipped = v["skipped"].as_array().cloned().unwrap_or_default();
    if !brought.is_empty() {
        let _ = writeln!(
            out,
            "{}",
            tf(
                "Brought from purchase #{}: {}",
                &[line, &brought.join(", ")]
            )
        );
    } else if skipped.is_empty() {
        let _ = writeln!(
            out,
            "{}",
            tf(
                "Nothing brought: purchase #{} carries nothing to bring.",
                &[line]
            )
        );
    } else {
        let _ = writeln!(out, "{}", tf("Nothing brought from purchase #{}.", &[line]));
    }
    let ids = |why: &str| -> Vec<String> {
        skipped
            .iter()
            .filter(|s| s["why"] == why)
            .map(|s| {
                format!(
                    "#{} {}",
                    s["id"],
                    attachment_type(s["type"].as_str().unwrap_or_default())
                )
            })
            .collect()
    };
    let before = ids("brought");
    if !before.is_empty() {
        let _ = writeln!(
            out,
            "  {}",
            tf("already brought: {}", &[&before.join(", ")])
        );
    }
    let other = ids("type");
    if !other.is_empty() {
        let _ = writeln!(
            out,
            "  {}",
            tf("left, of another type: {}", &[&other.join(", ")])
        );
    }
    let _ = writeln!(out);
}

/// `  1. #12 2024-05-03  Amazon  Bosch GSB 13 RE ×1  1999.00 TRY  (73: model gsb13re 60, …)`.
fn candidate_lines(out: &mut String, list: &Value) {
    for (i, c) in list.as_array().into_iter().flatten().enumerate() {
        let _ = writeln!(out, "  {}. {}", i + 1, candidate_line(c));
    }
}

/// One candidate line without its number: the purchase, then its score and reasons.
fn candidate_line(c: &Value) -> String {
    let why = c["why"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|w| format!("{} {}", s(w, "why"), w["points"]))
        .collect::<Vec<_>>()
        .join(", ");
    let linked = if c["linked"] == true {
        format!("  {}", t("[linked]"))
    } else {
        String::new()
    };
    format!(
        "{}{linked}  ({}: {why})",
        purchase_line(&c["purchase"]),
        decimal(&c["score"])
    )
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
                let _ = writeln!(
                    out,
                    "  {}: {}{sale}{}",
                    disposition(d),
                    line(n),
                    shred_mark(n)
                );
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
    ] {
        if head(out, title, &c[key]) {
            for n in v[key].as_array().into_iter().flatten() {
                let extra = match key {
                    "repairs" => n["note"].as_str().map(|x| format!("  ({x})")),
                    "expiring" => Some(tf("  {} ({} days)", &[&s(n, "expires"), &n["days_left"]])),
                    // A thing that only waits for another is not parked anywhere.
                    "parked" => Some(format!(
                        "{}{}",
                        if n["why"] == "waits_for" {
                            String::new()
                        } else {
                            tf(
                                "  (parked in {})",
                                &[&n["in"]["code"]
                                    .as_str()
                                    .map_or_else(|| s(&n["in"], "path_text"), str::to_string)],
                            )
                        },
                        n.get("waits_for")
                            .map(|w| tf("  waits for #{} {}", &[&w["id"], &s(w, "name")]))
                            .unwrap_or_default()
                    )),
                    // Begun, it is still to count, and says so.
                    "uncounted" if n["review"]["status"] == "counting" => {
                        Some(format!("  {}", t("[being counted]")))
                    }
                    _ => None,
                }
                .unwrap_or_default();
                let id = n["id"]
                    .as_i64()
                    .map(|i| format!("#{i} "))
                    .unwrap_or_default();
                let _ = writeln!(out, "  {id}{}{extra}", s(n, "path_text"));
            }
        }
    }
    // Only a photo needed now is listed; a place not counted yet gets its photo on its tour.
    if head(
        out,
        t("Photo of the current state needed"),
        &c["photos_now"],
    ) {
        for n in v["photos"].as_array().into_iter().flatten() {
            if n["when"] == "now" {
                let _ = writeln!(out, "  {}{}", s(n, "path_text"), photo_reason(n));
            }
        }
    }
    let later = c["photos"].as_u64().unwrap_or(0) - c["photos_now"].as_u64().unwrap_or(0);
    if later > 0 {
        let _ = writeln!(
            out,
            "\n{}",
            tf("{} more places get their photo on their tour", &[&later])
        );
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
    if head(out, t("Coverage ending"), &c["coverage_ending"]) {
        for cv in v["coverage_ending"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "  {}", coverage_line(cv));
            for n in cv["nodes"].as_array().into_iter().flatten() {
                let _ = writeln!(out, "    {}", line(n));
            }
        }
    }
    if head(out, t("Coverage not asked"), &c["coverage"]) {
        let cv = &v["coverage"];
        let _ = writeln!(
            out,
            "  {}",
            tf(
                "valuable things (from {}) with no warranty or insurance recorded; the dearest:",
                &[&amount(&s(cv, "threshold"), &s(cv, "currency"))]
            )
        );
        for n in cv["top"].as_array().into_iter().flatten() {
            let _ = writeln!(
                out,
                "  {}  {}",
                line(n),
                amount(&s(n, "worth"), n["currency"].as_str().unwrap_or("TRY"))
            );
        }
    }
    if head(out, t("Value not asked"), &c["values"]) {
        let _ = writeln!(
            out,
            "  {}",
            t("bought things with no value recorded; the dearest:")
        );
        for n in v["values"]["top"].as_array().into_iter().flatten() {
            let _ = writeln!(
                out,
                "  {}  {}",
                line(n),
                amount(&s(n, "worth"), n["currency"].as_str().unwrap_or("TRY"))
            );
        }
    }
    if head(out, t("Purchases to link"), &c["purchases"]) {
        let _ = writeln!(
            out,
            "  {}",
            t(
                "durable purchase lines not linked to a thing yet: ev buy list --open --bucket durable"
            )
        );
    }
}

/// A coverage's status in the reader's language, with the days left when it ends soon.
fn coverage_status(cv: &Value) -> String {
    match cv["status"].as_str().unwrap_or_default() {
        "active" if cv["end"].is_null() => t("active, lifetime").to_string(),
        "active" => tf("active until {}", &[&s(cv, "end")]),
        "ending" => tf("ends {} ({} days left)", &[&s(cv, "end"), &cv["days_left"]]),
        "ended" => tf("ended {}", &[&s(cv, "end")]),
        _ => t("undetermined: no start known").to_string(),
    }
}

/// A coverage's term as `ev` keeps it (`2 years`, `1 month`, `lifetime`) in the reader's words.
fn term_text(term: &str) -> String {
    if term == "lifetime" {
        return t("lifetime").to_string();
    }
    let Some((n, unit)) = term.split_once(' ') else {
        return term.to_string();
    };
    let one = n == "1";
    let unit = match unit.trim_end_matches('s') {
        "year" if one => t("year"),
        "year" => t("years"),
        "month" if one => t("month"),
        "month" => t("months"),
        "week" if one => t("week"),
        "week" => t("weeks"),
        "day" if one => t("day"),
        "day" => t("days"),
        _ => return term.to_string(),
    };
    format!("{n} {unit}")
}

/// `#3 manufacturer  Bosch  2 years from delivery  active until 2026-05-03`.
pub(crate) fn coverage_line(cv: &Value) -> String {
    let kind = coverage_kind(cv["kind"].as_str().unwrap_or_default());
    let mut parts = vec![format!("#{} {kind}", cv["id"])];
    for k in ["issuer", "number", "term", "usage"] {
        if let Some(x) = cv[k].as_str() {
            parts.push(if k == "term" {
                term_text(x)
            } else {
                x.to_string()
            });
        }
    }
    if let Some(r) = cv["repair_days"].as_i64() {
        parts.push(tf("+{} days in repair", &[&r]));
    }
    parts.push(coverage_status(cv));
    parts.join("  ")
}

/// One coverage: its line, what it covers, its documents.
fn coverage(out: &mut String, cv: &Value) {
    let _ = writeln!(out, "{}", coverage_line(cv));
    for (k, label) in [("scope", t("scope")), ("note", t("note"))] {
        if let Some(x) = cv[k].as_str() {
            let _ = writeln!(out, "  {label}: {x}");
        }
    }
    if let Some(p) = cv["premium"].as_str() {
        let cur = cv["currency"].as_str().unwrap_or("TRY");
        let _ = writeln!(out, "  {}: {}", t("premium"), amount(p, cur));
    }
    for n in cv["nodes"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  → #{} {}", n["id"], s(n, "path_text"));
    }
    // The line it was bought as.
    if let Some(p) = cv["purchase"].as_object() {
        let paid = p
            .get("paid")
            .and_then(Value::as_str)
            .map(|a| {
                format!(
                    " · {}",
                    amount(
                        a,
                        p.get("currency").and_then(Value::as_str).unwrap_or("TRY")
                    )
                )
            })
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  {}: #{} {}{paid}",
            t("bought as"),
            cv["purchase"]["id"],
            s(&cv["purchase"], "name")
        );
    }
    for d in cv["documents"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  {}: {}", t("document"), doc_line(d));
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
    if !free.is_empty() {
        let _ = writeln!(
            out,
            "  {}",
            tf("free ({}): {}", &[&free.len(), &free.join(" ")])
        );
    }
    // Each box by its cells, then its code (what its label says, for sticking labels on), then
    // its name.
    let boxes = grid["boxes"].as_array().cloned().unwrap_or_default();
    let width = boxes
        .iter()
        .filter_map(|b| b["code"].as_str())
        .map(|c| c.chars().count())
        .max()
        .unwrap_or(0);
    for b in &boxes {
        let code = b["code"].as_str().unwrap_or_default();
        let line = if width == 0 {
            format!("{:<7} {}", s(b, "cells"), s(b, "name"))
        } else {
            format!("{:<7} {code:<width$}  {}", s(b, "cells"), s(b, "name"))
        };
        let _ = writeln!(out, "  {}", line.trim_end());
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
pub(crate) fn label(n: &Value) -> String {
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
        let empty = v["empty"].as_array().cloned().unwrap_or_default();
        if !empty.is_empty() {
            let _ = writeln!(out, "{}", t("Empty boxes to start it in:"));
        }
        for b in &empty {
            let here = if b["same_room"] == true {
                t("  (same room)")
            } else {
                ""
            };
            let _ = writeln!(out, "  #{} {}{here}", b["id"], s(b, "path_text"));
        }
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
                    &decimal(&x["score"]),
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
                format!(
                    "{}{mark} {} ({})",
                    s(m, "term"),
                    decimal(&m["points"]),
                    match_source(&s(m, "from"))
                )
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
                tf("score {}", &[&decimal(&x["score"])])
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
                tf("score {}", &[&decimal(&x["score"])])
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

/// `ev history`: newest first under a heading per day, each event in the words of `ev ui`'s
/// History tab. A place the event names is its `#id` (the text has no tree to name it by), and a
/// thing that came, went or was added here leads with its own.
fn history(out: &mut String, events: &[Value]) {
    let place = |v: &Value| -> String {
        match v {
            Value::Number(n) => format!("#{n}"),
            Value::String(s) => s.clone(),
            _ => "—".into(),
        }
    };
    let mut day = None;
    for e in events.iter().rev() {
        let at = crate::history::local_time(e);
        let this = at.map(|a| a.date_naive());
        if this != day {
            day = this;
            let _ = writeln!(out, "{}", crate::history::day_heading(this));
        }
        let time = at.map_or_else(String::new, |a| a.format("%H:%M").to_string());
        let (verb, detail, _) = crate::history::event_words(e, &place);
        let detail = match e["item"]["id"].as_i64() {
            Some(id) => format!("#{id} {detail}"),
            None => detail,
        };
        let row = format!("  {time}  {verb}  {detail}");
        let _ = writeln!(out, "{}", row.trim_end());
    }
}

/// `ev layout`: a piece of furniture read across its places, then the drafted layout when asked.
fn layout(out: &mut String, v: &Value) {
    let f = &v["furniture"];
    let _ = writeln!(
        out,
        "{}  {}",
        label(f),
        tf(
            "{}: {} places, {} things",
            &[&s(f, "name"), &v["places"], &v["things"]]
        )
    );
    let section = |out: &mut String, key: &str, title: &str| -> Vec<Value> {
        let list = v[key].as_array().cloned().unwrap_or_default();
        if !list.is_empty() {
            let _ = writeln!(out, "\n{title}");
        }
        list
    };
    for e in section(out, "spread", t("Kinds spread over several places:")) {
        let places: Vec<String> = e["places"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| format!("{} {}", label(&p["place"]), p["things"]))
            .collect();
        let _ = writeln!(
            out,
            "  {} ({}): {}",
            s(&e, "word"),
            e["things"],
            places.join(", ")
        );
    }
    for e in section(out, "overlap", t("Places that read alike:")) {
        let pair: Vec<String> = e["places"]
            .as_array()
            .into_iter()
            .flatten()
            .map(label)
            .collect();
        let _ = writeln!(
            out,
            "  {} ({}): {}",
            pair.join(" ~ "),
            e["alike"],
            list_str(&e["shared"])
        );
    }
    for e in section(out, "merge", t("Nearly empty, could join another:")) {
        if !e["into"].is_object() {
            // Nothing in the furniture reads like it.
            let _ = writeln!(
                out,
                "  {}  ({})",
                label(&e["place"]),
                tf("{} things", &[&e["things"]])
            );
            continue;
        }
        let _ = writeln!(
            out,
            "  {} → {}  {}",
            label(&e["place"]),
            label(&e["into"]),
            tf(
                "({} things; shared: {})",
                &[&e["things"], &list_str(&e["shared"])]
            )
        );
    }
    for e in section(out, "split", t("Full or mixed, could be split:")) {
        let why = if s(&e, "why") == "full" {
            t("full")
        } else {
            t("mixed")
        };
        let fill = e["fill"]
            .as_u64()
            .map(|p| format!(", %{p}"))
            .unwrap_or_default();
        let groups: Vec<String> = e["groups"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|g| format!("{} {}", s(g, "word"), g["things"]))
            .collect();
        let _ = writeln!(
            out,
            "  {} ({}, {}{fill}): {}",
            label(&e["place"]),
            why,
            tf("{} things", &[&e["things"]]),
            groups.join(", ")
        );
    }
    let p = &v["proposal"];
    if !p.is_object() {
        return;
    }
    let _ = writeln!(out, "\n{}", t("Draft layout (nothing is moved):"));
    // Places not counted yet are left out: their records may still be wrong.
    let not_counted: Vec<String> = p["not_counted"]
        .as_array()
        .into_iter()
        .flatten()
        .map(label)
        .collect();
    if !not_counted.is_empty() {
        let _ = writeln!(
            out,
            "  {}",
            tf(
                "Not counted yet, left out of the draft: {}",
                &[&not_counted.join(", ")]
            )
        );
    }
    let moves = p["moves"].as_array().cloned().unwrap_or_default();
    for th in p["themes"].as_array().into_iter().flatten() {
        let place = label(&th["place"]);
        let coming: Vec<&Value> = moves.iter().filter(|m| label(&m["to"]) == place).collect();
        let _ = writeln!(
            out,
            "  {place}  {}",
            tf(
                "{} ({} things, {} to bring)",
                &[&s(th, "theme"), &th["things"], &coming.len()]
            )
        );
        for m in coming {
            let _ = writeln!(
                out,
                "    {}  {}",
                thing(&m["thing"]),
                tf("from {}", &[&label(&m["from"])])
            );
        }
    }
    let kept: Vec<String> = p["kept"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|k| format!("{} {}", s(k, "word"), k["things"]))
        .collect();
    if !kept.is_empty() {
        let _ = writeln!(
            out,
            "  {}",
            tf(
                "No place left for these, they stay where they are: {}",
                &[&kept.join(", ")]
            )
        );
    }
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

/// A result as JSON for a program: one line (spec/output.md).
pub fn json(v: &Value) -> String {
    serde_json::to_string(&for_program(v)).unwrap_or_default()
}

/// A result as a program reads it: below the top level, a field with no value is left out, so
/// a missing field reads as null; the top-level keys stay, a payload's sections always there to
/// look for — except in the node payload (`show`, and the commands answering with the node they
/// changed), which keeps `node` and only the sections with something in them (spec/output.md).
pub fn for_program(v: &Value) -> Value {
    let mut v = v.clone();
    match &mut v {
        Value::Object(m) => {
            if m.contains_key("node") && m.contains_key("children") {
                m.retain(|k, x| k == "node" || has_something(x));
            }
            m.values_mut().for_each(drop_nulls);
        }
        Value::Array(a) => a.iter_mut().for_each(drop_nulls),
        _ => {}
    }
    v
}

/// A value that says something: not null, and not a list or an object of nothing.
fn has_something(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Array(a) => !a.is_empty(),
        Value::Object(m) => m.values().any(has_something),
        _ => true,
    }
}

fn drop_nulls(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.retain(|_, x| !x.is_null());
            m.values_mut().for_each(drop_nulls);
        }
        Value::Array(a) => a.iter_mut().for_each(drop_nulls),
        _ => {}
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one branch per output shape; the long shapes have their own functions"
)]
pub fn human(v: &Value) -> String {
    let mut out = String::new();
    if v.get("overview").is_some() && v.get("tour").is_some() {
        stats_text(&mut out, v);
        return out;
    }
    if v.get("brought_from").is_some() {
        bring_all(&mut out, v);
        return out;
    }
    // `ev empty`: the boxes now known to be empty on the person's word.
    if let Some(list) = v.get("empty").and_then(Value::as_array).filter(|_| {
        v.as_object()
            .is_some_and(|o| o.keys().all(|k| k == "empty" || k == "open_tasks"))
    }) {
        let _ = writeln!(out, "{}", t("Empty, on the person's word:"));
        for n in list {
            let _ = writeln!(out, "  {}", line(n));
            // Its note may say otherwise: read it back.
            if let Some(note) = n["note"].as_str() {
                let _ = writeln!(out, "    {}: {note}", t("note"));
            }
        }
        for task in v["open_tasks"].as_array().into_iter().flatten() {
            let _ = writeln!(
                out,
                "{}",
                tf(
                    "Task #{} is still open on {}: {} (done, or drop it?)",
                    &[&task["task"], &label(&task["on"]), &s(task, "title")]
                )
            );
        }
        return out;
    }
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
    // Several sketched at once (`ev sketch --stdin`): each as one is shown.
    if let Some(list) = v.get("sketched").and_then(Value::as_array)
        && v.as_object().is_some_and(|o| o.len() == 1)
    {
        for one in list {
            sketch(&mut out, one);
        }
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
                "{} new, {} updated, {} unchanged, {} skipped; {} document links, {} documents skipped; {} attachments; {} joined to another source's line",
                &[
                    &i["new"],
                    &i["updated"],
                    &i["unchanged"],
                    &i["skipped"],
                    &i["document_links"],
                    &i["documents_skipped"],
                    &i["attachments"],
                    &i["joined"]
                ]
            )
        );
        return out;
    }
    if v.get("purchase").is_some_and(Value::is_object) {
        purchase(&mut out, &v["purchase"]);
        // `ev buy unlink`: what it took back from the thing, and what it left there.
        if let Some(taken) = v["taken_back"].as_array() {
            let types: Vec<&str> = taken.iter().filter_map(|a| a["type"].as_str()).collect();
            let _ = writeln!(
                out,
                "{}",
                tf("Taken back from the thing: {}", &[&types.join(", ")])
            );
        }
        for l in v["left"].as_array().into_iter().flatten() {
            let _ = writeln!(
                out,
                "{}",
                tf("Left on the thing: {} ({})", &[&s(l, "type"), &s(l, "how")])
            );
        }
        return out;
    }
    if let (Some(list), Some(node)) = (
        v.get("valuations").and_then(Value::as_array),
        v.get("node").filter(|_| v.get("added").is_some()),
    ) {
        let _ = writeln!(out, "{}", line(node));
        if list.is_empty() {
            let _ = writeln!(out, "  {}", t("(no value recorded)"));
        }
        for x in list {
            let _ = writeln!(out, "  {}", valuation_line(x));
        }
        return out;
    }
    if let (Some(list), Some(node)) = (
        v.get("links").and_then(Value::as_array),
        v.get("node").filter(|_| v.get("children").is_none()),
    ) {
        let _ = writeln!(out, "{}", line(node));
        if list.is_empty() {
            let _ = writeln!(out, "  {}", t("(no links)"));
        }
        for l in list {
            let _ = writeln!(out, "  {}", link_line(l));
        }
        return out;
    }
    if v.get("coverage").is_some_and(|c| c.get("status").is_some()) {
        coverage(&mut out, &v["coverage"]);
        return out;
    }
    if let Some(list) = v.get("coverages").and_then(Value::as_array)
        && v.get("node").is_none()
    {
        if list.is_empty() {
            let _ = writeln!(out, "{}", t("(no coverage)"));
        }
        for cv in list {
            let _ = writeln!(out, "{}", coverage_line(cv));
            // What each covers, so the list answers "which thing?" without a `cover show`.
            for n in cv["nodes"].as_array().into_iter().flatten() {
                let _ = writeln!(out, "  → #{} {}", n["id"], s(n, "path_text"));
            }
        }
        return out;
    }
    if let Some(m) = v.get("money") {
        let latest = m["latest"].as_str().unwrap_or("-");
        let stale = if m["stale"] == true {
            format!("  {}", t("(stale: fetch again)"))
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "{}",
            tf(
                "{} index: {} periods, latest {}{}; {} rates, {} missing; home currency {}",
                &[
                    &s(m, "index"),
                    &m["periods"],
                    &latest,
                    &stale,
                    &m["rates"],
                    &m["missing_rates"],
                    &s(m, "home_currency")
                ]
            )
        );
        return out;
    }
    // `ev money needs`: what tools/money should fetch (it reads the JSON; this is for a person).
    if let Some(m) = v.get("money_needs") {
        let from = m["from_month"].as_str().unwrap_or("-");
        let _ = writeln!(
            out,
            "{}",
            tf(
                "{} index from {}; home currency {}, country {}",
                &[
                    &s(m, "index"),
                    &from,
                    &s(m, "home_currency"),
                    &s(m, "home_country")
                ]
            )
        );
        let rates = m["rates"].as_array().cloned().unwrap_or_default();
        if rates.is_empty() {
            let _ = writeln!(out, "{}", t("No exchange rate missing."));
        } else {
            let _ = writeln!(
                out,
                "{}",
                tf("Exchange rates missing ({}):", &[&rates.len()])
            );
            for r in &rates {
                let _ = writeln!(out, "  {} {}", s(r, "currency"), s(r, "day"));
            }
        }
        return out;
    }
    if let Some(m) = v.get("money_imported") {
        let _ = writeln!(
            out,
            "{}",
            tf(
                "{} index values, {} rates imported",
                &[&m["index"], &m["rates"]]
            )
        );
        return out;
    }
    if let Some(r) = v.get("removed") {
        let _ = writeln!(out, "{}", tf("coverage #{} removed", &[r]));
        return out;
    }
    if let Some(o) = v.get("inventory_settings").and_then(Value::as_object) {
        for (k, x) in o {
            let default = if x["default"] == true {
                format!("  {}", t("(default)"))
            } else {
                String::new()
            };
            let _ = writeln!(out, "{k}: {}{default}", s(x, "value"));
        }
        return out;
    }
    if let Some(list) = v.get("backfill").and_then(Value::as_array) {
        if list.is_empty() {
            let _ = writeln!(
                out,
                "{}",
                tf(
                    "(none of {} unlinked things in toured places could be a purchase)",
                    &[&v["toured_things"]]
                )
            );
        } else {
            let _ = writeln!(
                out,
                "{}",
                tf(
                    "{} of {} unlinked things in toured places could be a purchase, best first:",
                    &[&list.len(), &v["toured_things"]]
                )
            );
        }
        for (i, b) in list.iter().enumerate() {
            let _ = writeln!(out, "  {}. {}", i + 1, line(&b["node"]));
            let _ = writeln!(out, "     {}", candidate_line(&b["candidate"]));
        }
        return out;
    }
    if let Some(list) = v.get("candidates") {
        let _ = writeln!(out, "{}", line(&v["node"]));
        if list.as_array().is_none_or(Vec::is_empty) {
            let _ = writeln!(out, "  {}", t("(no purchase could be this)"));
        }
        candidate_lines(&mut out, list);
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
        let status = match v["status"].as_str() {
            Some("open") => t("still to get"),
            Some("got") => t("got"),
            Some("dropped") => t("dropped"),
            _ => "?",
        };
        let _ = writeln!(out, "{}  [{status}]", need_line(v));
        return out;
    }
    if v.get("open_tasks").is_some() {
        next(&mut out, v);
        return out;
    }
    if v.get("units").is_some() && v.get("places").is_some() {
        if v["scope"].is_object() {
            let _ = writeln!(out, "{}", s(&v["scope"], "path_text"));
        }
        let _ = writeln!(out, "{}", progress_line(v));
        for p in v["places"].as_array().into_iter().flatten() {
            let tasks: Vec<String> = p["tasks"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|t_| format!("#{} {}", t_["id"], s(t_, "title")))
                .collect();
            let tasks = if tasks.is_empty() {
                String::new()
            } else {
                format!("  [{}]", tasks.join(", "))
            };
            let _ = writeln!(
                out,
                "  {} {}{tasks}",
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
        if v["stopped"].is_object() {
            let _ = writeln!(
                out,
                "{}",
                tf("#{} is back to open, not finished", &[&v["stopped"]["id"]])
            );
        }
        return out;
    }
    if v.get("goal").is_some() && v.as_object().is_some_and(|o| o.len() == 1) {
        let _ = writeln!(out, "{}", tf("Goal: {}", &[&goal(v)]));
        return out;
    }
    // `ev kit part` / `drop`: the parts added, or the one taken off, and the kit's counts.
    if let Some(kit) = v.get("kit").filter(|k| k.is_object())
        && (v.get("added").is_some() || v.get("dropped").is_some())
    {
        for p in v["added"].as_array().into_iter().flatten() {
            kit_part_lines(&mut out, p);
        }
        if let Some(d) = v.get("dropped") {
            let _ = writeln!(
                out,
                "{}",
                tf("Part {} taken off: {}", &[&d["n"], &s(d, "text")])
            );
        }
        let c = &v["counts"];
        let _ = writeln!(
            out,
            "#{} {}  {}",
            kit["id"],
            s(kit, "name"),
            tf(
                "{} of {} found · {} lost · {} still missing",
                &[&c["found"], &c["expected"], &c["lost"], &c["open"]]
            )
        );
        return out;
    }
    // `ev photo add` of several photos: each record with the photo it got and where from.
    if let Some(added) = v.get("added").and_then(Value::as_array) {
        let _ = writeln!(out, "{}", tf("Attached {} photo(s):", &[&added.len()]));
        for a in added {
            let _ = writeln!(
                out,
                "  {}  {} {}  ← {}",
                line(&a["node"]),
                t("photo"),
                a["photo"],
                s(a, "from")
            );
        }
        return out;
    }
    // `ev buy bring`: what was brought to the thing, what was left and why.
    if let Some(node) = v.get("node").filter(|_| v.get("from_purchase").is_some()) {
        bring_summary(&mut out, v);
        let _ = writeln!(out, "{}", line(node));
        return out;
    }
    if let Some(node) = v.get("node").filter(|_| v.get("children").is_some()) {
        show(&mut out, v, node);
        // `ev review --as toured`: which records the final photo shows, and which not yet.
        let check = &v["photo_check"];
        let unshown = check["not_located"].as_array().map_or(0, Vec::len);
        if unshown > 0 {
            let _ = writeln!(
                out,
                "\n{}",
                tf(
                    "Not shown on the final photo yet ({} of {}): say which is which",
                    &[
                        &unshown,
                        &(unshown + check["located"].as_array().map_or(0, Vec::len))
                    ]
                )
            );
            for n in check["not_located"].as_array().into_iter().flatten() {
                let _ = writeln!(out, "  {}", line(n));
            }
        }
        // `ev review`: what is still not counted around the place (spec/counting.md).
        let left = &v["left_here"];
        for (key, head) in [
            ("furniture", "Still not counted in {}:"),
            ("room", "Still not counted elsewhere in {}:"),
        ] {
            if left[key].is_object() {
                let where_ = s(&left[key]["node"], "path_text");
                let _ = writeln!(out, "\n{}", tf(head, &[&where_]));
                for p in left[key]["places"].as_array().into_iter().flatten() {
                    let _ = writeln!(out, "{}", left_place_line(p));
                }
            }
        }
        return out;
    }
    if let (Some(kit), Some(parts)) = (
        v.get("kit").filter(|k| k.is_object()),
        v.get("parts").and_then(Value::as_array),
    ) {
        kit_parts(&mut out, v, kit, parts);
        return out;
    }
    // `ev kit link` / `unlink`: the part that changed and the kit's counts.
    if let (Some(kit), Some(part)) = (
        v.get("kit").filter(|k| k.is_object()),
        v.get("part").filter(|p| p.is_object()),
    ) {
        kit_part_lines(&mut out, part);
        let c = &v["counts"];
        let _ = writeln!(
            out,
            "#{} {}  {}",
            kit["id"],
            s(kit, "name"),
            tf(
                "{} of {} found · {} lost · {} still missing",
                &[&c["found"], &c["expected"], &c["lost"], &c["open"]]
            )
        );
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
            if let Some(c) = p.get("purchase_candidates") {
                let _ = writeln!(out, "    {}", t("Could be one of these purchases:"));
                candidate_lines(&mut out, c);
            }
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
    if v.get("furniture").is_some() && v.get("split").is_some() {
        layout(&mut out, v);
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
        history(&mut out, events);
        return out;
    }
    // `ev edit`: the record, then each field it changed.
    if let (Some(node), Some(_)) = (v.get("node"), v.get("changed").and_then(Value::as_object)) {
        let _ = writeln!(out, "{}", line(node));
        changed_lines(&mut out, &v["changed"]);
        if let Some(c) = v.get("purchase_candidates") {
            let _ = writeln!(out, "  {}", t("Could be one of these purchases:"));
            candidate_lines(&mut out, c);
        }
        return out;
    }
    // `ev done` and `ev cancel` of several: each record where it is now.
    for (key, word) in [("done", "moved"), ("cancelled", "plan dropped")] {
        if let Some(list) = v.get(key).and_then(Value::as_array)
            && v.as_object().is_some_and(|o| o.len() == 1)
        {
            for n in list {
                let _ = writeln!(out, "#{} {}  ({})", n["id"], s(n, "path_text"), t(word));
            }
            return out;
        }
    }
    // `ev move` of several: each record and where it went, or is planned to go.
    for (key, arrow) in [("moved", "→"), ("planned", "⇢")] {
        if let Some(list) = v.get(key).and_then(Value::as_array) {
            let to = v["to"]["path_text"].as_str().unwrap_or_default();
            for n in list {
                let _ = writeln!(out, "#{} {}  {arrow} {to}", n["id"], s(n, "name"));
            }
            return out;
        }
    }
    for key in ["results", "created", "edited"] {
        if let Some(list) = v.get(key).and_then(Value::as_array) {
            if list.is_empty() {
                let _ = writeln!(out, "{}", t("(none)"));
            }
            // `ev find`: the portions of one thing kept in several places stand together, under
            // a line for the whole thing, where the first of them ranked.
            let mut shown = std::collections::HashSet::new();
            for n in list {
                if let Some(id) = n["thing"]["id"].as_i64() {
                    if !shown.insert(id) {
                        continue;
                    }
                    let th = &n["thing"];
                    let _ = writeln!(
                        out,
                        "{}",
                        tf(
                            "{} ×{} in {} places · in use {} · spare {}",
                            &[
                                &s(n, "name"),
                                &th["total"],
                                &th["places"],
                                &th["in_use"],
                                &th["spare"]
                            ]
                        )
                    );
                    for p in list.iter().filter(|p| p["thing"]["id"] == id) {
                        let _ = writeln!(out, "  {}", line(p));
                    }
                    continue;
                }
                let _ = writeln!(out, "{}", line(n));
                changed_lines(&mut out, &n["changed"]);
                // An `ev edit --stdin` line that set make or model, asked as a single edit is.
                if let Some(c) = n.get("purchase_candidates") {
                    let _ = writeln!(out, "  {}", t("Could be one of these purchases:"));
                    candidate_lines(&mut out, c);
                }
            }
            // `ev find --empty`: boxes with nothing recorded in them only because nobody looked.
            let unknown = v["not_known"].as_array().cloned().unwrap_or_default();
            if !unknown.is_empty() {
                let _ = writeln!(
                    out,
                    "\n{}",
                    t("Nothing recorded in these, but never counted: empty is not known")
                );
                for n in &unknown {
                    let _ = writeln!(out, "  {}", line(n));
                }
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
        legend(&mut out, v);
        return out;
    }
    // `ev photo cut --preview`: nothing attached, what each number would be.
    if v.get("preview").is_some() && v.get("legend").is_some() {
        legend(&mut out, v);
        let _ = writeln!(out, "{}", tf("Preview: {}", &[&s(v, "preview")]));
        return out;
    }
    // `ev photo mark`: each label and where it is, then the numbered copy.
    if v.get("marks").is_some() && v.get("marked").is_some() {
        for m in v["marks"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "{:<2} {}", s(m, "label"), s(m, "at"));
        }
        legend(&mut out, v);
        return out;
    }
    // `ev focus`: what a running `ev ui` was asked to show.
    if let Some(f) = v.get("focus")
        && v.as_object().is_some_and(|o| o.len() == 1)
    {
        let said = match (f["id"].as_i64(), f["photo"].as_i64(), f["files"].as_array()) {
            (Some(id), Some(p), _) => tf("Sent to ev ui: #{}, photo {}", &[&id, &p]),
            (Some(id), None, _) => tf("Sent to ev ui: #{}", &[&id]),
            (None, _, Some(files)) => tf(
                "Sent to ev ui: {} picture(s), {} in the series",
                &[
                    &files.len(),
                    &f["series"].as_u64().unwrap_or(files.len() as u64),
                ],
            ),
            _ => t("The request to ev ui is cleared.").to_string(),
        };
        let _ = writeln!(out, "{said}");
        // The pictures this request put in the series, by the name the person reads on screen.
        let fs: Vec<&str> = f["f"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !fs.is_empty() {
            let _ = writeln!(out, "  {}", fs.join(" "));
        }
        return out;
    }
    // `ev focus --list`: the series of marked photos on screen.
    if let Some(series) = v.get("series")
        && v.as_object().is_some_and(|o| o.len() == 1)
    {
        if series.is_null() {
            let _ = writeln!(out, "{}", t("No marked photo series in ev ui."));
            return out;
        }
        let _ = writeln!(out, "{}", tf("Next number: {}", &[&series["next"]]));
        for p in series["pictures"].as_array().into_iter().flatten() {
            let note = p["note"].as_str().unwrap_or("");
            let _ = writeln!(out, "{} · {note}  {}", s(p, "f"), s(p, "file"));
            for f in p["frames"].as_array().into_iter().flatten() {
                let what = match f["ref"].is_object() {
                    true => line(&f["ref"]),
                    false => s(f, "at"),
                };
                let _ = writeln!(out, "   {:<3} {what}", f["n"]);
            }
        }
        return out;
    }
    if let Some(list) = v.get("recoded").and_then(Value::as_array) {
        for r in list {
            let code = |k: &str| r[k].as_str().unwrap_or("—").to_string();
            let change = if r["before"] == r["after"] {
                format!("{}  {}", code("after"), t("(unchanged)"))
            } else {
                format!("{} → {}", code("before"), code("after"))
            };
            let _ = writeln!(out, "#{} {}  {change}", r["id"], s(r, "name"));
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
    // `ev past`: two lists, remembered first, each by year then each thing (spec/past-belongings.md).
    if let (Some(remembered), Some(recorded)) = (
        v.get("remembered").filter(|l| l.is_object()),
        v.get("left_inventory").filter(|l| l.is_object()),
    ) {
        let count = |l: &Value| l["past"].as_array().map_or(0, Vec::len);
        if count(remembered) + count(recorded) == 0 {
            let _ = writeln!(out, "{}", t("(nothing past)"));
            return out;
        }
        let mut first = true;
        for (l, head) in [
            (remembered, "Remembered ({})"),
            (recorded, "Left the inventory ({})"),
        ] {
            if count(l) == 0 {
                continue;
            }
            if !first {
                let _ = writeln!(out);
            }
            first = false;
            let _ = writeln!(out, "{}", tf(head, &[&count(l)]));
            past_list(&mut out, l);
        }
        return out;
    }
    // `ev past --year`: what was ours that year.
    if let (Some(year), Some(list)) = (v.get("year"), v.get("owned").and_then(Value::as_array)) {
        let _ = writeln!(out, "{}", tf("ours in {}: {}", &[year, &list.len()]));
        for n in list {
            let mut when = tf("came {}", &[&s(n, "came")]);
            if let Some(l) = n["left"].as_str() {
                when = format!("{when} · {}", tf("left {}", &[&l]));
            }
            let _ = writeln!(out, "  {}  ({when})", line(n));
        }
        let unknown = v["unknown"].as_u64().unwrap_or(0);
        if unknown > 0 {
            let _ = writeln!(
                out,
                "{}",
                tf(
                    "{} more are not counted: nothing says when they came, or when they left",
                    &[&unknown]
                )
            );
        }
        return out;
    }
    if let Some(groups) = v.get("disposals").and_then(Value::as_object) {
        for (d, list) in groups {
            if list.as_array().is_none_or(Vec::is_empty) {
                continue;
            }
            let _ = writeln!(out, "{}:", disposition(d));
            for n in list.as_array().into_iter().flatten() {
                let _ = writeln!(out, "  {}{}", line(n), shred_mark(n));
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
        // `ev photo rotate`: every record whose photo or crop turned with it.
        if let Some(r) = v.get("rotated") {
            let _ = writeln!(out, "{}", tf("turned {}° clockwise, on:", &[&r["degrees"]]));
            for n in r["records"].as_array().into_iter().flatten() {
                let _ = writeln!(out, "  {}", line(n));
            }
        }
        return out;
    }
    let _ = writeln!(out, "{v}");
    out
}

/// What an edit changed, a field a line: `note: old → new`, or `note: + added` when the new
/// value only adds to the old (a line appended to a note); `(nothing changed)` for none.
fn changed_lines(out: &mut String, changed: &Value) {
    let Some(fields) = changed.as_object() else {
        return;
    };
    if fields.is_empty() {
        let _ = writeln!(out, "  {}", t("(nothing changed)"));
    }
    // `field=+text` adds on a new line; a value that only grows on the same line (a theme
    // written out longer) is a replacement, shown whole.
    let added = |c: &Value| -> Option<String> {
        let (before, after) = (c["before"].as_str()?, c["after"].as_str()?);
        let rest = after
            .strip_prefix(before)
            .filter(|_| !before.is_empty())?
            .strip_prefix('\n')?;
        Some(rest.replace('\n', " · "))
    };
    let shown = |v: &Value| match v {
        Value::Null => "—".to_string(),
        // A note of several lines stays on its one line here.
        Value::String(s) => s.replace('\n', " · "),
        Value::Array(a) if a.is_empty() => "—".to_string(),
        Value::Array(a) => a
            .iter()
            .map(|x| x.as_str().map_or_else(|| x.to_string(), str::to_string))
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_string(),
    };
    for (key, c) in fields {
        let field = crate::history::field_word(key);
        // A kind is a word the reader has, not the stored one.
        let value = |v: &Value| match (key.as_str(), v.as_str()) {
            ("kind", Some(k)) => kind(k),
            _ => shown(v),
        };
        let _ = match added(c) {
            Some(rest) => writeln!(out, "  {field}: + {rest}"),
            None => writeln!(
                out,
                "  {field}: {} → {}",
                value(&c["before"]),
                value(&c["after"])
            ),
        };
    }
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

/// What else waits in a place while it is open: one line per job, named by kind.
fn while_there(out: &mut String, w: &Value) {
    let Some(w) = w.as_object().filter(|w| !w.is_empty()) else {
        return;
    };
    let _ = writeln!(out, "    {}", t("while there:"));
    for (key, list) in w {
        for n in list.as_array().into_iter().flatten() {
            let text = match key.as_str() {
                "photos" => format!(
                    "{}{}",
                    tf("photo: {}", &[&s(n, "path_text")]),
                    photo_reason(n)
                ),
                "labels" => tf("label: {}", &[&s(n, "code")]),
                "unclear" => tf("unclear: {}", &[&s(n, "name")]),
                "parked" => tf("waiting for its place: {}", &[&s(n, "name")]),
                "leaving" => tf(
                    "take along: {} → {}",
                    &[&s(&n["node"], "name"), &s(&n["to"], "path_text")],
                ),
                "disposals" => tf(
                    "leaving the home ({}): {}",
                    &[&disposition(&s(n, "as")), &s(n, "name")],
                ),
                "lost" => tf("lost, last seen here: {}", &[&s(&n["node"], "name")]),
                "coverage" => tf("ask about its warranty: {}", &[&s(n, "name")]),
                "values" => tf("ask its value: {}", &[&s(n, "name")]),
                _ => continue,
            };
            let _ = writeln!(out, "      · {text}");
        }
    }
}

/// `ev next`: the goal, how far the home is counted, the next task with its places, and the
/// raw places no task covers.
fn next(out: &mut String, v: &Value) {
    let _ = writeln!(out, "{}", tf("Goal: {}", &[&goal(v)]));
    let _ = writeln!(out, "{}", progress_line(&v["progress"]));
    if v["task"].is_object() {
        let _ = writeln!(out, "\n{}", tf("Next task ({} open):", &[&v["open_tasks"]]));
        task_line(out, &v["task"]);
        if v["task"]["picked"] == "due" {
            let _ = writeln!(out, "     {}", t("(first because it is due)"));
        }
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
            while_there(out, &p["while_there"]);
        }
    } else {
        let _ = writeln!(out, "\n{}", t("(no open task)"));
    }
    let hints = v["hints"].as_array().map_or(0, Vec::len);
    if hints > 0 {
        let _ = writeln!(out, "\n{}", t("On the order:"));
        for h in v["hints"].as_array().into_iter().flatten() {
            let text = match h["kind"].as_str() {
                Some("due") => due_text(&s(h, "due"), &h["days_left"]),
                Some("settles_moves") => tf("settles {} planned moves", &[&h["moves"]]),
                _ => continue,
            };
            let _ = writeln!(out, "  #{}  {text}", h["task"]);
        }
    }
    let near = v["left_nearby"].as_array().map_or(0, Vec::len);
    if near > 0 {
        let _ = writeln!(
            out,
            "\n{}",
            tf(
                "Not counted in the same furniture, outside this task ({}):",
                &[&near]
            )
        );
        for p in v["left_nearby"].as_array().into_iter().flatten() {
            let _ = writeln!(out, "{}", left_place_line(p));
        }
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
/// What accounts for a thing's units: bought, here, gone by how they left, and what is
/// unaccounted for or more than was bought. Nothing when it has neither purchases nor gone units.
fn accounted(th: &Value) -> Option<String> {
    let gone: Vec<String> = th["gone"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(d, n)| format!("{} {n}", crate::history::left_as(d)))
        .collect();
    let bought = th["bought"].as_i64();
    if bought.is_none() && gone.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if let Some(b) = bought {
        parts.push(tf("bought {}", &[&b]));
    }
    parts.push(tf("here {}", &[&th["total"]]));
    if !gone.is_empty() {
        parts.push(tf("gone: {}", &[&gone.join(", ")]));
    }
    match th["unaccounted"].as_i64() {
        Some(u) if u > 0 => parts.push(tf("{} unaccounted for", &[&u])),
        Some(u) if u < 0 => parts.push(tf("{} more than bought", &[&-u])),
        _ => {}
    }
    Some(tf("accounted: {}", &[&parts.join(" · ")]))
}

/// For what a thing kept in several places has on another of its portions: which one.
fn on(v: &Value) -> String {
    v["on"]
        .as_i64()
        .map(|id| tf("  (on #{})", &[&id]))
        .unwrap_or_default()
}

fn show(out: &mut String, v: &Value, node: &Value) {
    for w in v["warnings"].as_array().into_iter().flatten() {
        let _ = writeln!(
            out,
            "{}",
            tf("warning: {}", &[&w.as_str().unwrap_or_default()])
        );
    }
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
            // A note of several lines keeps each line under the first.
            let pad = " ".repeat(label.chars().count() + 4);
            let _ = writeln!(out, "  {label}: {}", x.replace('\n', &format!("\n{pad}")));
        }
    }
    if let Some(f) = node["fill"].as_i64() {
        let _ = writeln!(out, "  {}: {}", t("fill"), tf("{}%", &[&f]));
    }
    if let Some(tags) = node["tags"].as_array().filter(|t| !t.is_empty()) {
        let tags: Vec<_> = tags.iter().filter_map(Value::as_str).collect();
        let _ = writeln!(out, "  {}: {}", t("tags"), tags.join(", "));
    }
    // One thing kept in several places: all its units, then where the others are.
    let th = &v["thing"];
    if th.is_object() {
        let lost = th["lost"]
            .as_i64()
            .filter(|l| *l > 0)
            .map(|l| tf(" · lost {}", &[&l]))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  {}{lost}",
            tf(
                "thing: ×{} in {} places · in use {} · spare {}",
                &[&th["total"], &th["places"], &th["in_use"], &th["spare"]]
            )
        );
        for p in th["elsewhere"].as_array().into_iter().flatten() {
            let used = if p["in_use"] == true {
                t(" (in use)")
            } else {
                ""
            };
            let _ = writeln!(
                out,
                "    {}{used}",
                tf(
                    "elsewhere: #{} {} ×{}",
                    &[&p["id"], &s(p, "path_text"), &p["qty"]]
                )
            );
        }
        if let Some(line) = accounted(th) {
            let _ = writeln!(out, "    {line}");
        }
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
                &[
                    &review_mark(&v["review"]),
                    &crate::history::local_day(&s(&v["review"], "at"))
                ]
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
            "condition" => t("condition"),
            "shred" => t("shred first"),
            "photo_ok" => t("photo still current"),
            "photo_stale" => t("photo out of date"),
            other => other,
        };
        let what = [
            m["value"].as_str().map(|x| {
                match x {
                    "new" => t("new"),
                    "like-new" => t("like new"),
                    "used" => t("used"),
                    "needed" => t("to print"),
                    "printed" => t("printed"),
                    "listed" => t("listed"),
                    "reserved" => t("reserved"),
                    "yes" => t("yes"),
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
        if what.is_empty() {
            let _ = writeln!(out, "  {kind}");
        } else {
            let _ = writeln!(out, "  {kind}: {what}");
        }
    }
    for n in v["needs"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  {}: {}", t("to get"), need_line(n));
    }
    for p in v["purchases"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  {}: {}{}", t("bought"), purchase_line(p), on(p));
    }
    // Only on a fresh `ev add`: what to ask while the thing is in hand.
    if let Some(c) = v.get("purchase_candidates") {
        let _ = writeln!(out, "  {}", t("Could be one of these purchases:"));
        candidate_lines(out, c);
    }
    // The latest observation is the current value; older ones are history (`ev value X`).
    if let Some(vals) = v["valuations"].as_array().filter(|l| !l.is_empty()) {
        let earlier = if vals.len() > 1 {
            tf("  (+{} earlier)", &[&(vals.len() - 1)])
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "  {}: {}{earlier}",
            t("value"),
            valuation_line(&vals[0])
        );
    }
    for l in v["links"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  {}: {}{}", t("link"), link_line(l), on(l));
    }
    for d in v["documents"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  {}: {}{}", t("document"), doc_line(d), on(d));
    }
    for cv in v["coverages"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  {}: {}{}", t("coverage"), coverage_line(cv), on(cv));
    }
    if let Some(p) = v.get("coverage_proposal").filter(|p| p.is_object()) {
        let _ = writeln!(
            out,
            "  {}",
            tf(
                "proposed: statutory warranty until {} (2 years from delivery {}), not recorded",
                &[&s(p, "end"), &s(p, "start")]
            )
        );
    }
    for (subject, label) in [("value", t("value")), ("coverage", t("coverage"))] {
        let d = &v["tracking"][subject];
        if d.is_object() {
            let what = match d["decision"].as_str() {
                Some("later") => t("not now"),
                _ => t("not tracked"),
            };
            let inherited = if d["on"] == node["id"] {
                String::new()
            } else {
                tf("  (via #{})", &[&d["on"]])
            };
            let why = d["why"]
                .as_str()
                .map(|w| format!(" — {w}"))
                .unwrap_or_default();
            let _ = writeln!(out, "  {label}: {what}{why}{inherited}");
        }
    }
    for t_ in v["tasks"].as_array().into_iter().flatten() {
        let via = if t_["via"] == node["id"] {
            String::new()
        } else {
            tf("  (via #{})", &[&t_["via"]])
        };
        // `task #16` together: with the position between them, an agent read `#16` as a node.
        let task = tf("task #{}: {}", &[&t_["id"], &s(t_, "title")]);
        let order = t_["position"]
            .as_i64()
            .map(|p| format!(" {}", tf("(order {})", &[&p])))
            .unwrap_or_default();
        let _ = writeln!(out, "  {task}{order}{via}");
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
    // A box known to be empty, and when the person said so.
    match v["empty"]["from"].as_str() {
        Some("said") => {
            let _ = writeln!(out, "  {}: {}", t("empty"), empty_said(&v["empty"]));
        }
        Some(_) => {
            let _ = writeln!(out, "  {}", t("empty (counted)"));
        }
        None => {}
    }
    // When it came, and how a gone one left (spec/past-belongings.md).
    if let Some(c) = v["came"].as_str() {
        let _ = writeln!(out, "  {}: {c}", t("came"));
    }
    for f in v["traded_from"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  {}: {}", t("came in a trade for"), line(f));
    }
    if let Some(left) = departure_text(v) {
        let label = if v["departure"]["pending"] == true {
            t("sold")
        } else {
            t("left")
        };
        let _ = writeln!(out, "  {label}: {left}");
        if let Some(n) = v["departure"]["note"].as_str() {
            let _ = writeln!(out, "    {n}");
        }
    }
    // What its place waits for, and what waits for it.
    if v["waits_for"].is_object() {
        let _ = writeln!(out, "  {}: {}", t("waits for"), line(&v["waits_for"]));
    }
    for w in v["waited_for_by"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "  {}: {}", t("waited for by"), line(w));
    }
    // Found: the things that waited for it, to settle now.
    if let Some(list) = v["waiting"].as_array().filter(|l| !l.is_empty()) {
        let ids: Vec<String> = list.iter().map(|w| format!("#{}", w["id"])).collect();
        let _ = writeln!(
            out,
            "  {}",
            tf(
                "{} waited for this: where do they go now?",
                &[&ids.join(", ")]
            )
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
        "#{} {} ×{}  {}",
        kit["id"],
        s(kit, "name"),
        kit["copies"],
        tf(
            "{} of {} found · {} lost · {} still missing",
            &[&c["found"], &c["expected"], &c["lost"], &c["open"]]
        )
    );
    // The line the whole set was bought as.
    if let Some(p) = kit.get("purchase").filter(|p| p.is_object()) {
        let _ = writeln!(out, "  {}: {}", t("bought"), purchase_line(p));
    }
    for p in parts {
        kit_part_lines(out, p);
    }
}

/// One part of a kit with its count, and each record that is it by name and the holder it is
/// in, not its whole path: the list is read against the case in hand.
fn kit_part_lines(out: &mut String, p: &Value) {
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
        let path = n["path_text"].as_str().unwrap_or_default();
        let holder = path.rsplit(" › ").nth(1).unwrap_or(path);
        let qty = n["qty"]
            .as_i64()
            .map(|q| format!(" ×{q}"))
            .unwrap_or_default();
        let lost = if n["lost"] == true { t("  [lost]") } else { "" };
        let _ = writeln!(
            out,
            "       #{} {}{qty}  ← {holder}{lost}",
            n["id"],
            s(n, "name")
        );
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
    if let Some(list) = v["same_name"].as_array().filter(|l| !l.is_empty()) {
        let _ = writeln!(
            out,
            "{}",
            t("The same name in more than one place, one thing? `ev join`:")
        );
        for x in list {
            let ids: Vec<String> = x["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|n| format!("#{}", n["id"]))
                .collect();
            let _ = writeln!(out, "  {}  {}", s(x, "name"), ids.join(" "));
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

/// `ev photo cut`: what each number on the numbered photo is, then where that photo and the
/// contact sheet are, and whether `ev ui` was asked to show it.
fn legend(out: &mut String, v: &Value) {
    for e in v["legend"].as_array().into_iter().flatten() {
        let _ = writeln!(out, "{:<2} {}", e["n"], line(&e["ref"]));
    }
    if let Some(p) = v["marked"].as_str() {
        let _ = writeln!(out, "{}", tf("Numbered photo: {}", &[&p]));
    }
    if let Some(p) = v["sheet"].as_str() {
        let _ = writeln!(out, "{}", tf("Contact sheet: {}", &[&p]));
    }
    if v["shown"].is_object() {
        let _ = writeln!(out, "{}", t("Sent to ev ui."));
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
    if let Some(w) = v["series_tile"].as_u64() {
        let _ = writeln!(out, "{}", tf("Series grid: pictures {} cells wide", &[&w]));
    }
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

/// Amounts per currency (`{"TRY": "1999.00"}`) in the reader's way, joined; `—` when none.
fn amounts(per: &Value) -> String {
    let parts: Vec<String> = per
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(c, a)| a.as_str().map(|a| amount(a, c)))
        .collect();
    if parts.is_empty() {
        "\u{2014}".to_string()
    } else {
        parts.join(", ")
    }
}

/// `ev stats` as headed sections of lines, each line with the record it names, if any: the
/// text output and the Statistics tab of `ev ui` both read it.
pub(crate) type StatSection = (String, Vec<(Option<i64>, String)>);

pub(crate) fn stats_sections(v: &Value) -> Vec<StatSection> {
    let mut out: Vec<StatSection> = Vec::new();
    let line = |text: String| (None, text);
    let node = |n: &Value, text: String| (n["id"].as_i64(), text);

    let o = &v["overview"];
    out.push((
        t("OVERVIEW").to_string(),
        vec![
            line(tf(
                "{} records of things, {} units",
                &[&o["records"], &o["units"]],
            )),
            line(tf(
                "{} rooms · {} pieces of furniture · {} containers",
                &[&o["rooms"], &o["furniture"], &o["containers"]],
            )),
            line(tf(
                "{} things kept in several places",
                &[&o["in_several_places"]],
            )),
            line(tf(
                "{} records with a photo · {} documents",
                &[&o["with_photo"], &o["documents"]],
            )),
        ],
    ));

    let m = &v["value"];
    let mut value = vec![
        line(tf(
            "What the things here cost, by their linked purchases: {}",
            &[&amounts(&m["cost"])],
        )),
        line(tf(
            "known for {} of {} records",
            &[&m["things_with_cost"], &m["things"]],
        )),
    ];
    if let Some(today) = m["today"].as_object() {
        value.push(line(tf(
            "in today's money: {} ({} of {} lines)",
            &[
                &amount(
                    today["amount"].as_str().unwrap_or_default(),
                    today["currency"].as_str().unwrap_or_default(),
                ),
                &today["lines"],
                &today["of"],
            ],
        )));
    }
    for d in m["dearest"].as_array().into_iter().flatten() {
        let today = d["today"]
            .as_object()
            .map(|t| {
                tf(
                    " (today {})",
                    &[&amount(
                        t["amount"].as_str().unwrap_or_default(),
                        t["currency"].as_str().unwrap_or_default(),
                    )],
                )
            })
            .unwrap_or_default();
        value.push(node(
            d,
            format!("  {}{today}  {}", amounts(&d["cost"]), s(d, "path_text")),
        ));
    }
    if m["valued"]["things"].as_i64().unwrap_or(0) > 0 {
        value.push(line(tf(
            "latest values recorded: {} things, {}",
            &[&m["valued"]["things"], &amounts(&m["valued"]["latest"])],
        )));
    }
    out.push((t("WHAT IT COST").to_string(), value));

    // Rooms with something recorded in them, then how many have nothing yet.
    let all_rooms = v["rooms"].as_array().cloned().unwrap_or_default();
    let (some, none): (Vec<&Value>, Vec<&Value>) = all_rooms
        .iter()
        .partition(|r| r["records"].as_i64().unwrap_or(0) + r["holders"].as_i64().unwrap_or(0) > 0);
    let mut rooms: Vec<(Option<i64>, String)> = some
        .iter()
        .map(|r| {
            node(
                r,
                tf(
                    "{}: {} records, {} units, {} holders · {}",
                    &[
                        &s(r, "name"),
                        &r["records"],
                        &r["units"],
                        &r["holders"],
                        &amounts(&r["cost"]),
                    ],
                ),
            )
        })
        .collect();
    if !none.is_empty() {
        rooms.push(line(tf(
            "{} rooms with nothing recorded yet",
            &[&none.len()],
        )));
    }
    out.push((t("ROOMS").to_string(), rooms));

    let c = &v["tour"];
    let share = |a: &Value, b: &Value| {
        let (a, b) = (a.as_i64().unwrap_or(0), b.as_i64().unwrap_or(0));
        if b > 0 { a * 100 / b } else { 0 }
    };
    out.push((
        t("COUNTING").to_string(),
        vec![
            line(tf(
                "{} of {} places counted · {} being counted · {} not counted",
                &[&c["toured"], &c["places"], &c["counting"], &c["raw"]],
            )),
            line(tf(
                "{} changed since they were counted",
                &[&c["changed_since"]],
            )),
            line(tf(
                "{} of {} records are in counted places ({}%)",
                &[
                    &c["things_in_counted_places"],
                    &c["things"],
                    &share(&c["things_in_counted_places"], &c["things"]),
                ],
            )),
        ],
    ));

    let p = &v["purchases"];
    let mut buys = vec![line(tf(
        "{} lines · {} linked · {} settled · {} durable still open",
        &[
            &p["lines"],
            &p["linked"],
            &p["dismissed"],
            &p["open_durable"],
        ],
    ))];
    for y in p["years"].as_array().into_iter().flatten() {
        buys.push(line(tf(
            "  {}: {} lines · {}",
            &[
                &y["year"].as_str().unwrap_or(t("no date")),
                &y["lines"],
                &amounts(&y["paid"]),
            ],
        )));
    }
    // What the money went to: things, clothes, and what is never a thing.
    let buckets: Vec<String> = p["buckets"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|b| {
            let name = match b["bucket"].as_str() {
                Some("durable") => t("things"),
                Some("clothing") => t("clothing"),
                Some("digital") => t("digital"),
                Some("service") => t("services"),
                _ => "?",
            };
            format!("{name} {} · {}", b["lines"], amounts(&b["paid"]))
        })
        .collect();
    if !buckets.is_empty() {
        buys.push(line(format!("  {}", buckets.join("; "))));
    }
    for sh in p["shops"].as_array().into_iter().flatten() {
        buys.push(line(tf(
            "  {}: {} lines · {}",
            &[&s(sh, "shop"), &sh["lines"], &amounts(&sh["paid"])],
        )));
    }
    out.push((t("PURCHASES").to_string(), buys));

    let a = &v["activity"];
    let mut recent = vec![line(tf(
        "{} added · {} moves · {} photos",
        &[&a["added"], &a["moved"], &a["photos"]],
    ))];
    let gone: Vec<String> = a["gone"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(how, n)| format!("{} {n}", crate::history::left_as(how)))
        .collect();
    if !gone.is_empty() {
        recent.push(line(tf("left the home: {}", &[&gone.join(" · ")])));
    }
    if let Some(d) = a["busiest_day"].as_object() {
        recent.push(line(tf(
            "busiest day: {} ({} events)",
            &[&d["day"].as_str().unwrap_or_default(), &d["events"]],
        )));
    }
    out.push((t("LAST 30 DAYS").to_string(), recent));

    let h = &v["holders"];
    let mut boxes = vec![
        line(tf(
            "{} containers · {} known to be empty · {} with nothing recorded, never counted",
            &[&h["containers"], &h["empty"], &h["empty_not_known"]],
        )),
        line(tf(
            "{} with a fill, {}% on average · {} full",
            &[
                &h["with_fill"],
                &h["average_fill"].as_i64().unwrap_or(0),
                &h["full"],
            ],
        )),
    ];
    for b in h["most_records"].as_array().into_iter().flatten() {
        boxes.push(node(
            b,
            tf("  {} records  {}", &[&b["records"], &s(b, "path_text")]),
        ));
    }
    out.push((t("BOXES").to_string(), boxes));

    let cv = &v["coverage"];
    out.push((
        t("COVERAGE").to_string(),
        vec![line(tf(
            "{} active of {} · {} ending soon",
            &[&cv["active"], &cv["coverages"], &cv["ending"]],
        ))],
    ));

    out.push((
        t("TAGS").to_string(),
        v["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|g| line(format!("{} ({})", s(g, "tag"), g["records"])))
            .collect(),
    ));

    out.push((
        t("BOUGHT LONGEST AGO").to_string(),
        v["oldest"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|n| node(n, format!("{}  {}", s(n, "bought"), s(n, "path_text"))))
            .collect(),
    ));

    // Past belongings, apart from today's numbers (spec/past-belongings.md).
    let p = &v["past"];
    let mut past = Vec::new();
    if p["records"].as_u64().unwrap_or(0) > 0 {
        let how: Vec<String> = p["how"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(d, n)| format!("{n} {}", crate::history::left_as(d)))
            .collect();
        past.push(line(tf(
            "{} things that were ours: {}",
            &[&p["records"], &how.join(", ")],
        )));
        for (key, label) in [("paid", "paid for them: {}"), ("got", "got for them: {}")] {
            if p[key].as_object().is_some_and(|m| !m.is_empty()) {
                past.push(line(tf(label, &[&amounts(&p[key])])));
            }
        }
    }
    out.push((t("PAST").to_string(), past));
    out.retain(|(_, lines)| !lines.is_empty());
    out
}

fn stats_text(out: &mut String, v: &Value) {
    for (i, (heading, lines)) in stats_sections(v).into_iter().enumerate() {
        if i > 0 {
            let _ = writeln!(out);
        }
        let _ = writeln!(out, "{heading}");
        for (_, l) in lines {
            let _ = writeln!(out, "  {l}");
        }
    }
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
        assert!(out.contains("task #1: Sort it (order 2)"), "{out}");
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

    #[test]
    fn the_node_payload_keeps_only_the_sections_with_something_in_them() {
        let v = json!({
            "node": {"id": 5, "note": null},
            "children": [],
            "kits": [],
            "marks": {},
            "tracking": {"value": null, "coverage": null},
            "pending": null,
            "tasks": [{"id": 1}],
        });
        assert_eq!(
            super::for_program(&v),
            json!({"node": {"id": 5}, "tasks": [{"id": 1}]})
        );
    }

    #[test]
    fn a_field_with_no_value_is_left_out_below_the_top_level() {
        let v = json!({
            "goal": null,
            "results": [{"id": 1, "code": null, "lost": false, "tags": [], "grid": {"face": null}}],
        });
        assert_eq!(
            super::for_program(&v),
            json!({
                "goal": null,
                "results": [{"id": 1, "lost": false, "tags": [], "grid": {}}],
            })
        );
    }
}
