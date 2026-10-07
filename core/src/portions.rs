//! One thing kept in several places (spec/portions.md). Each place holds a portion: an ordinary
//! record with its own place and count. The portions of one thing share `nodes.thing` (the id
//! of its first record) and what the thing is: name, kind, make, model, size and tags, kept
//! equal on every live portion. Units split off a portion into a new one, and a portion that
//! arrives where another portion of the same thing already is joins it.

use rusqlite::{Connection, params};
use serde_json::{Value, json};

use crate::Result;
use crate::error::{refuse, usage};
use crate::model::{Kind, Node, State};
use crate::store::{apply_edit, brief_json, event, ids, label, load, touch};

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
        Some("spread_items_only")
    } else if n.serial.is_some() {
        Some("spread_serial_one_unit")
    } else if n.lost {
        Some("spread_lost")
    } else if n.with.is_some() {
        Some("spread_lent")
    } else if n.pending_to.is_some() {
        Some("spread_pending")
    } else if !ids(
        conn,
        "SELECT id FROM nodes WHERE parent_id = ?1 AND state != 'gone' LIMIT 1",
        [n.id],
    )?
    .is_empty()
    {
        Some("spread_holds_things")
    } else {
        None
    };
    match why {
        Some(id) => Err(refuse(
            id,
            json!({ "node": label(n) }),
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
        return Err(usage("portion_qty_too_small", Value::Null));
    }
    let have = units(n);
    if q > have {
        return Err(refuse(
            "not_that_many",
            json!({ "node": label(n), "have": have, "qty": q }),
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

/// The record a verb with `--qty` acts on (spec §4.1): `node` itself for all of it, or the new
/// portion `qty` of its units were split off into, in the same transaction as the verb.
pub(crate) fn take(conn: &Connection, node: Node, qty: Option<i64>) -> Result<Node> {
    match part_of(&node, qty)? {
        Some(count) => load(conn, split_off(conn, &node, count)?),
        None => Ok(node),
    }
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

/// Why `n` cannot be a portion of a thing kept in several places, in words that name the real
/// reason; `None` when it can. Only items are, and not one with a serial (it is one unit).
pub(crate) fn not_a_portion(n: &Node) -> Option<crate::Error> {
    if n.kind != Kind::Item {
        return Some(refuse(
            "portion_not_an_item",
            json!({ "node": label(n), "kind": n.kind.to_string() }),
            Value::Null,
        ));
    }
    if n.serial.is_some() {
        return Some(refuse(
            "portion_has_serial",
            json!({ "node": label(n) }),
            Value::Null,
        ));
    }
    None
}

/// `ev join` (spec §4.3): records made separately are one thing. What it is comes from the
/// first, a make, model or size only the others know filling in, and the tags of all of them;
/// a make or model that differs is refused with every value, for the person to settle. A record
/// already a portion of another thing brings that thing's portions along. A gone record may be
/// among them (one used up, and the same one bought again) as long as one lives; it keeps what
/// it was. Portions that end up in one place join. Returns the live record that holds the
/// units of the first live one.
pub(crate) fn join(conn: &Connection, nodes: &[Node]) -> Result<i64> {
    for n in nodes {
        if let Some(e) = not_a_portion(n) {
            return Err(e);
        }
    }
    if nodes.iter().all(|n| n.state == State::Gone) {
        return Err(refuse("join_all_gone", Value::Null, Value::Null));
    }
    let first = &nodes[0];
    for (field, values) in [
        (
            "make",
            nodes.iter().map(|n| n.make.clone()).collect::<Vec<_>>(),
        ),
        ("model", nodes.iter().map(|n| n.model.clone()).collect()),
    ] {
        let mut seen: Vec<String> = values.into_iter().flatten().collect();
        seen.sort();
        seen.dedup();
        if seen.len() > 1 {
            return Err(refuse(
                "join_differ",
                json!({ "field": field, "seen": seen.join(" / ") }),
                json!({ "field": field, "values": seen }),
            ));
        }
    }
    let key = first.thing.unwrap_or(first.id);
    let mut members: Vec<i64> = Vec::new();
    for n in nodes {
        let mut tied = match n.thing {
            Some(t) => ids(conn, "SELECT id FROM nodes WHERE thing = ?1", [t])?,
            None => vec![n.id],
        };
        tied.retain(|id| !members.contains(id));
        members.extend(tied);
    }
    let pick = |f: fn(&Node) -> Option<String>| nodes.iter().find_map(f);
    let make = pick(|n| n.make.clone());
    let model = pick(|n| n.model.clone());
    let size = pick(|n| n.size.clone());
    let mut tags: Vec<String> = nodes.iter().flat_map(|n| n.tags.clone()).collect();
    tags.sort();
    tags.dedup();
    for id in &members {
        let was = load(conn, *id)?;
        conn.execute(
            "UPDATE nodes SET thing = ?1 WHERE id = ?2",
            params![key, id],
        )?;
        if was.state == State::Gone {
            continue;
        }
        conn.execute(
            "UPDATE nodes SET name = ?1, make = ?2, model = ?3, size = ?4 WHERE id = ?5",
            params![first.name, make, model, size, id],
        )?;
        for t in &tags {
            conn.execute(
                "INSERT OR IGNORE INTO tags (node_id, tag) VALUES (?1, ?2)",
                params![id, t],
            )?;
        }
        if was.thing != Some(key) {
            touch(conn, *id)?;
            event(conn, *id, "join", json!({ "thing": key }))?;
        }
    }
    // The record returned is a live one: a gone record lends what the thing is, not a place.
    let lead = nodes
        .iter()
        .find(|n| n.state != State::Gone)
        .map_or(first.id, |n| n.id);
    for id in members.iter().filter(|id| **id != lead) {
        join_here(conn, *id)?;
    }
    join_here(conn, lead)
}

/// `ev unjoin`: a portion is a thing of its own after all. It keeps what it is and leaves the
/// thing; the others stay one thing, and what was linked to it (a purchase) stays with it.
pub(crate) fn unjoin(conn: &Connection, n: &Node) -> Result<()> {
    let Some(thing) = n.thing else {
        return Err(refuse(
            "not_in_several_places",
            json!({ "node": label(n) }),
            Value::Null,
        ));
    };
    conn.execute("UPDATE nodes SET thing = NULL WHERE id = ?1", [n.id])?;
    touch(conn, n.id)?;
    event(conn, n.id, "unjoin", json!({ "thing": thing }))?;
    Ok(())
}

/// Every record of the thing `n` is a portion of, gone ones too, `n` first: what was linked to
/// any of them (a purchase, an invoice, a warranty) is the thing's. Just `n` when it is not
/// kept in several places.
pub(crate) fn members(conn: &Connection, n: &Node) -> Result<Vec<i64>> {
    let Some(thing) = n.thing else {
        return Ok(vec![n.id]);
    };
    let mut all = vec![n.id];
    all.extend(ids(
        conn,
        "SELECT id FROM nodes WHERE thing = ?1 AND id != ?2 ORDER BY id",
        params![thing, n.id],
    )?);
    Ok(all)
}

/// `read` over every record of `n`'s thing (spec §3), each entry once by its `id`; one linked
/// on another portion is marked `on` with that portion.
pub(crate) fn across(
    conn: &Connection,
    n: &Node,
    read: impl Fn(i64) -> Result<Vec<Value>>,
) -> Result<Vec<Value>> {
    let mut out: Vec<Value> = Vec::new();
    for m in members(conn, n)? {
        for mut v in read(m)? {
            if out.iter().any(|o| o["id"] == v["id"] && !v["id"].is_null()) {
                continue;
            }
            if m != n.id {
                v["on"] = json!(m);
            }
            out.push(v);
        }
    }
    Ok(out)
}

/// Units the thing's purchases account for: the units linked on any of its records.
pub(crate) fn bought(conn: &Connection, n: &Node) -> Result<Option<i64>> {
    let mut total = 0;
    let mut any = false;
    for m in members(conn, n)? {
        let linked: Option<i64> = conn.query_row(
            "SELECT SUM(qty) FROM purchase_links WHERE node_id = ?1",
            [m],
            |r| r.get(0),
        )?;
        if let Some(q) = linked {
            total += q;
            any = true;
        }
    }
    Ok(any.then_some(total))
}

/// The units of `n`'s thing that are here: its live portions, lost ones left out.
pub(crate) fn here(conn: &Connection, n: &Node) -> Result<i64> {
    let Some(thing) = n.thing else {
        return Ok(units(n));
    };
    Ok(live(conn, thing)?
        .iter()
        .filter(|p| !p.lost)
        .map(units)
        .sum())
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
            return Err(refuse(
                "portion_field_apart",
                json!({ "node": label(&n), "field": field }),
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
/// places, how many in use (inside an item: a device, a toy) and how many spare, each other
/// portion with its count, and what accounts for the units: bought (its purchases), here and
/// gone by how they left (portions that joined another are not gone). `None` for a record
/// kept in one place, and for a last portion with nothing gone beside it.
pub(crate) fn thing_json(conn: &Connection, n: &Node) -> Result<Option<Value>> {
    let Some(thing) = n.thing else {
        return Ok(None);
    };
    let portions = live(conn, thing)?;
    let mut gone = serde_json::Map::new();
    for m in members(conn, n)? {
        let p = load(conn, m)?;
        let Some(d) = p.disposition.filter(|_| p.state == State::Gone) else {
            continue;
        };
        if d == crate::model::Disposition::Merged {
            continue;
        }
        let was = gone.get(d.as_str()).and_then(Value::as_i64).unwrap_or(0);
        gone.insert(d.as_str().to_string(), json!(was + units(&p)));
    }
    if portions.iter().all(|p| p.id == n.id) && gone.is_empty() {
        return Ok(None);
    }
    let (mut total, mut in_use, mut lost) = (0, 0, 0);
    let mut elsewhere = Vec::new();
    for p in &portions {
        let used = p
            .parent_id
            // In a device or a car, they are in use (spec/vehicles-homes.md).
            .map(|parent| load(conn, parent).map(|h| matches!(h.kind, Kind::Item | Kind::Vehicle)))
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
    let left: i64 = gone.values().filter_map(Value::as_i64).sum();
    let bought = bought(conn, n)?;
    // Bought units not here, not gone and not lost: a fact the person may want to look into.
    let unaccounted = bought.map(|b| b - total - lost - left);
    Ok(Some(json!({
        "id": thing,
        "total": total,
        // A lost portion is in no place: it counts under `lost`, as its units do.
        "places": portions.iter().filter(|p| !p.lost).count(),
        "in_use": in_use,
        "spare": total - in_use,
        "lost": lost,
        "elsewhere": elsewhere,
        "bought": bought,
        "gone": gone,
        "unaccounted": unaccounted,
    })))
}
