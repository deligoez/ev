//! Past belongings (spec/past-belongings.md): when a thing came, and how a gone one left: when,
//! where it was then, and what a sale brought.

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::error::{Error, Result};

/// A date as a person remembers it: a year (`2016`), a month (`2016-06`) or a day
/// (`2016-06-14`). Never widened to a day nobody said.
pub(crate) fn partial_date(text: &str) -> Result<String> {
    let t = text.trim();
    let bad = || {
        Error::Usage(format!(
            "`{t}` is no date: give a year (2016), a month (2016-06) or a day (2016-06-14)"
        ))
    };
    let parts: Vec<&str> = t.split('-').collect();
    let num = |s: &str, len: usize| -> Option<u32> {
        (s.len() == len && s.chars().all(|c| c.is_ascii_digit()))
            .then(|| s.parse().ok())
            .flatten()
    };
    let year = parts
        .first()
        .and_then(|y| num(y, 4))
        .filter(|y| (1900..=2100).contains(y))
        .ok_or_else(bad)?;
    match parts.as_slice() {
        [_] => {}
        [_, m] => {
            num(m, 2).filter(|m| (1..=12).contains(m)).ok_or_else(bad)?;
        }
        [_, m, d] => {
            let m = num(m, 2).ok_or_else(bad)?;
            let d = num(d, 2).ok_or_else(bad)?;
            chrono::NaiveDate::from_ymd_opt(year as i32, m, d).ok_or_else(bad)?;
        }
        _ => return Err(bad()),
    }
    Ok(t.to_string())
}

/// Records when and where a gone record left, keeping what a sale already said.
pub(super) fn set_departure(
    conn: &Connection,
    node: i64,
    at: Option<&str>,
    place: Option<&str>,
) -> Result<()> {
    if at.is_none() && place.is_none() {
        return Ok(());
    }
    let at = at.map(partial_date).transpose()?;
    let place = place
        .map(|p| super::places::place_or_create(conn, p))
        .transpose()?;
    conn.execute(
        "INSERT INTO departures (node_id, at, place_id) VALUES (?1, ?2, ?3)
         ON CONFLICT(node_id) DO UPDATE SET at = COALESCE(excluded.at, at),
           place_id = COALESCE(excluded.place_id, place_id)",
        params![node, at, place],
    )?;
    Ok(())
}

/// How a gone record left, as `ev show` gives it: `at` (said, else the day it was recorded
/// gone), `where`, and a sale's `price`, `currency`, `via` and `note`. Null for a record that
/// has not left.
pub(crate) fn departure_json(conn: &Connection, node: i64) -> Result<Value> {
    let state: String = conn.query_row("SELECT state FROM nodes WHERE id = ?1", [node], |r| {
        r.get(0)
    })?;
    if state != "gone" {
        return Ok(Value::Null);
    }
    type Row = (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let row: Option<Row> = conn
        .query_row(
            "SELECT d.at, p.name, d.price, d.currency, d.via, d.note
               FROM departures d LEFT JOIN places p ON p.id = d.place_id
              WHERE d.node_id = ?1",
            [node],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()?;
    let (at, place, price, currency, via, note) = row.unwrap_or_default();
    // Not said: the day it was recorded gone.
    let at = match at {
        Some(a) => a,
        None => conn
            .query_row(
                "SELECT substr(MAX(at), 1, 10) FROM events WHERE node_id = ?1 AND type = 'gone'",
                [node],
                |r| r.get::<_, Option<String>>(0),
            )?
            .unwrap_or_default(),
    };
    Ok(json!({
        "at": at, "where": place, "price": price, "currency": currency, "via": via, "note": note,
    }))
}
