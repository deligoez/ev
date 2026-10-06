//! Past belongings (spec/past-belongings.md): when a thing came, and how a gone one left: when,
//! where it was then, and what a sale brought.

use std::collections::BTreeMap;

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
    // A date remembered is a date that was: never one still to come.
    let today = crate::store::today().format("%Y-%m-%d").to_string();
    if t > &today[..t.len()] {
        return Err(Error::Usage(format!("`{t}` is still to come")));
    }
    Ok(t.to_string())
}

/// Refuses a thing said to have come after it left: compared as far as both dates say.
pub(crate) fn came_before_left(came: &str, left: &str) -> Result<()> {
    let n = came.len().min(left.len());
    if came[..n] > left[..n] {
        return Err(Error::Usage(format!(
            "it came {came} but left {left}; one of the dates is not right"
        )));
    }
    Ok(())
}

/// When a record came: as said, else the earliest order date of a purchase linked to it.
pub(crate) fn came_of(conn: &Connection, node: i64) -> Result<Option<String>> {
    Ok(conn.query_row(
        &format!("SELECT {CAME} FROM nodes n WHERE n.id = ?1"),
        [node],
        |r| r.get(0),
    )?)
}

/// When a gone record left: as said, else the day it was recorded gone.
pub(crate) fn left_of(conn: &Connection, node: i64) -> Result<Option<String>> {
    Ok(conn.query_row(
        &format!(
            "SELECT {LEFT} FROM nodes n LEFT JOIN departures d ON d.node_id = n.id WHERE n.id = ?1"
        ),
        [node],
        |r| r.get(0),
    )?)
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
    if let Some(a) = &at
        && let Some(c) = came_of(conn, node)?
    {
        came_before_left(&c, a)?;
    }
    if place.is_some_and(|p| {
        let p = p.trim();
        p.starts_with('#') || (!p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
    }) {
        return Err(Error::Usage(
            "--where names a place (a former home) by its name, not a record or an id".into(),
        ));
    }
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
    // Set aside, a sale may already be said (`ev sold` before it leaves); nothing else is.
    if state != "gone" {
        let sold: Option<(Option<String>, Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT price, currency, via FROM departures WHERE node_id = ?1 AND price IS NOT NULL",
                [node],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        return Ok(match sold {
            Some((price, currency, via)) => {
                json!({ "price": price, "currency": currency, "via": via, "pending": true })
            }
            None => Value::Null,
        });
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
    // Not said: the day it was recorded gone, unless it was recorded already gone, when
    // nothing says when it left.
    let at = match at {
        Some(a) => Some(a),
        None => conn.query_row(
            &format!(
                "SELECT date(MAX(at), 'localtime') FROM events
                  WHERE node_id = ?1 AND type = 'gone' AND {NOT_RECALLED}"
            ),
            [node],
            |r| r.get::<_, Option<String>>(0),
        )?,
    };
    // What came in exchange for a thing traded away.
    let traded_for: Option<i64> = conn
        .query_row(
            "SELECT traded_for FROM departures WHERE node_id = ?1",
            [node],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let traded_for = traded_for.map(|t| super::brief_json(conn, t)).transpose()?;
    Ok(json!({
        "at": at, "where": place, "price": price, "currency": currency, "via": via, "note": note,
        "traded_for": traded_for,
    }))
}

/// The things traded away for `node`: what it came in exchange for.
pub(crate) fn traded_from(conn: &Connection, node: i64) -> Result<Value> {
    let ids = super::ids(
        conn,
        "SELECT node_id FROM departures WHERE traded_for = ?1 ORDER BY node_id",
        [node],
    )?;
    Ok(json!(
        ids.into_iter()
            .map(|i| super::brief_json(conn, i))
            .collect::<Result<Vec<_>>>()?
    ))
}

/// Where a sale in progress was listed, carried into the departure of a thing gone `--as sell`
/// as what it went through. The price it asked is not carried: an asking price is not what the
/// sale brought, and the sale mark keeps it in sight until `ev sold` says.
pub(super) fn carry_sale(conn: &Connection, node: i64, listed_on: i64) -> Result<()> {
    let listed: Option<Option<String>> = conn
        .query_row(
            "SELECT note FROM marks WHERE node_id = ?1 AND kind = 'sale'",
            [listed_on],
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
        // Not said again, the currency said before stands; never said, the home one.
        let said_before: Option<String> = tx
            .query_row(
                "SELECT currency FROM departures WHERE node_id = ?1 AND price IS NOT NULL",
                [id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let currency = match currency.map(|c| c.trim().to_uppercase()) {
            Some(c) if c.len() == 3 && c.chars().all(|ch| ch.is_ascii_alphabetic()) => c,
            Some(c) => return Err(Error::Usage(format!("`{c}` is no currency code like EUR"))),
            None => match said_before {
                Some(c) => c,
                None => crate::money::home_currency(&tx)?,
            },
        };
        let at = at.map(partial_date).transpose()?;
        if let Some(a) = &at
            && let Some(c) = came_of(&tx, id)?
        {
            came_before_left(&c, a)?;
        }
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

impl Inventory {
    /// A thing gone, left as a trade (spec/past-belongings.md, decided 2026-10-06), with what came
    /// in exchange when it is recorded. Also corrects a thing first recorded as given or sold,
    /// and links what came later, once it is recorded.
    pub fn traded(&mut self, reference: &str, for_ref: Option<&str>) -> Result<Value> {
        let tx = self.conn.transaction()?;
        // A trade is said of a thing that left, so a gone one is found by name too.
        let id = resolve(&tx, reference, true)?;
        let (state, how): (String, Option<String>) = tx.query_row(
            "SELECT state, disposition FROM nodes WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if state != "gone" {
            return Err(refused(
                format!("node {id} has not left; `ev gone {id} --as trade` first"),
                Value::Null,
            ));
        }
        // Only a leaving that handed the thing to someone may have been a swap.
        let how_text = how.as_deref().unwrap_or("unknown");
        if !matches!(how_text, "give" | "sell" | "trade") {
            return Err(refused(
                format!(
                    "node {id} left as {how_text}; only a thing given, sold or traded can be a trade"
                ),
                Value::Null,
            ));
        }
        let other = for_ref
            .map(|r| super::resolve_for_history(&tx, r))
            .transpose()?;
        if other == Some(id) {
            return Err(Error::Usage("a thing is not traded for itself".into()));
        }
        let before: Option<i64> = tx
            .query_row(
                "SELECT traded_for FROM departures WHERE node_id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        if how_text == "trade" && (other.is_none() || other == before) {
            return Err(refused(
                format!(
                    "node {id} is already recorded as traded{}; nothing to change",
                    before.map(|b| format!(" for #{b}")).unwrap_or_default()
                ),
                Value::Null,
            ));
        }
        if let Some(o) = other {
            trade_check(&tx, left_of(&tx, id)?.as_deref(), o)?;
        }
        tx.execute("UPDATE nodes SET disposition = 'trade' WHERE id = ?1", [id])?;
        // A swap brought no money: a sale price said before no longer stands.
        tx.execute(
            "INSERT INTO departures (node_id, traded_for) VALUES (?1, ?2)
             ON CONFLICT(node_id) DO UPDATE SET traded_for = COALESCE(excluded.traded_for, traded_for),
               price = NULL, currency = NULL",
            params![id, other],
        )?;
        event(&tx, id, "traded", json!({ "was": how, "for": other }))?;
        tx.commit()?;
        show(&self.conn, id)
    }
}

/// Whether `other` can be what came in exchange for a thing that left on `left`: a thing, not
/// a place, still ours when the swap happened, and not ours before it. Nothing saying when the
/// swap was, only the first is checked.
pub(crate) fn trade_check(conn: &Connection, left: Option<&str>, other: i64) -> Result<()> {
    let kind: String = conn.query_row("SELECT kind FROM nodes WHERE id = ?1", [other], |r| {
        r.get(0)
    })?;
    if matches!(kind.as_str(), "home" | "room") {
        return Err(Error::Usage(format!(
            "#{other} is a place; a thing is traded for a thing"
        )));
    }
    let Some(left) = left else { return Ok(()) };
    let before = |a: &str, b: &str| {
        let n = a.len().min(b.len());
        a[..n] < b[..n]
    };
    let gone: bool = conn.query_row(
        "SELECT state = 'gone' FROM nodes WHERE id = ?1",
        [other],
        |r| r.get(0),
    )?;
    if gone
        && let Some(o) = left_of(conn, other)?
        && before(&o, left)
    {
        return Err(Error::Usage(format!(
            "#{other} left {o}, before the swap ({left}); it cannot have come in exchange"
        )));
    }
    if let Some(c) = came_of(conn, other)? {
        if before(&c, left) {
            return Err(Error::Usage(format!(
                "#{other} came {c}, before the swap ({left}); it was ours already"
            )));
        }
        if c.get(..4) > left.get(..4) {
            return Err(Error::Usage(format!(
                "#{other} came {c}, years after the swap ({left}); one of the dates is not right"
            )));
        }
    }
    Ok(())
}

impl Inventory {
    /// `trade_check` for a swap still to be written (`ev gone --traded-for`, `ev add --gone
    /// trade --traded-for`), so a refused one leaves nothing half recorded.
    /// `left` not said, the swap is today when it is leaving `now`, else not known.
    pub fn check_trade(&self, left: Option<&str>, now: bool, other: i64) -> Result<()> {
        let left = match left {
            Some(l) => Some(partial_date(l)?),
            None if now => Some(crate::store::today().format("%Y-%m-%d").to_string()),
            None => None,
        };
        trade_check(&self.conn, left.as_deref(), other)
    }
}

/// Ways of leaving that are no past belonging: a record that was never real, a portion that
/// joined another, a paper kept as a copy.
const NOT_PAST: &str = "('mistake', 'merged', 'digitize')";

/// When a record came: as said (`came`), else the earliest order date of a purchase linked to
/// it. As SQL over `n`, a day or a partial date, or NULL when nothing says.
const CAME: &str =
    "COALESCE(n.came_at, (SELECT substr(MIN(COALESCE(p.ordered_at, p.delivered_at)), 1, 10)
       FROM purchase_links l JOIN purchases p ON p.id = l.purchase_id WHERE l.node_id = n.id))";

/// A `gone` event written as it happened, not a past thing recorded already gone (`ev add
/// --gone`): only the first says when it left.
const NOT_RECALLED: &str = "COALESCE(json_extract(data, '$.past'), 0) = 0";

/// When a gone record left: as said, else the day it was recorded gone; NULL for a past thing
/// recorded already gone with no date said.
const LEFT: &str = "COALESCE(d.at, (SELECT date(MAX(e.at), 'localtime') FROM events e
       WHERE e.node_id = n.id AND e.type = 'gone'
         AND COALESCE(json_extract(e.data, '$.past'), 0) = 0))";

/// The year a partial date falls in.
fn year_of(date: &str) -> Option<i32> {
    date.get(..4)?.parse().ok()
}

/// Cents per currency as `{"TRY": "1999.00"}`.
fn sums_json(sums: &BTreeMap<String, i64>) -> Value {
    sums.iter()
        .map(|(c, cents)| (c.clone(), json!(crate::purchases::money(*cents))))
        .collect::<serde_json::Map<_, _>>()
        .into()
}

impl Inventory {
    /// The past belongings (spec/past-belongings.md), last gone first: when each came and left,
    /// how, where, what was paid for it (its linked purchases) and what a sale brought; and per
    /// year how many left, with the money paid for them and got for them by currency.
    /// `name` keeps those whose name holds the word; `place` those left in that place.
    pub fn past(&self, name: Option<&str>, place: Option<&str>) -> Result<Value> {
        let conn = &self.conn;
        let place = place
            .map(|p| super::places::resolve_place(conn, p))
            .transpose()?;
        let word = name.map(crate::fold::fold).filter(|w| !w.is_empty());
        let home = crate::money::home_currency(conn)?;
        // Remembered: it left before it was recorded, so it was added already gone, or gone with
        // a date said (decided with the person, 2026-10-06). A date said later (`ev sold --at`,
        // `ev edit left=`) dates the leaving but does not make it remembered.
        let mut stmt = conn.prepare(&format!(
            "SELECT n.id, n.name, n.disposition, {CAME}, {LEFT}, pl.name, d.place_id,
                    d.price, d.currency, d.via,
                    EXISTS (SELECT 1 FROM events e WHERE e.node_id = n.id AND e.type = 'gone'
                      AND (json_extract(e.data, '$.past') = 1
                        OR json_extract(e.data, '$.at') IS NOT NULL))
               FROM nodes n LEFT JOIN departures d ON d.node_id = n.id
               LEFT JOIN places pl ON pl.id = d.place_id
              WHERE n.state = 'gone' AND n.disposition NOT IN {NOT_PAST}"
        ))?;
        type Row = (
            i64,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<String>,
            Option<String>,
            bool,
        );
        let rows: Vec<Row> = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                ))
            })?
            .collect::<std::result::Result<_, _>>()?;
        let mut paid_stmt = conn.prepare(
            "SELECT COALESCE(p.currency, ''), SUM(p.paid * l.qty / MAX(p.qty * p.pack, 1))
               FROM purchase_links l JOIN purchases p ON p.id = l.purchase_id
              WHERE l.node_id = ?1 AND p.paid IS NOT NULL GROUP BY 1",
        )?;
        // Year -> (how many left, paid, got).
        type Year = (usize, BTreeMap<String, i64>, BTreeMap<String, i64>);
        // Per list: its years, those nothing says when they left (counted apart, never put in a
        // year), and its things.
        #[derive(Default)]
        struct List {
            years: BTreeMap<i32, Year>,
            undated: Year,
            things: Vec<Value>,
        }
        let mut lists: [List; 2] = Default::default();
        for (id, name, how, came, left, where_, place_id, price, currency, via, remembered) in rows
        {
            if place.is_some() && place_id != place {
                continue;
            }
            if let Some(w) = &word
                && !crate::fold::fold(&name).contains(w.as_str())
            {
                continue;
            }
            let list = &mut lists[usize::from(!remembered)];
            let mut paid: BTreeMap<String, i64> = BTreeMap::new();
            for row in
                paid_stmt.query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            {
                let (c, cents) = row?;
                let c = if c.is_empty() { home.clone() } else { c };
                *paid.entry(c).or_default() += cents;
            }
            let got = price
                .as_deref()
                .map(crate::purchases::parse_money)
                .transpose()?;
            let currency = currency.unwrap_or_else(|| home.clone());
            {
                let e = match left.as_deref().and_then(year_of) {
                    Some(y) => list.years.entry(y).or_default(),
                    None => &mut list.undated,
                };
                e.0 += 1;
                for (c, cents) in &paid {
                    *e.1.entry(c.clone()).or_default() += cents;
                }
                if let Some(g) = got {
                    *e.2.entry(currency.clone()).or_default() += g;
                }
            }
            list.things.push(json!({
                "id": id, "name": name, "came": came, "left": left, "how": how,
                "where": where_, "paid": sums_json(&paid),
                "got": got.map(|g| json!({
                    "price": crate::purchases::money(g), "currency": currency, "via": via,
                })),
            }));
        }
        let year = |(n, paid, got): Year| json!({ "left": n, "paid": sums_json(&paid), "got": sums_json(&got) });
        let [remembered, recorded] = lists.map(|mut l| {
            // Last gone first, those nothing dates at the end; a year sorts before any month or
            // day in it, which is the order a person reading back expects.
            l.things
                .sort_by(|a, b| b["left"].as_str().cmp(&a["left"].as_str()));
            let years: Vec<Value> = l
                .years
                .into_iter()
                .rev()
                .map(|(y, e)| {
                    let mut v = year(e);
                    v["year"] = json!(y);
                    v
                })
                .collect();
            let undated = (l.undated.0 > 0).then(|| year(l.undated));
            json!({ "past": l.things, "years": years, "undated": undated })
        });
        Ok(json!({ "remembered": remembered, "left_inventory": recorded }))
    }

    /// What was ours in `year` (spec/past-belongings.md): every thing, past or present, that
    /// came by the end of that year and had not left before it began. A thing whose coming
    /// nothing says (no `came`, no linked purchase) cannot be placed in a year: those are
    /// counted apart, never guessed in. Things kept for someone else are not ours.
    pub fn past_year(&self, year: i32) -> Result<Value> {
        use chrono::Datelike;
        if !(1900..=crate::store::today().year()).contains(&year) {
            return Err(Error::Usage(format!("`{year}` is no year to look back on")));
        }
        let conn = &self.conn;
        let mut stmt = conn.prepare(&format!(
            "SELECT n.id, n.state, {CAME}, {LEFT}
               FROM nodes n LEFT JOIN departures d ON d.node_id = n.id
              WHERE n.kind IN ('item', 'furniture') AND n.owner_place IS NULL
                AND (n.state != 'gone' OR n.disposition NOT IN {NOT_PAST})"
        ))?;
        let rows: Vec<(i64, String, Option<String>, Option<String>)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<std::result::Result<_, _>>()?;
        let mut owned = Vec::new();
        let mut unknown = 0;
        for (id, state, came, left) in rows {
            let gone = state == "gone";
            let left = left.filter(|_| gone);
            if left.as_deref().and_then(year_of).is_some_and(|y| y < year) {
                continue;
            }
            match came.as_deref().and_then(year_of) {
                // Gone, nothing saying when: ours the year it came, not known after.
                Some(y) if gone && left.is_none() && y < year => {
                    unknown += 1;
                    continue;
                }
                Some(y) if y <= year => {}
                Some(_) => continue,
                // Nothing says when it came, but it is here this year, or it left in it.
                None if (!gone && year == crate::store::today().year())
                    || left.as_deref().and_then(year_of) == Some(year) => {}
                None => {
                    unknown += 1;
                    continue;
                }
            }
            let mut v = super::brief_json(conn, id)?;
            v["came"] = json!(came);
            if gone {
                v["left"] = json!(left);
            }
            owned.push(v);
        }
        owned.sort_by(|a, b| a["came"].as_str().cmp(&b["came"].as_str()));
        Ok(json!({ "year": year, "owned": owned, "unknown": unknown }))
    }
}

impl Inventory {
    /// The "past" section of `ev stats`: how many past belongings, how they left, and the money
    /// paid for them and got for them by currency. Today's numbers never count them.
    pub(crate) fn past_summary(&self) -> Result<Value> {
        let all = self.past(None, None)?;
        let lists = [&all["remembered"], &all["left_inventory"]];
        let mut how: BTreeMap<String, usize> = BTreeMap::new();
        let mut records = 0;
        for n in lists
            .iter()
            .flat_map(|l| l["past"].as_array().into_iter().flatten())
        {
            records += 1;
            *how.entry(n["how"].as_str().unwrap_or_default().to_string())
                .or_default() += 1;
        }
        let total = |key: &str| -> Result<Value> {
            let mut sums: BTreeMap<String, i64> = BTreeMap::new();
            for l in lists {
                let years = l["years"].as_array().into_iter().flatten();
                for y in years.chain(Some(&l["undated"]).filter(|u| u.is_object())) {
                    for (c, a) in y[key].as_object().into_iter().flatten() {
                        *sums.entry(c.clone()).or_default() +=
                            crate::purchases::parse_money(a.as_str().unwrap_or_default())?;
                    }
                }
            }
            Ok(sums_json(&sums))
        };
        Ok(json!({
            "records": records,
            "how": how,
            "paid": total("paid")?,
            "got": total("got")?,
        }))
    }
}
