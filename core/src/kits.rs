//! Kits (spec §29): what a bought set should contain, part by part, and which records in the
//! inventory are those parts. "What is still missing from the project kit?" is then read off
//! the records instead of being kept in someone's head or in a note.

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::fold::fold;
use crate::store::{Inventory, brief_json, event, load, now, resolve};

/// A kit by id (`12`, `#12`) or by name, compared folded.
fn kit_id(conn: &Connection, reference: &str) -> Result<i64> {
    let r = reference.trim();
    if let Ok(id) = r.trim_start_matches('#').parse::<i64>() {
        let found: Option<i64> = conn
            .query_row("SELECT id FROM kits WHERE id = ?1", [id], |r| r.get(0))
            .optional()?;
        if let Some(id) = found {
            return Ok(id);
        }
    }
    conn.query_row(
        "SELECT id FROM kits WHERE name_folded = ?1",
        [fold(r)],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| Error::NotFound(format!("no kit `{r}`; `ev kit list` shows them")))
}

/// Appends parts to a kit, numbering on from its last.
fn add_parts(conn: &Connection, kit: i64, parts: &[(String, i64)]) -> Result<()> {
    let mut next: i64 = conn.query_row(
        "SELECT COALESCE(MAX(position), 0) FROM kit_parts WHERE kit_id = ?1",
        [kit],
        |r| r.get(0),
    )?;
    for (text, qty) in parts {
        let text = text.trim();
        if text.is_empty() {
            return Err(Error::Usage("a kit part needs a name".into()));
        }
        if *qty < 1 {
            return Err(Error::Usage(format!(
                "`{text}`: a part comes at least once"
            )));
        }
        next += 1;
        conn.execute(
            "INSERT INTO kit_parts (kit_id, position, text, qty) VALUES (?1, ?2, ?3, ?4)",
            params![kit, next, text, qty],
        )?;
    }
    Ok(())
}

/// The part numbered `n` (from 1) of a kit, or an error naming how many it has.
fn part(conn: &Connection, kit: i64, n: i64) -> Result<String> {
    conn.query_row(
        "SELECT text FROM kit_parts WHERE kit_id = ?1 AND position = ?2",
        params![kit, n],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM kit_parts WHERE kit_id = ?1",
                [kit],
                |r| r.get(0),
            )
            .unwrap_or(0);
        Error::NotFound(format!("part {n} does not exist; the kit has {count}"))
    })
}

/// Sets the purchase line a kit was bought as (`None` clears it), with a `kit_purchase` event on
/// every record linked to the kit, so each one's history says where its purchase came from.
fn set_purchase(conn: &Connection, kit: i64, line: Option<i64>) -> Result<()> {
    if let Some(l) = line {
        let found: Option<i64> = conn
            .query_row("SELECT id FROM purchases WHERE id = ?1", [l], |r| r.get(0))
            .optional()?;
        if found.is_none() {
            return Err(Error::NotFound(format!("no purchase with id {l}")));
        }
    }
    let name: String =
        conn.query_row("SELECT name FROM kits WHERE id = ?1", [kit], |r| r.get(0))?;
    conn.execute(
        "UPDATE kits SET purchase_id = ?1 WHERE id = ?2",
        params![line, kit],
    )?;
    for node in crate::store::ids(
        conn,
        "SELECT DISTINCT node_id FROM kit_links WHERE kit_id = ?1 ORDER BY node_id",
        [kit],
    )? {
        event(
            conn,
            node,
            "kit_purchase",
            json!({ "kit": name, "purchase": line }),
        )?;
    }
    Ok(())
}

/// A part's counts from the records linked to it: `found` (here, not lost), `lost`, and
/// `open` — expected but not recorded yet. A record without a count is one thing; a gone one
/// counts for nothing.
fn tally(conn: &Connection, kit: i64, n: i64) -> Result<(Vec<Value>, i64, i64)> {
    let mut stmt = conn.prepare(
        "SELECT node_id FROM kit_links WHERE kit_id = ?1 AND position = ?2 ORDER BY node_id",
    )?;
    let ids = stmt
        .query_map(params![kit, n], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let (mut found, mut lost, mut nodes) = (0, 0, Vec::new());
    for id in ids {
        let node = load(conn, id)?;
        let count = node.qty.unwrap_or(1);
        match (node.state, node.lost) {
            (crate::State::Gone, _) => {}
            (_, true) => lost += count,
            _ => found += count,
        }
        nodes.push(brief_json(conn, id)?);
    }
    Ok((nodes, found, lost))
}

impl Inventory {
    /// Records a kit: its name, how many of it were bought (`copies`), and its parts, each
    /// with how many come in one copy; with `purchase`, the line it was bought as.
    pub fn kit_add(
        &mut self,
        name: &str,
        copies: Option<i64>,
        note: Option<&str>,
        parts: &[(String, i64)],
        purchase: Option<i64>,
    ) -> Result<Value> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Usage("a kit needs a name".into()));
        }
        let copies = copies.unwrap_or(1);
        if copies < 1 {
            return Err(Error::Usage("a kit is bought at least once".into()));
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO kits (name, name_folded, copies, note, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![name, fold(name), copies, note.map(str::trim), now()],
        )
        .map_err(|e| match e {
            rusqlite::Error::SqliteFailure(f, _)
                if f.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Error::Usage(format!("there is already a kit named `{name}`"))
            }
            e => e.into(),
        })?;
        let id = tx.last_insert_rowid();
        add_parts(&tx, id, parts)?;
        if purchase.is_some() {
            set_purchase(&tx, id, purchase)?;
        }
        tx.commit()?;
        self.kit_show(&id.to_string())
    }

    /// Adds parts to the end of a kit's list.
    pub fn kit_parts_add(&mut self, kit: &str, parts: &[(String, i64)]) -> Result<Value> {
        if parts.is_empty() {
            return Err(Error::Usage("give at least one part".into()));
        }
        let tx = self.conn.transaction()?;
        let id = kit_id(&tx, kit)?;
        add_parts(&tx, id, parts)?;
        tx.commit()?;
        self.kit_show(&id.to_string())
    }

    /// Says these records are part `n` of a kit. Each record's history keeps it.
    pub fn kit_link(&mut self, kit: &str, n: i64, references: &[String]) -> Result<Value> {
        if references.is_empty() {
            return Err(Error::Usage("give the records that are this part".into()));
        }
        let tx = self.conn.transaction()?;
        let id = kit_id(&tx, kit)?;
        let text = part(&tx, id, n)?;
        let name: String =
            tx.query_row("SELECT name FROM kits WHERE id = ?1", [id], |r| r.get(0))?;
        for r in references {
            let node = resolve(&tx, r, false)?;
            let added = tx.execute(
                "INSERT OR IGNORE INTO kit_links (kit_id, position, node_id) VALUES (?1, ?2, ?3)",
                params![id, n, node],
            )?;
            if added > 0 {
                event(
                    &tx,
                    node,
                    "kit_link",
                    json!({ "kit": name, "part": n, "text": text }),
                )?;
            }
        }
        tx.commit()?;
        self.kit_part(id, n)
    }

    /// What a link or an unlink changed: the part, with its count, and the kit's counts. The
    /// whole checklist is `kit_show`'s (spec/output.md).
    fn kit_part(&self, id: i64, n: i64) -> Result<Value> {
        let v = self.kit_show(&id.to_string())?;
        let part = v["parts"]
            .as_array()
            .and_then(|p| p.iter().find(|p| p["n"] == n))
            .cloned()
            .unwrap_or(Value::Null);
        Ok(json!({
            "kit": { "id": id, "name": v["kit"]["name"] },
            "part": part,
            "counts": v["counts"],
        }))
    }

    /// Links a kit to the purchase line it was bought as (spec/kit-purchase.md), or clears it
    /// with `None`. Every record linked to the kit gets a `kit_purchase` event.
    pub fn kit_purchase(&mut self, kit: &str, line: Option<i64>) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let id = kit_id(&tx, kit)?;
        set_purchase(&tx, id, line)?;
        tx.commit()?;
        self.kit_show(&id.to_string())
    }

    /// Undoes a link: the record is not that part after all.
    pub fn kit_unlink(&mut self, kit: &str, n: i64, reference: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let id = kit_id(&tx, kit)?;
        let text = part(&tx, id, n)?;
        let node = resolve(&tx, reference, true)?;
        let name: String =
            tx.query_row("SELECT name FROM kits WHERE id = ?1", [id], |r| r.get(0))?;
        if tx.execute(
            "DELETE FROM kit_links WHERE kit_id = ?1 AND position = ?2 AND node_id = ?3",
            params![id, n, node],
        )? == 0
        {
            return Err(Error::NotFound(format!(
                "#{node} is not linked to part {n} of {name}"
            )));
        }
        event(
            &tx,
            node,
            "kit_unlink",
            json!({ "kit": name, "part": n, "text": text }),
        )?;
        tx.commit()?;
        self.kit_part(id, n)
    }

    /// One kit, part by part: how many are expected (per copy × copies), which records are
    /// that part and where they are, how many are found or lost, and how many are still open.
    pub fn kit_show(&self, kit: &str) -> Result<Value> {
        let id = kit_id(&self.conn, kit)?;
        let (name, copies, note, purchase): (String, i64, Option<String>, Option<i64>) =
            self.conn.query_row(
                "SELECT name, copies, note, purchase_id FROM kits WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
        // The line the set was bought as, in short: `ev buy show` has the rest.
        let purchase = purchase
            .map(|p| -> Result<Value> {
                let l = crate::purchases::purchase_row(&self.conn, p)?;
                Ok(json!({
                    "id": p, "name": l["name"], "shop": l["shop"], "ordered_at": l["ordered_at"],
                    "paid": l["paid"], "currency": l["currency"],
                }))
            })
            .transpose()?;
        let mut stmt = self.conn.prepare(
            "SELECT position, text, qty FROM kit_parts WHERE kit_id = ?1 ORDER BY position",
        )?;
        let rows = stmt
            .query_map([id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let (mut expected_all, mut found_all, mut lost_all, mut open_all) = (0, 0, 0, 0);
        let mut parts = Vec::new();
        for (n, text, qty) in rows {
            let expected = qty * copies;
            let (nodes, found, lost) = tally(&self.conn, id, n)?;
            let open = (expected - found - lost).max(0);
            expected_all += expected;
            found_all += found.min(expected);
            lost_all += lost;
            open_all += open;
            parts.push(json!({
                "n": n, "text": text, "qty": qty, "expected": expected,
                "found": found, "lost": lost, "open": open, "nodes": nodes,
            }));
        }
        Ok(json!({
            "kit": { "id": id, "name": name, "copies": copies, "note": note, "purchase": purchase },
            "counts": {
                "expected": expected_all, "found": found_all, "lost": lost_all, "open": open_all,
            },
            "parts": parts,
        }))
    }

    /// Every kit with its counts, most still open first.
    pub fn kit_list(&self) -> Result<Value> {
        let ids = crate::store::ids(&self.conn, "SELECT id FROM kits ORDER BY id", [])?;
        let mut kits = ids
            .iter()
            .map(|id| {
                let v = self.kit_show(&id.to_string())?;
                Ok(json!({
                    "id": v["kit"]["id"], "name": v["kit"]["name"], "copies": v["kit"]["copies"],
                    "parts": v["parts"].as_array().map_or(0, Vec::len),
                    "counts": v["counts"],
                }))
            })
            .collect::<Result<Vec<_>>>()?;
        kits.sort_by_key(|k| std::cmp::Reverse(k["counts"]["open"].as_i64().unwrap_or(0)));
        Ok(json!({ "kits": kits }))
    }

    /// Removes a kit and its links; the records themselves stay.
    pub fn kit_remove(&mut self, kit: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let id = kit_id(&tx, kit)?;
        tx.execute("DELETE FROM kit_links WHERE kit_id = ?1", [id])?;
        tx.execute("DELETE FROM kit_parts WHERE kit_id = ?1", [id])?;
        tx.execute("DELETE FROM kits WHERE id = ?1", [id])?;
        tx.commit()?;
        self.kit_list()
    }
}

/// The kit parts a record is linked to, for its details: `[{kit, n, text}]`.
pub(crate) fn kits_of(conn: &Connection, node: i64) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT k.name, p.position, p.text FROM kit_links l
           JOIN kits k ON k.id = l.kit_id
           JOIN kit_parts p ON p.kit_id = l.kit_id AND p.position = l.position
          WHERE l.node_id = ?1 ORDER BY k.name, p.position",
    )?;
    let rows = stmt
        .query_map([node], |r| {
            Ok(
                json!({ "kit": r.get::<_, String>(0)?, "n": r.get::<_, i64>(1)?,
                        "text": r.get::<_, String>(2)? }),
            )
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
