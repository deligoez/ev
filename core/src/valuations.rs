//! What a thing is worth (purchases spec §3.3): dated observations, such as a second-hand
//! listing, a shop's price or an appraisal. The latest is the current value; the purchase price
//! is never one.

use rusqlite::{Connection, params};
use serde_json::{Value, json};

use crate::purchases::{money, parse_money};
use crate::store::{Inventory, brief, now, resolve};
use crate::{Error, Result};

/// A new observation; the amount is required, the rest defaults (home currency, today).
#[derive(Debug, Clone, Default)]
pub struct NewValuation {
    pub amount: String,
    pub currency: Option<String>,
    pub at: Option<String>,
    /// The date is a guess (an import that knows only when the record last changed).
    pub approximate: bool,
    pub source: Option<String>,
    pub note: Option<String>,
}

fn text(v: &Option<String>) -> Option<String> {
    v.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// A node's observations, newest first, each with its worth in today's money when known.
pub(crate) fn valuations_of(conn: &Connection, node: i64) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT id, amount, currency, at, approximate, source, note FROM valuations
          WHERE node_id = ?1 ORDER BY at DESC, id DESC",
    )?;
    let rows = stmt
        .query_map([node], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(id, amount, currency, at, approximate, source, note)| {
            let mut v = json!({
                "id": id,
                "amount": money(amount),
                "currency": currency,
                "at": at,
                "approximate": approximate,
                "source": source,
                "note": note,
            });
            if let Some(t) = crate::money::today_money(conn, amount, Some(&currency), Some(&at))? {
                v["today"] = t;
            }
            Ok(v)
        })
        .collect()
}

/// Records one observation on a node and clears the person's "do not track" on its value:
/// entering data is the answer (spec §3.7).
pub(crate) fn add_valuation(conn: &Connection, node: i64, new: &NewValuation) -> Result<i64> {
    let amount = parse_money(&new.amount)?;
    if amount <= 0 {
        return Err(Error::Usage("a value must be more than zero".into()));
    }
    let currency = match text(&new.currency) {
        Some(c) => crate::money::currency_code(&c)?,
        None => crate::money::home_currency(conn)?,
    };
    let at = match text(&new.at) {
        Some(d) => crate::purchases::date(&Some(d))?.unwrap_or_default(),
        None => crate::store::today().to_string(),
    };
    // What a thing is worth is seen, never foreseen.
    if at > crate::store::today().to_string() {
        return Err(Error::Usage(format!("`{at}` is still to come")));
    }
    conn.execute(
        "INSERT INTO valuations (node_id, amount, currency, at, approximate, source, note, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            node,
            amount,
            currency,
            at,
            new.approximate,
            text(&new.source),
            text(&new.note),
            now()
        ],
    )?;
    let id = conn.last_insert_rowid();
    crate::coverage::clear_decision(conn, node, "value")?;
    Ok(id)
}

impl Inventory {
    /// Records a value for a thing when `new` is given, and answers with its observations.
    pub fn value(&mut self, reference: &str, new: Option<&NewValuation>) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let node = resolve(&tx, reference, false)?;
        let added = new.map(|v| add_valuation(&tx, node, v)).transpose()?;
        tx.commit()?;
        Ok(json!({
            "node": brief(&self.conn, node)?,
            "added": added,
            "valuations": valuations_of(&self.conn, node)?,
        }))
    }

    /// Removes an observation recorded by mistake.
    pub fn value_remove(&mut self, id: i64) -> Result<Value> {
        let node: i64 = self
            .conn
            .query_row("SELECT node_id FROM valuations WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .map_err(|_| Error::NotFound(format!("no value with id {id}")))?;
        self.conn
            .execute("DELETE FROM valuations WHERE id = ?1", [id])?;
        Ok(json!({
            "node": brief(&self.conn, node)?,
            "added": null,
            "valuations": valuations_of(&self.conn, node)?,
        }))
    }
}
