//! What an adapter hangs on a purchase line for the thing it becomes (purchases spec §6): a
//! link, a value, a coverage. Nothing reaches a thing until the line is linked and the person
//! says to bring them along (`ev buy bring`). Also the joining of one purchase seen by two
//! sources (`same_as`).

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value, json};

use crate::coverage::{COVERAGE_KINDS, NewCoverage};
use crate::purchases::{money, parse_money};
use crate::store::{Inventory, ids, resolve};
use crate::valuations::NewValuation;
use crate::{Error, Result};

pub(crate) const ATTACHMENT_KINDS: [&str; 4] = ["link", "valuation", "coverage", "image"];

fn text(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(|x| match x {
            Value::String(s) => Some(s.trim().to_string()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
        .filter(|s| !s.is_empty())
}

/// The fields an attachment line carries, checked and normalised, as stored.
pub(crate) fn attachment_data(kind: &str, v: &Value) -> Result<String> {
    let mut d = Map::new();
    let mut put = |k: &str, x: Option<String>| {
        if let Some(x) = x {
            d.insert(k.to_string(), json!(x));
        }
    };
    match kind {
        "link" => {
            let url = text(v, "url").ok_or_else(|| Error::Usage("`url` is required".into()))?;
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(Error::Usage(format!("`{url}` is not a web address")));
            }
            put("url", Some(url));
            let k = text(v, "kind").unwrap_or_else(|| "info".into());
            if !crate::links::LINK_KINDS.contains(&k.as_str()) {
                return Err(Error::Usage(format!("`{k}` is not a link kind")));
            }
            put("kind", Some(k));
            put("archive", text(v, "archive"));
            put("note", text(v, "note"));
        }
        "valuation" => {
            let amount =
                text(v, "amount").ok_or_else(|| Error::Usage("`amount` is required".into()))?;
            put("amount", Some(money(parse_money(&amount)?)));
            put("currency", text(v, "currency").map(|c| c.to_uppercase()));
            put("at", crate::purchases::date(&text(v, "at"))?);
            if v.get("approximate").and_then(Value::as_bool) == Some(true) {
                d.insert("approximate".into(), json!(true));
            }
            let mut put = |k: &str, x: Option<String>| {
                if let Some(x) = x {
                    d.insert(k.to_string(), json!(x));
                }
            };
            put("from", text(v, "from"));
            put("note", text(v, "note"));
        }
        "coverage" => {
            let k = text(v, "kind").ok_or_else(|| Error::Usage("`kind` is required".into()))?;
            if !COVERAGE_KINDS.contains(&k.as_str()) {
                return Err(Error::Usage(format!("`{k}` is not a coverage kind")));
            }
            put("kind", Some(k));
            for f in ["term", "from", "ends", "issuer", "number", "note"] {
                put(f, text(v, f));
            }
        }
        // The shop's product picture, downloaded by the adapter: brought as a document of kind
        // `image`, so it never stands in for the thing's own photo.
        "image" => {
            let file = text(v, "file").ok_or_else(|| Error::Usage("`file` is required".into()))?;
            put("file", Some(file));
            put("note", text(v, "note"));
        }
        other => return Err(Error::Usage(format!("unknown attachment `{other}`"))),
    }
    Ok(Value::Object(d).to_string())
}

/// Stores an attachment on a line; true when it was not there yet.
pub(crate) fn attach(conn: &Connection, purchase: i64, kind: &str, data: &str) -> Result<bool> {
    Ok(conn.execute(
        "INSERT OR IGNORE INTO purchase_attachments (purchase_id, kind, data) VALUES (?1, ?2, ?3)",
        params![purchase, kind, data],
    )? > 0)
}

/// A line's attachments and those of the lines joined to it.
pub(crate) fn attachments_of(conn: &Connection, purchase: i64) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT id, purchase_id, kind, data, brought_to FROM purchase_attachments
          WHERE purchase_id = ?1 OR purchase_id IN (SELECT id FROM purchases WHERE same_as = ?1)
          ORDER BY id",
    )?;
    let rows = stmt
        .query_map([purchase], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<i64>>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .map(|(id, p, kind, data, brought)| {
            let mut v: Value = serde_json::from_str(&data).unwrap_or_else(|_| json!({}));
            v["id"] = json!(id);
            v["purchase"] = json!(p);
            v["type"] = json!(kind);
            v["brought_to"] = json!(brought);
            v
        })
        .collect())
}

/// Joins lines that are the same purchase seen by two sources: a line whose order address
/// names another source's order number, narrowed by the product key when the order has several
/// lines; or, with no order, whose product address names exactly one other line's product key.
/// The other line is the one kept; this one points at it. Returns how many were joined.
pub(crate) fn join_same(conn: &Connection) -> Result<i64> {
    let mut stmt = conn.prepare(
        "SELECT id, source, order_url, product_url FROM purchases
          WHERE same_as IS NULL AND (order_url IS NOT NULL OR product_url IS NOT NULL)",
    )?;
    let lines = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut joined = 0;
    for (id, source, order_url, product_url) in lines {
        // A line already kept by another is not joined to a third.
        if conn.query_row(
            "SELECT COUNT(*) FROM purchases WHERE same_as = ?1",
            [id],
            |r| r.get::<_, i64>(0),
        )? > 0
        {
            continue;
        }
        let urls = format!(
            "{} {}",
            order_url.as_deref().unwrap_or_default(),
            product_url.as_deref().unwrap_or_default()
        );
        let by_order = match order_url.as_deref() {
            Some(u) => ids(
                conn,
                "SELECT id FROM purchases
                  WHERE source != ?1 AND same_as IS NULL AND length(order_no) >= 6
                    AND instr(?2, order_no) > 0
                  ORDER BY id",
                params![source, u],
            )?,
            None => Vec::new(),
        };
        let by_product = |among: &[i64]| -> Result<Vec<i64>> {
            let all = ids(
                conn,
                "SELECT id FROM purchases
                  WHERE source != ?1 AND same_as IS NULL AND length(shop_sku) >= 6
                    AND instr(?2, shop_sku) > 0
                  ORDER BY id",
                params![source, urls],
            )?;
            Ok(if among.is_empty() {
                all
            } else {
                all.into_iter().filter(|x| among.contains(x)).collect()
            })
        };
        let target = match by_order.len() {
            1 => Some(by_order[0]),
            0 if order_url.is_none() => Some(by_product(&[])?)
                .filter(|v| v.len() == 1)
                .map(|v| v[0]),
            0 => None,
            _ => match by_product(&by_order)?.as_slice() {
                [one] => Some(*one),
                _ => by_name(conn, id, &by_order)?,
            },
        };
        if let Some(t) = target.filter(|t| *t != id) {
            conn.execute(
                "UPDATE purchases SET same_as = ?1 WHERE id = ?2",
                params![t, id],
            )?;
            joined += 1;
        }
    }
    Ok(joined)
}

fn words(s: &str) -> std::collections::HashSet<String> {
    crate::fold(s)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3)
        .map(str::to_string)
        .collect()
}

/// Among an order's lines, the one whose name shares clearly the most words with this line's:
/// at least two, and more than any other line shares.
fn by_name(conn: &Connection, id: i64, among: &[i64]) -> Result<Option<i64>> {
    let name_of = |p: i64| -> Result<String> {
        Ok(
            conn.query_row("SELECT name FROM purchases WHERE id = ?1", [p], |r| {
                r.get(0)
            })?,
        )
    };
    let mine = words(&name_of(id)?);
    let mut scored = among
        .iter()
        .map(|&p| Ok((words(&name_of(p)?).intersection(&mine).count(), p)))
        .collect::<Result<Vec<_>>>()?;
    scored.sort_by_key(|a| std::cmp::Reverse(a.0));
    Ok(match scored.as_slice() {
        [(best, p), rest @ ..] if *best >= 2 && rest.first().is_none_or(|(n, _)| n < best) => {
            Some(*p)
        }
        _ => None,
    })
}

impl Inventory {
    /// Brings a line's attachments (and those of the lines joined to it) to a thing it is
    /// linked to: links, values and coverages become the thing's own. `only` picks some by id;
    /// those already brought are skipped.
    pub fn buy_bring(&mut self, id: i64, reference: &str, only: &[i64]) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let node = resolve(&tx, reference, false)?;
        let linked: Option<i64> = tx
            .query_row(
                "SELECT 1 FROM purchase_links WHERE purchase_id = ?1 AND node_id = ?2",
                params![id, node],
                |r| r.get(0),
            )
            .optional()?;
        if linked.is_none() {
            return Err(Error::Usage(format!(
                "purchase {id} is not linked to {reference}; ev buy link it first"
            )));
        }
        let mut brought = Vec::new();
        for a in attachments_of(&tx, id)? {
            let aid = a["id"].as_i64().unwrap_or_default();
            if !a["brought_to"].is_null() || !only.is_empty() && !only.contains(&aid) {
                continue;
            }
            let s = |k: &str| a[k].as_str().map(str::to_string);
            match a["type"].as_str() {
                Some("link") => {
                    crate::links::add_link(
                        &tx,
                        &self.doc_dir,
                        node,
                        &s("url").unwrap_or_default(),
                        &s("kind").unwrap_or_else(|| "info".into()),
                        s("archive").as_deref(),
                        s("note").as_deref(),
                    )?;
                }
                Some("valuation") => {
                    crate::valuations::add_valuation(
                        &tx,
                        node,
                        &NewValuation {
                            amount: s("amount").unwrap_or_default(),
                            currency: s("currency"),
                            at: s("at"),
                            approximate: a["approximate"] == true,
                            source: s("from"),
                            note: s("note"),
                        },
                    )?;
                }
                Some("coverage") => {
                    crate::coverage::add_coverage(
                        &tx,
                        &[node],
                        &NewCoverage {
                            kind: s("kind").unwrap_or_default(),
                            term: s("term"),
                            from: s("from"),
                            ends: s("ends"),
                            issuer: s("issuer"),
                            number: s("number"),
                            note: s("note"),
                            ..Default::default()
                        },
                    )?;
                }
                Some("image") => {
                    let file = s("file").unwrap_or_default();
                    let (doc, _) = crate::docs::store_doc(
                        &tx,
                        &self.doc_dir,
                        std::path::Path::new(&file),
                        &crate::docs::NewDoc {
                            kind: "image".into(),
                            note: s("note"),
                            ..Default::default()
                        },
                    )?;
                    crate::docs::link_node(&tx, doc, node, "image")?;
                }
                _ => continue,
            }
            tx.execute(
                "UPDATE purchase_attachments SET brought_to = ?1 WHERE id = ?2",
                params![node, aid],
            )?;
            brought.push(aid);
        }
        tx.commit()?;
        let mut v = crate::store::show(&self.conn, node)?;
        v["brought"] = json!(brought);
        Ok(v)
    }
}
