use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::error::refused;
use crate::model::{Disposition, Kind, NewNode, Node, NodeRef, PathSegment, State};
use crate::{Error, Result, fold};

/// The schema version this build writes (`PRAGMA user_version`).
pub const SCHEMA_VERSION: i64 = 1;

/// Guards every upward walk against a corrupted parent chain.
const MAX_DEPTH: usize = 10_000;

const SCHEMA_V1: &str = "
BEGIN;
CREATE TABLE nodes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    parent_id INTEGER REFERENCES nodes(id),
    code TEXT,
    code_folded TEXT,
    address TEXT,
    qty INTEGER,
    note TEXT,
    theme TEXT,
    fill INTEGER,
    state TEXT NOT NULL DEFAULT 'active',
    disposition TEXT,
    lost INTEGER NOT NULL DEFAULT 0,
    pending_to INTEGER REFERENCES nodes(id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX nodes_parent ON nodes(parent_id);
CREATE TABLE tags (
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    tag TEXT NOT NULL,
    PRIMARY KEY (node_id, tag)
);
CREATE TABLE photos (
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    position INTEGER NOT NULL,
    path TEXT NOT NULL,
    PRIMARY KEY (node_id, position)
);
CREATE TABLE events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    at TEXT NOT NULL,
    type TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX events_node ON events(node_id);
PRAGMA user_version = 1;
COMMIT;
";

const NODE_COLUMNS: &str = "id, name, kind, parent_id, code, address, qty, note, theme, fill, \
     state, disposition, lost, pending_to, created_at, updated_at";

pub struct Inventory {
    conn: Connection,
}

impl Inventory {
    /// Opens or creates the database; refuses a file written by a newer schema (exit 6).
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir)
                .map_err(|e| Error::Internal(format!("cannot create {}: {e}", dir.display())))?;
        }
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(Error::NewerSchema {
                found: version,
                supported: SCHEMA_VERSION,
            });
        }
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        if version < 1 {
            conn.execute_batch(SCHEMA_V1)?;
        }
        Ok(Self { conn })
    }

    /// Changes whenever another connection commits a write; used to refresh readers.
    pub fn data_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("PRAGMA data_version", [], |r| r.get(0))?)
    }

    pub fn resolve(&self, reference: &str, include_gone: bool) -> Result<i64> {
        resolve(&self.conn, reference, include_gone)
    }

    pub fn node(&self, id: i64) -> Result<Node> {
        load(&self.conn, id)
    }

    pub fn brief(&self, id: i64) -> Result<NodeRef> {
        brief(&self.conn, id)
    }

    pub fn add(&mut self, new: NewNode) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let parent = match new.parent.as_deref() {
            Some(r) if r.starts_with('@') => {
                return Err(Error::Usage(
                    "`@key` references only work inside a batch".into(),
                ));
            }
            Some(r) => Some(resolve(&tx, r, false)?),
            None => None,
        };
        let id = add_one(&tx, &new, parent)?;
        tx.commit()?;
        show(&self.conn, id)
    }

    /// Adds every line or none (spec §6, §11.5).
    pub fn add_batch(&mut self, lines: Vec<NewNode>) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let mut keys: HashMap<String, i64> = HashMap::new();
        let mut created = Vec::with_capacity(lines.len());
        for (i, new) in lines.iter().enumerate() {
            let line = i + 1;
            let parent = match new.parent.as_deref() {
                Some(r) if r.starts_with('@') => Some(*keys.get(&r[1..]).ok_or_else(|| {
                    Error::Usage(format!("unknown batch key `{r}`")).at_line(line)
                })?),
                Some(r) => Some(resolve(&tx, r, false).map_err(|e| e.at_line(line))?),
                None => None,
            };
            let id = add_one(&tx, new, parent).map_err(|e| e.at_line(line))?;
            if let Some(key) = &new.key
                && keys.insert(key.clone(), id).is_some()
            {
                return Err(Error::Usage(format!("duplicate batch key `{key}`")).at_line(line));
            }
            created.push(id);
        }
        tx.commit()?;
        let nodes = created
            .iter()
            .map(|id| brief(&self.conn, *id))
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({ "created": nodes }))
    }

    pub fn show(&self, reference: &str, include_gone: bool) -> Result<Value> {
        let id = resolve(&self.conn, reference, include_gone)?;
        show(&self.conn, id)
    }

    pub fn tree(&self, reference: Option<&str>, depth: Option<usize>) -> Result<Value> {
        let depth = depth.unwrap_or(usize::MAX);
        match reference {
            Some(r) => {
                let id = resolve(&self.conn, r, false)?;
                Ok(json!({ "tree": [subtree(&self.conn, id, depth)?] }))
            }
            None => {
                let homes: Vec<i64> = ids(
                    &self.conn,
                    "SELECT id FROM nodes WHERE kind = 'home' AND state != 'gone' ORDER BY id",
                    [],
                )?;
                let tree = homes
                    .iter()
                    .map(|id| subtree(&self.conn, *id, depth))
                    .collect::<Result<Vec<_>>>()?;
                let unplaced: Vec<i64> = ids(
                    &self.conn,
                    "SELECT id FROM nodes WHERE parent_id IS NULL AND kind != 'home' AND state != 'gone' ORDER BY id",
                    [],
                )?;
                let unplaced = unplaced
                    .iter()
                    .map(|id| brief(&self.conn, *id))
                    .collect::<Result<Vec<_>>>()?;
                Ok(json!({ "tree": tree, "unplaced": unplaced }))
            }
        }
    }

    pub fn find(
        &self,
        text: &str,
        tag: Option<&str>,
        kind: Option<Kind>,
        include_gone: bool,
    ) -> Result<Value> {
        let needle = fold(text);
        if needle.is_empty() {
            return Err(Error::Usage("search text is empty".into()));
        }
        let tag = tag.map(|t| t.trim().to_lowercase());
        let mut results = Vec::new();
        for id in ids(&self.conn, "SELECT id FROM nodes ORDER BY id", [])? {
            let n = load(&self.conn, id)?;
            if (n.state == State::Gone && !include_gone)
                || kind.is_some_and(|k| k != n.kind)
                || tag.as_ref().is_some_and(|t| !n.tags.contains(t))
            {
                continue;
            }
            let haystacks = [
                Some(&n.name),
                n.code.as_ref(),
                n.note.as_ref(),
                n.theme.as_ref(),
            ];
            let hit = haystacks
                .iter()
                .flatten()
                .any(|h| fold(h).contains(&needle))
                || n.tags.iter().any(|t| fold(t).contains(&needle));
            if hit {
                results.push(brief(&self.conn, id)?);
            }
        }
        Ok(json!({ "query": text, "results": results }))
    }

    /// Applies `field=value` assignments (spec §6, §11.6).
    pub fn edit(&mut self, reference: &str, assignments: &[String]) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let id = resolve(&tx, reference, false)?;
        let mut changes = serde_json::Map::new();
        for a in assignments {
            let (field, value) = a
                .split_once('=')
                .ok_or_else(|| Error::Usage(format!("`{a}` is not field=value")))?;
            let field = field.trim();
            let before = load(&tx, id)?;
            apply_edit(&tx, &before, field, value)?;
            let after = load(&tx, id)?;
            let (b, a2) = (field_value(&before, field), field_value(&after, field));
            if b != a2 {
                changes.insert(field.to_string(), json!({ "before": b, "after": a2 }));
            }
        }
        if !changes.is_empty() {
            touch(&tx, id)?;
            event(&tx, id, "edit", Value::Object(changes))?;
        }
        tx.commit()?;
        show(&self.conn, id)
    }

    pub fn move_to(&mut self, reference: &str, to: &str, plan: bool) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let node = load(&tx, resolve(&tx, reference, false)?)?;
        let target = resolve(&tx, to, false)?;
        if plan {
            if let Some(p) = node.pending_to {
                return Err(refused(
                    format!(
                        "{} already has a pending move; cancel it first",
                        label(&node)
                    ),
                    json!({ "pending": brief(&tx, p)? }),
                ));
            }
            tx.execute(
                "UPDATE nodes SET pending_to = ?1 WHERE id = ?2",
                params![target, node.id],
            )?;
            touch(&tx, node.id)?;
            event(&tx, node.id, "plan", json!({ "to": target }))?;
        } else {
            apply_move(&tx, &node, target, "move")?;
        }
        tx.commit()?;
        show(&self.conn, node.id)
    }

    pub fn pending(&self) -> Result<Value> {
        let mut moves = Vec::new();
        let rows: Vec<(i64, i64)> = pairs(
            &self.conn,
            "SELECT id, pending_to FROM nodes WHERE pending_to IS NOT NULL AND state != 'gone' ORDER BY id",
        )?;
        for (id, to) in rows {
            moves.push(json!({ "node": brief(&self.conn, id)?, "to": brief(&self.conn, to)? }));
        }
        Ok(json!({ "pending": moves }))
    }

    pub fn done(&mut self, reference: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let node = load(&tx, resolve(&tx, reference, false)?)?;
        let target = node
            .pending_to
            .ok_or_else(|| refused(format!("{} has no pending move", label(&node)), Value::Null))?;
        apply_move(&tx, &node, target, "done")?;
        tx.commit()?;
        show(&self.conn, node.id)
    }

    pub fn cancel(&mut self, reference: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let node = load(&tx, resolve(&tx, reference, false)?)?;
        let target = node
            .pending_to
            .ok_or_else(|| refused(format!("{} has no pending move", label(&node)), Value::Null))?;
        tx.execute(
            "UPDATE nodes SET pending_to = NULL WHERE id = ?1",
            [node.id],
        )?;
        touch(&tx, node.id)?;
        event(&tx, node.id, "cancel", json!({ "to": target }))?;
        tx.commit()?;
        show(&self.conn, node.id)
    }

    pub fn dispose(&mut self, reference: &str, disposition: Disposition) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let node = load(&tx, resolve(&tx, reference, false)?)?;
        if node.state != State::Active {
            return Err(refused(
                format!(
                    "{} is already a candidate; use `ev gone` or `ev restore`",
                    label(&node)
                ),
                Value::Null,
            ));
        }
        require_no_active_inside(&tx, &node)?;
        set_candidate(&tx, node.id, disposition)?;
        tx.commit()?;
        show(&self.conn, node.id)
    }

    pub fn restore(&mut self, reference: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let node = load(&tx, resolve(&tx, reference, false)?)?;
        if node.state != State::Candidate {
            return Err(refused(
                format!("{} is not a candidate", label(&node)),
                Value::Null,
            ));
        }
        tx.execute(
            "UPDATE nodes SET state = 'active', disposition = NULL WHERE id = ?1",
            [node.id],
        )?;
        touch(&tx, node.id)?;
        event(&tx, node.id, "restore", json!({ "was": node.disposition }))?;
        tx.commit()?;
        show(&self.conn, node.id)
    }

    /// Final step of spec §3.3; one step from active when `--as` is given.
    pub fn gone(&mut self, reference: &str, disposition: Option<Disposition>) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let node = load(&tx, resolve(&tx, reference, false)?)?;
        if node.state == State::Active && disposition.is_none() {
            return Err(refused(
                format!(
                    "{} is active; say how it left with --as trash|give|sell",
                    label(&node)
                ),
                Value::Null,
            ));
        }
        let inside = require_no_active_inside(&tx, &node)?;
        let final_disposition = match (node.state, disposition) {
            (State::Active, Some(d)) => {
                set_candidate(&tx, node.id, d)?;
                d
            }
            (_, Some(d)) => d,
            (_, None) => node.disposition.unwrap_or(Disposition::Trash),
        };
        tx.execute(
            "UPDATE nodes SET state = 'gone', disposition = ?1, pending_to = NULL WHERE id = ?2",
            params![final_disposition.as_str(), node.id],
        )?;
        touch(&tx, node.id)?;
        event(
            &tx,
            node.id,
            "gone",
            json!({ "as": final_disposition, "dropped_pending": node.pending_to }),
        )?;
        // Candidates inside leave with it, each keeping its own disposition.
        for n in &inside {
            tx.execute(
                "UPDATE nodes SET state = 'gone', pending_to = NULL WHERE id = ?1",
                [n.id],
            )?;
            touch(&tx, n.id)?;
            event(
                &tx,
                n.id,
                "gone",
                json!({ "as": n.disposition, "with": node.id, "dropped_pending": n.pending_to }),
            )?;
        }
        tx.commit()?;
        show(&self.conn, node.id)
    }

    /// Every candidate grouped by disposition. A candidate held by another candidate leaves
    /// with it (spec §12.1), so it is listed under its outermost candidate as a part.
    pub fn disposals(&self, filter: Option<Disposition>) -> Result<Value> {
        let mut groups = serde_json::Map::new();
        for d in [Disposition::Trash, Disposition::Give, Disposition::Sell] {
            if filter.is_some_and(|f| f != d) {
                continue;
            }
            let list: Vec<i64> = ids(
                &self.conn,
                "SELECT id FROM nodes WHERE state = 'candidate' AND disposition = ?1 ORDER BY id",
                [d.as_str()],
            )?;
            let list = list
                .iter()
                .map(|id| brief(&self.conn, *id))
                .collect::<Result<Vec<_>>>()?;
            groups.insert(d.as_str().into(), json!(list));
        }
        Ok(json!({ "disposals": groups }))
    }

    pub fn mark_lost(&mut self, reference: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let node = load(&tx, resolve(&tx, reference, false)?)?;
        if node.kind == Kind::Home {
            return Err(refused("a home cannot be lost", Value::Null));
        }
        if !node.lost {
            tx.execute("UPDATE nodes SET lost = 1 WHERE id = ?1", [node.id])?;
            touch(&tx, node.id)?;
            event(&tx, node.id, "lost", json!({ "last_seen": node.parent_id }))?;
        }
        tx.commit()?;
        show(&self.conn, node.id)
    }

    pub fn found(&mut self, reference: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let node = load(&tx, resolve(&tx, reference, false)?)?;
        if !node.lost {
            return Err(refused(
                format!("{} is not lost", label(&node)),
                Value::Null,
            ));
        }
        if node.parent_id.is_none() {
            return Err(refused(
                format!(
                    "{} has no known place; use `ev move` to put it somewhere",
                    label(&node)
                ),
                Value::Null,
            ));
        }
        tx.execute("UPDATE nodes SET lost = 0 WHERE id = ?1", [node.id])?;
        touch(&tx, node.id)?;
        event(&tx, node.id, "found", json!({ "at": node.parent_id }))?;
        tx.commit()?;
        show(&self.conn, node.id)
    }

    pub fn lost_list(&self) -> Result<Value> {
        let mut out = Vec::new();
        for id in ids(
            &self.conn,
            "SELECT id FROM nodes WHERE lost = 1 AND state != 'gone' ORDER BY id",
            [],
        )? {
            let n = load(&self.conn, id)?;
            let last_seen = n.parent_id.map(|p| brief(&self.conn, p)).transpose()?;
            out.push(json!({ "node": brief(&self.conn, id)?, "last_seen": last_seen }));
        }
        Ok(json!({ "lost": out }))
    }

    pub fn history(&self, reference: &str) -> Result<Value> {
        let id = resolve(&self.conn, reference, true)?;
        let mut stmt = self
            .conn
            .prepare("SELECT at, type, data FROM events WHERE node_id = ?1 ORDER BY id")?;
        let events = stmt
            .query_map([id], |r| {
                let data: String = r.get(2)?;
                Ok(json!({
                    "at": r.get::<_, String>(0)?,
                    "type": r.get::<_, String>(1)?,
                    "data": serde_json::from_str::<Value>(&data).unwrap_or(Value::Null),
                }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(json!({ "node": brief(&self.conn, id)?, "events": events }))
    }
}

// ---------- reading ----------

fn db_enum<T: std::str::FromStr<Err = Error>>(idx: usize, s: String) -> rusqlite::Result<T> {
    s.parse::<T>().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn load(conn: &Connection, id: i64) -> Result<Node> {
    let node = conn
        .query_row(
            &format!("SELECT {NODE_COLUMNS} FROM nodes WHERE id = ?1"),
            [id],
            |r| {
                Ok(Node {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    kind: db_enum(2, r.get(2)?)?,
                    parent_id: r.get(3)?,
                    code: r.get(4)?,
                    address: r.get(5)?,
                    qty: r.get(6)?,
                    note: r.get(7)?,
                    theme: r.get(8)?,
                    fill: r.get(9)?,
                    tags: Vec::new(),
                    photos: Vec::new(),
                    state: db_enum(10, r.get(10)?)?,
                    disposition: r
                        .get::<_, Option<String>>(11)?
                        .map(|s| db_enum(11, s))
                        .transpose()?,
                    lost: r.get(12)?,
                    pending_to: r.get(13)?,
                    created_at: r.get(14)?,
                    updated_at: r.get(15)?,
                })
            },
        )
        .optional()?;
    let mut node = node.ok_or_else(|| Error::NotFound(format!("no node with id {id}")))?;
    node.tags = strings(
        conn,
        "SELECT tag FROM tags WHERE node_id = ?1 ORDER BY tag",
        id,
    )?;
    node.photos = strings(
        conn,
        "SELECT path FROM photos WHERE node_id = ?1 ORDER BY position",
        id,
    )?;
    Ok(node)
}

fn strings(conn: &Connection, sql: &str, id: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(rows)
}

fn ids<P: rusqlite::Params>(conn: &Connection, sql: &str, p: P) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map(p, |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<i64>>>()?;
    Ok(rows)
}

fn pairs(conn: &Connection, sql: &str) -> Result<Vec<(i64, i64)>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn path(conn: &Connection, id: i64) -> Result<Vec<PathSegment>> {
    let mut segments = Vec::new();
    let mut cur = Some(id);
    while let Some(c) = cur {
        if segments.len() > MAX_DEPTH {
            return Err(Error::Internal(format!(
                "parent chain of node {id} does not end"
            )));
        }
        let (code, name, parent): (Option<String>, String, Option<i64>) = conn.query_row(
            "SELECT code, name, parent_id FROM nodes WHERE id = ?1",
            [c],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        segments.push(PathSegment { id: c, code, name });
        cur = parent;
    }
    segments.reverse();
    Ok(segments)
}

fn path_text(segments: &[PathSegment]) -> String {
    segments
        .iter()
        .map(|s| s.code.clone().unwrap_or_else(|| s.name.clone()))
        .collect::<Vec<_>>()
        .join(" › ")
}

fn label(n: &Node) -> String {
    match &n.code {
        Some(c) => format!("{c} ({})", n.name),
        None => format!("#{} {}", n.id, n.name),
    }
}

fn brief(conn: &Connection, id: i64) -> Result<NodeRef> {
    let n = load(conn, id)?;
    let segments = path(conn, id)?;
    Ok(NodeRef {
        id,
        code: n.code,
        name: n.name,
        kind: n.kind,
        state: n.state,
        lost: n.lost,
        disposition: n.disposition,
        qty: n.qty,
        path_text: path_text(&segments),
        path: segments,
    })
}

fn show(conn: &Connection, id: i64) -> Result<Value> {
    let n = load(conn, id)?;
    let segments = path(conn, id)?;
    let mut node = serde_json::to_value(&n).map_err(|e| Error::Internal(e.to_string()))?;
    node["path_text"] = json!(path_text(&segments));
    node["path"] = json!(segments);
    let children: Vec<i64> = ids(
        conn,
        "SELECT id FROM nodes WHERE parent_id = ?1 AND state != 'gone' ORDER BY id",
        [id],
    )?;
    let children = children
        .iter()
        .map(|c| brief(conn, *c))
        .collect::<Result<Vec<_>>>()?;
    let pending = n.pending_to.map(|p| brief(conn, p)).transpose()?;
    let last_seen = match (n.lost, n.parent_id) {
        (true, Some(p)) => Some(brief(conn, p)?),
        _ => None,
    };
    Ok(json!({ "node": node, "children": children, "pending": pending, "last_seen": last_seen }))
}

fn subtree(conn: &Connection, id: i64, depth: usize) -> Result<Value> {
    let n = load(conn, id)?;
    let mut v = json!({
        "id": n.id, "code": n.code, "name": n.name, "kind": n.kind,
        "state": n.state, "lost": n.lost,
    });
    if let Some(q) = n.qty {
        v["qty"] = json!(q);
    }
    if let Some(d) = n.disposition {
        v["disposition"] = json!(d);
    }
    if let Some(p) = n.pending_to {
        v["pending_to"] = json!(p);
    }
    v["updated_at"] = json!(n.updated_at);
    let children = if depth == 0 {
        Vec::new()
    } else {
        let kids: Vec<i64> = ids(
            conn,
            "SELECT id FROM nodes WHERE parent_id = ?1 AND state != 'gone' ORDER BY id",
            [id],
        )?;
        kids.iter()
            .map(|k| subtree(conn, *k, depth - 1))
            .collect::<Result<Vec<_>>>()?
    };
    v["children"] = json!(children);
    Ok(v)
}

// ---------- references (spec §4, §11.4) ----------

fn resolve(conn: &Connection, reference: &str, include_gone: bool) -> Result<i64> {
    let r = reference.trim();
    if r.is_empty() {
        return Err(Error::Usage("empty reference".into()));
    }
    if r.chars().all(|c| c.is_ascii_digit()) {
        let state: Option<String> = r
            .parse::<i64>()
            .ok()
            .map(|id| {
                conn.query_row("SELECT state FROM nodes WHERE id = ?1", [id], |x| x.get(0))
                    .optional()
            })
            .transpose()?
            .flatten();
        return match state {
            Some(s) if include_gone || s != "gone" => Ok(r.parse().unwrap_or_default()),
            _ => Err(Error::NotFound(format!("no node with id {r}"))),
        };
    }
    let wanted = fold(r);
    let mut stmt = conn.prepare("SELECT id, code, name, state FROM nodes ORDER BY id")?;
    let rows: Vec<(i64, Option<String>, String, String)> = stmt
        .query_map([], |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?, x.get(3)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let visible: Vec<_> = rows
        .into_iter()
        .filter(|x| include_gone || x.3 != "gone")
        .collect();

    let code_hits: Vec<_> = visible
        .iter()
        .filter(|x| x.1.as_deref().map(fold) == Some(wanted.clone()))
        .collect();
    if !code_hits.is_empty() {
        if code_hits.len() == 1 {
            return Ok(code_hits[0].0);
        }
        let active: Vec<_> = code_hits.iter().filter(|x| x.3 != "gone").collect();
        if active.len() == 1 {
            return Ok(active[0].0);
        }
        return ambiguous(conn, r, code_hits.iter().map(|x| x.0));
    }
    let name_hits: Vec<_> = visible.iter().filter(|x| fold(&x.2) == wanted).collect();
    match name_hits.len() {
        0 => Err(Error::NotFound(format!(
            "no node matches `{r}`; search with `ev find` and retry with an id"
        ))),
        1 => Ok(name_hits[0].0),
        _ => ambiguous(conn, r, name_hits.iter().map(|x| x.0)),
    }
}

fn ambiguous(conn: &Connection, r: &str, hits: impl Iterator<Item = i64>) -> Result<i64> {
    let candidates = hits
        .map(|id| {
            brief(conn, id)
                .and_then(|b| serde_json::to_value(b).map_err(|e| Error::Internal(e.to_string())))
        })
        .collect::<Result<Vec<_>>>()?;
    Err(Error::Ambiguous {
        message: format!("`{r}` matches {} nodes; retry with an id", candidates.len()),
        candidates,
    })
}

// ---------- writing ----------

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn touch(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE nodes SET updated_at = ?1 WHERE id = ?2",
        params![now(), id],
    )?;
    Ok(())
}

fn event(conn: &Connection, id: i64, kind: &str, data: Value) -> Result<()> {
    conn.execute(
        "INSERT INTO events (node_id, at, type, data) VALUES (?1, ?2, ?3, ?4)",
        params![id, now(), kind, data.to_string()],
    )?;
    Ok(())
}

fn brief_json(conn: &Connection, id: i64) -> Result<Value> {
    serde_json::to_value(brief(conn, id)?).map_err(|e| Error::Internal(e.to_string()))
}

/// Validates a code and returns its folded form (spec §3.2 rule 5, §11.3).
fn check_code(conn: &Connection, code: &str, except: Option<i64>) -> Result<String> {
    let c = code.trim();
    if c.is_empty() {
        return Err(Error::Usage("code is empty".into()));
    }
    if c.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(refused(
            format!("code `{c}` is only digits and would read as an id"),
            json!({ "code": c }),
        ));
    }
    let folded = fold(c);
    let clash: Option<i64> = conn
        .query_row(
            "SELECT id FROM nodes WHERE code_folded = ?1 AND state != 'gone' AND id != ?2",
            params![folded, except.unwrap_or(-1)],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(other) = clash {
        return Err(refused(
            format!("code `{c}` is already in use"),
            json!({ "node": brief_json(conn, other)? }),
        ));
    }
    Ok(folded)
}

fn is_descendant(conn: &Connection, node: i64, ancestor: i64) -> Result<bool> {
    let mut cur: Option<i64> =
        conn.query_row("SELECT parent_id FROM nodes WHERE id = ?1", [node], |r| {
            r.get(0)
        })?;
    let mut steps = 0;
    while let Some(c) = cur {
        if c == ancestor {
            return Ok(true);
        }
        steps += 1;
        if steps > MAX_DEPTH {
            return Err(Error::Internal(format!(
                "parent chain of node {node} does not end"
            )));
        }
        cur = conn.query_row("SELECT parent_id FROM nodes WHERE id = ?1", [c], |r| {
            r.get(0)
        })?;
    }
    Ok(false)
}

/// Placement rules of spec §3.2 (1–4, 7) and §11.10.
fn check_placement(
    conn: &Connection,
    kind: Kind,
    parent: Option<i64>,
    lost: bool,
    moving: Option<i64>,
) -> Result<()> {
    if kind == Kind::Home {
        return match parent {
            Some(_) => Err(refused(
                "a home cannot be placed inside another node",
                Value::Null,
            )),
            None if lost => Err(refused("a home cannot be lost", Value::Null)),
            None => Ok(()),
        };
    }
    let Some(pid) = parent else {
        return if lost {
            Ok(())
        } else {
            Err(refused(
                format!("a {kind} needs a place: give --in, or --lost if its place is unknown"),
                Value::Null,
            ))
        };
    };
    let p = load(conn, pid)?;
    if p.state == State::Gone {
        return Err(refused(
            format!("{} is gone and cannot hold anything", label(&p)),
            Value::Null,
        ));
    }
    if kind == Kind::Room && !matches!(p.kind, Kind::Home | Kind::Room) {
        return Err(refused(
            format!(
                "a room can only be inside a home or another room, not a {}",
                p.kind
            ),
            json!({ "parent": brief_json(conn, pid)? }),
        ));
    }
    if let Some(m) = moving
        && (pid == m || is_descendant(conn, pid, m)?)
    {
        return Err(refused(
            "a node cannot be moved into itself or into something it contains",
            json!({ "target": brief_json(conn, pid)? }),
        ));
    }
    Ok(())
}

fn check_ranges(qty: Option<i64>, fill: Option<i64>) -> Result<()> {
    if qty.is_some_and(|q| q < 1) {
        return Err(Error::Usage("qty must be at least 1".into()));
    }
    if fill.is_some_and(|f| !(0..=100).contains(&f)) {
        return Err(Error::Usage("fill must be between 0 and 100".into()));
    }
    Ok(())
}

fn normalize_tag(t: &str) -> Result<String> {
    let t = t.trim().to_lowercase();
    if t.is_empty() {
        return Err(Error::Usage("tag is empty".into()));
    }
    Ok(t)
}

fn absolute(p: &str) -> Result<String> {
    let p = p.trim();
    if p.is_empty() {
        return Err(Error::Usage("photo path is empty".into()));
    }
    std::path::absolute(p)
        .map(|a| a.to_string_lossy().into_owned())
        .map_err(|e| Error::Usage(format!("photo path `{p}`: {e}")))
}

fn non_empty(v: &Option<String>) -> Option<String> {
    v.as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn add_one(conn: &Connection, new: &NewNode, parent: Option<i64>) -> Result<i64> {
    let kind: Kind = new.kind.parse()?;
    let name = new.name.trim();
    if name.is_empty() {
        return Err(Error::Usage("name is empty".into()));
    }
    check_ranges(new.qty, new.fill)?;
    let address = non_empty(&new.address);
    if address.is_some() && kind != Kind::Home {
        return Err(refused("only a home has an address", Value::Null));
    }
    check_placement(conn, kind, parent, new.lost, None)?;
    let code = non_empty(&new.code);
    let code_folded = code
        .as_deref()
        .map(|c| check_code(conn, c, None))
        .transpose()?;
    let tags = new
        .tags
        .iter()
        .map(|t| normalize_tag(t))
        .collect::<Result<Vec<_>>>()?;
    let photos = new
        .photos
        .iter()
        .map(|p| absolute(p))
        .collect::<Result<Vec<_>>>()?;
    let at = now();
    conn.execute(
        "INSERT INTO nodes (name, kind, parent_id, code, code_folded, address, qty, note, theme, fill, lost, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
        params![
            name,
            kind.as_str(),
            parent,
            code,
            code_folded,
            address,
            new.qty,
            non_empty(&new.note),
            non_empty(&new.theme),
            new.fill,
            new.lost,
            at
        ],
    )?;
    let id = conn.last_insert_rowid();
    for t in &tags {
        conn.execute(
            "INSERT OR IGNORE INTO tags (node_id, tag) VALUES (?1, ?2)",
            params![id, t],
        )?;
    }
    for (i, p) in photos.iter().enumerate() {
        conn.execute(
            "INSERT INTO photos (node_id, position, path) VALUES (?1, ?2, ?3)",
            params![id, i as i64, p],
        )?;
    }
    event(
        conn,
        id,
        "create",
        json!({ "name": name, "kind": kind, "parent": parent, "code": code, "lost": new.lost }),
    )?;
    Ok(id)
}

fn apply_move(conn: &Connection, node: &Node, target: i64, kind: &str) -> Result<()> {
    check_placement(conn, node.kind, Some(target), false, Some(node.id))?;
    conn.execute(
        "UPDATE nodes SET parent_id = ?1, pending_to = NULL, lost = 0 WHERE id = ?2",
        params![target, node.id],
    )?;
    touch(conn, node.id)?;
    let dropped = if kind == "move" {
        node.pending_to
    } else {
        None
    };
    event(
        conn,
        node.id,
        kind,
        json!({ "from": node.parent_id, "to": target, "dropped_pending": dropped, "was_lost": node.lost }),
    )?;
    Ok(())
}

/// Every non-gone node below `id`, depth first.
fn live_descendants(conn: &Connection, id: i64) -> Result<Vec<Node>> {
    let mut out = Vec::new();
    let mut stack = vec![id];
    while let Some(cur) = stack.pop() {
        let kids: Vec<i64> = ids(
            conn,
            "SELECT id FROM nodes WHERE parent_id = ?1 AND state != 'gone' ORDER BY id DESC",
            [cur],
        )?;
        for k in kids {
            if out.len() > MAX_DEPTH {
                return Err(Error::Internal(format!(
                    "subtree of node {id} does not end"
                )));
            }
            out.push(load(conn, k)?);
            stack.push(k);
        }
    }
    Ok(out)
}

/// Refuses when anything below `node` is still active (spec §3.3, §12.1); candidates
/// below it are already set aside and do not block.
fn require_no_active_inside(conn: &Connection, node: &Node) -> Result<Vec<Node>> {
    let below = live_descendants(conn, node.id)?;
    let active: Vec<i64> = below
        .iter()
        .filter(|n| n.state == State::Active)
        .map(|n| n.id)
        .collect();
    if active.is_empty() {
        return Ok(below);
    }
    let children = active
        .iter()
        .map(|k| brief_json(conn, *k))
        .collect::<Result<Vec<_>>>()?;
    Err(refused(
        format!(
            "{} still holds {} active node(s); move or dispose of them first",
            label(node),
            active.len()
        ),
        json!({ "children": children }),
    ))
}

fn set_candidate(conn: &Connection, id: i64, d: Disposition) -> Result<()> {
    conn.execute(
        "UPDATE nodes SET state = 'candidate', disposition = ?1 WHERE id = ?2",
        params![d.as_str(), id],
    )?;
    touch(conn, id)?;
    event(conn, id, "dispose", json!({ "as": d }))?;
    Ok(())
}

fn field_value(n: &Node, field: &str) -> Value {
    match field {
        "name" => json!(n.name),
        "code" => json!(n.code),
        "kind" => json!(n.kind),
        "address" => json!(n.address),
        "qty" => json!(n.qty),
        "note" => json!(n.note),
        "theme" => json!(n.theme),
        "fill" => json!(n.fill),
        "tags" => json!(n.tags),
        "photos" => json!(n.photos),
        _ => Value::Null,
    }
}

fn parse_int(field: &str, value: &str) -> Result<Option<i64>> {
    let v = value.trim();
    if v.is_empty() {
        return Ok(None);
    }
    v.parse::<i64>()
        .map(Some)
        .map_err(|_| Error::Usage(format!("{field} must be an integer, got `{v}`")))
}

fn apply_edit(conn: &Connection, n: &Node, field: &str, value: &str) -> Result<()> {
    let text = |v: &str| -> Option<String> { Some(v.trim().to_string()).filter(|s| !s.is_empty()) };
    match field {
        "name" => {
            let v = text(value).ok_or_else(|| Error::Usage("name cannot be empty".into()))?;
            conn.execute("UPDATE nodes SET name = ?1 WHERE id = ?2", params![v, n.id])?;
        }
        "code" => {
            let v = text(value);
            let folded = v
                .as_deref()
                .map(|c| check_code(conn, c, Some(n.id)))
                .transpose()?;
            conn.execute(
                "UPDATE nodes SET code = ?1, code_folded = ?2 WHERE id = ?3",
                params![v, folded, n.id],
            )?;
        }
        "kind" => {
            let k: Kind = value.parse()?;
            check_placement(conn, k, n.parent_id, n.lost, None)?;
            if n.address.is_some() && k != Kind::Home {
                return Err(refused(
                    "only a home has an address; clear it first",
                    Value::Null,
                ));
            }
            if !matches!(k, Kind::Home | Kind::Room) {
                let rooms: Vec<i64> = ids(
                    conn,
                    "SELECT id FROM nodes WHERE parent_id = ?1 AND kind = 'room' AND state != 'gone'",
                    [n.id],
                )?;
                if !rooms.is_empty() {
                    return Err(refused(
                        format!("{} holds rooms, so it must stay a home or a room", label(n)),
                        Value::Null,
                    ));
                }
            }
            conn.execute(
                "UPDATE nodes SET kind = ?1 WHERE id = ?2",
                params![k.as_str(), n.id],
            )?;
        }
        "address" => {
            let v = text(value);
            if v.is_some() && n.kind != Kind::Home {
                return Err(refused("only a home has an address", Value::Null));
            }
            conn.execute(
                "UPDATE nodes SET address = ?1 WHERE id = ?2",
                params![v, n.id],
            )?;
        }
        "note" | "theme" => {
            conn.execute(
                &format!("UPDATE nodes SET {field} = ?1 WHERE id = ?2"),
                params![text(value), n.id],
            )?;
        }
        "qty" | "fill" => {
            let v = parse_int(field, value)?;
            let (q, f) = if field == "qty" { (v, None) } else { (None, v) };
            check_ranges(q, f)?;
            conn.execute(
                &format!("UPDATE nodes SET {field} = ?1 WHERE id = ?2"),
                params![v, n.id],
            )?;
        }
        "tags" => {
            let (op, t) = split_op(field, value)?;
            let t = normalize_tag(t)?;
            if op == '+' {
                conn.execute(
                    "INSERT OR IGNORE INTO tags (node_id, tag) VALUES (?1, ?2)",
                    params![n.id, t],
                )?;
            } else {
                conn.execute(
                    "DELETE FROM tags WHERE node_id = ?1 AND tag = ?2",
                    params![n.id, t],
                )?;
            }
        }
        "photos" => {
            let (op, p) = split_op(field, value)?;
            let p = absolute(p)?;
            if op == '+' {
                let next: i64 = conn.query_row(
                    "SELECT COALESCE(MAX(position) + 1, 0) FROM photos WHERE node_id = ?1",
                    [n.id],
                    |r| r.get(0),
                )?;
                conn.execute(
                    "INSERT INTO photos (node_id, position, path) VALUES (?1, ?2, ?3)",
                    params![n.id, next, p],
                )?;
            } else {
                conn.execute(
                    "DELETE FROM photos WHERE node_id = ?1 AND path = ?2",
                    params![n.id, p],
                )?;
            }
        }
        other => {
            return Err(Error::Usage(format!(
                "unknown or read-only field `{other}`; editable: name, code, kind, address, qty, note, theme, fill, tags, photos"
            )));
        }
    }
    Ok(())
}

fn split_op<'a>(field: &str, value: &'a str) -> Result<(char, &'a str)> {
    let v = value.trim();
    match v.chars().next() {
        Some(c @ ('+' | '-')) => Ok((c, &v[1..])),
        _ => Err(Error::Usage(format!(
            "{field} takes +value or -value, got `{v}`"
        ))),
    }
}
