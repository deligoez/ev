//! Purchases (purchases spec §3.2): lines of what was bought. A line never creates a node; it is
//! linked to one on the person's word. Lines come from adapters as NDJSON (spec §6) or are
//! entered by hand, and the documents an adapter sends (invoices) hang on them.

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::docs::NewDoc;
use crate::error::{not_found, refuse, usage};
use crate::store::{Inventory, brief, event, ids, now, resolve};
use crate::{Error, Result};

/// The buckets a line may be in (spec §3.2); consumables are not imported for now (§11).
/// `digital` (a licence, a game key, a membership) and `service` (a diet programme, a repair)
/// are paid for and never a thing in the home: they never wait to be linked, and `ev stats`
/// counts them apart.
pub const BUCKETS: [&str; 4] = ["durable", "clothing", "digital", "service"];

/// Why a line will never be a node.
/// `cancelled`: the shop cancelled the order (a payment declined, never shipped), so nothing came
/// and nothing was spent, unlike `returned`.
pub const DISMISSALS: [&str; 7] = [
    "consumed",
    "given",
    "returned",
    "cancelled",
    "elsewhere",
    "not-mine",
    "duplicate",
];

/// An amount in minor units (kuruş, cents) from `1234.56`, `1.234,56` or `1234`.
pub(crate) fn parse_money(s: &str) -> Result<i64> {
    let bad = || usage("purchase_not_an_amount", json!({ "amount": s }));
    let t: String = s.trim().chars().filter(|c| !c.is_whitespace()).collect();
    let neg = t.starts_with('-');
    let t = t.trim_start_matches('-');
    let (whole, frac) = match (t.rfind('.'), t.rfind(',')) {
        (Some(d), Some(c)) => {
            let at = d.max(c);
            (t[..at].replace(['.', ','], ""), &t[at + 1..])
        }
        (Some(at), None) | (None, Some(at)) if t.matches(['.', ',']).count() == 1 => {
            (t[..at].to_string(), &t[at + 1..])
        }
        (None, None) => (t.to_string(), ""),
        _ => return Err(bad()),
    };
    if whole.is_empty() && frac.is_empty()
        || !whole.chars().all(|c| c.is_ascii_digit())
        || !frac.chars().all(|c| c.is_ascii_digit())
        || frac.len() > 2
    {
        return Err(bad());
    }
    let w: i64 = if whole.is_empty() {
        0
    } else {
        whole.parse().map_err(|_| bad())?
    };
    let f: i64 = format!("{frac:0<2}").parse().map_err(|_| bad())?;
    let v = w
        .checked_mul(100)
        .and_then(|w| w.checked_add(f))
        .ok_or_else(bad)?;
    Ok(if neg { -v } else { v })
}

/// Minor units back to `1234.56`.
pub(crate) fn money(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    format!("{sign}{}.{:02}", cents.abs() / 100, cents.abs() % 100)
}

fn text(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(|x| match x {
            Value::String(s) => Some(s.trim().to_string()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
        .filter(|s| !s.is_empty())
}

pub(crate) fn date(v: &Option<String>) -> Result<Option<String>> {
    v.as_deref()
        .map(|d| {
            let d = d.get(..10).unwrap_or(d);
            // A day, or a month or a year as the person remembers it (`2018-03`, `2018`).
            let partial = || {
                let parts: Vec<&str> = d.split('-').collect();
                let digits =
                    |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
                match parts.as_slice() {
                    [y] => digits(y, 4),
                    [y, m] => digits(y, 4) && digits(m, 2) && ("01"..="12").contains(m),
                    _ => false,
                }
            };
            if chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok() || partial() {
                Ok(d.to_string())
            } else {
                Err(usage("not_a_date", json!({ "date": d })))
            }
        })
        .transpose()
}

/// A purchase line with what came with it: its attachments and its documents.
pub(crate) fn purchase_json(conn: &Connection, id: i64) -> Result<Value> {
    let mut p = purchase_row(conn, id)?;
    p["attachments"] = json!(crate::attachments::attachments_of(conn, id)?);
    p["documents"] = json!(
        ids(
            conn,
            "SELECT DISTINCT document_id FROM document_links
              WHERE target = 'purchase'
                AND (target_id = ?1 OR target_id IN (SELECT id FROM purchases WHERE same_as = ?1))
              ORDER BY document_id",
            [id],
        )?
        .into_iter()
        .map(|d| crate::docs::doc_json(conn, d))
        .collect::<Result<Vec<_>>>()?
    );
    Ok(p)
}

/// A purchase line: its own fields, what it is linked to and what is left of it.
pub(crate) fn purchase_row(conn: &Connection, id: i64) -> Result<Value> {
    let row = conn
        .query_row(
            "SELECT source, source_key, shop, merchant, order_no, order_url, product_url, shop_sku,
                    name, brand, category, ordered_at, delivered_at, qty, paid, currency,
                    billed_to, status, bucket, dismissed, why, raw, same_as, imported_at, pack,
                    period, about_id
               FROM purchases WHERE id = ?1",
            [id],
            |r| {
                let paid: Option<i64> = r.get(14)?;
                Ok(json!({
                    "id": id,
                    "source": r.get::<_, String>(0)?,
                    "source_key": r.get::<_, String>(1)?,
                    "shop": r.get::<_, Option<String>>(2)?,
                    "merchant": r.get::<_, Option<String>>(3)?,
                    "order_no": r.get::<_, Option<String>>(4)?,
                    "order_url": r.get::<_, Option<String>>(5)?,
                    "product_url": r.get::<_, Option<String>>(6)?,
                    "shop_sku": r.get::<_, Option<String>>(7)?,
                    "name": r.get::<_, String>(8)?,
                    "brand": r.get::<_, Option<String>>(9)?,
                    "category": r.get::<_, Option<String>>(10)?,
                    "ordered_at": r.get::<_, Option<String>>(11)?,
                    "delivered_at": r.get::<_, Option<String>>(12)?,
                    "qty": r.get::<_, i64>(13)?,
                    "pack": r.get::<_, i64>(24)?,
                    "units": r.get::<_, i64>(13)? * r.get::<_, i64>(24)?,
                    "paid": paid.map(money),
                    "currency": r.get::<_, Option<String>>(15)?,
                    "billed_to": r.get::<_, Option<String>>(16)?,
                    "status": r.get::<_, String>(17)?,
                    "bucket": r.get::<_, String>(18)?,
                    "dismissed": r.get::<_, Option<String>>(19)?,
                    "why": r.get::<_, Option<String>>(20)?,
                    "raw": r.get::<_, Option<String>>(21)?,
                    "same_as": r.get::<_, Option<i64>>(22)?,
                    "imported_at": r.get::<_, String>(23)?,
                    "period": r.get::<_, Option<String>>(25)?,
                    "about_id": r.get::<_, Option<i64>>(26)?,
                }))
            },
        )
        .optional()?;
    let mut p = row.ok_or_else(|| not_found("no_purchase_with_id", json!({ "id": id })))?;
    // The record a payment is about (spec/ak.md), not one of its units.
    p["about"] = match p["about_id"].as_i64() {
        Some(n) => {
            serde_json::to_value(brief(conn, n)?).map_err(|e| Error::Internal(e.to_string()))?
        }
        None => Value::Null,
    };
    if let Some(o) = p.as_object_mut() {
        o.remove("about_id");
    }
    let mut stmt = conn.prepare(
        "SELECT node_id, qty FROM purchase_links WHERE purchase_id = ?1 ORDER BY node_id",
    )?;
    let links = stmt
        .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let linked: i64 = links.iter().map(|(_, q)| q).sum();
    p["linked"] = json!(
        links
            .iter()
            .map(|(n, q)| Ok(json!({ "node": brief(conn, *n)?, "qty": q })))
            .collect::<Result<Vec<_>>>()?
    );
    // A kit bought as this line settles it (spec/kit-purchase.md): its parts are the units.
    let kits: Vec<String> = {
        let mut stmt =
            conn.prepare_cached("SELECT name FROM kits WHERE purchase_id = ?1 ORDER BY name")?;
        stmt.query_map([id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    // A line joined to another (the same purchase seen by a second source) is settled through
    // that line: nothing of it is left open.
    // So does a coverage bought as this line (an extended warranty sold on its own).
    let coverages = ids(
        conn,
        "SELECT id FROM coverages WHERE purchase_id = ?1 ORDER BY id",
        [id],
    )?;
    if !coverages.is_empty() {
        p["coverages"] = json!(coverages);
    }
    // What is never a thing (digital, service), or went back to the shop, waits for nothing.
    let waits =
        !matches!(p["bucket"].as_str(), Some("digital" | "service")) && p["status"] == "delivered";
    p["open_qty"] = if waits && p["same_as"].is_null() && kits.is_empty() && coverages.is_empty() {
        json!((p["units"].as_i64().unwrap_or(0) - linked).max(0))
    } else {
        json!(0)
    };
    if !kits.is_empty() {
        p["kits"] = json!(kits);
    }
    p["joined"] = json!(ids(
        conn,
        "SELECT id FROM purchases WHERE same_as = ?1 ORDER BY id",
        [id]
    )?);
    // A joined line names the line it joins and what that is linked to, so the way back from
    // a source's key to the thing is one call (spec/ak.md).
    if let Some(t) = p["same_as"].as_i64() {
        let (source, key): (String, String) = conn.query_row(
            "SELECT source, source_key FROM purchases WHERE id = ?1",
            [t],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let mut stmt = conn.prepare_cached(
            "SELECT node_id, qty FROM purchase_links WHERE purchase_id = ?1 ORDER BY node_id",
        )?;
        let linked = stmt
            .query_map([t], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .map(|(n, q)| Ok(json!({ "node": brief(conn, n)?, "qty": q })))
            .collect::<Result<Vec<_>>>()?;
        p["joined_to"] = json!({ "id": t, "source": source, "source_key": key, "linked": linked });
    }
    // The things the person said it is not.
    let mut stmt = conn.prepare(
        "SELECT node_id, why FROM purchase_declines WHERE purchase_id = ?1 ORDER BY node_id",
    )?;
    let declined = stmt
        .query_map([id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    p["declined"] = json!(
        declined
            .into_iter()
            .map(|(n, why)| Ok(json!({ "node": brief(conn, n)?, "why": why })))
            .collect::<Result<Vec<_>>>()?
    );
    Ok(p)
}

/// A line as `buy list` prints it (spec/output.md): its own fields with the adapter's key, what
/// it is linked to, and how many attachments of each type and documents came with it; the raw
/// file, the order and product pages and the attachments and documents themselves are
/// `buy show`'s.
fn list_row(conn: &Connection, id: i64) -> Result<Value> {
    let mut p = purchase_row(conn, id)?;
    if let Value::Object(m) = &mut p {
        for k in ["merchant", "order_url", "product_url", "raw", "imported_at"] {
            m.remove(k);
        }
    }
    let mut stmt = conn.prepare_cached(
        "SELECT kind, COUNT(*) FROM purchase_attachments
          WHERE purchase_id = ?1 OR purchase_id IN (SELECT id FROM purchases WHERE same_as = ?1)
          GROUP BY kind ORDER BY kind",
    )?;
    let counts = stmt
        .query_map([id], |r| {
            Ok((r.get::<_, String>(0)?, json!(r.get::<_, i64>(1)?)))
        })?
        .collect::<rusqlite::Result<serde_json::Map<String, Value>>>()?;
    p["attachments"] = Value::Object(counts);
    p["documents"] = json!(conn.query_row(
        "SELECT COUNT(DISTINCT document_id) FROM document_links
          WHERE target = 'purchase'
            AND (target_id = ?1 OR target_id IN (SELECT id FROM purchases WHERE same_as = ?1))",
        [id],
        |r| r.get::<_, i64>(0),
    )?);
    Ok(p)
}

/// The purchase lines linked to a node, newest first, each with the quantity linked to it.
pub(crate) fn purchases_of(conn: &Connection, node: i64) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, l.qty FROM purchases p JOIN purchase_links l ON l.purchase_id = p.id
          WHERE l.node_id = ?1 ORDER BY COALESCE(p.delivered_at, p.ordered_at) DESC, p.id",
    )?;
    let rows = stmt
        .query_map([node], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out = rows
        .into_iter()
        .map(|(id, q)| {
            let mut p = purchase_json(conn, id)?;
            if let Some(o) = p.as_object_mut() {
                o.remove("linked");
                o.remove("documents");
                o.remove("raw");
            }
            p["linked_qty"] = json!(q);
            // What the linked part of the line cost, in today's money (spec §3.9).
            with_today(conn, &mut p, q)?;
            Ok(p)
        })
        .collect::<Result<Vec<_>>>()?;
    // The line of a kit this record is a part of (spec/kit-purchase.md): the set's purchase,
    // named by the kit, with no share of its own and no price of its own.
    for (id, kit) in kit_purchases_of(conn, node)? {
        if out.iter().any(|p| p["id"] == id) {
            continue;
        }
        let mut p = purchase_json(conn, id)?;
        if let Some(o) = p.as_object_mut() {
            o.remove("linked");
            o.remove("documents");
            o.remove("raw");
        }
        p["kit"] = json!(kit);
        out.push(p);
    }
    Ok(out)
}

/// The purchase lines of the kits a record is a part of, with each kit's name.
pub(crate) fn kit_purchases_of(conn: &Connection, node: i64) -> Result<Vec<(i64, String)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT DISTINCT k.purchase_id, k.name FROM kit_links l JOIN kits k ON k.id = l.kit_id
          WHERE l.node_id = ?1 AND k.purchase_id IS NOT NULL ORDER BY k.name",
    )?;
    stmt.query_map([node], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()
        .map_err(Into::into)
}

/// Adds `today`: what `qty` units of the line cost, in today's home money, when it can be
/// computed.
fn with_today(conn: &Connection, p: &mut Value, qty: i64) -> Result<()> {
    if let (Some(paid), Some(of)) = (
        p["paid"].as_str().and_then(|x| parse_money(x).ok()),
        p["units"].as_i64().filter(|q| *q > 0),
    ) {
        let date = p["delivered_at"].as_str().or(p["ordered_at"].as_str());
        if let Some(t) =
            crate::money::today_money(conn, paid * qty / of, p["currency"].as_str(), date)?
        {
            p["today"] = t;
        }
    }
    Ok(())
}

/// One normalized purchase line, checked.
struct Line {
    source: String,
    key: String,
    fields: [(&'static str, Option<String>); 14],
    /// The ev record an ak payment is about (spec/ak.md): tied or linked on import.
    thing: Option<i64>,
    qty: i64,
    paid: Option<i64>,
    status: String,
    bucket: String,
    /// The line said its bucket; when it did not, a bucket set before (`ev buy bucket`) stays.
    bucket_said: bool,
}

/// Which lines `buy_list_where` keeps: each filter given narrows them, none keeps every line.
#[derive(Clone, Copy, Default)]
pub struct BuyFilter<'a> {
    /// Only lines with something left to link, not dismissed.
    pub open: bool,
    pub bucket: Option<&'a str>,
    /// Only lines whose shop holds it, compared folded.
    pub shop: Option<&'a str>,
    /// Only lines ordered (else delivered) on this day or later.
    pub since: Option<&'a str>,
    /// Only lines where every word of it is in the name, shop, brand, product code, order
    /// number or account, compared folded.
    pub query: Option<&'a str>,
    /// Only lines billed to an account holding it (an Apple ID of a family member), folded.
    pub billed_to: Option<&'a str>,
    /// Only lines from this source (`ak`, a shop's importer), exactly.
    pub source: Option<&'a str>,
    /// Only the line keyed so and the lines keyed as its parts: `412` keeps `412` and `412.2`
    /// (one of ak's payments and its items, spec/ak.md), `412.2` that item alone.
    pub key: Option<&'a str>,
    /// Only dismissed lines: `Some(None)` any of them, `Some(Some(reason))` those dismissed so
    /// (`elsewhere`: what belongs in ak, spec/ak.md).
    pub dismissed: Option<Option<&'a str>>,
    /// Only lines bought in this month (`2025-03`), by `ordered_at`, else `delivered_at`.
    pub month: Option<&'a str>,
    /// Only lines paid in this currency.
    pub currency: Option<&'a str>,
    /// The dearest first instead of the newest.
    pub by_paid: bool,
}

/// The fields of a purchase line ev reads besides `LINE_FIELDS`.
const PURCHASE_KEYS: [&str; 9] = [
    "type", "source", "key", "name", "status", "bucket", "qty", "paid", "thing",
];

const LINE_FIELDS: [(&str, &str); 14] = [
    ("period", "period"),
    ("shop", "shop"),
    ("merchant", "merchant"),
    ("order_no", "order"),
    ("order_url", "order_url"),
    ("product_url", "product_url"),
    ("shop_sku", "sku"),
    ("brand", "brand"),
    ("category", "category"),
    ("ordered_at", "ordered_at"),
    ("delivered_at", "delivered_at"),
    ("currency", "currency"),
    ("billed_to", "billed_to"),
    ("raw", "raw"),
];

fn line_from(v: &Value) -> Result<Option<(Line, String)>> {
    let source =
        text(v, "source").ok_or_else(|| usage("field_required", json!({ "field": "source" })))?;
    let key = text(v, "key").ok_or_else(|| usage("field_required", json!({ "field": "key" })))?;
    let name =
        text(v, "name").ok_or_else(|| usage("field_required", json!({ "field": "name" })))?;
    let status = text(v, "status").unwrap_or_else(|| "delivered".into());
    let bucket_said = text(v, "bucket").is_some();
    let bucket = text(v, "bucket").unwrap_or_else(|| "durable".into());
    // Cancelled lines and consumables are not imported (spec §3.2, §11).
    if status == "cancelled" || bucket == "consumable" {
        return Ok(None);
    }
    if !["delivered", "returned"].contains(&status.as_str()) {
        return Err(usage(
            "purchase_status_unknown",
            json!({ "status": status }),
        ));
    }
    if !BUCKETS.contains(&bucket.as_str()) {
        return Err(usage(
            "purchase_import_bucket_unknown",
            json!({ "bucket": bucket, "buckets": BUCKETS.join(", ") }),
        ));
    }
    // A count as a number or as the text a receipt printed (`"2"`, as ak sends it); anything
    // else is refused rather than read as 1.
    let qty = match v.get("qty") {
        None | Some(Value::Null) => 1,
        Some(q) => q
            .as_i64()
            .or_else(|| q.as_str().and_then(|s| s.trim().parse::<i64>().ok()))
            .ok_or_else(|| usage("purchase_qty_not_a_count", json!({ "qty": q.to_string() })))?,
    };
    if qty < 1 {
        return Err(usage("qty_below_one", Value::Null));
    }
    let paid = text(v, "paid").map(|p| parse_money(&p)).transpose()?;
    let mut fields = LINE_FIELDS.map(|(col, k)| (col, text(v, k)));
    for (col, d) in fields.iter_mut() {
        if col.ends_with("_at") {
            *d = date(d)?;
        }
        if *col == "currency" {
            *d = d.as_deref().map(crate::money::currency_code).transpose()?;
        }
    }
    Ok(Some((
        Line {
            source,
            key,
            fields,
            thing: v.get("thing").and_then(|t| {
                t.as_i64().or_else(|| {
                    t.as_str()
                        .and_then(|s| s.trim_start_matches('#').parse().ok())
                })
            }),
            qty,
            paid,
            status,
            bucket,
            bucket_said,
        },
        name,
    )))
}

/// Inserts or updates one line; never touches its links, dismissal or `same_as`.
/// Returns its id and whether it was `new`, `updated` or `unchanged`.
fn upsert(conn: &Connection, l: &Line, name: &str) -> Result<(i64, &'static str)> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM purchases WHERE source = ?1 AND source_key = ?2",
            params![l.source, l.key],
            |r| r.get(0),
        )
        .optional()?;
    let cols: Vec<&str> = l.fields.iter().map(|(c, _)| *c).collect();
    let vals: Vec<Option<String>> = l.fields.iter().map(|(_, v)| v.clone()).collect();
    match existing {
        None => {
            let sql = format!(
                "INSERT INTO purchases (source, source_key, name, qty, paid, status, bucket, {},
                                        imported_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, {}, ?{n}, ?{n})",
                cols.join(", "),
                (8..8 + cols.len())
                    .map(|i| format!("?{i}"))
                    .collect::<Vec<_>>()
                    .join(", "),
                n = 8 + cols.len()
            );
            let mut p: Vec<Box<dyn rusqlite::ToSql>> = vec![
                Box::new(l.source.clone()),
                Box::new(l.key.clone()),
                Box::new(name.to_string()),
                Box::new(l.qty),
                Box::new(l.paid),
                Box::new(l.status.clone()),
                Box::new(l.bucket.clone()),
            ];
            p.extend(
                vals.into_iter()
                    .map(|v| Box::new(v) as Box<dyn rusqlite::ToSql>),
            );
            p.push(Box::new(now()));
            conn.execute(
                &sql,
                rusqlite::params_from_iter(p.iter().map(|b| b.as_ref())),
            )?;
            Ok((conn.last_insert_rowid(), "new"))
        }
        Some(id) => {
            let before = purchase_json(conn, id)?;
            let sql = format!(
                "UPDATE purchases SET name = ?1, qty = ?2, paid = ?3, status = ?4,
                   bucket = COALESCE(?5, bucket), {}
                 WHERE id = ?{}",
                cols.iter()
                    .enumerate()
                    .map(|(i, c)| format!("{c} = ?{}", i + 6))
                    .collect::<Vec<_>>()
                    .join(", "),
                6 + cols.len()
            );
            let mut p: Vec<Box<dyn rusqlite::ToSql>> = vec![
                Box::new(name.to_string()),
                Box::new(l.qty),
                Box::new(l.paid),
                Box::new(l.status.clone()),
                Box::new(l.bucket_said.then(|| l.bucket.clone())),
            ];
            p.extend(
                vals.into_iter()
                    .map(|v| Box::new(v) as Box<dyn rusqlite::ToSql>),
            );
            p.push(Box::new(id));
            conn.execute(
                &sql,
                rusqlite::params_from_iter(p.iter().map(|b| b.as_ref())),
            )?;
            let after = purchase_json(conn, id)?;
            if before == after {
                Ok((id, "unchanged"))
            } else {
                conn.execute(
                    "UPDATE purchases SET updated_at = ?1 WHERE id = ?2",
                    params![now(), id],
                )?;
                Ok((id, "updated"))
            }
        }
    }
}

fn link_doc_to_purchase(conn: &Connection, doc: i64, purchase: i64) -> Result<bool> {
    Ok(conn.execute(
        "INSERT OR IGNORE INTO document_links (document_id, target, target_id, at)
         VALUES (?1, 'purchase', ?2, ?3)",
        params![doc, purchase, now()],
    )? > 0)
}

fn check_open(conn: &Connection, id: i64, node: i64, qty: i64) -> Result<()> {
    let p = purchase_json(conn, id)?;
    if p["dismissed"].is_string() {
        return Err(refuse(
            "purchase_dismissed",
            json!({ "id": id, "as": p["dismissed"] }),
            Value::Null,
        ));
    }
    let already: i64 = conn
        .query_row(
            "SELECT qty FROM purchase_links WHERE purchase_id = ?1 AND node_id = ?2",
            params![id, node],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    let open = p["open_qty"].as_i64().unwrap_or(0) + already;
    if qty > open {
        return Err(refuse(
            "purchase_not_enough_open",
            json!({ "id": id, "open": open, "qty": qty }),
            Value::Null,
        ));
    }
    Ok(())
}

impl Inventory {
    /// Imports NDJSON from an adapter (spec §6): `purchase` lines, and `document` lines that hang
    /// on purchases of the same source by their keys. All or nothing; importing the same lines
    /// again changes nothing.
    pub fn buy_import(&mut self, ndjson: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let mut counts = std::collections::BTreeMap::<&str, i64>::new();
        // Fields of a purchase line that ev does not read: kept nowhere, so the adapter is told
        // (a typo such as `orderd_at` would otherwise lose a date without a word).
        let mut unknown = std::collections::BTreeMap::<String, i64>::new();
        let mut docs_added = 0;
        let mut things_unknown: Vec<Value> = Vec::new();
        let mut now_cancelled: Vec<i64> = Vec::new();
        let mut cancelled_held: Vec<i64> = Vec::new();
        for (i, raw) in ndjson.lines().enumerate() {
            let raw = raw.trim();
            if raw.is_empty() {
                continue;
            }
            let at = |e: Error| e.at_line(i + 1);
            let v: Value = serde_json::from_str(raw)
                .map_err(|e| usage("line_not_json", json!({ "error": e.to_string() })))
                .map_err(at)?;
            match v.get("type").and_then(Value::as_str).unwrap_or("purchase") {
                "purchase" => {
                    for k in v.as_object().into_iter().flat_map(|o| o.keys()) {
                        if !PURCHASE_KEYS.contains(&k.as_str())
                            && !LINE_FIELDS.iter().any(|(_, f)| f == k)
                        {
                            *unknown.entry(k.clone()).or_default() += 1;
                        }
                    }
                    // An order the shop cancelled after it was imported: the line ev has is
                    // settled as cancelled, or named when the person's word stands in the way.
                    if text(&v, "status").as_deref() == Some("cancelled")
                        && let (Some(source), Some(key)) = (text(&v, "source"), text(&v, "key"))
                        && let Some(id) = tx
                            .query_row(
                                "SELECT id FROM purchases WHERE source = ?1 AND source_key = ?2",
                                params![source, key],
                                |r| r.get::<_, i64>(0),
                            )
                            .optional()?
                    {
                        match cancel_line(&tx, id)? {
                            Cancelled::Now => now_cancelled.push(id),
                            Cancelled::Already => *counts.entry("skipped").or_default() += 1,
                            Cancelled::Held => cancelled_held.push(id),
                        }
                        continue;
                    }
                    let line = line_from(&v).map_err(at)?;
                    match line {
                        None => *counts.entry("skipped").or_default() += 1,
                        Some((l, name)) => {
                            let (id, how) = upsert(&tx, &l, &name).map_err(at)?;
                            *counts.entry(how).or_default() += 1;
                            // A payment about a thing (spec/ak.md): tied, or linked, once.
                            if let Some(thing) = l.thing
                                && !tie_to_thing(&tx, id, thing)?
                            {
                                things_unknown.push(json!({ "key": l.key, "thing": thing }));
                            }
                        }
                    }
                }
                "document" => {
                    let source = text(&v, "source")
                        .ok_or_else(|| usage("field_required", json!({ "field": "source" })))
                        .map_err(at)?;
                    let file = text(&v, "file")
                        .ok_or_else(|| usage("field_required", json!({ "field": "file" })))
                        .map_err(at)?;
                    let keys: Vec<String> = v
                        .get("purchases")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|k| k.as_str().map(str::to_string))
                        .collect();
                    let new = NewDoc {
                        kind: text(&v, "kind").unwrap_or_else(|| "invoice".into()),
                        number: text(&v, "number"),
                        ettn: text(&v, "ettn"),
                        issued: text(&v, "issued"),
                        issuer: text(&v, "issuer"),
                        note: text(&v, "note"),
                    };
                    // A document may name lines that were not imported (consumables); one that
                    // names none of the imported lines is not stored at all.
                    let mut targets = Vec::new();
                    for k in keys {
                        let p: Option<i64> = tx
                            .query_row(
                                "SELECT id FROM purchases WHERE source = ?1 AND source_key = ?2",
                                params![source, k],
                                |r| r.get(0),
                            )
                            .optional()?;
                        targets.extend(p);
                    }
                    if targets.is_empty() {
                        *counts.entry("documents_skipped").or_default() += 1;
                        continue;
                    }
                    let (doc, _) = crate::docs::store_doc(
                        &tx,
                        &self.doc_dir,
                        std::path::Path::new(&file),
                        &new,
                    )
                    .map_err(at)?;
                    for p in targets {
                        if link_doc_to_purchase(&tx, doc, p)? {
                            docs_added += 1;
                        }
                    }
                }
                kind if crate::attachments::ATTACHMENT_KINDS.contains(&kind) => {
                    let source = text(&v, "source")
                        .ok_or_else(|| usage("field_required", json!({ "field": "source" })))
                        .map_err(at)?;
                    let data = crate::attachments::attachment_data(kind, &v).map_err(at)?;
                    let mut keys: Vec<String> = v
                        .get("purchases")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|k| k.as_str().map(str::to_string))
                        .collect();
                    keys.extend(text(&v, "purchase"));
                    let mut any = false;
                    for k in keys {
                        let p: Option<i64> = tx
                            .query_row(
                                "SELECT id FROM purchases WHERE source = ?1 AND source_key = ?2",
                                params![source, k],
                                |r| r.get(0),
                            )
                            .optional()?;
                        if let Some(p) = p {
                            any = true;
                            if crate::attachments::attach(&tx, p, kind, &data)? {
                                *counts.entry("attachments").or_default() += 1;
                            }
                        }
                    }
                    if !any {
                        *counts.entry("attachments_skipped").or_default() += 1;
                    }
                }
                other => {
                    return Err(
                        usage("purchase_line_type_unknown", json!({ "type": other }))
                            .at_line(i + 1),
                    );
                }
            }
        }
        let (relayed, unjoined) = crate::attachments::join_relayed(&tx)?;
        let joined = crate::attachments::join_same(&tx)? + relayed;
        tx.commit()?;
        let mut v = json!({
            "imported": {
                "new": counts.get("new").copied().unwrap_or(0),
                "updated": counts.get("updated").copied().unwrap_or(0),
                "unchanged": counts.get("unchanged").copied().unwrap_or(0),
                "skipped": counts.get("skipped").copied().unwrap_or(0),
                "document_links": docs_added,
                "documents_skipped": counts.get("documents_skipped").copied().unwrap_or(0),
                "attachments": counts.get("attachments").copied().unwrap_or(0),
                "attachments_skipped": counts.get("attachments_skipped").copied().unwrap_or(0),
                "joined": joined,
            }
        });
        if !unknown.is_empty() {
            v["imported"]["unknown_fields"] = json!(unknown);
        }
        if !things_unknown.is_empty() {
            v["imported"]["things_unknown"] = json!(things_unknown);
        }
        if !now_cancelled.is_empty() {
            v["imported"]["now_cancelled"] = json!(now_cancelled);
        }
        if !cancelled_held.is_empty() {
            v["imported"]["cancelled_held"] = json!(cancelled_held);
        }
        // Relayed lines of an order ev has several lines of, none of them told: for the person.
        if !unjoined.is_empty() {
            v["imported"]["unjoined"] = json!(unjoined);
        }
        Ok(v)
    }

    /// A purchase entered by hand (bought in a shop, a gift); linked to `for_ref` at once when
    /// given.
    pub fn buy_add(&mut self, line: &Value, for_ref: Option<&str>) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let mut v = line.clone();
        v["source"] = json!("manual");
        // Paid by hand with no currency said: the home one, as everywhere else.
        if v["paid"].is_string() && v["currency"].as_str().is_none_or(|c| c.trim().is_empty()) {
            v["currency"] = json!(crate::money::home_currency(&tx)?);
        }
        let next: i64 =
            tx.query_row("SELECT COALESCE(MAX(id), 0) + 1 FROM purchases", [], |r| {
                r.get(0)
            })?;
        v["key"] = json!(format!("manual-{next}"));
        // Recorded by hand, a purchase was made: its date is not one still to come.
        if let Some(d) = v["ordered_at"].as_str()
            && d.get(..10).unwrap_or(d) > crate::store::today().to_string().as_str()
        {
            return Err(usage("date_still_to_come", json!({ "date": d })));
        }
        if v["bucket"] == "consumable" {
            return Err(usage(
                "purchase_consumable_not_recorded",
                json!({ "buckets": BUCKETS.join(", ") }),
            ));
        }
        let (l, name) =
            line_from(&v)?.ok_or_else(|| usage("purchase_manual_cancelled", Value::Null))?;
        let (id, _) = upsert(&tx, &l, &name)?;
        let pack = v.get("pack").and_then(Value::as_i64).unwrap_or(1);
        set_pack(&tx, id, pack)?;
        if let Some(r) = for_ref {
            // A past thing's purchase too (spec/past-belongings.md).
            let node = resolve(&tx, r, true)?;
            link_in(&tx, id, node, l.qty * pack)?;
        }
        tx.commit()?;
        self.buy_show(id)
    }

    /// Corrects a line entered by hand (`source: manual`), the person's own: `field=value` for
    /// name, date (a day, or a month or a year as remembered), paid, currency, shop, brand,
    /// order and qty; an empty value clears what may be empty. A line from a source is that
    /// source's: corrected there and imported again.
    pub fn buy_edit(&mut self, id: i64, fields: &[String]) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let p = purchase_json(&tx, id)?;
        if p["source"] != "manual" {
            return Err(refuse(
                "purchase_edit_not_manual",
                json!({ "id": id, "source": p["source"] }),
                Value::Null,
            ));
        }
        if fields.is_empty() {
            return Err(usage("purchase_edit_nothing", Value::Null));
        }
        let set = |column: &str, value: rusqlite::types::Value| -> Result<()> {
            tx.execute(
                &format!("UPDATE purchases SET {column} = ?1, updated_at = ?2 WHERE id = ?3"),
                params![value, now(), id],
            )?;
            Ok(())
        };
        let text = |v: Option<String>| v.map_or(rusqlite::types::Value::Null, Into::into);
        for f in fields {
            let Some((key, value)) = f.split_once('=') else {
                return Err(usage("purchase_edit_field_bad", json!({ "field": f })));
            };
            let value = Some(value.trim().to_string()).filter(|v| !v.is_empty());
            match key.trim() {
                "name" => {
                    let name = value.ok_or_else(|| usage("name_empty", Value::Null))?;
                    set("name", name.into())?;
                }
                "date" => {
                    let d = date(&value)?;
                    if let Some(d) = &d
                        && d.as_str() > crate::store::today().to_string().as_str()
                    {
                        return Err(usage("date_still_to_come", json!({ "date": d })));
                    }
                    set("ordered_at", text(d))?;
                }
                "paid" => {
                    let cents = value.as_deref().map(parse_money).transpose()?;
                    set(
                        "paid",
                        cents.map_or(rusqlite::types::Value::Null, Into::into),
                    )?;
                }
                "currency" => {
                    let c = value
                        .as_deref()
                        .map(crate::money::currency_code)
                        .transpose()?;
                    set("currency", text(c))?;
                }
                "shop" => set("shop", text(value))?,
                "brand" => set("brand", text(value))?,
                "order" => set("order_no", text(value))?,
                "qty" => {
                    let qty = value
                        .as_deref()
                        .and_then(|q| q.parse::<i64>().ok())
                        .filter(|q| *q >= 1)
                        .ok_or_else(|| usage("qty_below_one", Value::Null))?;
                    let linked: i64 = tx.query_row(
                        "SELECT COALESCE(SUM(qty), 0) FROM purchase_links WHERE purchase_id = ?1",
                        [id],
                        |r| r.get(0),
                    )?;
                    let pack = p["pack"].as_i64().unwrap_or(1);
                    if qty * pack < linked {
                        return Err(refuse(
                            "purchase_qty_below_linked",
                            json!({ "id": id, "qty": qty, "linked": linked }),
                            Value::Null,
                        ));
                    }
                    set("qty", qty.into())?;
                }
                other => {
                    return Err(usage(
                        "purchase_edit_field_unknown",
                        json!({ "field": other,
                                "fields": "name, date, paid, currency, shop, brand, order, qty" }),
                    ));
                }
            }
        }
        tx.commit()?;
        self.buy_show(id)
    }

    /// Lines, open ones first: `open` keeps only those with something left to link and not
    /// dismissed.
    pub fn buy_list(
        &self,
        open: bool,
        bucket: Option<&str>,
        shop: Option<&str>,
        since: Option<&str>,
    ) -> Result<Value> {
        self.buy_list_matching(open, bucket, shop, since, None)
    }

    /// `buy_list`, with `query`: only the lines where every word of it is in the name, the
    /// shop, the brand, the shop's product code or the order number, compared folded. Answers
    /// "is there a purchase of X?" without a record to rank lines for.
    pub fn buy_list_matching(
        &self,
        open: bool,
        bucket: Option<&str>,
        shop: Option<&str>,
        since: Option<&str>,
        query: Option<&str>,
    ) -> Result<Value> {
        self.buy_list_where(&BuyFilter {
            open,
            bucket,
            shop,
            since,
            query,
            ..BuyFilter::default()
        })
    }

    /// The lines `filter` keeps, newest first.
    pub fn buy_list_where(&self, filter: &BuyFilter) -> Result<Value> {
        let BuyFilter {
            open,
            bucket,
            shop,
            since,
            query,
            billed_to,
            source,
            key,
            dismissed,
            month,
            currency,
            by_paid,
        } = *filter;
        let currency = currency.map(crate::money::currency_code).transpose()?;
        if let Some(Some(r)) = dismissed
            && !DISMISSALS.contains(&r)
        {
            return Err(usage(
                "purchase_reason_unknown",
                json!({ "reason": r, "reasons": DISMISSALS.join(", ") }),
            ));
        }
        let billed = billed_to.map(crate::fold);
        if let Some(b) = bucket
            && !BUCKETS.contains(&b)
        {
            return Err(usage(
                "purchase_bucket_unknown",
                json!({ "bucket": b, "buckets": BUCKETS.join(", ") }),
            ));
        }
        let words: Vec<String> = query
            .map(crate::fold)
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_string)
            .collect();
        let all = ids(
            &self.conn,
            "SELECT id FROM purchases ORDER BY COALESCE(delivered_at, ordered_at) DESC, id DESC",
            [],
        )?;
        let shop = shop.map(crate::fold);
        let mut out = Vec::new();
        for id in all {
            let p = list_row(&self.conn, id)?;
            let is_open = p["dismissed"].is_null() && p["open_qty"].as_i64().unwrap_or(0) > 0;
            if open && !is_open
                || bucket.is_some_and(|b| p["bucket"] != b)
                || source.is_some_and(|s| p["source"] != s)
                || dismissed.is_some_and(|d| match d {
                    None => p["dismissed"].is_null(),
                    Some(r) => p["dismissed"] != r,
                })
                || key.is_some_and(|k| {
                    !p["source_key"].as_str().is_some_and(|x| {
                        x == k || x.strip_prefix(k).is_some_and(|r| r.starts_with('.'))
                    })
                })
                || shop.as_ref().is_some_and(|s| {
                    !p["shop"]
                        .as_str()
                        .is_some_and(|x| crate::fold(x).contains(s.as_str()))
                })
                || billed.as_ref().is_some_and(|b| {
                    !p["billed_to"]
                        .as_str()
                        .is_some_and(|x| crate::fold(x).contains(b.as_str()))
                })
                || since.is_some_and(|d| {
                    p["ordered_at"]
                        .as_str()
                        .or(p["delivered_at"].as_str())
                        .is_none_or(|x| x < d)
                })
                || month.is_some_and(|m| {
                    !p["ordered_at"]
                        .as_str()
                        .or(p["delivered_at"].as_str())
                        .is_some_and(|x| x.starts_with(m.trim()))
                })
                || currency
                    .as_ref()
                    .is_some_and(|c| p["currency"] != c.as_str())
                || !words.is_empty() && {
                    let text = crate::fold(
                        &["name", "shop", "brand", "shop_sku", "order_no", "billed_to"]
                            .iter()
                            .filter_map(|k| p[*k].as_str())
                            .collect::<Vec<_>>()
                            .join(" "),
                    );
                    !words.iter().all(|w| text.contains(w.as_str()))
                }
            {
                continue;
            }
            out.push(p);
        }
        // The dearest first; lines with no amount last, newest first among equals.
        if by_paid {
            let cents = |p: &Value| {
                p["paid"]
                    .as_str()
                    .and_then(|x| parse_money(x).ok())
                    .unwrap_or(i64::MIN)
            };
            out.sort_by_key(|p| std::cmp::Reverse(cents(p)));
        }
        Ok(json!({ "purchases": out }))
    }

    /// `ev buy list --duplicates` (spec/purchases.md §14): open lines that look like one
    /// purchase seen twice: the same name (folded), the same amount and currency, bought within
    /// three days of each other, joined to no other line. Groups of two or more; the person
    /// decides, ev never merges.
    pub fn buy_duplicates(&self) -> Result<Value> {
        let all = ids(&self.conn, "SELECT id FROM purchases ORDER BY id", [])?;
        let mut by_key: std::collections::BTreeMap<(String, String, String), Vec<Value>> =
            std::collections::BTreeMap::new();
        for id in all {
            let p = list_row(&self.conn, id)?;
            let open = p["dismissed"].is_null() && p["open_qty"].as_i64().unwrap_or(0) > 0;
            if !open || !p["same_as"].is_null() {
                continue;
            }
            let key = (
                crate::fold(p["name"].as_str().unwrap_or_default()),
                p["paid"].as_str().unwrap_or_default().to_string(),
                p["currency"].as_str().unwrap_or_default().to_string(),
            );
            by_key.entry(key).or_default().push(p);
        }
        let day = |p: &Value| {
            p["ordered_at"]
                .as_str()
                .or(p["delivered_at"].as_str())
                .and_then(|d| chrono::NaiveDate::parse_from_str(d.get(..10)?, "%Y-%m-%d").ok())
        };
        let mut groups = Vec::new();
        for (_, mut lines) in by_key {
            if lines.len() < 2 {
                continue;
            }
            lines.sort_by_key(|p| day(p));
            // Lines a few days apart are one purchase seen twice; a month apart, two purchases.
            let mut group: Vec<Value> = Vec::new();
            for p in lines {
                let close = group.last().is_some_and(|q| match (day(q), day(&p)) {
                    (Some(a), Some(b)) => (b - a).num_days() <= 3,
                    _ => false,
                });
                if !close && group.len() > 1 {
                    groups.push(json!(std::mem::take(&mut group)));
                } else if !close {
                    group.clear();
                }
                group.push(p);
            }
            if group.len() > 1 {
                groups.push(json!(group));
            }
        }
        Ok(json!({ "duplicates": groups }))
    }

    /// How many lines each bucket has, every line counted: what `buy_list` returns for it with
    /// no filter, without reading every line (a sidebar's counts on every change).
    pub fn buy_counts(&self) -> Result<Value> {
        let mut stmt = self
            .conn
            .prepare("SELECT bucket, COUNT(*) FROM purchases GROUP BY bucket")?;
        let counts = stmt
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, json!(r.get::<_, i64>(1)?)))
            })?
            .collect::<rusqlite::Result<serde_json::Map<String, Value>>>()?;
        Ok(Value::Object(counts))
    }

    pub fn buy_show(&self, id: i64) -> Result<Value> {
        let mut p = purchase_json(&self.conn, id)?;
        let units = p["units"].as_i64().unwrap_or(1);
        with_today(&self.conn, &mut p, units)?;
        Ok(json!({ "purchase": p }))
    }

    /// Links a line to a node on the person's word, for `qty` of it (all that is open by
    /// default), and remembers the product for the next purchase of it.
    pub fn buy_link(&mut self, id: i64, reference: &str, qty: Option<i64>) -> Result<Value> {
        let tx = self.conn.transaction()?;
        // A gone record too: a past thing's purchase is settled by it (spec/past-belongings.md).
        let node = resolve(&tx, reference, true)?;
        let p = purchase_json(&tx, id)?;
        let already: i64 = tx
            .query_row(
                "SELECT qty FROM purchase_links WHERE purchase_id = ?1 AND node_id = ?2",
                params![id, node],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        let left = p["open_qty"].as_i64().unwrap_or(0) + already;
        // A line in packs (a set, an 8-pack) is split among things: by default a thing takes as
        // many units as it stands for (one body, an 8-pack recorded as ×8), never the whole line.
        let default = if p["pack"].as_i64().unwrap_or(1) > 1 {
            let n = crate::store::load(&tx, node)?;
            left.min(crate::portions::units(&n))
        } else {
            left
        };
        if qty.is_some_and(|q| q < 1) {
            return Err(usage("qty_below_one", Value::Null));
        }
        let qty = qty.unwrap_or(default);
        if qty < 1 {
            // A service or a download is never a thing to link.
            return Err(match p["bucket"].as_str() {
                Some(b @ ("digital" | "service")) => refuse(
                    "purchase_never_a_thing",
                    json!({ "id": id, "bucket": b }),
                    Value::Null,
                ),
                _ => refuse("purchase_nothing_open", json!({ "id": id }), Value::Null),
            });
        }
        link_in(&tx, id, node, qty)?;
        tx.commit()?;
        self.buy_show(id)
    }

    /// Sets how many units each bought quantity of a line holds (an 8-pack, a set), so its
    /// units can be linked to several things.
    pub fn buy_pack(&mut self, id: i64, pack: i64) -> Result<Value> {
        let tx = self.conn.transaction()?;
        set_pack(&tx, id, pack)?;
        tx.commit()?;
        self.buy_show(id)
    }

    /// Says what kind of purchase a line is (`BUCKETS`). A line linked to a thing stays a thing's
    /// purchase: it is not made digital or a service while linked.
    pub fn buy_bucket(&mut self, id: i64, bucket: &str) -> Result<Value> {
        let bucket = bucket.trim().to_lowercase();
        if !BUCKETS.contains(&bucket.as_str()) {
            return Err(usage(
                "purchase_bucket_unknown",
                json!({ "bucket": bucket, "buckets": BUCKETS.join(", ") }),
            ));
        }
        let p = purchase_json(&self.conn, id)?;
        if p["bucket"] == bucket.as_str() {
            return Err(refuse(
                "purchase_already_bucket",
                json!({ "id": id, "bucket": bucket }),
                Value::Null,
            ));
        }
        let linked = p["linked"].as_array().is_some_and(|l| !l.is_empty());
        if linked && matches!(bucket.as_str(), "digital" | "service") {
            return Err(refuse(
                "purchase_linked_unlink_first",
                json!({ "id": id }),
                Value::Null,
            ));
        }
        self.conn.execute(
            "UPDATE purchases SET bucket = ?1, updated_at = ?2 WHERE id = ?3",
            params![bucket, now(), id],
        )?;
        self.buy_show(id)
    }

    /// `ev buy about <line> <ref>`: a service line (a tax, an insurance premium, a repair bill)
    /// is about this record without being one of its units (spec/ak.md); `clear` unties it.
    pub fn buy_about(&mut self, id: i64, reference: Option<&str>, clear: bool) -> Result<Value> {
        let p = purchase_json(&self.conn, id)?;
        if clear {
            self.conn.execute(
                "UPDATE purchases SET about_id = NULL, updated_at = ?1 WHERE id = ?2",
                params![now(), id],
            )?;
            return self.buy_show(id);
        }
        if !matches!(p["bucket"].as_str(), Some("digital" | "service")) {
            return Err(refuse(
                "purchase_about_needs_service",
                json!({ "id": id, "bucket": p["bucket"] }),
                Value::Null,
            ));
        }
        let reference =
            reference.ok_or_else(|| usage("purchase_about_needs_thing", Value::Null))?;
        let node = resolve(&self.conn, reference, true)?;
        self.conn.execute(
            "UPDATE purchases SET about_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![node, now(), id],
        )?;
        self.buy_show(id)
    }

    pub fn buy_unlink(&mut self, id: i64, reference: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let node = resolve(&tx, reference, true)?;
        let removed = tx.execute(
            "DELETE FROM purchase_links WHERE purchase_id = ?1 AND node_id = ?2",
            params![id, node],
        )?;
        if removed == 0 {
            return Err(refuse(
                "purchase_not_linked_to",
                json!({ "id": id, "node": node }),
                Value::Null,
            ));
        }
        event(&tx, node, "purchase_unlinked", json!({ "purchase": id }))?;
        // A wrong link's product pictures and pages show another product: they go with it.
        let (taken, left) = crate::attachments::take_back(&tx, &self.doc_dir, id, node)?;
        tx.commit()?;
        let mut v = self.buy_show(id)?;
        if !taken.is_empty() {
            v["taken_back"] = json!(taken);
        }
        if !left.is_empty() {
            v["left"] = json!(left);
        }
        Ok(v)
    }

    /// Settles a line that will never be a node, or (`reason` None) clears that.
    pub fn buy_dismiss(
        &mut self,
        id: i64,
        reason: Option<&str>,
        why: Option<&str>,
    ) -> Result<Value> {
        purchase_json(&self.conn, id)?;
        if let Some(r) = reason
            && !DISMISSALS.contains(&r)
        {
            return Err(usage(
                "purchase_reason_unknown",
                json!({ "reason": r, "reasons": DISMISSALS.join(", ") }),
            ));
        }
        self.conn.execute(
            "UPDATE purchases SET dismissed = ?1, why = ?2 WHERE id = ?3",
            params![reason, why.map(str::trim).filter(|w| !w.is_empty()), id],
        )?;
        self.buy_show(id)
    }

    /// Joins line `id` to `into` on the person's word: the same purchase seen by two sources, as
    /// an import joins what it can tell (an ak line listed `unjoined`, spec/ak.md). `id` then
    /// counts as settled through `into`, which is the line linked to a thing. `None` takes a
    /// join back. Refused for a line joined to itself, to a line that joins another (join to
    /// that one), for a line linked to a thing (unlink it first) or one others join.
    pub fn buy_join(&mut self, id: i64, into: Option<i64>) -> Result<Value> {
        let tx = self.conn.transaction()?;
        purchase_json(&tx, id)?;
        if let Some(t) = into {
            let other = purchase_json(&tx, t)?;
            if t == id {
                return Err(refuse(
                    "purchase_join_itself",
                    json!({ "id": id }),
                    Value::Null,
                ));
            }
            if let Some(kept) = other["same_as"].as_i64() {
                return Err(refuse(
                    "purchase_join_to_joined",
                    json!({ "into": t, "kept": kept }),
                    Value::Null,
                ));
            }
            let count = |sql: &str| -> Result<i64> { Ok(tx.query_row(sql, [id], |r| r.get(0))?) };
            if count("SELECT COUNT(*) FROM purchase_links WHERE purchase_id = ?1")? > 0 {
                return Err(refuse(
                    "purchase_join_linked",
                    json!({ "id": id }),
                    Value::Null,
                ));
            }
            if count("SELECT COUNT(*) FROM purchases WHERE same_as = ?1")? > 0 {
                return Err(refuse(
                    "purchase_join_kept",
                    json!({ "id": id }),
                    Value::Null,
                ));
            }
        }
        tx.execute(
            "UPDATE purchases SET same_as = ?1 WHERE id = ?2",
            params![into, id],
        )?;
        tx.commit()?;
        self.buy_show(id)
    }

    /// The person's "not this one": line `id` is not the thing `reference`. The line stays
    /// open for other things and is no longer offered to this one; `clear` takes it back.
    pub fn buy_decline(
        &mut self,
        id: i64,
        reference: &str,
        why: Option<&str>,
        clear: bool,
    ) -> Result<Value> {
        let tx = self.conn.transaction()?;
        purchase_json(&tx, id)?;
        // A past thing's lines are declined as its others are (spec/past-belongings.md).
        let node = resolve(&tx, reference, true)?;
        let linked: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM purchase_links WHERE purchase_id = ?1 AND node_id = ?2)",
            params![id, node],
            |r| r.get(0),
        )?;
        if linked && !clear {
            return Err(refuse(
                "line_linked_unlink_first",
                json!({ "id": id, "node": node }),
                Value::Null,
            ));
        }
        if clear {
            if tx.execute(
                "DELETE FROM purchase_declines WHERE purchase_id = ?1 AND node_id = ?2",
                params![id, node],
            )? == 0
            {
                return Err(refuse(
                    "line_not_declined",
                    json!({ "id": id, "node": node }),
                    Value::Null,
                ));
            }
            event(
                &tx,
                node,
                "purchase_decline_cleared",
                json!({ "purchase": id }),
            )?;
        } else {
            let why = why.map(str::trim).filter(|w| !w.is_empty());
            tx.execute(
                "INSERT INTO purchase_declines (purchase_id, node_id, why, at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (purchase_id, node_id) DO UPDATE SET why = excluded.why, at = excluded.at",
                params![id, node, why, now()],
            )?;
            event(
                &tx,
                node,
                "purchase_declined",
                json!({ "purchase": id, "why": why }),
            )?;
        }
        tx.commit()?;
        self.buy_show(id)
    }
}

/// A pack below 1, or one that would leave fewer units than are already linked, is refused.
fn set_pack(conn: &Connection, id: i64, pack: i64) -> Result<()> {
    if pack < 1 {
        return Err(usage("purchase_pack_below_one", Value::Null));
    }
    let p = purchase_json(conn, id)?;
    let qty = p["qty"].as_i64().unwrap_or(1);
    let linked: i64 = p["linked"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| l["qty"].as_i64())
        .sum();
    if qty * pack < linked {
        return Err(usage(
            "purchase_pack_too_small",
            json!({ "id": id, "linked": linked, "pack": pack, "units": qty * pack }),
        ));
    }
    conn.execute(
        "UPDATE purchases SET pack = ?1 WHERE id = ?2",
        params![pack, id],
    )?;
    Ok(())
}

/// What a thing costs (spec/ak.md), by currency and never converted: `bought` (its linked lines'
/// share of what was paid), `upkeep` (the service lines about it, refunds negative), `cover`
/// (premiums of its coverages), `sold` (what a sale brought). Null when nothing is known.
pub(crate) fn cost_of(conn: &Connection, node: i64) -> Result<Value> {
    let home = crate::money::home_currency(conn)?;
    let mut by: std::collections::BTreeMap<String, [i64; 4]> = std::collections::BTreeMap::new();
    let mut add = |currency: Option<String>, slot: usize, cents: i64| {
        by.entry(currency.unwrap_or_else(|| home.clone()))
            .or_insert([0; 4])[slot] += cents;
    };
    let mut stmt = conn.prepare(
        "SELECT p.paid, p.currency, l.qty, p.qty * p.pack FROM purchase_links l
           JOIN purchases p ON p.id = l.purchase_id
          WHERE l.node_id = ?1 AND p.paid IS NOT NULL",
    )?;
    let rows = stmt
        .query_map([node], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (paid, currency, linked, units) in rows {
        add(currency, 0, paid * linked / units.max(1));
    }
    let mut stmt = conn.prepare(
        "SELECT paid, currency FROM purchases
          WHERE about_id = ?1 AND paid IS NOT NULL AND dismissed IS NULL",
    )?;
    let rows = stmt
        .query_map([node], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (paid, currency) in rows {
        add(currency, 1, paid);
    }
    let mut stmt = conn.prepare(
        "SELECT c.premium, c.currency FROM coverages c
           JOIN coverage_nodes cn ON cn.coverage_id = c.id
          WHERE cn.node_id = ?1 AND c.premium IS NOT NULL",
    )?;
    let rows = stmt
        .query_map([node], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (premium, currency) in rows {
        add(currency, 2, premium);
    }
    let sale: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT price, currency FROM departures WHERE node_id = ?1 AND price IS NOT NULL",
            [node],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((Some(price), currency)) = sale
        && let Ok(cents) = parse_money(&price)
    {
        add(currency, 3, cents);
    }
    if by.is_empty() {
        return Ok(Value::Null);
    }
    let out: serde_json::Map<String, Value> = by
        .into_iter()
        .map(|(currency, [bought, upkeep, cover, sold])| {
            let mut v = serde_json::Map::new();
            for (k, cents) in [
                ("bought", bought),
                ("upkeep", upkeep),
                ("cover", cover),
                ("sold", sold),
            ] {
                if cents != 0 {
                    v.insert(k.into(), json!(money(cents)));
                }
            }
            (currency, Value::Object(v))
        })
        .collect();
    Ok(Value::Object(out))
}

/// What a re-imported `cancelled` did to a line ev already had.
enum Cancelled {
    /// Open: settled as cancelled now.
    Now,
    /// Settled as cancelled before: nothing to do.
    Already,
    /// Linked to a thing, joined, or settled otherwise by the person: left for them.
    Held,
}

/// A line ev already has whose order the shop has since cancelled: settled as `cancelled`
/// when nothing of the person's stands on it, else left as it is for them to decide.
fn cancel_line(conn: &Connection, id: i64) -> Result<Cancelled> {
    let p = purchase_json(conn, id)?;
    if p["dismissed"] == "cancelled" {
        return Ok(Cancelled::Already);
    }
    let linked = p["linked"].as_array().is_some_and(|l| !l.is_empty());
    if linked || !p["dismissed"].is_null() || !p["same_as"].is_null() {
        return Ok(Cancelled::Held);
    }
    // No `why`: the reason says it, in the reader's language.
    conn.execute(
        "UPDATE purchases SET dismissed = 'cancelled', updated_at = ?1 WHERE id = ?2",
        params![now(), id],
    )?;
    Ok(Cancelled::Now)
}

/// An imported line about `thing` (spec/ak.md): a service or a download is tied to it
/// (`about`), anything else linked with all its open units. Once only: a line already tied or
/// linked is left as the person last left it. False when ev has no such record, or it cannot
/// hold a purchase (a place, a record closed by mistake), so the import can say so.
fn tie_to_thing(conn: &Connection, id: i64, thing: i64) -> Result<bool> {
    let exists: Option<i64> = conn
        .query_row("SELECT id FROM nodes WHERE id = ?1", [thing], |r| r.get(0))
        .optional()?;
    if exists.is_none() {
        return Ok(false);
    }
    let p = purchase_json(conn, id)?;
    if matches!(p["bucket"].as_str(), Some("digital" | "service")) {
        if p["about"].is_null() {
            conn.execute(
                "UPDATE purchases SET about_id = ?1 WHERE id = ?2",
                params![thing, id],
            )?;
        }
        return Ok(true);
    }
    let linked = p["linked"].as_array().is_some_and(|l| !l.is_empty());
    let open = p["open_qty"].as_i64().unwrap_or(0);
    if linked || open < 1 {
        return Ok(true);
    }
    Ok(link_in(conn, id, thing, open).is_ok())
}

fn link_in(conn: &Connection, id: i64, node: i64, qty: i64) -> Result<()> {
    // A purchase is a thing's: never a place's, nor a record that was never a thing of its own.
    let (kind, state, how): (String, String, Option<String>) = conn.query_row(
        "SELECT kind, state, disposition FROM nodes WHERE id = ?1",
        [node],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if matches!(kind.as_str(), "home" | "room") {
        return Err(usage("purchase_link_to_place", json!({ "node": node })));
    }
    if state == "gone"
        && let Some(h @ ("mistake" | "merged" | "digitize")) = how.as_deref()
    {
        return Err(refuse(
            "left_holds_no_purchase",
            json!({ "node": node, "how": h }),
            Value::Null,
        ));
    }
    check_open(conn, id, node, qty)?;
    // A thing that left was not bought after it left.
    let left = crate::store::past::departure_json(conn, node)?;
    let bought: Option<String> = conn.query_row(
        "SELECT COALESCE(ordered_at, delivered_at) FROM purchases WHERE id = ?1",
        [id],
        |r| r.get(0),
    )?;
    if left["pending"] != true
        && let (Some(l), Some(b)) = (left["at"].as_str(), bought.as_deref())
    {
        let n = l.len().min(b.len());
        if b[..n] > l[..n] {
            return Err(refuse(
                "bought_after_left",
                json!({ "bought": b, "node": node, "left": l }),
                Value::Null,
            ));
        }
    }
    // Linked on the person's word, the line is this thing after all: an earlier "not this
    // one" no longer stands.
    conn.execute(
        "DELETE FROM purchase_declines WHERE purchase_id = ?1 AND node_id = ?2",
        params![id, node],
    )?;
    conn.execute(
        "INSERT INTO purchase_links (purchase_id, node_id, qty, at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (purchase_id, node_id) DO UPDATE SET qty = excluded.qty, at = excluded.at",
        params![id, node, qty, now()],
    )?;
    let (shop, sku): (Option<String>, Option<String>) = conn.query_row(
        "SELECT shop, shop_sku FROM purchases WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if let (Some(shop), Some(sku)) = (shop, sku) {
        conn.execute(
            "INSERT OR IGNORE INTO purchase_aliases (shop, shop_sku, node_id, at)
             VALUES (?1, ?2, ?3, ?4)",
            params![shop, sku, node, now()],
        )?;
    }
    event(
        conn,
        node,
        "purchase_linked",
        json!({ "purchase": id, "qty": qty }),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{money, parse_money};

    #[test]
    fn amounts_read_with_a_decimal_point_or_comma_and_thousands() {
        for (s, c) in [
            ("1234.56", 123456),
            ("1.234,56", 123456),
            ("1,234.56", 123456),
            ("407,7", 40770),
            ("12", 1200),
            ("0.05", 5),
            ("-3,10", -310),
        ] {
            assert_eq!(parse_money(s).unwrap(), c, "{s}");
        }
        // A lone separator before three digits is a thousands mark in one locale and a decimal
        // in another: refused rather than guessed.
        for s in ["", "abc", "1.2.3", "1.234", "12,345"] {
            assert!(parse_money(s).is_err(), "{s}");
        }
        assert_eq!(money(123456), "1234.56");
        assert_eq!(money(-5), "-0.05");
    }
}
