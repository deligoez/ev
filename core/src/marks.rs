//! Things to do that hang on a node, a list of things to get, and `ev todo`, the one list of
//! everything waiting (spec §18).
//!
//! Most of what is "to do" in a home is already state on a node: a planned move, a thing meant
//! for another household, a disposal, a lost thing. Those stay where they are and close with
//! their own verbs (`done`, `gone`, `back`, `found`); `todo` only gathers them, so nothing is
//! kept twice. This module adds the few kinds that had no state yet — a label to print, broken,
//! a use-by date, a sale in progress — and needs, which are not in the tree at all.

use chrono::{Datelike, NaiveDate};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::error::refused;
use crate::model::{Disposition, Node, State};
use crate::store::{Inventory, brief, ids, live_nodes, now, place_errands, resolve, show};
use crate::{Error, Result};

/// How close a use-by date has to be before it shows in `todo`.
const EXPIRY_WINDOW_DAYS: i64 = 60;

/// Words that mark a record as a guess to clear up with the person.
const UNSURE: [&str; 3] = ["belirsiz", "muhtemelen", "?"];

fn brief_value(conn: &Connection, id: i64) -> Result<Value> {
    serde_json::to_value(brief(conn, id)?).map_err(|e| Error::Internal(e.to_string()))
}

pub(crate) fn mark(conn: &Connection, id: i64, kind: &str) -> Result<Value> {
    Ok(conn
        .query_row(
            "SELECT value, amount, note, at FROM marks WHERE node_id = ?1 AND kind = ?2",
            params![id, kind],
            |r| {
                Ok(json!({
                    "value": r.get::<_, Option<String>>(0)?,
                    "amount": r.get::<_, Option<i64>>(1)?,
                    "note": r.get::<_, Option<String>>(2)?,
                    "at": r.get::<_, String>(3)?,
                }))
            },
        )
        .optional()?
        .unwrap_or(Value::Null))
}

pub(crate) fn marks_of(conn: &Connection, id: i64) -> Result<Value> {
    let mut out = serde_json::Map::new();
    for kind in ["label", "broken", "expires", "sale"] {
        let m = mark(conn, id, kind)?;
        if !m.is_null() {
            out.insert(kind.into(), m);
        }
    }
    Ok(Value::Object(out))
}

fn set_mark(
    conn: &Connection,
    id: i64,
    kind: &str,
    value: Option<&str>,
    amount: Option<i64>,
    note: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO marks (node_id, kind, value, amount, note, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(node_id, kind) DO UPDATE SET value = excluded.value,
           amount = excluded.amount, note = excluded.note, at = excluded.at",
        params![
            id,
            kind,
            value,
            amount,
            note.map(str::trim).filter(|n| !n.is_empty()),
            now()
        ],
    )?;
    Ok(())
}

fn clear_mark(conn: &Connection, id: i64, kind: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM marks WHERE node_id = ?1 AND kind = ?2",
        params![id, kind],
    )?;
    Ok(())
}

/// A new or changed code needs a new label; a removed code needs none.
pub(crate) fn code_changed(conn: &Connection, id: i64, has_code: bool) -> Result<()> {
    if has_code {
        set_mark(conn, id, "label", Some("needed"), None, None)
    } else {
        clear_mark(conn, id, "label")
    }
}

/// A use-by date: `YYYY-MM-DD`, or `YYYY-MM` meaning the end of that month.
fn parse_date(s: &str) -> Result<NaiveDate> {
    let s = s.trim();
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(d);
    }
    let bad = || Error::Usage(format!("`{s}` is not YYYY-MM-DD or YYYY-MM"));
    let first = NaiveDate::parse_from_str(&format!("{s}-01"), "%Y-%m-%d").map_err(|_| bad())?;
    let next = if first.month() == 12 {
        NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
    };
    next.and_then(|d| d.pred_opt()).ok_or_else(bad)
}

fn today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}

fn need_json(conn: &Connection, id: i64) -> Result<Value> {
    let row = conn
        .query_row(
            "SELECT text, qty, make, for_node, status, note, created_at, closed_at FROM needs WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    json!({
                        "id": id,
                        "text": r.get::<_, String>(0)?,
                        "qty": r.get::<_, Option<i64>>(1)?,
                        "make": r.get::<_, bool>(2)?,
                        "status": r.get::<_, String>(4)?,
                        "note": r.get::<_, Option<String>>(5)?,
                        "created_at": r.get::<_, String>(6)?,
                        "closed_at": r.get::<_, Option<String>>(7)?,
                    }),
                    r.get::<_, Option<i64>>(3)?,
                ))
            },
        )
        .optional()?;
    let (mut v, for_node) = row.ok_or_else(|| Error::NotFound(format!("no need with id {id}")))?;
    v["for"] = json!(for_node.map(|n| brief_value(conn, n)).transpose()?);
    Ok(v)
}

/// Open needs meant for a node.
pub(crate) fn needs_for(conn: &Connection, id: i64) -> Result<Vec<Value>> {
    ids(
        conn,
        "SELECT id FROM needs WHERE for_node = ?1 AND status = 'open' ORDER BY id",
        [id],
    )?
    .into_iter()
    .map(|n| need_json(conn, n))
    .collect()
}

