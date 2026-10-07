//! Places outside the home (households, people) and what goes to, returns to or comes back
//! from each.

use super::*;
use crate::error::{not_found, usage};

// ---------- places (spec §13) ----------

/// Folded place key; apostrophes are dropped so "Ayşe'lar" and "Ayşelar" match.
fn place_key(text: &str) -> String {
    fold(text).replace(['\'', '’'], "")
}

/// The place whose name or alias folds to `text`, if any.
fn find_place(conn: &Connection, text: &str) -> Result<Option<i64>> {
    let folded = place_key(text);
    if folded.is_empty() {
        return Err(usage("place_name_empty", Value::Null));
    }
    Ok(conn
        .query_row(
            "SELECT place_id FROM place_aliases WHERE alias_folded = ?1",
            [folded],
            |r| r.get(0),
        )
        .optional()?)
}

pub(super) fn place_or_create(conn: &Connection, text: &str) -> Result<i64> {
    if let Some(id) = find_place(conn, text)? {
        return Ok(id);
    }
    let name = text.trim();
    conn.execute(
        "INSERT INTO places (name, created_at) VALUES (?1, ?2)",
        params![name, now()],
    )?;
    let id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO place_aliases (place_id, alias, alias_folded) VALUES (?1, ?2, ?3)",
        params![id, name, place_key(name)],
    )?;
    Ok(id)
}

fn place_json(conn: &Connection, id: i64) -> Result<Value> {
    let name: String =
        conn.query_row("SELECT name FROM places WHERE id = ?1", [id], |r| r.get(0))?;
    let aliases = strings(
        conn,
        "SELECT alias FROM place_aliases WHERE place_id = ?1 ORDER BY rowid",
        id,
    )?;
    Ok(json!({ "id": id, "name": name, "aliases": aliases }))
}

pub(super) fn resolve_place(conn: &Connection, text: &str) -> Result<i64> {
    find_place(conn, text)?
        .ok_or_else(|| not_found("place_no_such", json!({ "name": text.trim() })))
}

fn nodes_at(conn: &Connection, column: &str, place: i64) -> Result<Vec<Value>> {
    let list = ids(
        conn,
        &format!(
            "SELECT id FROM nodes WHERE {column} = ?1 AND state != 'gone'{} ORDER BY id",
            // Returning a thing to its owner already takes it there; list it once, as a return.
            if column == "to_place" {
                " AND owner_place IS NOT to_place"
            } else {
                ""
            }
        ),
        [place],
    )?;
    list.iter().map(|id| brief_json(conn, *id)).collect()
}

pub(crate) fn place_errands(conn: &Connection, place: i64) -> Result<Value> {
    Ok(json!({
        "place": place_json(conn, place)?,
        "take": nodes_at(conn, "to_place", place)?,
        "return": nodes_at(conn, "owner_place", place)?,
        "collect": nodes_at(conn, "with_place", place)?,
    }))
}

impl Inventory {
    /// Creates a place with extra aliases; an alias already used elsewhere is refused.
    pub fn place_add(&mut self, name: &str, aliases: &[String]) -> Result<Value> {
        let tx = self.conn.transaction()?;
        if let Some(existing) = find_place(&tx, name)? {
            return Err(refuse(
                "place_name_taken",
                json!({ "name": name.trim() }),
                json!({ "place": place_json(&tx, existing)? }),
            ));
        }
        let id = place_or_create(&tx, name)?;
        for a in aliases {
            add_alias(&tx, id, a)?;
        }
        tx.commit()?;
        place_json(&self.conn, id)
    }

    pub fn place_alias(&mut self, place: &str, alias: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let id = resolve_place(&tx, place)?;
        add_alias(&tx, id, alias)?;
        tx.commit()?;
        place_json(&self.conn, id)
    }

    /// Folds `from` into `into`: every reference and alias moves, `from` disappears.
    pub fn place_merge(&mut self, from: &str, into: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let a = resolve_place(&tx, from)?;
        let b = resolve_place(&tx, into)?;
        if a == b {
            return Err(refuse("places_already_one", Value::Null, Value::Null));
        }
        for column in ["owner_place", "with_place", "to_place"] {
            tx.execute(
                &format!("UPDATE nodes SET {column} = ?1 WHERE {column} = ?2"),
                params![b, a],
            )?;
        }
        tx.execute(
            "UPDATE place_aliases SET place_id = ?1 WHERE place_id = ?2",
            params![b, a],
        )?;
        tx.execute("DELETE FROM places WHERE id = ?1", [a])?;
        tx.commit()?;
        place_json(&self.conn, b)
    }

    /// A place that was a home of ours becomes a home that was left (spec/vehicles-homes.md):
    /// made with the place's name, `came`, the day it was `left` and its `address`; the things
    /// that named the place as where they were move into it, and the place goes with its
    /// aliases. A place that is also another household's (something to take there, return
    /// there or collect from there) is refused: that is a household, not a home of ours.
    pub fn place_home(
        &mut self,
        place: &str,
        came: Option<&str>,
        left: Option<&str>,
        address: Option<&str>,
    ) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let pid = resolve_place(&tx, place)?;
        let errands: i64 = tx.query_row(
            "SELECT COUNT(*) FROM nodes
              WHERE owner_place = ?1 OR with_place = ?1 OR to_place = ?1",
            [pid],
            |r| r.get(0),
        )?;
        if errands > 0 {
            return Err(refuse(
                "place_is_a_household",
                json!({ "place": place.trim(), "count": errands }),
                json!({ "errands": place_errands(&tx, pid)? }),
            ));
        }
        let name: String =
            tx.query_row("SELECT name FROM places WHERE id = ?1", [pid], |r| r.get(0))?;
        let home = add_one(
            &tx,
            &crate::model::NewNode {
                name: name.clone(),
                kind: "home".into(),
                gone: Some("moved".into()),
                came: came.map(Into::into),
                at: left.map(Into::into),
                address: address.map(Into::into),
                ..Default::default()
            },
            None,
        )?;
        let moved = ids(
            &tx,
            "SELECT node_id FROM departures WHERE place_id = ?1 ORDER BY node_id",
            [pid],
        )?;
        for id in &moved {
            tx.execute(
                "UPDATE nodes SET parent_id = ?1, lost = 0 WHERE id = ?2",
                params![home, id],
            )?;
        }
        tx.execute(
            "UPDATE departures SET place_id = NULL WHERE place_id = ?1",
            [pid],
        )?;
        tx.execute("DELETE FROM place_aliases WHERE place_id = ?1", [pid])?;
        tx.execute("DELETE FROM places WHERE id = ?1", [pid])?;
        event(
            &tx,
            home,
            "was_place",
            json!({ "place": name, "moved": moved }),
        )?;
        tx.commit()?;
        let mut v = show(&self.conn, home)?;
        v["moved_in"] = json!(moved.len());
        Ok(v)
    }

    pub fn place_list(&self) -> Result<Value> {
        let list = ids(&self.conn, "SELECT id FROM places ORDER BY name", [])?;
        let mut out = Vec::new();
        for id in list {
            let mut p = place_json(&self.conn, id)?;
            for (key, column) in [
                ("take", "to_place"),
                ("return", "owner_place"),
                ("collect", "with_place"),
            ] {
                let n: i64 = self.conn.query_row(
                    &format!("SELECT COUNT(*) FROM nodes WHERE {column} = ?1 AND state != 'gone'"),
                    [id],
                    |r| r.get(0),
                )?;
                p[key] = json!(n);
            }
            out.push(p);
        }
        Ok(json!({ "places": out }))
    }

    /// What to take to, return to, or collect from a place; every place when none is given.
    pub fn errands(&self, place: Option<&str>) -> Result<Value> {
        match place {
            Some(p) => {
                let id = resolve_place(&self.conn, p)?;
                place_errands(&self.conn, id)
            }
            None => {
                let list = ids(
                    &self.conn,
                    "SELECT DISTINCT p.id FROM places p JOIN nodes n
                       ON p.id IN (n.to_place, n.owner_place, n.with_place)
                     WHERE n.state != 'gone' ORDER BY p.name",
                    [],
                )?;
                let all = list
                    .iter()
                    .map(|id| place_errands(&self.conn, *id))
                    .collect::<Result<Vec<_>>>()?;
                Ok(json!({ "errands": all }))
            }
        }
    }

    /// Lends a node of ours to a place; it stays in the tree where it returns to.
    pub fn lend(&mut self, reference: &str, to: &str) -> Result<Value> {
        self.lend_qty(reference, to, None)
    }

    /// `lend` for `qty` of a record's units: they go as a portion of their own (spec/
    /// portions.md §4.1), and come back to join the rest with `back`.
    pub fn lend_qty(&mut self, reference: &str, to: &str, qty: Option<i64>) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let node = load(&tx, resolve(&tx, reference, false)?)?;
        // Lent again to whom it is with: nothing to change. To someone else, it passed on.
        if let Some(w) = &node.with
            && crate::fold(w) == crate::fold(to.trim())
        {
            return Err(refuse(
                "already_lent_to",
                json!({ "node": label(&node), "to": w }),
                Value::Null,
            ));
        }
        let node = crate::portions::take(&tx, node, qty)?;
        if node.owner.is_some() {
            return Err(refuse(
                "not_ours_to_lend",
                json!({ "node": label(&node) }),
                Value::Null,
            ));
        }
        let place = place_or_create(&tx, to)?;
        tx.execute(
            "UPDATE nodes SET with_place = ?1 WHERE id = ?2",
            params![place, node.id],
        )?;
        touch(&tx, node.id)?;
        event(
            &tx,
            node.id,
            "lend",
            json!({ "to": place_json(&tx, place)?["name"] }),
        )?;
        tx.commit()?;
        show(&self.conn, node.id)
    }

    pub fn back(&mut self, reference: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let node = load(&tx, resolve(&tx, reference, false)?)?;
        let Some(with) = node.with.clone() else {
            return Err(refuse(
                "not_lent_out",
                json!({ "node": label(&node) }),
                Value::Null,
            ));
        };
        tx.execute(
            "UPDATE nodes SET with_place = NULL WHERE id = ?1",
            [node.id],
        )?;
        touch(&tx, node.id)?;
        event(&tx, node.id, "back", json!({ "from": with }))?;
        // Back home beside the rest of the same thing: the units join them.
        let holder = crate::portions::join_here(&tx, node.id)?;
        tx.commit()?;
        show(&self.conn, holder)
    }
}

fn add_alias(conn: &Connection, place: i64, alias: &str) -> Result<()> {
    let a = alias.trim();
    if let Some(other) = find_place(conn, a)? {
        if other == place {
            return Ok(());
        }
        return Err(refuse(
            "alias_names_another",
            json!({ "alias": a }),
            json!({ "place": place_json(conn, other)? }),
        ));
    }
    conn.execute(
        "INSERT INTO place_aliases (place_id, alias, alias_folded) VALUES (?1, ?2, ?3)",
        params![place, a, place_key(a)],
    )?;
    Ok(())
}
