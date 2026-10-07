//! Repairs and maintenance (spec/repairs.md): what was done to a thing and when it is due again.
//! What it cost is ak's; nothing here holds an amount.

use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::error::{Result, not_found, usage};
use crate::store::{Inventory, brief, event, ids, now, resolve, today};

/// The kinds of work: a repair of something broken, routine care, an official inspection.
pub const UPKEEP_KINDS: [&str; 3] = ["repair", "service", "inspection"];

/// How soon a date makes a piece of work due, and how close an odometer.
const DUE_DAYS: i64 = 60;
const DUE_KM: i64 = 1000;

/// What a new piece of work says.
#[derive(Debug, Clone, Default)]
pub struct NewUpkeep {
    pub kind: String,
    pub work: String,
    /// When it was done: a date or a partial one; today when left out.
    pub at: Option<String>,
    pub km: Option<i64>,
    pub by: Option<String>,
    pub next_at: Option<String>,
    pub next_km: Option<i64>,
    pub doc: Option<i64>,
    pub note: Option<String>,
}

fn text(v: Option<&str>) -> Option<String> {
    v.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn check_kind(kind: &str) -> Result<String> {
    let k = kind.trim().to_lowercase();
    if UPKEEP_KINDS.contains(&k.as_str()) {
        Ok(k)
    } else {
        Err(usage(
            "upkeep_kind_unknown",
            json!({ "kind": kind, "kinds": UPKEEP_KINDS.join(", ") }),
        ))
    }
}

/// A due date: a day (`2026-04-01`) or a month (`2026-04`, due from its first day).
fn due_day(text: &str) -> Result<NaiveDate> {
    let t = text.trim();
    NaiveDate::parse_from_str(t, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(&format!("{t}-01"), "%Y-%m-%d"))
        .map_err(|_| usage("upkeep_next_bad", json!({ "date": t })))
}

fn check_km(km: Option<i64>) -> Result<Option<i64>> {
    match km {
        Some(k) if k < 0 => Err(usage("upkeep_km_bad", json!({ "km": k }))),
        other => Ok(other),
    }
}

fn check_doc(conn: &Connection, doc: Option<i64>) -> Result<Option<i64>> {
    if let Some(d) = doc {
        let found: Option<i64> = conn
            .query_row("SELECT id FROM documents WHERE id = ?1", [d], |r| r.get(0))
            .optional()?;
        if found.is_none() {
            return Err(not_found("upkeep_doc_unknown", json!({ "doc": d })));
        }
    }
    Ok(doc)
}

/// One piece of work as it is answered.
pub(crate) fn upkeep_json(conn: &Connection, id: i64) -> Result<Value> {
    let mut v = conn
        .query_row(
            "SELECT node_id, kind, at, km, work, by, next_at, next_km, doc_id, note
               FROM upkeep WHERE id = ?1",
            [id],
            |r| {
                Ok(json!({
                    "id": id,
                    "node_id": r.get::<_, i64>(0)?,
                    "kind": r.get::<_, String>(1)?,
                    "at": r.get::<_, String>(2)?,
                    "km": r.get::<_, Option<i64>>(3)?,
                    "work": r.get::<_, String>(4)?,
                    "by": r.get::<_, Option<String>>(5)?,
                    "next_at": r.get::<_, Option<String>>(6)?,
                    "next_km": r.get::<_, Option<i64>>(7)?,
                    "doc": r.get::<_, Option<i64>>(8)?,
                    "note": r.get::<_, Option<String>>(9)?,
                }))
            },
        )
        .optional()?
        .ok_or_else(|| not_found("upkeep_not_found", json!({ "id": id })))?;
    let node = v["node_id"].as_i64().unwrap_or_default();
    v["node"] = serde_json::to_value(brief(conn, node)?)
        .map_err(|e| crate::Error::Internal(e.to_string()))?;
    if let Some(o) = v.as_object_mut() {
        o.remove("node_id");
    }
    Ok(v)
}

/// A thing's upkeep, newest first; `limit` of them when given.
pub(crate) fn upkeep_of(conn: &Connection, node: i64, limit: Option<usize>) -> Result<Vec<Value>> {
    let list = ids(
        conn,
        "SELECT id FROM upkeep WHERE node_id = ?1 ORDER BY at DESC, id DESC",
        [node],
    )?;
    list.into_iter()
        .take(limit.unwrap_or(usize::MAX))
        .map(|id| upkeep_json(conn, id))
        .collect()
}

/// What is due: for each thing and kind of work, the newest piece that says when it is due
/// again, when that is within `DUE_DAYS` or past, or its odometer within `DUE_KM` of the
/// thing's last known one. Each with `days_left` and `km_left` where they apply, soonest first.
pub(crate) fn upkeep_due(conn: &Connection) -> Result<Vec<Value>> {
    let rows: Vec<(i64, i64, String)> = {
        let mut stmt = conn.prepare(
            "SELECT u.id, u.node_id, u.kind FROM upkeep u JOIN nodes n ON n.id = u.node_id
              WHERE n.state != 'gone' AND (u.next_at IS NOT NULL OR u.next_km IS NOT NULL)
              ORDER BY u.at DESC, u.id DESC",
        )?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    let today = today();
    let mut seen: Vec<(i64, String)> = Vec::new();
    let mut out = Vec::new();
    for (id, node, kind) in rows {
        // A newer piece of the same kind says when it is due now.
        if seen.contains(&(node, kind.clone())) {
            continue;
        }
        seen.push((node, kind));
        let mut v = upkeep_json(conn, id)?;
        let days = v["next_at"]
            .as_str()
            .and_then(|d| due_day(d).ok())
            .map(|d| (d - today).num_days());
        let last_km: Option<i64> = conn.query_row(
            "SELECT MAX(km) FROM upkeep WHERE node_id = ?1",
            [node],
            |r| r.get(0),
        )?;
        let km_left = v["next_km"]
            .as_i64()
            .zip(last_km)
            .map(|(next, last)| next - last);
        let due = days.is_some_and(|d| d <= DUE_DAYS) || km_left.is_some_and(|k| k <= DUE_KM);
        if !due {
            continue;
        }
        v["days_left"] = json!(days);
        v["km_left"] = json!(km_left);
        v["last_km"] = json!(last_km);
        out.push(v);
    }
    out.sort_by_key(|v| v["days_left"].as_i64().unwrap_or(i64::MAX));
    Ok(out)
}

impl Inventory {
    /// `ev upkeep add`: a piece of work done to a thing.
    pub fn upkeep_add(&mut self, reference: &str, new: &NewUpkeep) -> Result<Value> {
        let kind = check_kind(&new.kind)?;
        let work = text(Some(&new.work)).ok_or_else(|| usage("upkeep_work_empty", Value::Null))?;
        let at = match text(new.at.as_deref()) {
            Some(a) => crate::store::past::partial_date(&a)?,
            None => today().format("%Y-%m-%d").to_string(),
        };
        if let Some(n) = text(new.next_at.as_deref()) {
            due_day(&n)?;
        }
        let km = check_km(new.km)?;
        let next_km = check_km(new.next_km)?;
        let tx = self.conn.transaction()?;
        // A car sold may have its service history completed afterwards.
        let node = match resolve(&tx, reference, false) {
            Err(e) if e.is_not_found() => resolve(&tx, reference, true)?,
            found => found?,
        };
        let doc = check_doc(&tx, new.doc)?;
        tx.execute(
            "INSERT INTO upkeep (node_id, kind, at, km, work, by, next_at, next_km, doc_id, note,
                                 created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                node,
                kind,
                at,
                km,
                work,
                text(new.by.as_deref()),
                text(new.next_at.as_deref()),
                next_km,
                doc,
                text(new.note.as_deref()),
                now()
            ],
        )?;
        let id = tx.last_insert_rowid();
        event(
            &tx,
            node,
            "upkeep_added",
            json!({ "upkeep": id, "kind": kind, "work": work, "at": at }),
        )?;
        tx.commit()?;
        Ok(json!({ "upkeep": upkeep_json(&self.conn, id)? }))
    }

    /// `ev upkeep list`: a thing's upkeep, or every thing's, newest first; with `due` only what
    /// is due (see `upkeep_due`).
    pub fn upkeep_list(&self, reference: Option<&str>, due: bool) -> Result<Value> {
        let node = reference
            .map(|r| match resolve(&self.conn, r, false) {
                Err(e) if e.is_not_found() => resolve(&self.conn, r, true),
                found => found,
            })
            .transpose()?;
        let list = if due {
            upkeep_due(&self.conn)?
                .into_iter()
                .filter(|v| node.is_none_or(|n| v["node"]["id"] == n))
                .collect()
        } else {
            match node {
                Some(n) => upkeep_of(&self.conn, n, None)?,
                None => ids(
                    &self.conn,
                    "SELECT id FROM upkeep ORDER BY at DESC, id DESC",
                    [],
                )?
                .into_iter()
                .map(|id| upkeep_json(&self.conn, id))
                .collect::<Result<Vec<_>>>()?,
            }
        };
        Ok(json!({ "upkeep": list, "due": due }))
    }

    /// `ev upkeep edit <id> field=value…`: corrects a piece of work; an empty value clears an
    /// optional field.
    pub fn upkeep_edit(&mut self, id: i64, fields: &[String]) -> Result<Value> {
        if fields.is_empty() {
            return Err(usage("upkeep_edit_nothing", Value::Null));
        }
        let tx = self.conn.transaction()?;
        let before = upkeep_json(&tx, id)?;
        let node = before["node"]["id"].as_i64().unwrap_or_default();
        for f in fields {
            let Some((key, value)) = f.split_once('=') else {
                return Err(usage("upkeep_edit_field_bad", json!({ "field": f })));
            };
            let value = text(Some(value));
            let set = |column: &str, v: rusqlite::types::Value| -> Result<()> {
                tx.execute(
                    &format!("UPDATE upkeep SET {column} = ?1 WHERE id = ?2"),
                    params![v, id],
                )?;
                Ok(())
            };
            let or_null = |v: Option<String>| v.map_or(rusqlite::types::Value::Null, Into::into);
            let number = |v: Option<String>| -> Result<rusqlite::types::Value> {
                let n = v
                    .map(|s| {
                        s.parse::<i64>()
                            .map_err(|_| usage("upkeep_km_bad", json!({ "km": s })))
                    })
                    .transpose()?;
                Ok(check_km(n)?.map_or(rusqlite::types::Value::Null, Into::into))
            };
            match key.trim() {
                "kind" => {
                    let k = check_kind(value.as_deref().unwrap_or_default())?;
                    set("kind", k.into())?;
                }
                "work" => {
                    let w = value.ok_or_else(|| usage("upkeep_work_empty", Value::Null))?;
                    set("work", w.into())?;
                }
                "at" => {
                    let a = value.ok_or_else(|| usage("upkeep_at_empty", Value::Null))?;
                    set("at", crate::store::past::partial_date(&a)?.into())?;
                }
                "km" => set("km", number(value)?)?,
                "next_km" => set("next_km", number(value)?)?,
                "next_at" => {
                    if let Some(n) = &value {
                        due_day(n)?;
                    }
                    set("next_at", or_null(value))?;
                }
                "by" => set("by", or_null(value))?,
                "note" => set("note", or_null(value))?,
                "doc" => {
                    let d = value
                        .map(|s| {
                            s.trim_start_matches('#')
                                .parse::<i64>()
                                .map_err(|_| usage("upkeep_edit_field_bad", json!({ "field": f })))
                        })
                        .transpose()?;
                    let d = check_doc(&tx, d)?;
                    set("doc_id", d.map_or(rusqlite::types::Value::Null, Into::into))?;
                }
                other => {
                    return Err(usage(
                        "upkeep_edit_field_unknown",
                        json!({ "field": other }),
                    ));
                }
            }
        }
        let after = upkeep_json(&tx, id)?;
        event(
            &tx,
            node,
            "upkeep_edited",
            json!({ "upkeep": id, "before": before, "after": after }),
        )?;
        tx.commit()?;
        Ok(json!({ "upkeep": upkeep_json(&self.conn, id)? }))
    }

    /// `ev upkeep remove <id>`: a piece of work recorded by mistake; the history keeps it.
    pub fn upkeep_remove(&mut self, id: i64) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let before = upkeep_json(&tx, id)?;
        let node = before["node"]["id"].as_i64().unwrap_or_default();
        tx.execute("DELETE FROM upkeep WHERE id = ?1", [id])?;
        event(&tx, node, "upkeep_removed", json!({ "upkeep": before }))?;
        tx.commit()?;
        Ok(json!({ "removed": before }))
    }
}
