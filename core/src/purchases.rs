//! Purchases (purchases spec §3.2): lines of what was bought. A line never creates a node; it is
//! linked to one on the person's word. Lines come from adapters as NDJSON (spec §6) or are
//! entered by hand, and the documents an adapter sends (invoices) hang on them.

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::docs::NewDoc;
use crate::store::{Inventory, brief, event, ids, now, resolve};
use crate::{Error, Result};

/// The buckets a line may be in (spec §3.2); consumables are not imported for now (§11).
pub const BUCKETS: [&str; 3] = ["durable", "clothing", "digital"];

/// Why a line will never be a node.
pub const DISMISSALS: [&str; 6] = [
    "consumed",
    "given",
    "returned",
    "elsewhere",
    "not-mine",
    "duplicate",
];

/// An amount in minor units (kuruş, cents) from `1234.56`, `1.234,56` or `1234`.
pub(crate) fn parse_money(s: &str) -> Result<i64> {
    let bad = || Error::Usage(format!("`{s}` is not an amount like 1234.56"));
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
            chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
                .map(|_| d.to_string())
                .map_err(|_| Error::Usage(format!("`{d}` is not a date YYYY-MM-DD")))
        })
        .transpose()
}

pub(crate) fn purchase_json(conn: &Connection, id: i64) -> Result<Value> {
    let row = conn
        .query_row(
            "SELECT source, source_key, shop, merchant, order_no, order_url, product_url, shop_sku,
                    name, brand, category, ordered_at, delivered_at, qty, paid, currency,
                    billed_to, status, bucket, dismissed, why, raw, same_as, imported_at, pack
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
                }))
            },
        )
        .optional()?;
    let mut p = row.ok_or_else(|| Error::NotFound(format!("no purchase with id {id}")))?;
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
    // A line joined to another (the same purchase seen by a second source) is settled through
    // that line: nothing of it is left open.
    p["open_qty"] = if p["same_as"].is_null() {
        json!((p["units"].as_i64().unwrap_or(0) - linked).max(0))
    } else {
        json!(0)
    };
    p["joined"] = json!(ids(
        conn,
        "SELECT id FROM purchases WHERE same_as = ?1 ORDER BY id",
        [id]
    )?);
    p["attachments"] = json!(crate::attachments::attachments_of(conn, id)?);
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

/// The purchase lines linked to a node, newest first, each with the quantity linked to it.
pub(crate) fn purchases_of(conn: &Connection, node: i64) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, l.qty FROM purchases p JOIN purchase_links l ON l.purchase_id = p.id
          WHERE l.node_id = ?1 ORDER BY COALESCE(p.delivered_at, p.ordered_at) DESC, p.id",
    )?;
    let rows = stmt
        .query_map([node], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
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
        .collect()
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
    fields: [(&'static str, Option<String>); 13],
    qty: i64,
    paid: Option<i64>,
    status: String,
    bucket: String,
}

const LINE_FIELDS: [(&str, &str); 13] = [
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
    let source = text(v, "source").ok_or_else(|| Error::Usage("`source` is required".into()))?;
    let key = text(v, "key").ok_or_else(|| Error::Usage("`key` is required".into()))?;
    let name = text(v, "name").ok_or_else(|| Error::Usage("`name` is required".into()))?;
    let status = text(v, "status").unwrap_or_else(|| "delivered".into());
    let bucket = text(v, "bucket").unwrap_or_else(|| "durable".into());
    // Cancelled lines and consumables are not imported (spec §3.2, §11).
    if status == "cancelled" || bucket == "consumable" {
        return Ok(None);
    }
    if !["delivered", "returned"].contains(&status.as_str()) {
        return Err(Error::Usage(format!(
            "status `{status}`; use delivered, returned or cancelled"
        )));
    }
    if !BUCKETS.contains(&bucket.as_str()) {
        return Err(Error::Usage(format!(
            "bucket `{bucket}`; use {} or consumable",
            BUCKETS.join(", ")
        )));
    }
    let qty = v.get("qty").and_then(Value::as_i64).unwrap_or(1);
    if qty < 1 {
        return Err(Error::Usage("qty must be at least 1".into()));
    }
    let paid = text(v, "paid").map(|p| parse_money(&p)).transpose()?;
    let mut fields = LINE_FIELDS.map(|(col, k)| (col, text(v, k)));
    for (col, d) in fields.iter_mut() {
        if col.ends_with("_at") {
            *d = date(d)?;
        }
        if *col == "currency" {
            *d = d.as_deref().map(str::to_uppercase);
        }
    }
    Ok(Some((
        Line {
            source,
            key,
            fields,
            qty,
            paid,
            status,
            bucket,
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
                "UPDATE purchases SET name = ?1, qty = ?2, paid = ?3, status = ?4, bucket = ?5, {}
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
                Box::new(l.bucket.clone()),
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
        return Err(Error::Usage(format!(
            "purchase {id} is dismissed as {}; clear that first",
            p["dismissed"]
        )));
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
        return Err(Error::Usage(format!(
            "purchase {id} has {open} left to link, not {qty}"
        )));
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
        let mut docs_added = 0;
        for (i, raw) in ndjson.lines().enumerate() {
            let raw = raw.trim();
            if raw.is_empty() {
                continue;
            }
            let at = |e: Error| e.at_line(i + 1);
            let v: Value = serde_json::from_str(raw)
                .map_err(|e| Error::Usage(format!("not JSON: {e}")))
                .map_err(at)?;
            match v.get("type").and_then(Value::as_str).unwrap_or("purchase") {
                "purchase" => match line_from(&v).map_err(at)? {
                    None => *counts.entry("skipped").or_default() += 1,
                    Some((l, name)) => {
                        let (_, how) = upsert(&tx, &l, &name).map_err(at)?;
                        *counts.entry(how).or_default() += 1;
                    }
                },
                "document" => {
                    let source = text(&v, "source")
                        .ok_or_else(|| Error::Usage("`source` is required".into()))
                        .map_err(at)?;
                    let file = text(&v, "file")
                        .ok_or_else(|| Error::Usage("`file` is required".into()))
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
                        .ok_or_else(|| Error::Usage("`source` is required".into()))
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
                    return Err(Error::Usage(format!("unknown line type `{other}`")).at_line(i + 1));
                }
            }
        }
        let joined = crate::attachments::join_same(&tx)?;
        tx.commit()?;
        Ok(json!({
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
        }))
    }

    /// A purchase entered by hand (bought in a shop, a gift); linked to `for_ref` at once when
    /// given.
    pub fn buy_add(&mut self, line: &Value, for_ref: Option<&str>) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let mut v = line.clone();
        v["source"] = json!("manual");
        let next: i64 =
            tx.query_row("SELECT COALESCE(MAX(id), 0) + 1 FROM purchases", [], |r| {
                r.get(0)
            })?;
        v["key"] = json!(format!("manual-{next}"));
        let (l, name) = line_from(&v)?
            .ok_or_else(|| Error::Usage("a manual purchase cannot be cancelled".into()))?;
        let (id, _) = upsert(&tx, &l, &name)?;
        if let Some(r) = for_ref {
            let node = resolve(&tx, r, false)?;
            link_in(&tx, id, node, l.qty)?;
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
        let all = ids(
            &self.conn,
            "SELECT id FROM purchases ORDER BY COALESCE(delivered_at, ordered_at) DESC, id DESC",
            [],
        )?;
        let shop = shop.map(crate::fold);
        let mut out = Vec::new();
        for id in all {
            let p = purchase_json(&self.conn, id)?;
            let is_open = p["dismissed"].is_null() && p["open_qty"].as_i64().unwrap_or(0) > 0;
            if open && !is_open
                || bucket.is_some_and(|b| p["bucket"] != b)
                || shop.as_ref().is_some_and(|s| {
                    !p["shop"]
                        .as_str()
                        .is_some_and(|x| crate::fold(x).contains(s.as_str()))
                })
                || since.is_some_and(|d| {
                    p["ordered_at"]
                        .as_str()
                        .or(p["delivered_at"].as_str())
                        .is_none_or(|x| x < d)
                })
            {
                continue;
            }
            out.push(p);
        }
        Ok(json!({ "purchases": out }))
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
        let node = resolve(&tx, reference, false)?;
        let p = purchase_json(&tx, id)?;
        let already: i64 = tx
            .query_row(
                "SELECT qty FROM purchase_links WHERE purchase_id = ?1 AND node_id = ?2",
                params![id, node],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        let qty = qty.unwrap_or(p["open_qty"].as_i64().unwrap_or(0) + already);
        if qty < 1 {
            return Err(Error::Usage(format!(
                "purchase {id} has nothing left to link"
            )));
        }
        link_in(&tx, id, node, qty)?;
        tx.commit()?;
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
            return Err(Error::Usage(format!(
                "purchase {id} is not linked to node {node}"
            )));
        }
        event(&tx, node, "purchase_unlinked", json!({ "purchase": id }))?;
        tx.commit()?;
        self.buy_show(id)
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
            return Err(Error::Usage(format!(
                "`{r}` is not a reason; use {}",
                DISMISSALS.join(", ")
            )));
        }
        self.conn.execute(
            "UPDATE purchases SET dismissed = ?1, why = ?2 WHERE id = ?3",
            params![reason, why.map(str::trim).filter(|w| !w.is_empty()), id],
        )?;
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
        let node = resolve(&tx, reference, false)?;
        if clear {
            tx.execute(
                "DELETE FROM purchase_declines WHERE purchase_id = ?1 AND node_id = ?2",
                params![id, node],
            )?;
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

fn link_in(conn: &Connection, id: i64, node: i64, qty: i64) -> Result<()> {
    check_open(conn, id, node, qty)?;
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
