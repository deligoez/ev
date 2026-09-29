use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::error::refused;
use crate::model::{Disposition, Kind, NewNode, Node, NodeRef, PathSegment, State};
use crate::{Error, Result, fold};

/// The schema version this build writes (`PRAGMA user_version`).
pub const SCHEMA_VERSION: i64 = 9;

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

/// Places (spec §13): a person, a household or anywhere outside the tree, with aliases.
const SCHEMA_V2: &str = "
BEGIN;
CREATE TABLE places (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE TABLE place_aliases (
    place_id INTEGER NOT NULL REFERENCES places(id),
    alias TEXT NOT NULL,
    alias_folded TEXT NOT NULL UNIQUE
);
ALTER TABLE nodes ADD COLUMN owner_place INTEGER REFERENCES places(id);
ALTER TABLE nodes ADD COLUMN with_place INTEGER REFERENCES places(id);
ALTER TABLE nodes ADD COLUMN to_place INTEGER REFERENCES places(id);
PRAGMA user_version = 2;
COMMIT;
";

/// Placement rules the agent must weigh on every suggestion (spec §14).
const SCHEMA_V3: &str = "
BEGIN;
CREATE TABLE rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    text TEXT NOT NULL,
    created_at TEXT NOT NULL
);
PRAGMA user_version = 3;
COMMIT;
";

/// Holders whose contents were never inventoried (spec §15).
const SCHEMA_V4: &str = "
BEGIN;
ALTER TABLE nodes ADD COLUMN unknown INTEGER NOT NULL DEFAULT 0;
PRAGMA user_version = 4;
COMMIT;
";

/// Photo provenance (spec §16): the stored original a crop was cut from, the crop, a note.
const SCHEMA_V5: &str = "
BEGIN;
ALTER TABLE photos ADD COLUMN source TEXT;
ALTER TABLE photos ADD COLUMN crop TEXT;
ALTER TABLE photos ADD COLUMN note TEXT;
ALTER TABLE photos ADD COLUMN added_at TEXT;
PRAGMA user_version = 5;
COMMIT;
";

/// The tidy-up plan (spec §17): how far each place has been gone through, what was noticed
/// there, an ordered work list, and settings such as the household's goal.
const SCHEMA_V6: &str = "
BEGIN;
CREATE TABLE reviews (
    node_id INTEGER PRIMARY KEY REFERENCES nodes(id),
    status TEXT NOT NULL,
    at TEXT NOT NULL,
    note TEXT
);
CREATE TABLE observations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    text TEXT NOT NULL,
    photo TEXT,
    at TEXT NOT NULL
);
CREATE INDEX observations_node ON observations(node_id);
CREATE TABLE tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    why TEXT NOT NULL,
    rank INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    note TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    closed_at TEXT
);
CREATE TABLE task_nodes (
    task_id INTEGER NOT NULL REFERENCES tasks(id),
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    PRIMARY KEY (task_id, node_id)
);
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
PRAGMA user_version = 6;
COMMIT;
";

/// Things to do that hang on one node (spec §18): a label to print, broken, a use-by date, a
/// sale in progress — one row per node and kind — and a list of things to buy or make.
const SCHEMA_V7: &str = "
BEGIN;
CREATE TABLE marks (
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    kind TEXT NOT NULL,
    value TEXT,
    amount INTEGER,
    note TEXT,
    at TEXT NOT NULL,
    PRIMARY KEY (node_id, kind)
);
CREATE TABLE needs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    text TEXT NOT NULL,
    qty INTEGER,
    make INTEGER NOT NULL DEFAULT 0,
    for_node INTEGER REFERENCES nodes(id),
    status TEXT NOT NULL DEFAULT 'open',
    note TEXT,
    created_at TEXT NOT NULL,
    closed_at TEXT
);
PRAGMA user_version = 7;
COMMIT;
";

/// Gridfinity-style holders (spec §22): a drawer's grid, and the rectangle of cells each box
/// in it covers. Columns and rows are 0-based here; people read them as A… and 1…, row 1 at
/// the back.
const SCHEMA_V8: &str = "
BEGIN;
CREATE TABLE grids (
    node_id INTEGER PRIMARY KEY REFERENCES nodes(id),
    cols INTEGER NOT NULL,
    rows INTEGER NOT NULL
);
CREATE TABLE cells (
    node_id INTEGER PRIMARY KEY REFERENCES nodes(id),
    col INTEGER NOT NULL,
    row INTEGER NOT NULL,
    width INTEGER NOT NULL,
    depth INTEGER NOT NULL
);
PRAGMA user_version = 8;
COMMIT;
";

/// A box's outer size (spec §23), `WxDxH` in the units the person uses (gridfinity units for
/// bins), so a fuller box can be matched with a bigger spare one.
const SCHEMA_V9: &str = "
BEGIN;
ALTER TABLE nodes ADD COLUMN size TEXT;
CREATE TABLE synonyms (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    words TEXT NOT NULL,
    created_at TEXT NOT NULL
);
PRAGMA user_version = 9;
COMMIT;
";

const NODE_COLUMNS: &str = "id, name, kind, parent_id, code, address, qty, note, theme, fill, \
     state, disposition, lost, pending_to, created_at, updated_at, \
     (SELECT name FROM places WHERE id = owner_place), \
     (SELECT name FROM places WHERE id = with_place), \
     (SELECT name FROM places WHERE id = to_place), unknown, size";

pub struct Inventory {
    pub(crate) conn: Connection,
    photo_dir: std::path::PathBuf,
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
        if version < 2 {
            conn.execute_batch(SCHEMA_V2)?;
        }
        if version < 3 {
            conn.execute_batch(SCHEMA_V3)?;
        }
        if version < 4 {
            conn.execute_batch(SCHEMA_V4)?;
        }
        if version < 5 {
            conn.execute_batch(SCHEMA_V5)?;
        }
        if version < 6 {
            conn.execute_batch(SCHEMA_V6)?;
        }
        if version < 7 {
            conn.execute_batch(SCHEMA_V7)?;
        }
        if version < 8 {
            conn.execute_batch(SCHEMA_V8)?;
        }
        if version < 9 {
            conn.execute_batch(SCHEMA_V9)?;
        }
        let photo_dir = path
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .join("photos");
        Ok(Self { conn, photo_dir })
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
        // A gone node is found by id only, and only its note may change: the record of why it
        // left belongs on it, while every other field describes a thing no longer here.
        let id = match resolve(&tx, reference, false) {
            Err(Error::NotFound(_)) if reference.trim().chars().all(|c| c.is_ascii_digit()) => {
                let id = resolve(&tx, reference, true)?;
                if let Some(a) = assignments
                    .iter()
                    .find(|a| a.split_once('=').is_none_or(|(f, _)| f.trim() != "note"))
                {
                    return Err(refused(
                        format!("node {id} is gone; only its note can change, not `{a}`"),
                        Value::Null,
                    ));
                }
                id
            }
            other => other?,
        };
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

    /// Gives several nodes new codes at once, so codes can be swapped or rotated when boxes
    /// change places in a grid: uniqueness is checked against the codes they end up with, not
    /// the ones they are leaving. An empty code clears it. All or nothing.
    pub fn recode(&mut self, pairs: &[(String, String)]) -> Result<Value> {
        if pairs.is_empty() {
            return Err(Error::Usage("give at least one <ref>=<code>".into()));
        }
        let tx = self.conn.transaction()?;
        let mut nodes = Vec::new();
        for (reference, code) in pairs {
            let n = load(&tx, resolve(&tx, reference, false)?)?;
            if nodes.iter().any(|(m, _): &(Node, &String)| m.id == n.id) {
                return Err(Error::Usage(format!("{} is given twice", label(&n))));
            }
            nodes.push((n, code));
        }
        let mut folded = std::collections::HashSet::new();
        for (_, code) in &nodes {
            let c = code.trim();
            if !c.is_empty() && !folded.insert(fold(c)) {
                return Err(Error::Usage(format!("code `{c}` is given twice")));
            }
        }
        for (n, _) in &nodes {
            tx.execute(
                "UPDATE nodes SET code = NULL, code_folded = NULL WHERE id = ?1",
                [n.id],
            )?;
        }
        let mut out = Vec::new();
        for (n, code) in &nodes {
            apply_edit(&tx, n, "code", code)?;
            let after = load(&tx, n.id)?;
            if after.code != n.code {
                touch(&tx, n.id)?;
                event(
                    &tx,
                    n.id,
                    "edit",
                    json!({ "code": { "before": n.code, "after": after.code } }),
                )?;
            }
            out.push(json!({ "id": n.id, "name": n.name, "before": n.code, "after": after.code }));
        }
        tx.commit()?;
        Ok(json!({ "recoded": out }))
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
        if disposition == Disposition::Mistake {
            return Err(Error::Usage(
                "a mistaken record is not set aside; close it with `ev gone --as mistake --why`"
                    .into(),
            ));
        }
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
        self.gone_because(reference, disposition, None)
    }

    /// `gone` with the reason recorded in the same step: it goes into the event and is
    /// appended to the note, where `ev show --include-gone` shows it later.
    pub fn gone_because(
        &mut self,
        reference: &str,
        disposition: Option<Disposition>,
        why: Option<&str>,
    ) -> Result<Value> {
        let why = why.map(str::trim).filter(|w| !w.is_empty());
        if disposition == Some(Disposition::Mistake) && why.is_none() {
            return Err(Error::Usage(
                "say why the record was a mistake with --why".into(),
            ));
        }
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
        if let Some(w) = why {
            let note = match node.note.as_deref() {
                Some(n) if !n.trim().is_empty() => format!("{n}\n{w}"),
                _ => w.to_string(),
            };
            tx.execute(
                "UPDATE nodes SET note = ?1 WHERE id = ?2",
                params![note, node.id],
            )?;
        }
        touch(&tx, node.id)?;
        event(
            &tx,
            node.id,
            "gone",
            json!({ "as": final_disposition, "why": why, "dropped_pending": node.pending_to }),
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
                "SELECT n.id FROM nodes n LEFT JOIN nodes p ON p.id = n.parent_id
                 WHERE n.state = 'candidate' AND n.disposition = ?1
                   AND (p.id IS NULL OR p.state != 'candidate')
                 ORDER BY n.id",
                [d.as_str()],
            )?;
            let mut entries = Vec::with_capacity(list.len());
            for id in list {
                let mut entry = brief_json(&self.conn, id)?;
                let parts = live_descendants(&self.conn, id)?
                    .into_iter()
                    .filter(|n| n.state == State::Candidate)
                    .map(|n| brief_json(&self.conn, n.id))
                    .collect::<Result<Vec<_>>>()?;
                entry["parts"] = json!(parts);
                entry["sale"] = crate::marks::mark(&self.conn, id, "sale")?;
                entries.push(entry);
            }
            groups.insert(d.as_str().into(), json!(entries));
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

pub(crate) fn load(conn: &Connection, id: i64) -> Result<Node> {
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
                    owner: r.get(16)?,
                    with: r.get(17)?,
                    to: r.get(18)?,
                    unknown: r.get(19)?,
                    size: r.get(20)?,
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

pub(crate) fn ids<P: rusqlite::Params>(conn: &Connection, sql: &str, p: P) -> Result<Vec<i64>> {
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

pub(crate) fn path(conn: &Connection, id: i64) -> Result<Vec<PathSegment>> {
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

pub(crate) fn path_text(segments: &[PathSegment]) -> String {
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

pub(crate) fn brief(conn: &Connection, id: i64) -> Result<NodeRef> {
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

pub(crate) fn show(conn: &Connection, id: i64) -> Result<Value> {
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
    let review = crate::plan::review_of(conn, id)?;
    let observations = crate::plan::observations_of(conn, id)?;
    let tasks = crate::plan::tasks_of(conn, id)?;
    let marks = crate::marks::marks_of(conn, id)?;
    let needs = crate::marks::needs_for(conn, id)?;
    let cells = crate::grid::cells_of(conn, id)?.map(|c| c.name());
    let grid = crate::grid::grid_json(conn, id)?;
    Ok(json!({
        "cells": cells,
        "grid": grid,
        "tasks": tasks,
        "marks": marks,
        "needs": needs,
        "node": node,
        "children": children,
        "pending": pending,
        "last_seen": last_seen,
        "review": review,
        "observations": observations,
    }))
}

/// Items anywhere below `id` that are not gone, counting each item's quantity.
fn item_total(conn: &Connection, id: i64) -> Result<i64> {
    Ok(conn.query_row(
        "WITH RECURSIVE d(id) AS (
             SELECT id FROM nodes WHERE parent_id = ?1 AND state != 'gone'
             UNION ALL
             SELECT n.id FROM nodes n JOIN d ON n.parent_id = d.id WHERE n.state != 'gone'
         )
         SELECT COALESCE(SUM(COALESCE(qty, 1)), 0) FROM nodes WHERE kind = 'item' AND id IN d",
        [id],
        |r| r.get(0),
    )?)
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
    for (k, val) in [("owner", &n.owner), ("with", &n.with), ("to", &n.to)] {
        if let Some(x) = val {
            v[k] = json!(x);
        }
    }
    if n.unknown {
        v["unknown"] = json!(true);
    }
    v["updated_at"] = json!(n.updated_at);
    v["items"] = json!(item_total(conn, id)?);
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

pub(crate) fn resolve(conn: &Connection, reference: &str, include_gone: bool) -> Result<i64> {
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
            Some(_) => Err(Error::NotFound(format!(
                "node {r} is gone; `ev show {r} --include-gone` or `ev history {r}` still find it"
            ))),
            None => Err(Error::NotFound(format!("no node with id {r}"))),
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

pub(crate) fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub(crate) fn touch(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE nodes SET updated_at = ?1 WHERE id = ?2",
        params![now(), id],
    )?;
    Ok(())
}

pub(crate) fn event(conn: &Connection, id: i64, kind: &str, data: Value) -> Result<()> {
    conn.execute(
        "INSERT INTO events (node_id, at, type, data) VALUES (?1, ?2, ?3, ?4)",
        params![id, now(), kind, data.to_string()],
    )?;
    Ok(())
}

pub(crate) fn brief_json(conn: &Connection, id: i64) -> Result<Value> {
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
    crate::marks::code_changed(conn, id, code.is_some())?;
    if let Some(s) = non_empty(&new.size) {
        conn.execute(
            "UPDATE nodes SET size = ?1 WHERE id = ?2",
            params![normalize_size(&s)?, id],
        )?;
    }
    if new.unknown {
        conn.execute("UPDATE nodes SET unknown = 1 WHERE id = ?1", [id])?;
    }
    for (column, text) in [("to_place", &new.to), ("owner_place", &new.owner)] {
        if let Some(t) = non_empty(text) {
            let place = place_or_create(conn, &t)?;
            conn.execute(
                &format!("UPDATE nodes SET {column} = ?1 WHERE id = ?2"),
                params![place, id],
            )?;
        }
    }
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
    // Cells are positions in the old holder's grid; they mean nothing anywhere else.
    if node.parent_id != Some(target) {
        conn.execute("DELETE FROM cells WHERE node_id = ?1", [node.id])?;
    }
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
        "to" => json!(n.to),
        "owner" => json!(n.owner),
        "with" => json!(n.with),
        "unknown" => json!(n.unknown),
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

pub(crate) fn apply_edit(conn: &Connection, n: &Node, field: &str, value: &str) -> Result<()> {
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
            if v != n.code {
                crate::marks::code_changed(conn, n.id, v.is_some())?;
            }
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
        "size" => {
            let v = text(value).map(|s| normalize_size(&s)).transpose()?;
            conn.execute("UPDATE nodes SET size = ?1 WHERE id = ?2", params![v, n.id])?;
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
        "unknown" => {
            let v = match value.trim() {
                "true" | "yes" | "1" => true,
                "false" | "no" | "0" | "" => false,
                other => {
                    return Err(Error::Usage(format!(
                        "unknown takes true or false, got `{other}`"
                    )));
                }
            };
            conn.execute(
                "UPDATE nodes SET unknown = ?1 WHERE id = ?2",
                params![v, n.id],
            )?;
        }
        "to" | "owner" | "with" => {
            let column = format!("{field}_place");
            let place = text(value).map(|t| place_or_create(conn, &t)).transpose()?;
            if field == "with" && place.is_some() && n.owner.is_some() {
                return Err(refused(
                    format!("{} is not ours; it cannot be lent out", label(n)),
                    Value::Null,
                ));
            }
            conn.execute(
                &format!("UPDATE nodes SET {column} = ?1 WHERE id = ?2"),
                params![place, n.id],
            )?;
        }
        other => {
            return Err(Error::Usage(format!(
                "unknown or read-only field `{other}`; editable: name, code, kind, address, qty, note, theme, fill, tags, photos, to, owner, with, unknown"
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

// ---------- places (spec §13) ----------

/// Folded place key; apostrophes are dropped so "Saliha'lar" and "Salihalar" match.
fn place_key(text: &str) -> String {
    fold(text).replace(['\'', '’'], "")
}

/// The place whose name or alias folds to `text`, if any.
fn find_place(conn: &Connection, text: &str) -> Result<Option<i64>> {
    let folded = place_key(text);
    if folded.is_empty() {
        return Err(Error::Usage("place name is empty".into()));
    }
    Ok(conn
        .query_row(
            "SELECT place_id FROM place_aliases WHERE alias_folded = ?1",
            [folded],
            |r| r.get(0),
        )
        .optional()?)
}

fn place_or_create(conn: &Connection, text: &str) -> Result<i64> {
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

fn resolve_place(conn: &Connection, text: &str) -> Result<i64> {
    find_place(conn, text)?.ok_or_else(|| {
        Error::NotFound(format!(
            "no place named `{}`; `ev place list` shows them",
            text.trim()
        ))
    })
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
            return Err(refused(
                format!("`{}` already names a place", name.trim()),
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
            return Err(refused(
                "both names already point to the same place",
                Value::Null,
            ));
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
        let tx = self.conn.transaction()?;
        let node = load(&tx, resolve(&tx, reference, false)?)?;
        if node.owner.is_some() {
            return Err(refused(
                format!("{} is not ours; it cannot be lent out", label(&node)),
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
            return Err(refused(
                format!("{} is not lent out", label(&node)),
                Value::Null,
            ));
        };
        tx.execute(
            "UPDATE nodes SET with_place = NULL WHERE id = ?1",
            [node.id],
        )?;
        touch(&tx, node.id)?;
        event(&tx, node.id, "back", json!({ "from": with }))?;
        tx.commit()?;
        show(&self.conn, node.id)
    }
}

fn add_alias(conn: &Connection, place: i64, alias: &str) -> Result<()> {
    let a = alias.trim();
    if let Some(other) = find_place(conn, a)? {
        if other == place {
            return Ok(());
        }
        return Err(refused(
            format!("`{a}` already names another place; use `ev place merge`"),
            json!({ "place": place_json(conn, other)? }),
        ));
    }
    conn.execute(
        "INSERT INTO place_aliases (place_id, alias, alias_folded) VALUES (?1, ?2, ?3)",
        params![place, a, place_key(a)],
    )?;
    Ok(())
}

// ---------- placement: rules, suggest, audit (spec §14) ----------

/// Words too common to say two items are alike.
const STOPWORDS: &[&str] = &[
    "icin",
    "ile",
    "veya",
    "gibi",
    "olan",
    "adet",
    "kutu",
    "kutusu",
    "plastik",
    "beyaz",
    "siyah",
    "mavi",
    "sari",
    "turuncu",
    "kucuk",
    "buyuk",
    "uzun",
    "kisa",
    "the",
    "and",
    "for",
    "with",
    "birkac",
    "metal",
    "olabilir",
    "seffaf",
    "diger",
    "kirmizi",
    "yesil",
    "gri",
    "mor",
    "mini",
    "renkli",
    "cesitli",
    "karisik",
    "tane",
    "kutulu",
    "uzerinde",
    "poset",
    "posette",
    "posetli",
    "posetlerde",
    "eski",
    "yeni",
    "iki",
    "tek",
    "muhtemelen",
    "belirsiz",
    "net",
    "degil",
    "parca",
    "parcalar",
    "parcasi",
    "gorunumlu",
    "benzeri",
    "turu",
    "tipi",
    "set",
    "seti",
    "bir",
    "cok",
    "az",
    "icerik",
    "fotografta",
    "fotograftan",
    "sayida",
    "bordo",
    "erkek",
];

fn words(text: &str) -> Vec<String> {
    fold(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3 && !STOPWORDS.contains(w))
        .map(str::to_string)
        .collect()
}

/// Stems a folded Turkish word might have, longest first: the word itself, the word without a
/// possessive ending ("-sı/-su" after a vowel, "-ı/-u" after a consonant, or "-ları/-leri"),
/// and that without a plural ("-lar/-ler"). Every stem keeps at least three letters.
fn stem_candidates(word: &str) -> Vec<String> {
    let is_vowel = |c: Option<char>| c.is_some_and(|c| "aeiou".contains(c));
    let cut = |w: &str, s: &str| {
        (w.ends_with(s) && w.chars().count() >= s.chars().count() + 3)
            .then(|| w[..w.len() - s.len()].to_string())
    };
    let mut out = vec![word.to_string()];
    let possessive = ["lari", "leri"]
        .iter()
        .find_map(|s| cut(word, s))
        .or_else(|| {
            ["si", "su"]
                .iter()
                .find_map(|s| cut(word, s).filter(|w| is_vowel(w.chars().last())))
        })
        .or_else(|| {
            ["i", "u"]
                .iter()
                .find_map(|s| cut(word, s).filter(|w| !is_vowel(w.chars().last())))
        });
    let base = possessive.clone().unwrap_or_else(|| word.to_string());
    out.extend(possessive);
    out.extend(["lar", "ler"].iter().find_map(|s| cut(&base, s)));
    out.dedup();
    out
}

/// Groups word forms for `audit`. A word's key is its shortest candidate stem that is either a
/// word seen on its own or a stem shared by two different words, so "vidası" joins "vida",
/// "kablolar" and "kablosu" meet at "kablo", but "kutu" stays "kutu" and "kartuşu" (folded
/// "kartusu") never collapses into "kart". The key is only for grouping, never shown.
fn stem_keys(vocab: &BTreeSet<String>) -> HashMap<String, String> {
    let mut sources: HashMap<String, BTreeSet<&str>> = HashMap::new();
    for w in vocab {
        for c in stem_candidates(w) {
            sources.entry(c).or_default().insert(w);
        }
    }
    vocab
        .iter()
        .map(|w| {
            let key = stem_candidates(w)
                .into_iter()
                .rev()
                .find(|c| vocab.contains(c) || sources.get(c).is_some_and(|s| s.len() >= 2))
                .unwrap_or_else(|| w.clone());
            (w.clone(), key)
        })
        .collect()
}

/// A query word matches an item word when they are equal or one starts with the other and the
/// shorter has at least four letters: "vida" finds "vidası", "kart" finds "kartı", but "boş"
/// does not find "bosch".
fn word_match(query: &str, word: &str) -> bool {
    let long_enough = |s: &str| s.chars().count() >= 4;
    query == word
        || (long_enough(query) && word.starts_with(query))
        || (long_enough(word) && query.starts_with(word))
}

fn node_text(n: &Node) -> String {
    let mut t = n.name.clone();
    for x in [&n.note, &n.theme, &n.code].into_iter().flatten() {
        t.push(' ');
        t.push_str(x);
    }
    for tag in &n.tags {
        t.push(' ');
        t.push_str(tag);
    }
    t
}

pub(crate) fn live_nodes(conn: &Connection) -> Result<Vec<Node>> {
    ids(
        conn,
        "SELECT id FROM nodes WHERE state != 'gone' ORDER BY id",
        [],
    )?
    .into_iter()
    .map(|id| load(conn, id))
    .collect()
}

/// Anything something can be put into: every node that is not a home and is either not an
/// item or already holds something.
fn is_holder(n: &Node, has_children: &std::collections::HashSet<i64>) -> bool {
    n.kind != Kind::Home && (n.kind != Kind::Item || has_children.contains(&n.id))
}

fn holder_json(conn: &Connection, n: &Node, all: &[Node]) -> Result<Value> {
    let segments = path(conn, n.id)?;
    let inside: Vec<&str> = all
        .iter()
        .filter(|c| c.parent_id == Some(n.id) && c.kind == Kind::Item)
        .map(|c| c.name.as_str())
        .collect();
    let mut v = json!({
        "id": n.id,
        "code": n.code,
        "name": n.name,
        "kind": n.kind,
        "path_text": path_text(&segments),
        "items": item_total(conn, n.id)?,
        "sample": inside.iter().take(6).collect::<Vec<_>>(),
    });
    for (k, val) in [("theme", &n.theme), ("note", &n.note)] {
        if let Some(x) = val {
            v[k] = json!(x);
        }
    }
    if let Some(f) = n.fill {
        v["fill"] = json!(f);
    }
    if n.lost {
        v["lost"] = json!(true);
    }
    if n.unknown {
        v["unknown"] = json!(true);
    }
    // A grid holder says how many cells are still free, and which: room for a new box.
    if let Some(g) = crate::grid::grid_json(conn, n.id)? {
        v["grid"] = json!({ "cols": g["cols"], "rows": g["rows"], "free": g["free"] });
    }
    // A box in a grid says where in it.
    if let Some(c) = crate::grid::cells_of(conn, n.id)? {
        v["cells"] = json!(c.name());
    }
    Ok(v)
}

pub(crate) fn rules_json(conn: &Connection) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare("SELECT id, text FROM rules ORDER BY id")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(json!({ "id": r.get::<_, i64>(0)?, "text": r.get::<_, String>(1)? }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

impl Inventory {
    pub fn rule_add(&mut self, text: &str) -> Result<Value> {
        let t = text.trim();
        if t.is_empty() {
            return Err(Error::Usage("rule text is empty".into()));
        }
        self.conn.execute(
            "INSERT INTO rules (text, created_at) VALUES (?1, ?2)",
            params![t, now()],
        )?;
        Ok(json!({ "rules": rules_json(&self.conn)? }))
    }

    pub fn rule_list(&self) -> Result<Value> {
        Ok(json!({ "rules": rules_json(&self.conn)? }))
    }

    pub fn rule_remove(&mut self, id: i64) -> Result<Value> {
        if self.conn.execute("DELETE FROM rules WHERE id = ?1", [id])? == 0 {
            return Err(Error::NotFound(format!("no rule with id {id}")));
        }
        Ok(json!({ "rules": rules_json(&self.conn)? }))
    }

    /// Where the inventory could be tidier: alike things split across places, holders without
    /// a theme, and items lying directly in a room or on furniture.
    pub fn audit(&self) -> Result<Value> {
        let all = live_nodes(&self.conn)?;
        let by_id: HashMap<i64, &Node> = all.iter().map(|n| (n.id, n)).collect();
        let has_children: std::collections::HashSet<i64> =
            all.iter().filter_map(|n| n.parent_id).collect();
        // Grouped by stem, so "vida", "vidası" and "vidalar" are one row; the row is named by
        // its shortest surface form and lists every form it merged.
        type Places = std::collections::BTreeMap<i64, Vec<String>>;
        let mut spread: std::collections::BTreeMap<String, (Places, BTreeSet<String>)> =
            Default::default();
        let item_words: Vec<(i64, &str, Vec<String>)> = all
            .iter()
            .filter(|n| n.kind == Kind::Item)
            .filter_map(|n| {
                let p = n.parent_id?;
                let mut ws = words(&n.name);
                ws.extend(n.tags.iter().flat_map(|t| words(t)));
                ws.retain(|w| w.chars().count() >= 4);
                Some((p, n.name.as_str(), ws))
            })
            .collect();
        let vocab: BTreeSet<String> = item_words
            .iter()
            .flat_map(|(_, _, ws)| ws.iter().cloned())
            .collect();
        let keys = stem_keys(&vocab);
        for (p, name, ws) in item_words {
            let mut seen = BTreeSet::new();
            for w in ws {
                let key = keys.get(&w).cloned().unwrap_or_else(|| w.clone());
                let entry = spread.entry(key.clone()).or_default();
                entry.1.insert(w);
                if seen.insert(key) {
                    entry.0.entry(p).or_default().push(name.to_string());
                }
            }
        }
        let mut spread: Vec<Value> = spread
            .into_values()
            .filter(|(places, _)| (2..=8).contains(&places.len()))
            .map(|(places, forms)| {
                let list = places
                    .iter()
                    .map(|(p, names)| {
                        let segs = path(&self.conn, *p)?;
                        Ok(json!({ "path_text": path_text(&segs), "items": names }))
                    })
                    .collect::<Result<Vec<_>>>()?;
                let word = forms
                    .iter()
                    .min_by_key(|f| (f.chars().count(), (*f).clone()))
                    .cloned()
                    .unwrap_or_default();
                Ok(json!({ "word": word, "forms": forms, "places": list }))
            })
            .collect::<Result<Vec<_>>>()?;
        spread.sort_by_key(|v| std::cmp::Reverse(v["places"].as_array().map_or(0, Vec::len)));
        spread.truncate(40);
        let no_theme = all
            .iter()
            .filter(|n| {
                is_holder(n, &has_children)
                    && n.kind != Kind::Room
                    && n.theme.is_none()
                    && n.state != State::Candidate
            })
            .filter(|n| {
                all.iter()
                    .any(|c| c.parent_id == Some(n.id) && c.kind == Kind::Item)
            })
            .map(|n| brief_json(&self.conn, n.id))
            .collect::<Result<Vec<_>>>()?;
        let loose = all
            .iter()
            .filter(|n| n.kind == Kind::Item)
            .filter(|n| {
                n.parent_id
                    .and_then(|p| by_id.get(&p))
                    .is_some_and(|p| matches!(p.kind, Kind::Home | Kind::Room | Kind::Furniture))
            })
            .map(|n| brief_json(&self.conn, n.id))
            .collect::<Result<Vec<_>>>()?;
        let unknown = all
            .iter()
            .filter(|n| n.unknown)
            .map(|n| brief_json(&self.conn, n.id))
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({ "spread": spread, "no_theme": no_theme, "loose": loose, "unknown": unknown }))
    }
}

impl Inventory {
    /// Undoes a `gone` recorded by mistake (spec §15); the reason is kept in the history.
    pub fn correct_gone(&mut self, reference: &str, why: &str) -> Result<Value> {
        let why = why.trim();
        if why.is_empty() {
            return Err(Error::Usage("say why the node was not really gone".into()));
        }
        let tx = self.conn.transaction()?;
        let node = load(&tx, resolve(&tx, reference, true)?)?;
        if node.state != State::Gone {
            return Err(refused(
                format!("{} is not gone", label(&node)),
                Value::Null,
            ));
        }
        if let Some(p) = node.parent_id {
            let parent = load(&tx, p)?;
            if parent.state == State::Gone {
                return Err(refused(
                    format!(
                        "{} left with {}; restore that first",
                        label(&node),
                        label(&parent)
                    ),
                    Value::Null,
                ));
            }
        }
        if let Some(code) = &node.code {
            check_code(&tx, code, Some(node.id))?;
        }
        tx.execute(
            "UPDATE nodes SET state = 'active', disposition = NULL WHERE id = ?1",
            [node.id],
        )?;
        touch(&tx, node.id)?;
        event(
            &tx,
            node.id,
            "restore",
            json!({ "correction": why, "was": node.disposition }),
        )?;
        tx.commit()?;
        show(&self.conn, node.id)
    }
}

impl Inventory {
    pub fn photo_dir(&self) -> &Path {
        &self.photo_dir
    }

    /// Copies a photo into the store and attaches it to a node; with `crop`, attaches only
    /// the cut-out and remembers the stored original it came from.
    pub fn photo_add(
        &mut self,
        reference: &str,
        file: &Path,
        crop: Option<crate::Crop>,
        note: Option<&str>,
    ) -> Result<Value> {
        self.photo_add_with(reference, file, crop, note, false)
    }

    /// Adds a photo. A whole (uncropped) photo already attached whole to another node is
    /// refused unless `whole` is set: a group photo belongs to the place, and the things in it
    /// get crops. The mistake this stops — one drawer photo on nine items — only shows once
    /// someone opens them.
    pub fn photo_add_with(
        &mut self,
        reference: &str,
        file: &Path,
        crop: Option<crate::Crop>,
        note: Option<&str>,
        whole: bool,
    ) -> Result<Value> {
        let id = resolve(&self.conn, reference, false)?;
        let original = crate::photo::store_file(&self.photo_dir, file)?;
        if crop.is_none() && !whole {
            let others = ids(
                &self.conn,
                "SELECT DISTINCT p.node_id FROM photos p JOIN nodes n ON n.id = p.node_id
                  WHERE p.path = ?1 AND p.crop IS NULL AND p.node_id != ?2 AND n.state != 'gone'
                  ORDER BY p.node_id",
                params![original.to_string_lossy(), id],
            )?;
            if !others.is_empty() {
                let nodes = others
                    .iter()
                    .map(|o| brief_json(&self.conn, *o))
                    .collect::<Result<Vec<_>>>()?;
                return Err(refused(
                    format!(
                        "this photo is already attached whole to {} other node(s); attach a --crop \
                         of the part that shows node {id}, or pass --whole if the whole view is meant",
                        others.len()
                    ),
                    json!({ "attached_to": nodes }),
                ));
            }
        }
        let (stored, source) = match crop {
            Some(c) => (
                crate::photo::store_crop(&self.photo_dir, &original, c)?,
                Some(original.to_string_lossy().into_owned()),
            ),
            None => (original, None),
        };
        let stored = stored.to_string_lossy().into_owned();
        let tx = self.conn.transaction()?;
        let next: i64 = tx.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM photos WHERE node_id = ?1",
            [id],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO photos (node_id, position, path, source, crop, note, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                next,
                stored,
                source,
                crop.map(|c| c.to_string()),
                note.map(str::trim),
                now()
            ],
        )?;
        touch(&tx, id)?;
        event(
            &tx,
            id,
            "photo",
            json!({ "path": stored, "crop": crop.map(|c| c.to_string()) }),
        )?;
        tx.commit()?;
        self.photo_list(&id.to_string())
    }

    /// Cuts one photo up among several nodes in one step: a crop for each `(reference, crop)`,
    /// and the whole photo on `place` when given (the drawer or box it shows). Every reference
    /// is resolved and every crop cut before anything is recorded, and the records go in one
    /// transaction, so a typo leaves nothing half attached. The whole-photo rule of
    /// `photo_add` applies to `place`.
    pub fn photo_cut(
        &mut self,
        file: &Path,
        place: Option<&str>,
        crops: &[(String, crate::Crop)],
        note: Option<&str>,
    ) -> Result<Value> {
        if place.is_none() && crops.is_empty() {
            return Err(Error::Usage(
                "give at least one <ref>=x,y,w,h, or --place <ref>".into(),
            ));
        }
        let place = place.map(|p| resolve(&self.conn, p, false)).transpose()?;
        let targets = crops
            .iter()
            .map(|(r, c)| Ok((resolve(&self.conn, r, false)?, *c)))
            .collect::<Result<Vec<_>>>()?;
        let original = crate::photo::store_file(&self.photo_dir, file)?;
        let original_text = original.to_string_lossy().into_owned();
        if let Some(pid) = place {
            let others = ids(
                &self.conn,
                "SELECT DISTINCT p.node_id FROM photos p JOIN nodes n ON n.id = p.node_id
                  WHERE p.path = ?1 AND p.crop IS NULL AND p.node_id != ?2 AND n.state != 'gone'
                  ORDER BY p.node_id",
                params![original_text, pid],
            )?;
            if !others.is_empty() {
                let nodes = others
                    .iter()
                    .map(|o| brief_json(&self.conn, *o))
                    .collect::<Result<Vec<_>>>()?;
                return Err(refused(
                    format!(
                        "this photo is already attached whole to {} other node(s)",
                        others.len()
                    ),
                    json!({ "attached_to": nodes }),
                ));
            }
        }
        let mut rows: Vec<(i64, String, Option<String>, Option<crate::Crop>)> = Vec::new();
        if let Some(pid) = place {
            rows.push((pid, original_text.clone(), None, None));
        }
        for (id, c) in targets {
            let cut = crate::photo::store_crop(&self.photo_dir, &original, c)?;
            rows.push((
                id,
                cut.to_string_lossy().into_owned(),
                Some(original_text.clone()),
                Some(c),
            ));
        }
        let tx = self.conn.transaction()?;
        let mut attached = Vec::new();
        for (id, stored, source, crop) in &rows {
            let next: i64 = tx.query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM photos WHERE node_id = ?1",
                [id],
                |r| r.get(0),
            )?;
            tx.execute(
                "INSERT INTO photos (node_id, position, path, source, crop, note, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id,
                    next,
                    stored,
                    source,
                    crop.map(|c| c.to_string()),
                    note.map(str::trim),
                    now()
                ],
            )?;
            touch(&tx, *id)?;
            event(
                &tx,
                *id,
                "photo",
                json!({ "path": stored, "crop": crop.map(|c| c.to_string()) }),
            )?;
            let mut b = brief_json(&tx, *id)?;
            b["photo"] = json!(next + 1);
            b["crop"] = json!(crop.map(|c| c.to_string()));
            b["path"] = json!(stored);
            attached.push(b);
        }
        tx.commit()?;
        Ok(json!({ "attached": attached }))
    }

    pub fn photo_list(&self, reference: &str) -> Result<Value> {
        let id = resolve(&self.conn, reference, true)?;
        let mut stmt = self.conn.prepare(
            "SELECT path, source, crop, note, added_at FROM photos WHERE node_id = ?1 ORDER BY position",
        )?;
        let photos = stmt
            .query_map([id], |r| {
                let path: String = r.get(0)?;
                Ok(json!({
                    "path": path,
                    "exists": Path::new(&path).exists(),
                    "source": r.get::<_, Option<String>>(1)?,
                    "crop": r.get::<_, Option<String>>(2)?,
                    "note": r.get::<_, Option<String>>(3)?,
                    "added_at": r.get::<_, Option<String>>(4)?,
                }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let photos: Vec<Value> = photos
            .into_iter()
            .enumerate()
            .map(|(i, mut p)| {
                p["n"] = json!(i + 1);
                p
            })
            .collect();
        Ok(json!({ "node": brief_json(&self.conn, id)?, "photos": photos }))
    }

    /// Detaches the n-th photo (1-based); the stored file stays, other nodes may share it.
    pub fn photo_remove(&mut self, reference: &str, n: usize) -> Result<Value> {
        let id = resolve(&self.conn, reference, false)?;
        let positions = ids(
            &self.conn,
            "SELECT position FROM photos WHERE node_id = ?1 ORDER BY position",
            [id],
        )?;
        let pos = n
            .checked_sub(1)
            .and_then(|i| positions.get(i))
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "photo {n} does not exist; it has {}",
                    positions.len()
                ))
            })?;
        self.conn.execute(
            "DELETE FROM photos WHERE node_id = ?1 AND position = ?2",
            params![id, pos],
        )?;
        self.photo_list(&id.to_string())
    }

    /// Copies every photo still referenced outside the store into it.
    pub fn photo_adopt(&mut self) -> Result<Value> {
        let dir = self.photo_dir.to_string_lossy().into_owned();
        let mut stmt = self
            .conn
            .prepare("SELECT node_id, position, path FROM photos ORDER BY node_id, position")?;
        let rows: Vec<(i64, i64, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        let (mut adopted, mut missing) = (0, Vec::new());
        for (node, pos, path) in rows {
            if path.starts_with(&dir) {
                continue;
            }
            if !Path::new(&path).exists() {
                missing.push(json!({ "node": node, "path": path }));
                continue;
            }
            let stored = crate::photo::store_file(&self.photo_dir, Path::new(&path))?;
            self.conn.execute(
                "UPDATE photos SET path = ?1, note = COALESCE(note, ?2) WHERE node_id = ?3 AND position = ?4",
                params![stored.to_string_lossy(), format!("adopted from {path}"), node, pos],
            )?;
            adopted += 1;
        }
        Ok(json!({ "adopted": adopted, "missing": missing, "store": dir }))
    }
}

#[cfg(test)]
mod migration_tests {
    use super::{Inventory, SCHEMA_V1};

    #[test]
    fn a_version_1_file_is_migrated_and_keeps_its_nodes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ev.db");
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute_batch(SCHEMA_V1).unwrap();
        c.execute(
            "INSERT INTO nodes (name, kind, created_at, updated_at) VALUES ('Ev', 'home', 'x', 'x')",
            [],
        )
        .unwrap();
        drop(c);
        let mut inv = Inventory::open(&path).unwrap();
        let v: i64 = rusqlite::Connection::open(&path)
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, super::SCHEMA_VERSION);
        inv.edit("Ev", &["owner=Mahmutlar".into()]).unwrap();
        assert_eq!(inv.node(1).unwrap().owner.as_deref(), Some("Mahmutlar"));
    }
}
