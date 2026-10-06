//! A record's history in words, shared by the History tab of `ev ui` and `ev history`'s text,
//! so the two read the same.

use serde_json::Value;

use crate::i18n::{t, tf};

/// What an event's line is about, for its colour on screen.
pub(crate) enum Tone {
    /// Something came in or was added here.
    Came,
    /// Something is planned to come here.
    Planned,
    /// Something went out of here.
    Went,
    /// The record itself changed.
    Own,
    /// An event with no words yet, shown raw.
    Other,
}

fn str_of(n: &Value, key: &str) -> String {
    n[key].as_str().unwrap_or_default().to_string()
}

/// When an event happened, in local time.
/// A stored moment (`2026-10-05T22:47:35Z`) as the local day it fell on, for text; anything
/// else as it is.
pub(crate) fn local_day(ts: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(ts)
        .map(|d| d.with_timezone(&chrono::Local).date_naive().to_string())
        .unwrap_or_else(|_| ts.to_string())
}

pub(crate) fn local_time(e: &Value) -> Option<chrono::DateTime<chrono::Local>> {
    chrono::DateTime::parse_from_rfc3339(e["at"].as_str().unwrap_or_default())
        .map(|d| d.with_timezone(&chrono::Local))
        .ok()
}

/// The heading over a day's events: today, yesterday, else the date.
pub(crate) fn day_heading(day: Option<chrono::NaiveDate>) -> String {
    let today = chrono::Local::now().date_naive();
    match day {
        Some(d) if d == today => t("Today").to_string(),
        Some(d) if Some(d) == today.pred_opt() => t("Yesterday").to_string(),
        Some(d) => d.format("%Y-%m-%d").to_string(),
        None => "?".into(),
    }
}

pub(crate) fn disposition_tr(d: &str) -> &'static str {
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
        _ => "?",
    }
}
/// How a gone record left, in the past tense: what happened, not what was planned ("sold", not
/// "sell").
pub(crate) fn left_as(d: &str) -> &'static str {
    match d {
        "trash" => t("thrown out"),
        "give" => t("given away"),
        "sell" => t("sold"),
        "return" => t("returned to the shop"),
        "mistake" => t("record error"),
        "digitize" => t("photographed, then thrown out"),
        "merged" => t("joined another portion"),
        "used" => t("used up"),
        "left" => t("left behind"),
        "stolen" => t("stolen"),
        "unknown" => t("how not known"),
        "trade" => t("traded"),
        _ => "?",
    }
}

/// An edit event in words: each field with what it became, and what it was when both are short
/// enough to read side by side.
/// A field of `ev edit` in the reader's words.
pub(crate) fn field_word(k: &str) -> String {
    match k {
        "name" => t("name").into(),
        "code" => t("code").into(),
        "kind" => t("kind").into(),
        "note" => t("note").into(),
        "theme" => t("theme").into(),
        "qty" => t("qty").into(),
        "size" => t("size").into(),
        "fill" => t("fill").into(),
        "tags" => t("tags").into(),
        "owner" => t("owner").into(),
        "with" => t("with").into(),
        "to" => t("to take to").into(),
        "address" => t("address").into(),
        "photos" => t("photos").into(),
        "make" => t("make").into(),
        "model" => t("model").into(),
        "serial" => t("serial").into(),
        "came" => t("came").into(),
        "left" => t("left").into(),
        "left_in" => t("left from").into(),
        "temporary" => t("temporary").into(),
        "waits_for" => t("waits for").into(),
        other => other.into(),
    }
}

pub(crate) fn edit_text(d: &Value) -> String {
    let name = field_word;
    // A note of several lines reads on one: an event is one row.
    let text = |v: &Value| match v {
        Value::Null => "—".to_string(),
        Value::String(s) => s.split('\n').collect::<Vec<_>>().join(" / "),
        Value::Array(a) if a.is_empty() => "—".to_string(),
        Value::Array(a) => a
            .iter()
            .map(|x| x.as_str().map_or_else(|| x.to_string(), str::to_string))
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_string(),
    };
    let Some(fields) = d.as_object() else {
        return d.to_string();
    };
    fields
        .iter()
        .map(|(k, c)| {
            // A kind is a word the reader has, not the stored one.
            let text = |v: &Value| match (k.as_str(), v.as_str()) {
                ("kind", Some(x)) => crate::render::kind(x),
                _ => text(v),
            };
            let (before, after) = (text(&c["before"]), text(&c["after"]));
            if c["after"].is_null() {
                format!("{}: {}", name(k), t("cleared"))
            } else if before.chars().count() + after.chars().count() <= 40 {
                format!("{}: {before} → {after}", name(k))
            } else {
                format!("{}: {after}", name(k))
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// One event in words: its verb, what it was about, and its tone. `place` names a place the
/// event points at (an id, or a name already given).
pub(crate) fn event_words(
    e: &Value,
    place: &dyn Fn(&Value) -> String,
) -> (&'static str, String, Tone) {
    let d = &e["data"];
    if e["item"].is_object() {
        let name = str_of(&e["item"], "name");
        return match (e["relation"].as_str(), e["type"].as_str()) {
            (Some("added"), _) => (t("added here"), name, Tone::Came),
            (Some("in"), Some("plan")) => (t("planned to come"), name, Tone::Planned),
            (Some("in"), _) => (
                t("came in"),
                format!("{name}  ← {}", place(&d["from"])),
                Tone::Came,
            ),
            _ => (
                t("went out"),
                format!("{name}  → {}", place(&d["to"])),
                Tone::Went,
            ),
        };
    }
    let own = |v: &'static str, s: String| (t(v), s, Tone::Own);
    let kit = |v: &'static str| {
        own(
            v,
            format!(
                "{} · {}. {}",
                str_of(d, "kit"),
                d["part"],
                str_of(d, "text")
            ),
        )
    };
    let document = |v: &'static str| {
        own(
            v,
            format!(
                "#{} {}",
                d["document"],
                crate::render::doc_kind(&str_of(d, "kind"))
            ),
        )
    };
    let coverage = |v: &'static str| {
        own(
            v,
            format!(
                "#{} {}",
                d["coverage"],
                crate::render::coverage_kind(&str_of(d, "kind"))
            ),
        )
    };
    match e["type"].as_str().unwrap_or_default() {
        "create" => own("created", place(&d["parent"])),
        "move" => own(
            "moved",
            format!("{} → {}", place(&d["from"]), place(&d["to"])),
        ),
        "done" => own(
            "moved as planned",
            format!("{} → {}", place(&d["from"]), place(&d["to"])),
        ),
        "plan" => own("move planned", format!("→ {}", place(&d["to"]))),
        "cancel" => own("plan cancelled", String::new()),
        "edit" => own("changed", edit_text(d)),
        "photo" => own(
            "photo added",
            if d["crop"].is_string() {
                t("(a crop)").to_string()
            } else {
                String::new()
            },
        ),
        "split" => own(
            "split into",
            d["into"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|p| format!("#{} {}", p["id"], str_of(p, "name")))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default(),
        ),
        "split_from" => own(
            "split from",
            format!("#{} {}", d["from"], str_of(d, "name")),
        ),
        "kit_link" => kit("linked to kit"),
        "kit_unlink" => kit("unlinked from kit"),
        "photo_remove" => own(
            "photo removed",
            [str_of(d, "note"), str_of(d, "crop")]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("  "),
        ),
        "observe" => own("observed", str_of(d, "text")),
        "unobserve" => own("observation removed", str_of(d, "text")),
        "review" => own("reviewed", {
            let status = crate::render::count_label(d["as"].as_str().unwrap_or("raw"));
            format!("{status}  {}", str_of(d, "note"))
        }),
        "dispose" => own(
            "set aside",
            disposition_tr(d["as"].as_str().unwrap_or_default()).to_string(),
        ),
        "gone" => own("gone", {
            // How, and for a thing that left long ago, when and from where, as it was said.
            let mut parts = vec![left_as(d["as"].as_str().unwrap_or_default()).to_string()];
            if let Some(at) = d["at"].as_str() {
                parts.push(at.to_string());
            }
            if let Some(w) = d["where"].as_str() {
                parts.push(w.to_string());
            }
            if let Some(why) = d["why"].as_str() {
                parts.push(why.to_string());
            }
            parts.join(" · ")
        }),
        "coverage_purchase" => own(
            "coverage bought as",
            match d["purchase"].as_i64() {
                Some(p) => format!("#{} → {} #{p}", d["coverage"], t("line")),
                None => format!("#{}  {}", d["coverage"], t("taken back")),
            },
        ),
        "kit_purchase" => own(
            "kit bought as",
            match d["purchase"].as_i64() {
                Some(p) => format!("{} → {} #{p}", str_of(d, "kit"), t("line")),
                None => format!("{}  {}", str_of(d, "kit"), t("taken back")),
            },
        ),
        "traded" => own(
            "traded",
            d["for"]
                .as_i64()
                .map(|i| tf("for #{}", &[&i]))
                .unwrap_or_default(),
        ),
        // What it brought, when (a date said moves the leaving) and through what.
        "sold" => own(
            "sold",
            [
                crate::render::amount(&str_of(d, "price"), &str_of(d, "currency")),
                str_of(d, "at"),
                str_of(d, "via"),
            ]
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(" · "),
        ),
        "restore" => own("restored", str_of(d, "correction")),
        "cell" => own(
            "cells",
            format!("{} → {}", place(&d["before"]), place(&d["after"])),
        ),
        "grid" => own("grid set", format!("{}×{}", d["after"][0], d["after"][1])),
        "decline" => own("move declined", str_of(d, "why")),
        "decline_cleared" => own("decline taken back", String::new()),
        "grid_face" => own(
            "grid seen from",
            t(if d["after"] == "front" {
                "the front"
            } else {
                "above"
            })
            .to_string(),
        ),
        "sketch" => {
            let a = &d["after"];
            let mut parts = Vec::new();
            if !a["x"].is_null() {
                parts.push(tf(
                    "at {},{} cm",
                    &[&crate::render::cm(&a["x"]), &crate::render::cm(&a["y"])],
                ));
            }
            if !a["w"].is_null() {
                parts.push(tf(
                    "{}×{} cm",
                    &[&crate::render::cm(&a["w"]), &crate::render::cm(&a["d"])],
                ));
            }
            if !a["on"].is_null() {
                parts.push(tf("on #{}", &[&a["on"]]));
            }
            if a.is_null() {
                own("sketch removed", String::new())
            } else {
                own("sketched", parts.join(" · "))
            }
        }
        "lost" => own("lost", String::new()),
        "found" => own("found", place(&d["at"])),
        "back" => own("returned", place(&d["from"])),
        "lend" => own("lent", place(&d["to"])),
        "sketch_import" => own(
            "plan imported",
            tf(
                "{} · {} rooms",
                &[&str_of(d, "file"), &d["rooms"].as_i64().unwrap_or(0)],
            ),
        ),
        "broken" => own("broken", str_of(d, "note")),
        "fixed" => own("fixed", String::new()),
        "purchase_linked" => own(
            "linked to purchase",
            tf("#{} ×{}", &[&d["purchase"], &d["qty"]]),
        ),
        "purchase_declined" => own(
            "not this purchase",
            format!("#{}  {}", d["purchase"], str_of(d, "why"))
                .trim_end()
                .to_string(),
        ),
        "purchase_decline_cleared" => own("purchase offered again", format!("#{}", d["purchase"])),
        "purchase_unlinked" => own("unlinked from purchase", format!("#{}", d["purchase"])),
        "doc_linked" => document("document added"),
        "doc_unlinked" => document("document removed"),
        "coverage_added" => coverage("coverage added"),
        "coverage_removed" => coverage("coverage removed"),
        "track" => own("decided", {
            let subject = if d["subject"] == "value" {
                t("value")
            } else {
                t("coverage")
            };
            let decision = match d["decision"].as_str() {
                Some("later") => t("not now"),
                Some("yes") => t("tracked again"),
                _ => t("not tracked"),
            };
            let why = str_of(d, "why");
            format!("{subject}: {decision}  {why}")
                .trim_end()
                .to_string()
        }),
        "photo_rotate" => own(
            "photo turned",
            tf("{}° clockwise", &[&d["degrees"].as_i64().unwrap_or(0)]),
        ),
        "portion_out" => own("part moved out", format!("×{}  → #{}", d["qty"], d["to"])),
        "portion_in" => own("part came in", format!("×{}  ← #{}", d["qty"], d["from"])),
        "merged" => own(
            "joined another portion",
            format!("×{}  → #{}", d["qty"], d["into"]),
        ),
        "joined" => own("portion joined", format!("×{}  ← #{}", d["qty"], d["from"])),
        "more_of" => own("more of", format!("#{}", d["of"])),
        "join" => own("joined to a thing", format!("#{}", d["thing"])),
        "unjoin" => own("taken from a thing", format!("#{}", d["thing"])),
        "empty" => own("found empty", str_of(d, "note")),
        other => (t("event"), format!("{other} {d}"), Tone::Other),
    }
}
