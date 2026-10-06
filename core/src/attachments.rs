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

/// Stores an attachment on a line; true when it was not there yet. Looked up first: an ignored
/// `INSERT OR IGNORE` still advances the table's AUTOINCREMENT counter, so a re-import of the
/// same lines would change the database file while changing nothing in it.
pub(crate) fn attach(conn: &Connection, purchase: i64, kind: &str, data: &str) -> Result<bool> {
    let there: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM purchase_attachments WHERE purchase_id = ?1 AND kind = ?2 AND data = ?3",
            params![purchase, kind, data],
            |r| r.get(0),
        )
        .optional()?;
    if there.is_some() {
        return Ok(false);
    }
    Ok(conn.execute(
        "INSERT INTO purchase_attachments (purchase_id, kind, data) VALUES (?1, ?2, ?3)",
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
    /// linked to: links, values, coverages and product images become the thing's own. `only`
    /// picks some by id, `types` some by type; those already brought are skipped. The result
    /// says what was brought (`brought`, `brought_types`) and what was not and why (`skipped`),
    /// so the agent can tell the person.
    pub fn buy_bring(
        &mut self,
        id: i64,
        reference: &str,
        only: &[i64],
        types: &[String],
    ) -> Result<Value> {
        check_types(types)?;
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
            return Err(crate::error::refused(
                format!("purchase {id} is not linked to {reference}; ev buy link it first"),
                serde_json::Value::Null,
            ));
        }
        let attachments = attachments_of(&tx, id)?;
        let unknown: Vec<String> = only
            .iter()
            .filter(|o| !attachments.iter().any(|a| a["id"] == **o))
            .map(|o| format!("#{o}"))
            .collect();
        if !unknown.is_empty() {
            return Err(Error::Usage(format!(
                "purchase {id} carries no attachment {}; ev buy show {id} lists them",
                unknown.join(", ")
            )));
        }
        let picked: Vec<Value> = attachments
            .into_iter()
            .filter(|a| only.is_empty() || only.iter().any(|o| a["id"] == *o))
            .collect();
        let (brought, skipped) = bring_into(&tx, &self.doc_dir, node, &picked, types)?;
        tx.commit()?;
        // What was done, not the whole thing again: `ev show` has the rest.
        Ok(json!({
            "node": crate::store::brief(&self.conn, node)?,
            "from_purchase": id,
            "brought_types": counts_by_type(&picked, &brought),
            "brought": brought,
            "skipped": skipped,
        }))
    }

    /// Brings what every linked line still carries (of `types` only, when given) to the thing
    /// it is linked to: the back-fill after an import or a run of `ev buy link`. A line linked
    /// to more than one thing is left for `ev buy bring <line> <ref>` (which one is the
    /// person's call), and so is one whose thing is gone; the result names both, and lists
    /// each line it brought from with what it brought.
    pub fn buy_bring_all(&mut self, types: &[String]) -> Result<Value> {
        check_types(types)?;
        let tx = self.conn.transaction()?;
        let lines: Vec<(i64, i64, String, String, i64)> = {
            let mut stmt = tx.prepare(
                "SELECT l.purchase_id, MIN(l.node_id), n.name, n.state,
                        COUNT(DISTINCT COALESCE('t' || n.thing, 'n' || n.id))
                 FROM purchase_links l JOIN nodes n ON n.id = l.node_id
                 GROUP BY l.purchase_id ORDER BY l.purchase_id",
            )?;
            stmt.query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect::<std::result::Result<_, _>>()?
        };
        let mut brought_from = Vec::new();
        let mut left = Vec::new();
        let mut totals = Map::new();
        // What was looked at, so an empty run says why it is empty: no line carries the type,
        // or what it carries is brought already.
        let (mut checked, mut carrying, mut already) = (0, 0, 0);
        for (line, node, name, state, things) in lines {
            checked += 1;
            let of_type: Vec<Value> = attachments_of(&tx, line)?
                .into_iter()
                .filter(|a| types.is_empty() || types.iter().any(|t| a["type"] == *t.as_str()))
                .collect();
            if !of_type.is_empty() {
                carrying += 1;
            }
            let (pending, done): (Vec<Value>, Vec<Value>) =
                of_type.into_iter().partition(|a| a["brought_to"].is_null());
            already += done.len();
            if pending.is_empty() {
                continue;
            }
            let why = if things > 1 {
                Some("several")
            } else if state == "gone" {
                Some("gone")
            } else {
                None
            };
            if let Some(why) = why {
                left.push(
                    json!({"purchase": line, "node": node, "name": name, "why": why,
                                 "waiting": pending.len()}),
                );
                continue;
            }
            let (brought, _) = bring_into(&tx, &self.doc_dir, node, &pending, types)?;
            let counts = counts_by_type(&pending, &brought);
            for (t, n) in counts.as_object().into_iter().flatten() {
                let sum = totals.get(t).and_then(Value::as_i64).unwrap_or(0);
                totals.insert(t.clone(), json!(sum + n.as_i64().unwrap_or(0)));
            }
            brought_from.push(json!({"purchase": line, "node": node, "name": name,
                                     "brought_types": counts, "brought": brought}));
        }
        tx.commit()?;
        Ok(
            json!({"brought_from": brought_from, "brought_types": totals, "left": left,
                  "checked": {"lines": checked, "carrying": carrying, "already": already}}),
        )
    }
}

fn check_types(types: &[String]) -> Result<()> {
    match types
        .iter()
        .find(|t| !ATTACHMENT_KINDS.contains(&t.as_str()))
    {
        Some(t) => Err(Error::Usage(format!(
            "attachment type '{t}' is not one of: {}",
            ATTACHMENT_KINDS.join(", ")
        ))),
        None => Ok(()),
    }
}

/// How many of each type `brought` holds, e.g. `{"image": 2, "link": 1}`.
fn counts_by_type(attachments: &[Value], brought: &[i64]) -> Value {
    let mut counts = Map::new();
    for a in attachments
        .iter()
        .filter(|a| brought.iter().any(|b| a["id"] == *b))
    {
        let t = a["type"].as_str().unwrap_or_default().to_string();
        let n = counts.get(&t).and_then(Value::as_i64).unwrap_or(0);
        counts.insert(t, json!(n + 1));
    }
    Value::Object(counts)
}

/// Brings `attachments` to `node`, those of `types` only when given. Returns the ids brought
/// and, for each one left, why: `brought` (already, `to` says where) or `type` (filtered out).
fn bring_into(
    tx: &Connection,
    doc_dir: &std::path::Path,
    node: i64,
    attachments: &[Value],
    types: &[String],
) -> Result<(Vec<i64>, Vec<Value>)> {
    let mut brought = Vec::new();
    let mut skipped = Vec::new();
    for a in attachments {
        let aid = a["id"].as_i64().unwrap_or_default();
        let kind = a["type"].as_str().unwrap_or_default();
        if let Some(to) = a["brought_to"].as_i64() {
            skipped.push(json!({"id": aid, "type": kind, "why": "brought", "to": to}));
            continue;
        }
        if !types.is_empty() && !types.iter().any(|t| t == kind) {
            skipped.push(json!({"id": aid, "type": kind, "why": "type"}));
            continue;
        }
        let s = |k: &str| a[k].as_str().map(str::to_string);
        match kind {
            "link" => {
                crate::links::add_link(
                    tx,
                    doc_dir,
                    node,
                    &s("url").unwrap_or_default(),
                    &s("kind").unwrap_or_else(|| "info".into()),
                    s("archive").as_deref(),
                    s("note").as_deref(),
                )?;
            }
            "valuation" => {
                crate::valuations::add_valuation(
                    tx,
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
            "coverage" => {
                crate::coverage::add_coverage(
                    tx,
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
            "image" => {
                let file = s("file").unwrap_or_default();
                let (doc, _) = crate::docs::store_doc(
                    tx,
                    doc_dir,
                    std::path::Path::new(&file),
                    &crate::docs::NewDoc {
                        kind: "image".into(),
                        note: s("note"),
                        ..Default::default()
                    },
                )?;
                crate::docs::link_node(tx, doc, node, "image")?;
            }
            _ => continue,
        }
        tx.execute(
            "UPDATE purchase_attachments SET brought_to = ?1 WHERE id = ?2",
            params![node, aid],
        )?;
        brought.push(aid);
    }
    Ok((brought, skipped))
}

/// What `purchase` brought to `node`, taken back when the link turns out wrong: its product
/// pictures and its links leave the node (they show and name another product). A value or a
/// coverage stays, since the person may have kept or corrected it; each is `left` with the
/// command that removes it. Every attachment is free to be brought again.
pub(crate) fn take_back(
    tx: &Connection,
    doc_dir: &std::path::Path,
    purchase: i64,
    node: i64,
) -> Result<(Vec<Value>, Vec<Value>)> {
    let (mut taken, mut left) = (Vec::new(), Vec::new());
    for a in attachments_of(tx, purchase)? {
        if a["brought_to"].as_i64() != Some(node) {
            continue;
        }
        let aid = a["id"].as_i64().unwrap_or_default();
        let kind = a["type"].as_str().unwrap_or_default();
        let s = |k: &str| a[k].as_str().unwrap_or_default().to_string();
        let back = match kind {
            "link" => {
                tx.execute(
                    "DELETE FROM links WHERE node_id = ?1 AND url = ?2",
                    params![node, s("url").trim()],
                )? > 0
            }
            "image" => {
                let file = std::path::PathBuf::from(s("file"));
                let doc: Option<i64> = match std::fs::read(&file) {
                    Ok(bytes) => {
                        // The store is content-addressed: the same bytes name the same file.
                        let ext = crate::docs::extension(&file);
                        let stored = crate::photo::store_bytes(doc_dir, &bytes, &ext)?;
                        tx.query_row(
                            "SELECT id FROM documents WHERE file = ev_store(?1)",
                            [stored.to_string_lossy()],
                            |r| r.get(0),
                        )
                        .optional()?
                    }
                    Err(_) => None,
                };
                match doc {
                    Some(doc)
                        if tx.execute(
                            "DELETE FROM document_links
                              WHERE document_id = ?1 AND target = 'node' AND target_id = ?2",
                            params![doc, node],
                        )? > 0 =>
                    {
                        crate::store::event(
                            tx,
                            node,
                            "doc_unlinked",
                            json!({ "document": doc, "kind": "image" }),
                        )?;
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        };
        if back {
            taken.push(json!({ "id": aid, "type": kind }));
        } else {
            let how = match kind {
                "valuation" => "ev value <ref> --remove",
                "coverage" => "ev cover remove <id>",
                _ => "not found on the record any more",
            };
            left.push(json!({ "id": aid, "type": kind, "how": how }));
        }
        tx.execute(
            "UPDATE purchase_attachments SET brought_to = NULL WHERE id = ?1",
            [aid],
        )?;
    }
    Ok((taken, left))
}
