//! Past belongings (spec/past-belongings.md): when a thing came, and how a gone one left: when,
//! where it was then, and what a sale brought.

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use super::{Inventory, event, resolve, show};
use crate::error::{Error, Result, refused};

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

/// Where a sale in progress was listed, carried into the departure of a thing gone `--as sell`
/// as what it went through. The price it asked is not carried: an asking price is not what the
/// sale brought, and the sale mark keeps it in sight until `ev sold` says.
pub(super) fn carry_sale(conn: &Connection, node: i64) -> Result<()> {
    let listed: Option<Option<String>> = conn
        .query_row(
            "SELECT note FROM marks WHERE node_id = ?1 AND kind = 'sale'",
            [node],
            |r| r.get(0),
        )
        .optional()?;
    let Some(Some(place)) = listed else {
        return Ok(());
    };
    conn.execute(
        "INSERT INTO departures (node_id, via) VALUES (?1, ?2)
         ON CONFLICT(node_id) DO UPDATE SET via = COALESCE(via, excluded.via)",
        params![node, place],
    )?;
    Ok(())
}

impl Inventory {
    /// What a sale brought (spec/past-belongings.md), on a thing gone, or set aside, as `sell`:
    /// the price and its currency (the home currency when not given), when (a partial date),
    /// through what (a marketplace, a trade-in), and a note. Said again, it is corrected.
    pub fn sold(
        &mut self,
        reference: &str,
        price: &str,
        currency: Option<&str>,
        at: Option<&str>,
        via: Option<&str>,
        note: Option<&str>,
    ) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let id = resolve(&tx, reference, true)?;
        let (state, how): (String, Option<String>) = tx.query_row(
            "SELECT state, disposition FROM nodes WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if how.as_deref() != Some("sell") || state == "active" {
            return Err(refused(
                format!(
                    "node {id} did not leave as sold; `ev gone {id} --as sell` first (or `ev dispose {id} --as sell` while it is still here)"
                ),
                Value::Null,
            ));
        }
        let cents = crate::purchases::parse_money(price)?;
        if cents <= 0 {
            return Err(Error::Usage("a sale brought more than nothing".into()));
        }
        let currency = match currency.map(|c| c.trim().to_uppercase()) {
            Some(c) if c.len() == 3 && c.chars().all(|ch| ch.is_ascii_alphabetic()) => c,
            Some(c) => return Err(Error::Usage(format!("`{c}` is no currency code like EUR"))),
            None => crate::money::home_currency(&tx)?,
        };
        let at = at.map(partial_date).transpose()?;
        let text = |t: Option<&str>| {
            t.map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
        };
        let (via, note) = (text(via), text(note));
        let price = crate::purchases::money(cents);
        tx.execute(
            "INSERT INTO departures (node_id, at, price, currency, via, note)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(node_id) DO UPDATE SET at = COALESCE(excluded.at, at),
               price = excluded.price, currency = excluded.currency,
               via = COALESCE(excluded.via, via), note = COALESCE(excluded.note, note)",
            params![id, at, price, currency, via, note],
        )?;
        event(
            &tx,
            id,
            "sold",
            json!({ "price": price, "currency": currency, "at": at, "via": via }),
        )?;
        tx.commit()?;
        show(&self.conn, id)
    }
}
