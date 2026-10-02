//! Coverage (purchases spec §3.6, §3.7): warranties and insurance as one kind of record, with a
//! computed status, proposals from linked purchases, and the person's decision not to track a
//! thing's coverage or value.

use chrono::{Days, Months, NaiveDate, NaiveDateTime};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::purchases::{money, parse_money};
use crate::store::{Inventory, brief, event, ids, now, resolve};
use crate::{Error, Result};

pub const COVERAGE_KINDS: [&str; 5] = [
    "statutory",
    "manufacturer",
    "extended",
    "store",
    "insurance",
];

/// How close an end has to be before a coverage shows as ending, by default.
pub const WARNING_DAYS: i64 = 60;

/// The legal minimum for goods sold with a warranty certificate in Turkey: two years from
/// delivery (Garanti Belgesi Yönetmeliği), proposed for a durable linked purchase.
const STATUTORY_YEARS: u32 = 2;

/// What a new coverage says; `term` is `2y`, `18m`, `90d`, `6w` or `lifetime`.
#[derive(Debug, Clone, Default)]
pub struct NewCoverage {
    pub kind: String,
    pub issuer: Option<String>,
    pub number: Option<String>,
    /// `delivery` (the default), a date `YYYY-MM-DD`, or `after:<coverage id>`.
    pub from: Option<String>,
    pub term: Option<String>,
    pub usage: Option<String>,
    pub ends: Option<String>,
    pub premium: Option<String>,
    pub deductible: Option<String>,
    pub currency: Option<String>,
    pub scope: Option<String>,
    pub note: Option<String>,
}

fn text(v: &Option<String>) -> Option<String> {
    v.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn parse_day(d: &str) -> Result<NaiveDate> {
    let d = d.trim();
    NaiveDate::parse_from_str(d.get(..10).unwrap_or(d), "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(&format!("{d}-01"), "%Y-%m-%d"))
        .map_err(|_| Error::Usage(format!("`{d}` is not a date YYYY-MM-DD")))
}

/// `2y` → (2, year); `lifetime` → (0, lifetime).
fn parse_term(t: &str) -> Result<(i64, &'static str)> {
    let t = t.trim().to_lowercase();
    if t == "lifetime" {
        return Ok((0, "lifetime"));
    }
    let bad = || Error::Usage(format!("term `{t}`; use e.g. 2y, 18m, 6w, 90d or lifetime"));
    let (n, unit) = t.split_at(t.find(|c: char| !c.is_ascii_digit()).ok_or_else(bad)?);
    let n: i64 = n.parse().map_err(|_| bad())?;
    let unit = match unit.trim() {
        "y" | "year" | "years" | "yıl" => "year",
        "m" | "month" | "months" | "ay" => "month",
        "w" | "week" | "weeks" | "hafta" => "week",
        "d" | "day" | "days" | "gün" => "day",
        _ => return Err(bad()),
    };
    if n < 1 {
        return Err(bad());
    }
    Ok((n, unit))
}

fn add_term(start: NaiveDate, n: i64, unit: &str) -> Option<NaiveDate> {
    match unit {
        "year" => start.checked_add_months(Months::new(12 * n as u32)),
        "month" => start.checked_add_months(Months::new(n as u32)),
        "week" => start.checked_add_days(Days::new(7 * n as u64)),
        "day" => start.checked_add_days(Days::new(n as u64)),
        _ => None,
    }
}

/// The days a node spent broken (from `broken` to the next `fixed`, or to today) after `from`.
fn repair_days(conn: &Connection, node: i64, from: NaiveDate, today: NaiveDate) -> Result<i64> {
    let mut stmt = conn.prepare(
        "SELECT type, at FROM events WHERE node_id = ?1 AND type IN ('broken', 'fixed') ORDER BY id",
    )?;
    let rows = stmt
        .query_map([node], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let day = |at: &str| {
        NaiveDateTime::parse_from_str(&at[..19.min(at.len())], "%Y-%m-%dT%H:%M:%S")
            .map(|t| t.date())
            .or_else(|_| parse_day(at).map_err(|_| ()))
            .ok()
    };
    let mut total = 0;
    let mut since: Option<NaiveDate> = None;
    for (kind, at) in rows {
        match (kind.as_str(), day(&at)) {
            ("broken", Some(d)) if since.is_none() => since = Some(d.max(from)),
            ("fixed", Some(d)) => {
                if let Some(s) = since.take()
                    && d > s
                {
                    total += (d - s).num_days();
                }
            }
            _ => {}
        }
    }
    if let Some(s) = since
        && today > s
    {
        total += (today - s).num_days();
    }
    Ok(total)
}

/// When the purchases linked to these nodes were delivered (or ordered), the earliest.
fn delivery(conn: &Connection, nodes: &[i64]) -> Result<Option<NaiveDate>> {
    let mut best: Option<NaiveDate> = None;
    for n in nodes {
        let mut stmt = conn.prepare(
            "SELECT COALESCE(p.delivered_at, p.ordered_at) FROM purchases p
               JOIN purchase_links l ON l.purchase_id = p.id WHERE l.node_id = ?1",
        )?;
        let dates = stmt
            .query_map([n], |r| r.get::<_, Option<String>>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for d in dates.into_iter().flatten() {
            if let Ok(d) = parse_day(&d) {
                best = Some(best.map_or(d, |b| b.min(d)));
            }
        }
    }
    Ok(best)
}

struct Row {
    kind: String,
    starts: String,
    start_date: Option<String>,
    after_id: Option<i64>,
    term_n: Option<i64>,
    term_unit: Option<String>,
    ends_on: Option<String>,
}

fn row(conn: &Connection, id: i64) -> Result<Row> {
    conn.query_row(
        "SELECT kind, starts, start_date, after_id, term_n, term_unit, ends_on
           FROM coverages WHERE id = ?1",
        [id],
        |r| {
            Ok(Row {
                kind: r.get(0)?,
                starts: r.get(1)?,
                start_date: r.get(2)?,
                after_id: r.get(3)?,
                term_n: r.get(4)?,
                term_unit: r.get(5)?,
                ends_on: r.get(6)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| Error::NotFound(format!("no coverage with id {id}")))
}

fn nodes_of(conn: &Connection, id: i64) -> Result<Vec<i64>> {
    ids(
        conn,
        "SELECT node_id FROM coverage_nodes WHERE coverage_id = ?1 ORDER BY node_id",
        [id],
    )
}

/// (start, end) of a coverage; an end of None with a start means lifetime. `depth` guards a
/// chain of `after:` coverages.
fn span(
    conn: &Connection,
    id: i64,
    today: NaiveDate,
    depth: usize,
) -> Result<(Option<NaiveDate>, Option<NaiveDate>, i64)> {
    let r = row(conn, id)?;
    let nodes = nodes_of(conn, id)?;
    let start = match r.starts.as_str() {
        "date" => r.start_date.as_deref().map(parse_day).transpose()?,
        "after" if depth < 8 => match r.after_id {
            Some(a) => span(conn, a, today, depth + 1)?.1,
            None => None,
        },
        "delivery" => delivery(conn, &nodes)?,
        _ => None,
    };
    let mut repair = 0;
    if let (Some(s), true) = (
        start,
        matches!(r.kind.as_str(), "statutory" | "manufacturer"),
    ) {
        for n in &nodes {
            repair = repair.max(repair_days(conn, *n, s, today)?);
        }
    }
    let end = match (&r.ends_on, start, r.term_unit.as_deref(), r.term_n) {
        (Some(e), _, _, _) => Some(parse_day(e)?),
        (None, Some(_), Some("lifetime"), _) => None,
        (None, Some(s), Some(u), Some(n)) => {
            add_term(s, n, u).and_then(|e| e.checked_add_days(Days::new(repair as u64)))
        }
        _ => None,
    };
    Ok((start, end, repair))
}

pub(crate) fn coverage_json(conn: &Connection, id: i64, warning: i64) -> Result<Value> {
    let today = chrono::Utc::now().date_naive();
    let mut v = conn.query_row(
        "SELECT kind, issuer, number, starts, start_date, after_id, term_n, term_unit, usage,
                ends_on, premium, deductible, currency, scope, note, created_at
           FROM coverages WHERE id = ?1",
        [id],
        |r| {
            let premium: Option<i64> = r.get(10)?;
            let deductible: Option<i64> = r.get(11)?;
            Ok(json!({
                "id": id,
                "kind": r.get::<_, String>(0)?,
                "issuer": r.get::<_, Option<String>>(1)?,
                "number": r.get::<_, Option<String>>(2)?,
                "starts": r.get::<_, String>(3)?,
                "start_date": r.get::<_, Option<String>>(4)?,
                "after": r.get::<_, Option<i64>>(5)?,
                "term": match (r.get::<_, Option<i64>>(6)?, r.get::<_, Option<String>>(7)?) {
                    (_, Some(u)) if u == "lifetime" => Some("lifetime".to_string()),
                    (Some(n), Some(u)) => Some(format!("{n} {u}{}", if n == 1 { "" } else { "s" })),
                    _ => None,
                },
                "usage": r.get::<_, Option<String>>(8)?,
                "ends_on": r.get::<_, Option<String>>(9)?,
                "premium": premium.map(money),
                "deductible": deductible.map(money),
                "currency": r.get::<_, Option<String>>(12)?,
                "scope": r.get::<_, Option<String>>(13)?,
                "note": r.get::<_, Option<String>>(14)?,
                "created_at": r.get::<_, String>(15)?,
            }))
        },
    )?;
    let (start, end, repair) = span(conn, id, today, 0)?;
    let lifetime = v["term"] == "lifetime";
    let status = match (start, end) {
        (None, _) if v["ends_on"].is_null() => "undetermined",
        (_, Some(e)) if e < today => "ended",
        (_, Some(e)) if (e - today).num_days() <= warning => "ending",
        (Some(_), None) if lifetime => "active",
        (_, Some(_)) => "active",
        _ => "undetermined",
    };
    v["start"] = json!(start.map(|d| d.to_string()));
    v["end"] = json!(end.map(|d| d.to_string()));
    v["days_left"] = json!(end.filter(|e| *e >= today).map(|e| (e - today).num_days()));
    if repair > 0 {
        v["repair_days"] = json!(repair);
    }
    v["status"] = json!(status);
    v["nodes"] = json!(
        nodes_of(conn, id)?
            .into_iter()
            .map(|n| brief(conn, n))
            .collect::<Result<Vec<_>>>()?
    );
    v["documents"] = json!(
        ids(
            conn,
            "SELECT document_id FROM document_links WHERE target = 'coverage' AND target_id = ?1",
            [id],
        )?
        .into_iter()
        .map(|d| crate::docs::doc_json(conn, d))
        .collect::<Result<Vec<_>>>()?
    );
    Ok(v)
}

pub(crate) fn warning_days(conn: &Connection) -> Result<i64> {
    Ok(conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'coverage_warning_days'",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .and_then(|v| v.parse().ok())
        .unwrap_or(WARNING_DAYS))
}

/// A node's coverages, and the statutory one its linked durable purchases propose when it has
/// none of that kind and no "do not track" decision.
pub(crate) fn coverages_of(conn: &Connection, node: i64) -> Result<(Vec<Value>, Option<Value>)> {
    let warning = warning_days(conn)?;
    let list = ids(
        conn,
        "SELECT coverage_id FROM coverage_nodes WHERE node_id = ?1 ORDER BY coverage_id",
        [node],
    )?
    .into_iter()
    .map(|c| {
        let mut v = coverage_json(conn, c, warning)?;
        if let Some(o) = v.as_object_mut() {
            o.remove("nodes");
        }
        Ok(v)
    })
    .collect::<Result<Vec<Value>>>()?;
    let has_statutory = list.iter().any(|c| c["kind"] == "statutory");
    let proposal = if has_statutory || decision(conn, node, "coverage")?.is_some() {
        None
    } else {
        statutory_proposal(conn, node)?
    };
    Ok((list, proposal))
}

/// Marketplaces whose sellers are abroad, by the shop name an adapter writes (folded to lower
/// case), with the country they sell from. A line from one of them is foreign unless that
/// country is the inventory's `home_country`: for a home in Germany, Amazon.de is at home.
/// A shop not listed is taken to sell in the home country; its currency still tells.
const FOREIGN_SHOPS: [(&str, &str); 9] = [
    ("aliexpress", "CN"),
    ("temu", "CN"),
    ("banggood", "CN"),
    ("amazon.com", "US"),
    ("amazon.co.uk", "GB"),
    ("amazon.de", "DE"),
    ("amazon.fr", "FR"),
    ("amazon.it", "IT"),
    ("amazon.es", "ES"),
];

/// Whether a line was sold under the home country's rules: paid in the home currency (or no
/// currency given) and not from a marketplace abroad. The currency alone is not enough: a
/// foreign marketplace may charge in the home currency.
fn sold_at_home(shop: Option<&str>, currency: Option<&str>, country: &str, home: &str) -> bool {
    let shop = shop.map(|s| s.trim().to_lowercase()).unwrap_or_default();
    currency.is_none_or(|c| c.eq_ignore_ascii_case(home))
        && FOREIGN_SHOPS
            .iter()
            .find(|(s, _)| *s == shop)
            .is_none_or(|(_, c)| c.eq_ignore_ascii_case(country))
}

/// The statutory coverage a node's linked durable purchases propose (spec §3.6): two years from
/// the earliest delivery of a line sold in the home country. None when every such line was
/// bought abroad (the Turkish minimum binds a seller in Türkiye, not a foreign marketplace's)
/// or when the two years, with any time in repair, have already passed: a proposal that has
/// ended asks nothing.
fn statutory_proposal(conn: &Connection, node: i64) -> Result<Option<Value>> {
    let country = crate::money::home_country(conn)?;
    let home = crate::money::home_currency(conn)?;
    let mut stmt = conn.prepare(
        "SELECT p.shop, p.currency, COALESCE(p.delivered_at, p.ordered_at)
           FROM purchases p JOIN purchase_links l ON l.purchase_id = p.id
          WHERE l.node_id = ?1 AND p.bucket = 'durable' AND p.status = 'delivered'",
    )?;
    let rows = stmt
        .query_map([node], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let start = rows
        .into_iter()
        .filter(|(shop, cur, _)| sold_at_home(shop.as_deref(), cur.as_deref(), &country, &home))
        .filter_map(|(_, _, d)| d.and_then(|d| parse_day(&d).ok()))
        .min();
    let Some(s) = start else {
        return Ok(None);
    };
    let today = chrono::Utc::now().date_naive();
    let end = add_term(s, STATUTORY_YEARS.into(), "year");
    let repair = repair_days(conn, node, s, today)?;
    if end
        .and_then(|e| e.checked_add_days(Days::new(repair as u64)))
        .is_some_and(|e| e < today)
    {
        return Ok(None);
    }
    Ok(Some(json!({
        "kind": "statutory",
        "term": "2 years",
        "start": s.to_string(),
        "end": end.map(|e| e.to_string()),
        "why": "a durable purchase is linked; goods sold with a warranty certificate carry at least two years from delivery",
    })))
}

/// The person's standing decision on a tracked subject (`value` or `coverage`) for a node,
/// its own or inherited from a holder: `(value, why, on)`.
pub(crate) fn decision(
    conn: &Connection,
    node: i64,
    subject: &str,
) -> Result<Option<(String, Option<String>, i64)>> {
    let kind = format!("{subject}_skip");
    let mut at = Some(node);
    let mut guard = 0;
    while let Some(n) = at {
        let m: Option<(Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT value, note FROM marks WHERE node_id = ?1 AND kind = ?2",
                params![n, kind],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((v, why)) = m {
            return Ok(Some((v.unwrap_or_else(|| "no".into()), why, n)));
        }
        at = conn.query_row("SELECT parent_id FROM nodes WHERE id = ?1", [n], |r| {
            r.get(0)
        })?;
        guard += 1;
        if guard > 10_000 {
            break;
        }
    }
    Ok(None)
}

/// The valuable-thing threshold, in minor units of the home currency (purchases spec §3.8).
pub(crate) fn threshold(conn: &Connection) -> Result<i64> {
    let v: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'valuable_threshold'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(v.and_then(|v| parse_money(&v).ok()).unwrap_or(100_000))
}

/// For `ev todo`: coverages ending within the warning window, one each; valuable things with no
/// coverage and no decision, and bought things with no value and no decision, each as a count
/// with the five dearest; and the open purchase lines, as a count.
pub(crate) fn todo_parts(conn: &Connection) -> Result<TodoParts> {
    let warning = warning_days(conn)?;
    let ending = ids(conn, "SELECT id FROM coverages ORDER BY id", [])?
        .into_iter()
        .map(|c| coverage_json(conn, c, warning))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .filter(|c| c["status"] == "ending")
        .collect();
    let home: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'home_currency'",
            [],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_else(|| "TRY".into());
    let at = threshold(conn)?;
    // What each uncovered thing cost: its dearest linked durable line, in today's money when
    // the index (and the rate, for another currency) is cached, else as paid when it was paid
    // in the home currency (spec §3.9).
    let mut stmt = conn.prepare(
        "SELECT l.node_id, p.paid * l.qty / (p.qty * p.pack), p.currency,
                COALESCE(p.delivered_at, p.ordered_at)
           FROM purchases p
           JOIN purchase_links l ON l.purchase_id = p.id
           JOIN nodes n ON n.id = l.node_id
          WHERE n.state != 'gone' AND p.bucket = 'durable' AND p.paid IS NOT NULL",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut worth: std::collections::BTreeMap<i64, f64> = std::collections::BTreeMap::new();
    for (node, paid, currency, date) in rows {
        let today = crate::money::today_money(conn, paid, currency.as_deref(), date.as_deref())?
            .and_then(|t| t["amount"].as_str().and_then(|a| parse_money(a).ok()));
        let v = match today {
            Some(t) => Some(t),
            None if currency
                .as_deref()
                .is_none_or(|c| c.eq_ignore_ascii_case(&home)) =>
            {
                Some(paid)
            }
            None => None,
        };
        if let Some(v) = v {
            let e = worth.entry(node).or_insert(0.0);
            *e = e.max(v as f64);
        }
    }
    let exists = |sql: &str, node: i64| -> Result<bool> {
        Ok(conn.query_row(sql, [node], |r| r.get::<_, i64>(0))? > 0)
    };
    let (mut uncovered, mut unvalued) = (Vec::new(), Vec::new());
    for (node, paid) in worth {
        if paid >= at as f64
            && !exists(
                "SELECT COUNT(*) FROM coverage_nodes WHERE node_id = ?1",
                node,
            )?
            && decision(conn, node, "coverage")?.is_none()
        {
            uncovered.push((paid, node));
        }
        if !exists("SELECT COUNT(*) FROM valuations WHERE node_id = ?1", node)?
            && decision(conn, node, "value")?.is_none()
        {
            unvalued.push((paid, node));
        }
    }
    let summary = |mut open: Vec<(f64, i64)>, extra: Value| -> Result<Value> {
        open.sort_by(|a, b| b.0.total_cmp(&a.0));
        let top = open
            .iter()
            .take(5)
            .map(|(paid, n)| {
                let mut v = serde_json::to_value(brief(conn, *n)?)
                    .map_err(|e| Error::Internal(e.to_string()))?;
                v["worth"] = json!(money(*paid as i64));
                v["currency"] = json!(home);
                Ok(v)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut v = json!({ "count": open.len(), "currency": home, "top": top });
        if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        Ok(v)
    };
    let dearest_first = |open: &[(f64, i64)]| {
        let mut o = open.to_vec();
        o.sort_by(|a, b| b.0.total_cmp(&a.0));
        o.into_iter().map(|(_, n)| n).collect::<Vec<_>>()
    };
    Ok(TodoParts {
        ending,
        uncovered: dearest_first(&uncovered),
        unvalued: dearest_first(&unvalued),
        coverage: summary(uncovered, json!({ "threshold": money(at) }))?,
        values: summary(unvalued, json!({}))?,
        purchases: open_purchases(conn)?,
    })
}

/// Open durable purchase lines: something left to link, not dismissed, not the second sight of
/// another line (spec §4.3).
fn open_purchases(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM purchases p
          WHERE p.bucket = 'durable' AND p.status = 'delivered' AND p.dismissed IS NULL
            AND p.same_as IS NULL
            AND p.qty * p.pack > COALESCE((SELECT SUM(qty) FROM purchase_links WHERE purchase_id = p.id), 0)",
        [],
        |r| r.get(0),
    )?)
}

/// What the coverage and value subjects add to `ev todo`.
pub(crate) struct TodoParts {
    pub ending: Vec<Value>,
    /// Every thing to ask about coverage, and about value, dearest first; `coverage` and
    /// `values` carry only their count and the top few.
    pub uncovered: Vec<i64>,
    pub unvalued: Vec<i64>,
    pub coverage: Value,
    pub values: Value,
    pub purchases: i64,
}

pub(crate) fn clear_decision(conn: &Connection, node: i64, subject: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM marks WHERE node_id = ?1 AND kind = ?2",
        params![node, format!("{subject}_skip")],
    )?;
    Ok(())
}

/// The inventory's own settings (purchases spec §3.8), kept in the database, with defaults.
pub const INVENTORY_SETTINGS: [(&str, &str); 5] = [
    ("home_country", "TR"),
    ("home_currency", "TRY"),
    ("price_index", "eurostat:TR"),
    ("valuable_threshold", "1000"),
    ("coverage_warning_days", "60"),
];

impl Inventory {
    /// Every inventory setting with its value (the default when unset), or sets one.
    pub fn inventory_settings(&mut self, name: Option<&str>, value: Option<&str>) -> Result<Value> {
        if let (Some(n), Some(v)) = (name, value) {
            let v = v.trim();
            let ok = match n {
                "home_country" => v.len() == 2 && v.chars().all(|c| c.is_ascii_alphabetic()),
                "home_currency" => v.len() == 3 && v.chars().all(|c| c.is_ascii_alphabetic()),
                "price_index" => v.contains(':'),
                "valuable_threshold" => parse_money(v).is_ok_and(|m| m >= 0),
                "coverage_warning_days" => v.parse::<i64>().is_ok_and(|d| d > 0),
                other => {
                    return Err(Error::Usage(format!(
                        "`{other}` is not an inventory setting; use {}",
                        INVENTORY_SETTINGS.map(|(k, _)| k).join(", ")
                    )));
                }
            };
            if !ok {
                return Err(Error::Usage(format!("`{v}` is not a value for {n}")));
            }
            let v = if n.starts_with("home_") {
                v.to_uppercase()
            } else {
                v.to_string()
            };
            self.conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![n, v],
            )?;
        }
        let mut out = serde_json::Map::new();
        for (k, default) in INVENTORY_SETTINGS {
            let set: Option<String> = self
                .conn
                .query_row("SELECT value FROM settings WHERE key = ?1", [k], |r| {
                    r.get(0)
                })
                .optional()?;
            out.insert(
                k.to_string(),
                json!({ "value": set.clone().unwrap_or_else(|| default.to_string()), "default": set.is_none() }),
            );
        }
        Ok(json!({ "inventory_settings": out }))
    }

    /// A warranty or an insurance covering one or more things.
    pub fn cover_add(&mut self, refs: &[String], new: &NewCoverage) -> Result<Value> {
        if refs.is_empty() {
            return Err(Error::Usage("name at least one thing it covers".into()));
        }
        let tx = self.conn.transaction()?;
        let nodes = refs
            .iter()
            .map(|r| resolve(&tx, r, false))
            .collect::<Result<Vec<_>>>()?;
        let id = add_coverage(&tx, &nodes, new)?;
        tx.commit()?;
        self.cover_show(id)
    }

    pub fn cover_show(&self, id: i64) -> Result<Value> {
        Ok(json!({ "coverage": coverage_json(&self.conn, id, warning_days(&self.conn)?)? }))
    }
}

/// Records a coverage over `nodes` and returns its id; clears their "do not track" decision.
pub(crate) fn add_coverage(tx: &Connection, nodes: &[i64], new: &NewCoverage) -> Result<i64> {
    let kind = new.kind.trim().to_lowercase();
    if !COVERAGE_KINDS.contains(&kind.as_str()) {
        return Err(Error::Usage(format!(
            "`{}` is not a coverage kind; use {}",
            new.kind,
            COVERAGE_KINDS.join(", ")
        )));
    }
    let (starts, start_date, after_id) = match text(&new.from).as_deref() {
        None | Some("delivery") => ("delivery", None, None),
        Some(a) if a.starts_with("after:") => {
            let id: i64 = a[6..]
                .trim()
                .parse()
                .map_err(|_| Error::Usage(format!("`{a}`: after:<coverage id>")))?;
            ("after", None, Some(id))
        }
        Some(d) => ("date", Some(parse_day(d)?.to_string()), None),
    };
    let term = text(&new.term).map(|t| parse_term(&t)).transpose()?;
    let ends = text(&new.ends)
        .map(|e| parse_day(&e).map(|d| d.to_string()))
        .transpose()?;
    if term.is_none() && ends.is_none() {
        return Err(Error::Usage(
            "give a --term (2y, 18m, lifetime) or an --ends date".into(),
        ));
    }
    if kind == "insurance" && ends.is_none() && term.is_none_or(|(_, u)| u == "lifetime") {
        return Err(Error::Usage(
            "an insurance needs an --ends date or a term".into(),
        ));
    }
    let premium = text(&new.premium).map(|p| parse_money(&p)).transpose()?;
    let deductible = text(&new.deductible).map(|p| parse_money(&p)).transpose()?;
    if let Some(a) = after_id {
        row(tx, a)?;
    }
    tx.execute(
        "INSERT INTO coverages (kind, issuer, number, starts, start_date, after_id, term_n,
                                term_unit, usage, ends_on, premium, deductible, currency,
                                scope, note, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            kind,
            text(&new.issuer),
            text(&new.number),
            starts,
            start_date,
            after_id,
            term.map(|(n, _)| n),
            term.map(|(_, u)| u),
            text(&new.usage),
            ends,
            premium,
            deductible,
            text(&new.currency).map(|c| c.to_uppercase()),
            text(&new.scope),
            text(&new.note),
            now()
        ],
    )?;
    let id = tx.last_insert_rowid();
    for &n in nodes {
        tx.execute(
            "INSERT OR IGNORE INTO coverage_nodes (coverage_id, node_id) VALUES (?1, ?2)",
            params![id, n],
        )?;
        // Entering data clears a "do not track" decision on the thing itself.
        clear_decision(tx, n, "coverage")?;
        event(
            tx,
            n,
            "coverage_added",
            json!({ "coverage": id, "kind": kind }),
        )?;
    }
    Ok(id)
}

impl Inventory {
    /// Every coverage, or only those ending within the warning window (and already ended
    /// ones are left out of that).
    pub fn cover_list(&self, ending: bool) -> Result<Value> {
        let warning = warning_days(&self.conn)?;
        let list = ids(&self.conn, "SELECT id FROM coverages ORDER BY id", [])?
            .into_iter()
            .map(|c| coverage_json(&self.conn, c, warning))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .filter(|c| !ending || c["status"] == "ending")
            .collect::<Vec<_>>();
        Ok(json!({ "coverages": list }))
    }

    /// Removes a coverage record (a mistake); its documents stay in the store.
    pub fn cover_remove(&mut self, id: i64) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let r = row(&tx, id)?;
        for n in nodes_of(&tx, id)? {
            event(
                &tx,
                n,
                "coverage_removed",
                json!({ "coverage": id, "kind": r.kind }),
            )?;
        }
        tx.execute("DELETE FROM coverage_nodes WHERE coverage_id = ?1", [id])?;
        tx.execute(
            "DELETE FROM document_links WHERE target = 'coverage' AND target_id = ?1",
            [id],
        )?;
        tx.execute(
            "UPDATE coverages SET after_id = NULL, starts = 'delivery' WHERE after_id = ?1",
            [id],
        )?;
        tx.execute("DELETE FROM coverages WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(json!({ "removed": id }))
    }

    /// The person's decision on a tracked subject: `no` (do not track), `later` (not now) or
    /// `yes` (ask again). Neither `no` nor `later` is ever raised again by `ev` on its own.
    pub fn track(
        &mut self,
        reference: &str,
        subject: &str,
        decision: &str,
        why: Option<&str>,
    ) -> Result<Value> {
        if !["value", "coverage"].contains(&subject) {
            return Err(Error::Usage(format!(
                "`{subject}` is not tracked; use value or coverage"
            )));
        }
        let tx = self.conn.transaction()?;
        let id = resolve(&tx, reference, false)?;
        match decision {
            "no" | "later" => {
                tx.execute(
                    "INSERT INTO marks (node_id, kind, value, note, at) VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT (node_id, kind) DO UPDATE
                       SET value = excluded.value, note = excluded.note, at = excluded.at",
                    params![
                        id,
                        format!("{subject}_skip"),
                        decision,
                        why.map(str::trim).filter(|w| !w.is_empty()),
                        now()
                    ],
                )?;
            }
            "yes" => clear_decision(&tx, id, subject)?,
            other => {
                return Err(Error::Usage(format!("`{other}`; use no, later or yes")));
            }
        }
        event(
            &tx,
            id,
            "track",
            json!({ "subject": subject, "decision": decision, "why": why }),
        )?;
        tx.commit()?;
        crate::store::show(&self.conn, id)
    }
}

#[cfg(test)]
mod tests {
    use super::{add_term, parse_term};
    use chrono::NaiveDate;

    #[test]
    fn terms_read_in_years_months_weeks_days_or_lifetime() {
        assert_eq!(parse_term("2y").unwrap(), (2, "year"));
        assert_eq!(parse_term("18 months").unwrap(), (18, "month"));
        assert_eq!(parse_term("lifetime").unwrap(), (0, "lifetime"));
        assert!(parse_term("2").is_err() && parse_term("0y").is_err() && parse_term("x").is_err());
        let d = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        assert_eq!(add_term(d, 2, "year").unwrap().to_string(), "2026-02-28");
    }
}
