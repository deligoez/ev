//! One thing kept in several places (spec/portions.md). Each place holds a portion: an ordinary
//! record with its own place and count. The portions of one thing share `nodes.thing` (the id
//! of its first record) and what the thing is: name, kind, make, model, size and tags, kept
//! equal on every live portion. Units split off a portion into a new one, and a portion that
//! arrives where another portion of the same thing already is joins it.

use rusqlite::{Connection, params};
use serde_json::{Value, json};

use crate::error::refused;
use crate::model::{Kind, Node, State};
use crate::store::{apply_edit, brief_json, event, ids, label, load, touch};
use crate::{Error, Result};

/// The fields every portion of a thing shares (spec §3); an edit of one reaches them all.
const SHARED: [&str; 5] = ["name", "make", "model", "size", "tags"];

/// The units a record stands for: its count, one when it has none.
pub(crate) fn units(n: &Node) -> i64 {
    n.qty.unwrap_or(1)
}

/// Refuses what cannot be kept in several places: anything but an item, one unit with a
/// serial, a holder with things inside (which unit would hold them?), and a portion that is
/// missing, lent out or already on its way somewhere.
fn check_spreadable(conn: &Connection, n: &Node) -> Result<()> {
    let why = if n.kind != Kind::Item {
        Some("only items are kept in several places".to_string())
    } else if n.serial.is_some() {
        Some("a record with a serial is one unit".to_string())
    } else if n.lost {
        Some("it is lost; find it first".to_string())
    } else if n.with.is_some() {
        Some("it is lent out; take it back first".to_string())
    } else if n.pending_to.is_some() {
        Some("it already has a pending move; cancel it first".to_string())
    } else if !ids(
        conn,
        "SELECT id FROM nodes WHERE parent_id = ?1 AND state != 'gone' LIMIT 1",
        [n.id],
    )?
    .is_empty()
    {
        Some("it holds things; move what is inside first".to_string())
    } else {
        None
    };
    match why {
        Some(w) => Err(refused(
            format!("{}: {w}", label(n)),
            json!({ "node": brief_json(conn, n.id)? }),
        )),
        None => Ok(()),
    }
}

/// How many of `node` an action takes: `None` (all of it) or a count it must have more than
/// zero of and at most all of. `Some(n)` below its units means a split.
pub(crate) fn part_of(n: &Node, qty: Option<i64>) -> Result<Option<i64>> {
    let Some(q) = qty else { return Ok(None) };
    if q < 1 {
        return Err(Error::Usage("--qty must be at least 1".into()));
    }
    let have = units(n);
    if q > have {
        return Err(refused(
            format!("{} has {have}; there are not {q} to take", label(n)),
            Value::Null,
        ));
    }
    Ok((q < have).then_some(q))
}

/// Splits `count` units off `node` into a new portion beside it, in the same state, and ties
/// both to one thing. Returns the new portion's id. `count` is below the node's units.
pub(crate) fn split_off(conn: &Connection, node: &Node, count: i64) -> Result<i64> {
    check_spreadable(conn, node)?;
    let thing = node.thing.unwrap_or(node.id);
    conn.execute(
        "UPDATE nodes SET thing = ?1, qty = ?2 WHERE id = ?3",
        params![thing, units(node) - count, node.id],
    )?;
    let at = crate::store::now();
    conn.execute(
        "INSERT INTO nodes (name, kind, parent_id, qty, state, disposition, owner_place, make,
                            model, size, thing, created_at, updated_at)
         SELECT name, kind, parent_id, ?2, state, disposition, owner_place, make, model, size,
                ?3, ?4, ?4
           FROM nodes WHERE id = ?1",
        params![node.id, count, thing, at],
    )?;
    let new = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO tags (node_id, tag) SELECT ?2, tag FROM tags WHERE node_id = ?1",
        params![node.id, new],
    )?;
    touch(conn, node.id)?;
    event(
        conn,
        node.id,
        "portion_out",
        json!({ "qty": count, "to": new }),
    )?;
    event(
        conn,
        new,
        "portion_in",
        json!({ "qty": count, "from": node.id }),
    )?;
    Ok(new)
}

/// After `id` has arrived where it is: when a live portion of the same thing is already there
/// in the same condition, the units join it and this record ends as `merged`. Returns the id
/// that holds the units now.
pub(crate) fn join_here(conn: &Connection, id: i64) -> Result<i64> {
    let n = load(conn, id)?;
    let Some(thing) = n.thing else { return Ok(id) };
    if n.state == State::Gone || n.lost || n.with.is_some() || n.pending_to.is_some() {
        return Ok(id);
    }
    let disposition = n.disposition.map(|d| d.as_str());
    let owner: Option<i64> =
        conn.query_row("SELECT owner_place FROM nodes WHERE id = ?1", [id], |r| {
            r.get(0)
        })?;
    let other = ids(
        conn,
        "SELECT id FROM nodes
          WHERE thing = ?1 AND id != ?2 AND parent_id IS ?3 AND state = ?4
            AND disposition IS ?5 AND owner_place IS ?6 AND lost = 0
            AND pending_to IS NULL AND with_place IS NULL
          ORDER BY id LIMIT 1",
        params![thing, id, n.parent_id, n.state.as_str(), disposition, owner],
    )?;
    match other.first() {
        Some(&into) => {
            merge(conn, &n, into)?;
            Ok(into)
        }
        None => Ok(id),
    }
}

/// The units of `from` join `into`; `from` ends as `merged`, keeping its photos and history.
/// What waits on it (tasks, kit parts) moves to the record that holds the units now.
fn merge(conn: &Connection, from: &Node, into: i64) -> Result<()> {
    let receiver = load(conn, into)?;
    let count = units(from);
    conn.execute(
        "UPDATE nodes SET qty = ?1 WHERE id = ?2",
        params![units(&receiver) + count, into],
    )?;
    conn.execute(
        "UPDATE nodes SET state = 'gone', disposition = 'merged' WHERE id = ?1",
        [from.id],
    )?;
    conn.execute("DELETE FROM cells WHERE node_id = ?1", [from.id])?;
    for table in ["task_nodes", "kit_links"] {
        conn.execute(
            &format!("UPDATE OR IGNORE {table} SET node_id = ?1 WHERE node_id = ?2"),
            params![into, from.id],
        )?;
        conn.execute(
            &format!("DELETE FROM {table} WHERE node_id = ?1"),
            [from.id],
        )?;
    }
    touch(conn, from.id)?;
    touch(conn, into)?;
    event(
        conn,
        from.id,
        "merged",
        json!({ "into": into, "qty": count }),
    )?;
    event(
        conn,
        into,
        "joined",
        json!({ "from": from.id, "qty": count }),
    )?;
    Ok(())
}

/// The live portions of `thing`, oldest first.
fn live(conn: &Connection, thing: i64) -> Result<Vec<Node>> {
    ids(
        conn,
        "SELECT id FROM nodes WHERE thing = ?1 AND state != 'gone' ORDER BY id",
        [thing],
    )?
    .into_iter()
    .map(|id| load(conn, id))
    .collect()
}

/// An edit of a shared field (spec §3) applied to `id` reaches every other live portion, each
/// with an `edit` event naming the portion it came through. A portion's kind stays an item and
/// a serial never lands on one: both would make it something else than its siblings.
pub(crate) fn share_identity(conn: &Connection, id: i64, assignments: &[String]) -> Result<()> {
    let n = load(conn, id)?;
    let Some(thing) = n.thing else { return Ok(()) };
    let mut shared = Vec::new();
    for a in assignments {
        let Some((field, value)) = a.split_once('=') else {
            continue;
        };
        let field = field.trim();
        if matches!(field, "kind" | "serial") {
            return Err(refused(
                format!(
                    "{} is one portion of a thing kept in several places; its {field} would set \
                     it apart: `ev unjoin` it first",
                    label(&n)
                ),
                Value::Null,
            ));
        }
        if SHARED.contains(&field) {
            shared.push((field, value));
        }
    }
    if shared.is_empty() {
        return Ok(());
    }
    for other in live(conn, thing)?.into_iter().filter(|o| o.id != id) {
        for (field, value) in &shared {
            let fresh = load(conn, other.id)?;
            apply_edit(conn, &fresh, field, value)?;
        }
        touch(conn, other.id)?;
        let fields: Vec<&str> = shared.iter().map(|(f, _)| *f).collect();
        event(
            conn,
            other.id,
            "edit",
            json!({ "via": id, "fields": fields }),
        )?;
    }
    Ok(())
}

/// The thing a record is a portion of, for `ev show` (spec §5): its units in all, in how many
/// places, how many in use (inside an item: a device, a toy) and how many spare, and each other
/// portion with its count. `None` for a record kept in one place.
pub(crate) fn thing_json(conn: &Connection, n: &Node) -> Result<Option<Value>> {
    let Some(thing) = n.thing else {
        return Ok(None);
    };
    let portions = live(conn, thing)?;
    if portions.iter().all(|p| p.id == n.id) {
        return Ok(None);
    }
    let (mut total, mut in_use, mut lost) = (0, 0, 0);
    let mut elsewhere = Vec::new();
    for p in &portions {
        let used = p
            .parent_id
            .map(|parent| load(conn, parent).map(|h| h.kind == Kind::Item))
            .transpose()?
            .unwrap_or(false);
        if p.lost {
            lost += units(p);
        } else {
            total += units(p);
            if used {
                in_use += units(p);
            }
        }
        if p.id != n.id {
            let mut b = brief_json(conn, p.id)?;
            b["qty"] = json!(units(p));
            b["in_use"] = json!(used);
            elsewhere.push(b);
        }
    }
    Ok(Some(json!({
        "id": thing,
        "total": total,
        "places": portions.len(),
        "in_use": in_use,
        "spare": total - in_use,
        "lost": lost,
        "elsewhere": elsewhere,
    })))
}
