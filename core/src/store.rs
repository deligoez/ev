use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};

use crate::error::refused;
use crate::model::{Disposition, Kind, NewNode, Node, NodeRef, PathSegment, State};
use crate::{Error, Result, fold};

mod audit;
mod edit;
mod files;
pub(crate) mod past;
mod photos;
mod places;
mod schema;
mod search;

pub(crate) use audit::{holder_json, is_holder, live_nodes, parking_of, rules_json};
pub(crate) use edit::{apply_edit, parse_size};
use edit::{edit_in, field_value, normalize_size, size_in_name};
pub(crate) use places::place_errands;
use places::place_or_create;
use schema::*;

/// The schema version this build writes (`PRAGMA user_version`).
pub const SCHEMA_VERSION: i64 = 35;

/// Guards every upward walk against a corrupted parent chain.
const MAX_DEPTH: usize = 10_000;

const NODE_COLUMNS: &str = "id, name, kind, parent_id, code, address, qty, note, theme, fill, \
     state, disposition, lost, pending_to, created_at, updated_at, \
     (SELECT name FROM places WHERE id = owner_place), \
     (SELECT name FROM places WHERE id = with_place), \
     (SELECT name FROM places WHERE id = to_place), size, temporary, make, model, serial, thing, \
     waits_for, came_at";

pub struct Inventory {
    pub(crate) conn: Connection,
    photo_dir: std::path::PathBuf,
    /// Where documents are copied (purchases spec §3.5), beside the photos.
    pub(crate) doc_dir: std::path::PathBuf,
    /// Where `ev focus` leaves its request for `ev ui`: beside the database (`ev.db-focus.json`),
    /// since a message to the UI is no change to the inventory.
    pub(crate) focus_file: std::path::PathBuf,
    /// The word index of every live holder (placement), with the state of the data it was
    /// built from: see `Inventory::word_index`.
    pub(crate) word_index: crate::placement::WordIndex,
}

impl Drop for Inventory {
    /// After a write, fold the write-ahead log back into the database file, so a copy or a
    /// commit of `ev.db` alone holds every change. FULL waits (within the busy timeout) for a
    /// reader on an older snapshot, such as a running `ev ui`, instead of leaving pages behind.
    fn drop(&mut self) {
        if self.conn.total_changes() > 0 {
            let _ = self.conn.execute_batch("PRAGMA wal_checkpoint(FULL);");
        }
    }
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
        let mut conn = Connection::open(path)?;
        // Absolute, so a stored file never depends on the directory ev was started in.
        let home = path
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let home = std::path::absolute(home).unwrap_or_else(|_| home.to_path_buf());
        files::register(&conn, &home)?;
        // Waiting beats failing: under a burst of parallel calls (measured: two MCP servers and
        // the CLI, 60 writes among 40 reads) a writer waited past 5 s in rollback-journal mode;
        // 30 s took all 60.
        conn.busy_timeout(Duration::from_secs(30))?;
        // A deferred transaction that reads and then writes gets SQLITE_BUSY at once, without
        // waiting, when another writer got in between; taking the write lock up front lets the
        // busy timeout queue concurrent writers (an MCP client's parallel calls, the CLI).
        conn.set_transaction_behavior(TransactionBehavior::Immediate);
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(Error::NewerSchema {
                found: version,
                supported: SCHEMA_VERSION,
            });
        }
        // Write-ahead log: readers (ev ui, an agent's reads) and the writer no longer wait for
        // each other, only writers queue. The mode is kept in the file; `Drop` folds the log
        // back into the database after a write, so the file alone (the one git commits) is whole.
        // Switching an older file needs it to itself for a moment; when another process holds
        // it, this command runs in the old mode and a later one switches (measured: 1 of 60
        // writers lost the race while the file switched).
        let _ = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get::<_, String>(0));
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
        if version < 10 {
            conn.execute_batch(SCHEMA_V10)?;
        }
        if version < 11 {
            conn.execute_batch(SCHEMA_V11)?;
        }
        if version < 12 {
            conn.execute_batch(SCHEMA_V12)?;
        }
        if version < 13 {
            conn.execute_batch(SCHEMA_V13)?;
        }
        if version < 14 {
            conn.execute_batch(SCHEMA_V14)?;
        }
        if version < 15 {
            conn.execute_batch(SCHEMA_V15)?;
        }
        if version < 16 {
            conn.execute_batch(SCHEMA_V16)?;
        }
        if version < 17 {
            conn.execute_batch(SCHEMA_V17)?;
        }
        if version < 18 {
            conn.execute_batch(SCHEMA_V18)?;
        }
        if version < 19 {
            conn.execute_batch(SCHEMA_V19)?;
        }
        if version < 20 {
            conn.execute_batch(SCHEMA_V20)?;
        }
        if version < 21 {
            conn.execute_batch(SCHEMA_V21)?;
        }
        if version < 22 {
            conn.execute_batch(SCHEMA_V22)?;
        }
        if version < 23 {
            conn.execute_batch(SCHEMA_V23)?;
        }
        if version < 24 {
            conn.execute_batch(SCHEMA_V24)?;
        }
        if version < 25 {
            conn.execute_batch(SCHEMA_V25)?;
        }
        if version < 26 {
            conn.execute_batch(SCHEMA_V26)?;
        }
        if version < 27 {
            conn.execute_batch(SCHEMA_V27)?;
        }
        if version < 28 {
            conn.execute_batch(SCHEMA_V28)?;
        }
        if version < 29 {
            conn.execute_batch(SCHEMA_V29)?;
        }
        if version < 30 {
            conn.execute_batch(SCHEMA_V30)?;
        }
        if version < 31 {
            conn.execute_batch(SCHEMA_V31)?;
        }
        if version < 32 {
            // Codes compare by what the label means (spec/codes.md): `_` is `-`, a number's
            // leading zeros go. Each code's folded form is worked out again; the code stays.
            let tx = conn.unchecked_transaction()?;
            let codes: Vec<(i64, String)> = {
                let mut stmt = tx.prepare("SELECT id, code FROM nodes WHERE code IS NOT NULL")?;
                stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<_>>()?
            };
            for (id, code) in codes {
                tx.execute(
                    "UPDATE nodes SET code_folded = ?1 WHERE id = ?2",
                    params![crate::fold::fold_code(&code), id],
                )?;
            }
            tx.execute_batch("PRAGMA user_version = 32")?;
            tx.commit()?;
        }
        if version < 33 {
            conn.execute_batch(SCHEMA_V33)?;
        }
        if version < 34 {
            conn.execute_batch(SCHEMA_V34)?;
        }
        if version < 35 {
            conn.execute_batch(SCHEMA_V35)?;
        }
        // A migration changes the schema but no row, so `Drop` would leave it in the log: fold
        // it into the file now, so a commit of `ev.db` is on the new schema too.
        if version < SCHEMA_VERSION {
            let _ = conn.execute_batch("PRAGMA wal_checkpoint(FULL);");
        }
        Ok(Self {
            conn,
            photo_dir: home.join("photos"),
            doc_dir: home.join("docs"),
            word_index: std::cell::RefCell::new(None),
            focus_file: home.join(format!(
                "{}-focus.json",
                path.file_name()
                    .map_or_else(|| "ev.db".into(), |n| n.to_string_lossy())
            )),
        })
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
        // While the person still holds the thing: the purchases it could be (purchases spec §5).
        offer_purchases(&self.conn, id, show(&self.conn, id)?)
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
        let index = TreeIndex::build(&self.conn)?;
        match reference {
            Some(r) => {
                let id = resolve(&self.conn, r, false)?;
                Ok(json!({ "tree": [subtree(&index, id, depth)] }))
            }
            None => {
                let mut homes: Vec<i64> = index
                    .nodes
                    .values()
                    .filter(|n| n.kind == Kind::Home)
                    .map(|n| n.id)
                    .collect();
                homes.sort_unstable();
                let tree: Vec<Value> = homes.iter().map(|id| subtree(&index, *id, depth)).collect();
                // Whose place is not known: every lost thing, each with where it was last seen.
                let mut lost: Vec<&Node> = index.nodes.values().filter(|n| n.lost).collect();
                lost.sort_by_key(|n| n.id);
                let lost = lost
                    .iter()
                    .map(|n| {
                        let mut v = subtree(&index, n.id, depth);
                        v["last_seen"] = match n.parent_id {
                            Some(p) => json!(brief(&self.conn, p)?),
                            None => Value::Null,
                        };
                        Ok(v)
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(json!({ "tree": tree, "lost": lost }))
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
        self.find_with(text, tag, kind, include_gone, false)
    }

    /// `find`, and with `empty` only the containers nothing is in: worked out from the records,
    /// so it never goes stale the way a hand-kept "empty" tag does.
    pub fn find_with(
        &self,
        text: &str,
        tag: Option<&str>,
        kind: Option<Kind>,
        include_gone: bool,
        empty: bool,
    ) -> Result<Value> {
        let query = search::Query::parse(&self.conn, text)?;
        // Without text a filter must narrow it: `--tag x` alone lists everything tagged x.
        if query.is_empty() && tag.is_none() && kind.is_none() && !empty {
            return Err(Error::Usage(
                "search text is empty; give text, or --tag / --kind / --empty to list".into(),
            ));
        }
        let tag = tag.map(|t| t.trim().to_lowercase());
        let filled: std::collections::HashSet<i64> = ids(
            &self.conn,
            // A lost thing is not in its last-seen place (as in `tree`).
            "SELECT DISTINCT parent_id FROM nodes
              WHERE parent_id IS NOT NULL AND state != 'gone' AND lost = 0",
            [],
        )?
        .into_iter()
        .collect();
        let mut nodes = Vec::new();
        // With `empty`: a container nothing is in only counts as empty when that is known (see
        // `known_empty`); one never gone through is listed apart, as not known.
        let mut unknown = Vec::new();
        for id in ids(&self.conn, "SELECT id FROM nodes ORDER BY id", [])? {
            let n = load(&self.conn, id)?;
            // A digitized thing left as paper but stays findable: its copy is why it was kept.
            let archived = n.disposition == Some(Disposition::Digitize);
            if (n.state == State::Gone && !include_gone && !archived)
                || kind.is_some_and(|k| k != n.kind)
                || tag.as_ref().is_some_and(|t| !n.tags.contains(t))
                || empty
                    && (n.kind != Kind::Container || n.state == State::Gone || filled.contains(&id))
            {
                continue;
            }
            if empty && !known_empty(&self.conn, id)? {
                unknown.push(n);
                continue;
            }
            nodes.push(n);
        }
        let hits: Vec<i64> = if query.is_empty() {
            nodes.iter().map(|n| n.id).collect()
        } else {
            query.rank(&nodes).into_iter().map(|(id, _)| id).collect()
        };
        // A portion of a thing kept in several places says so: all of it, and in how many places
        // (spec/portions.md §5). Agents keep acting on the portion's own id.
        let results = hits
            .into_iter()
            .map(|id| {
                let mut v = brief_json(&self.conn, id)?;
                let n = load(&self.conn, id)?;
                if let Some(th) = crate::portions::thing_json(&self.conn, &n)? {
                    v["thing"] = json!({
                        "id": th["id"], "total": th["total"], "places": th["places"],
                        "in_use": th["in_use"], "spare": th["spare"],
                    });
                }
                if empty {
                    v["slot"] = json!(is_slot(&self.conn, &n)?);
                }
                Ok(v)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut v = json!({ "query": text, "results": results });
        if empty {
            let unknown: Vec<i64> = if query.is_empty() {
                unknown.iter().map(|n| n.id).collect()
            } else {
                query.rank(&unknown).into_iter().map(|(id, _)| id).collect()
            };
            v["not_known"] = json!(
                unknown
                    .iter()
                    .map(|id| brief_json(&self.conn, *id))
                    .collect::<Result<Vec<_>>>()?
            );
        }
        Ok(v)
    }

    /// Applies `field=value` assignments (spec §6, §11.6).
    pub fn edit(&mut self, reference: &str, assignments: &[String]) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let (id, changed) = edit_in(&tx, reference, assignments)?;
        tx.commit()?;
        // What changed, each field before and after, and the record by name and place: the rest
        // the caller knew already, and `show` has it (spec/output.md).
        let v = json!({ "node": brief(&self.conn, id)?, "changed": changed });
        // A make or model just learned is what matches a purchase best: ask now, as `add` does.
        if sets_identity(assignments) {
            return offer_purchases(&self.conn, id, v);
        }
        Ok(v)
    }

    /// Applies the assignments of several records at once, all or none: a line that fails
    /// (by its number) leaves every record as it was. Each record gets its own `edit` event.
    pub fn edit_batch(&mut self, lines: &[(String, Vec<String>)]) -> Result<Value> {
        if lines.is_empty() {
            return Err(Error::Usage("no lines to edit".into()));
        }
        let tx = self.conn.transaction()?;
        let mut edited = Vec::with_capacity(lines.len());
        for (i, (reference, assignments)) in lines.iter().enumerate() {
            if assignments.is_empty() {
                return Err(Error::Usage("nothing to set".into()).at_line(i + 1));
            }
            edited.push(edit_in(&tx, reference, assignments).map_err(|e| e.at_line(i + 1))?);
        }
        tx.commit()?;
        // As a single edit does: a line that sets make or model asks about its purchases.
        let nodes = edited
            .iter()
            .zip(lines)
            .map(|((id, changed), (_, assignments))| {
                let mut v = serde_json::to_value(brief(&self.conn, *id)?)
                    .map_err(|e| Error::Internal(e.to_string()))?;
                v["changed"] = Value::Object(changed.clone());
                if sets_identity(assignments) {
                    offer_purchases(&self.conn, *id, v)
                } else {
                    Ok(v)
                }
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({ "edited": nodes }))
    }

    /// Splits one record into several kinds of thing: each `(name, qty)` becomes a new record
    /// beside it (same place, kind and tags), and the original keeps what is left — renamed
    /// and recounted with `rename` / `qty` when given. A set recorded as one thing (a probe, a
    /// board and a cable) becomes a record per part; one record of two kinds (straight and
    /// angled headers) becomes two. The history links both ways: `split` on the original
    /// names what came off it, `split_from` on each new record names where it came from.
    /// Photos stay on the original; the result lists them, so each part can get its crop from
    /// every photo it is in. A holder with things inside is not split. All or nothing.
    pub fn split(
        &mut self,
        reference: &str,
        parts: &[(String, Option<i64>)],
        rename: Option<&str>,
        qty: Option<i64>,
    ) -> Result<Value> {
        self.split_with(reference, parts, rename, qty, false)
    }

    /// `split`; with `take`, the parts are some of the original's units (two of four cells are
    /// another make) and their counts come off its count, instead of what each unit is made of.
    pub fn split_with(
        &mut self,
        reference: &str,
        parts: &[(String, Option<i64>)],
        rename: Option<&str>,
        qty: Option<i64>,
        take: bool,
    ) -> Result<Value> {
        if parts.is_empty() {
            return Err(Error::Usage(
                "give at least one <name>=<qty> to split off".into(),
            ));
        }
        let tx = self.conn.transaction()?;
        let n = load(&tx, resolve(&tx, reference, false)?)?;
        let inside: i64 = tx.query_row(
            "SELECT COUNT(*) FROM nodes WHERE parent_id = ?1 AND state != 'gone'",
            [n.id],
            |r| r.get(0),
        )?;
        // With --take a box of several (two battery cases as one record) gives up some of its
        // units empty; what is inside stays in the original. Without it, the parts would be what
        // each unit is made of, which a holder with things in it is not split into.
        if inside > 0 && !take {
            return Err(refused(
                format!(
                    "{} holds {inside} thing(s); split what is inside, move it out first, or \
                     take empty units off it with --take",
                    label(&n)
                ),
                json!({ "node": brief_json(&tx, n.id)? }),
            ));
        }
        let mut into = Vec::new();
        for (name, q) in parts {
            let name = name.trim();
            if name.is_empty() {
                return Err(Error::Usage("a split-off part needs a name".into()));
            }
            let new = NewNode {
                name: name.to_string(),
                kind: n.kind.to_string(),
                qty: *q,
                tags: n.tags.clone(),
                note: Some(format!("Split from #{} ({}).", n.id, n.name)),
                ..Default::default()
            };
            let id = add_one(&tx, &new, n.parent_id)?;
            event(
                &tx,
                id,
                "split_from",
                json!({ "from": n.id, "name": n.name }),
            )?;
            into.push(id);
        }
        // Parts are what each unit is made of (three sets → three cards, three cables) unless
        // `take`: then they are some of the units, and come off the original's count.
        let qty = if take {
            let taken: Option<i64> = parts.iter().map(|(_, q)| *q).sum();
            match (qty, n.qty, taken) {
                (Some(_), _, _) => {
                    return Err(Error::Usage(
                        "--take sets the original's count itself; leave --qty out".into(),
                    ));
                }
                (None, Some(had), Some(t)) if t < had => Some(had - t),
                (None, Some(had), Some(t)) => {
                    return Err(refused(
                        format!(
                            "the parts take {t} of the {had} of {}; nothing would be left on it: \
                             keep one part as the original with --rename and --qty instead",
                            label(&n)
                        ),
                        json!({ "node": brief_json(&tx, n.id)? }),
                    ));
                }
                _ => {
                    return Err(Error::Usage(
                        "--take needs a count on the original and on every part".into(),
                    ));
                }
            }
        } else {
            qty
        };
        let mut changes = serde_json::Map::new();
        let edits = [
            rename.map(|r| ("name", r.to_string())),
            qty.map(|q| ("qty", q.to_string())),
        ];
        for (field, value) in edits.into_iter().flatten() {
            let before = load(&tx, n.id)?;
            apply_edit(&tx, &before, field, &value)?;
            let after = load(&tx, n.id)?;
            let (b, a) = (field_value(&before, field), field_value(&after, field));
            if b != a {
                changes.insert(field.to_string(), json!({ "before": b, "after": a }));
            }
        }
        if !changes.is_empty() {
            event(&tx, n.id, "edit", Value::Object(changes))?;
        }
        let parts_json = into
            .iter()
            .map(|id| {
                let b = brief(&tx, *id)?;
                Ok(json!({ "id": b.id, "name": b.name, "qty": b.qty }))
            })
            .collect::<Result<Vec<_>>>()?;
        event(&tx, n.id, "split", json!({ "into": parts_json }))?;
        touch(&tx, n.id)?;
        tx.commit()?;
        // Each new part may be a purchase of its own, asked while it is in hand (spec §4.2).
        let into = into
            .iter()
            .map(|id| {
                let v = serde_json::to_value(brief(&self.conn, *id)?)
                    .map_err(|e| Error::Internal(e.to_string()))?;
                offer_purchases(&self.conn, *id, v)
            })
            .collect::<Result<Vec<_>>>()?;
        let photos = self.photo_list(&n.id.to_string())?["photos"].clone();
        Ok(json!({ "node": brief(&self.conn, n.id)?, "into": into, "photos": photos }))
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
        self.move_qty(reference, to, plan, None)
    }

    /// `move_to` for `qty` of a record's units (spec/portions.md §4.1): fewer than all of them
    /// are split off as a portion of their own, which moves (or, with `plan`, waits beside the
    /// rest for its move). Arriving where a portion of the same thing already is, it joins it.
    pub fn move_qty(
        &mut self,
        reference: &str,
        to: &str,
        plan: bool,
        qty: Option<i64>,
    ) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let id = move_in(&tx, reference, to, plan, qty)?;
        tx.commit()?;
        show(&self.conn, id)
    }

    /// Several records to one place in one step, all or none (a box emptied before it goes):
    /// each as `move_to` would move it, or plan it with `plan`. Answers with where each one is
    /// now (or is planned to go) rather than a node payload for each.
    pub fn move_many(&mut self, references: &[String], to: &str, plan: bool) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let mut moved = Vec::with_capacity(references.len());
        for r in references {
            let id = move_in(&tx, r, to, plan, None)?;
            if !moved.contains(&id) {
                moved.push(id);
            }
        }
        let target = resolve(&tx, to, false)?;
        tx.commit()?;
        let key = if plan { "planned" } else { "moved" };
        let rows = moved
            .iter()
            .map(|id| brief(&self.conn, *id))
            .collect::<Result<Vec<_>>>()?;
        let mut v = json!({ "to": brief(&self.conn, target)? });
        v[key] = json!(rows);
        Ok(v)
    }

    /// Records made separately are one thing kept in several places (spec/portions.md §4.3).
    /// Records made separately are one thing kept in several places (spec/portions.md §4.3). A
    /// gone record may be among them (a used-up one and its replacement), as long as one lives.
    pub fn join(&mut self, references: &[String]) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let mut nodes: Vec<Node> = Vec::new();
        for r in references {
            let n = load(&tx, resolve(&tx, r, true)?)?;
            if !nodes.iter().any(|m| m.id == n.id) {
                nodes.push(n);
            }
        }
        if nodes.len() < 2 {
            return Err(Error::Usage("name at least two records to join".into()));
        }
        let holder = crate::portions::join(&tx, &nodes)?;
        tx.commit()?;
        show(&self.conn, holder)
    }

    /// A portion is a thing of its own after all.
    pub fn unjoin(&mut self, reference: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let n = load(&tx, resolve(&tx, reference, false)?)?;
        crate::portions::unjoin(&tx, &n)?;
        tx.commit()?;
        show(&self.conn, n.id)
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
        let holder = crate::portions::join_here(&tx, node.id)?;
        tx.commit()?;
        show(&self.conn, holder)
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
        self.dispose_with(reference, disposition, false)
    }

    /// `dispose` that can also say the thing is shredded rather than thrown out whole: an old
    /// ID card, a boarding pass, a bank statement. Only what goes in the bin can be.
    pub fn dispose_with(
        &mut self,
        reference: &str,
        disposition: Disposition,
        shred: bool,
    ) -> Result<Value> {
        self.dispose_qty(reference, disposition, shred, None, None)
    }

    /// `dispose_with` for `qty` of a record's units: they are set apart as a portion of their
    /// own (spec/portions.md §4.1) and only they become candidates to leave.
    pub fn dispose_qty(
        &mut self,
        reference: &str,
        disposition: Disposition,
        shred: bool,
        qty: Option<i64>,
        why: Option<&str>,
    ) -> Result<Value> {
        check_shred(disposition, shred)?;
        not_merged(Some(disposition))?;
        if disposition == Disposition::Used {
            return Err(Error::Usage(
                "nothing waits to be used up; when it is, record it with `ev gone --as used`"
                    .into(),
            ));
        }
        if matches!(
            disposition,
            Disposition::Left | Disposition::Stolen | Disposition::Unknown
        ) {
            return Err(Error::Usage(format!(
                "nothing is set aside to be {d}; record it with `ev gone --as {d}`",
                d = disposition.as_str()
            )));
        }
        let tx = self.conn.transaction()?;
        let node = load(&tx, resolve(&tx, reference, false)?)?;
        let node = crate::portions::take(&tx, node, qty)?;
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
        if shred {
            crate::marks::mark_shred(&tx, node.id)?;
        }
        // What the person said about letting it go, as `gone --why` keeps it: in the note.
        if let Some(w) = why.map(str::trim).filter(|w| !w.is_empty()) {
            let note = match node.note.as_deref() {
                Some(n) if !n.trim().is_empty() => format!("{n}\n{w}"),
                _ => w.to_string(),
            };
            tx.execute(
                "UPDATE nodes SET note = ?1 WHERE id = ?2",
                params![note, node.id],
            )?;
            event(
                &tx,
                node.id,
                "edit",
                json!({ "note": { "before": node.note, "after": note } }),
            )?;
        }
        tx.commit()?;
        show(&self.conn, node.id)
    }

    pub fn restore(&mut self, reference: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let id = match resolve(&tx, reference, false) {
            Err(Error::NotFound(_)) if resolve_for_history(&tx, reference).is_ok() => {
                let id = resolve_for_history(&tx, reference)?;
                return Err(refused(
                    format!("node {id} is gone; `ev restore {id} --correction \"why\"` undoes it"),
                    Value::Null,
                ));
            }
            other => other?,
        };
        let node = load(&tx, id)?;
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
        crate::marks::clear_shred(&tx, node.id)?;
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
        self.gone_with(reference, disposition, why, false)
    }

    /// `gone_because` that can also say the thing was shredded (see `dispose_with`). A
    /// digitized thing leaves only with its copy: a photo or a document on its own record. When
    /// every copy is an image too small to read a ticket from, it still leaves, with a
    /// `warning` in the result, since only the person can tell whether the copy reads.
    pub fn gone_with(
        &mut self,
        reference: &str,
        disposition: Option<Disposition>,
        why: Option<&str>,
        shred: bool,
    ) -> Result<Value> {
        self.gone_qty(reference, disposition, why, shred, None)
    }

    /// `gone_with` for `qty` of a record's units: they leave as a portion of their own (spec/
    /// portions.md §4.1), and the rest stay.
    pub fn gone_qty(
        &mut self,
        reference: &str,
        disposition: Option<Disposition>,
        why: Option<&str>,
        shred: bool,
        qty: Option<i64>,
    ) -> Result<Value> {
        self.gone_left(reference, disposition, why, shred, qty, None, None)
    }

    /// `gone_qty` for a thing that left long ago (spec/past-belongings.md): `at`, a partial
    /// date (`2016`, `2016-06`), and `place`, where it was then (a place, made when new).
    #[expect(
        clippy::too_many_arguments,
        reason = "each is a fact of the leaving the person may say"
    )]
    pub fn gone_left(
        &mut self,
        reference: &str,
        disposition: Option<Disposition>,
        why: Option<&str>,
        shred: bool,
        qty: Option<i64>,
        at: Option<&str>,
        place: Option<&str>,
    ) -> Result<Value> {
        let why = why.map(str::trim).filter(|w| !w.is_empty());
        not_merged(disposition)?;
        if disposition == Some(Disposition::Mistake) && why.is_none() {
            return Err(Error::Usage(
                "say why the record was a mistake with --why".into(),
            ));
        }
        let tx = self.conn.transaction()?;
        let node = load(&tx, resolve(&tx, reference, false)?)?;
        // A part leaving is split off first; a sale listed on the whole is still its listing.
        let listed_on = node.id;
        let node = crate::portions::take(&tx, node, qty)?;
        if node.state == State::Active && disposition.is_none() {
            return Err(refused(
                format!(
                    "{} is active; say how it left with --as trash|give|sell|trade|used|digitize|left|stolen|unknown",
                    label(&node)
                ),
                Value::Null,
            ));
        }
        let inside = require_no_active_inside(&tx, &node)?;
        let leaving = disposition
            .or(node.disposition)
            .unwrap_or(Disposition::Trash);
        if shred {
            check_shred(leaving, true)?;
        }
        let mut warnings = Vec::new();
        for n in std::iter::once(&node).chain(&inside) {
            let d = if n.id == node.id {
                Some(leaving)
            } else {
                n.disposition
            };
            if d == Some(Disposition::Digitize) {
                warnings.extend(require_copy(&tx, n)?);
            }
        }
        let final_disposition = match (node.state, disposition) {
            (State::Active, Some(d)) => {
                set_candidate(&tx, node.id, d)?;
                d
            }
            (_, Some(d)) => d,
            (_, None) => leaving,
        };
        if shred {
            crate::marks::mark_shred(&tx, node.id)?;
        }
        tx.execute(
            "UPDATE nodes SET state = 'gone', disposition = ?1, pending_to = NULL WHERE id = ?2",
            params![final_disposition.as_str(), node.id],
        )?;
        past::set_departure(&tx, node.id, at, place)?;
        if final_disposition == Disposition::Sell {
            past::carry_sale(&tx, node.id, listed_on)?;
        }
        // Gone, it is on sale no more; a part that left leaves the rest still listed.
        if listed_on == node.id {
            crate::marks::clear_mark(&tx, node.id, "sale")?;
            crate::marks::clear_mark(&tx, node.id, "condition")?;
        }
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
        let mut v = show(&self.conn, node.id)?;
        if !warnings.is_empty() {
            v["warnings"] = json!(warnings);
        }
        Ok(v)
    }

    /// Every candidate grouped by disposition. A candidate held by another candidate leaves
    /// with it (spec §12.1), so it is listed under its outermost candidate as a part.
    pub fn disposals(&self, filter: Option<Disposition>) -> Result<Value> {
        let mut groups = serde_json::Map::new();
        for d in [
            Disposition::Trash,
            Disposition::Digitize,
            Disposition::Give,
            Disposition::Sell,
            Disposition::Trade,
            Disposition::Return,
        ] {
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
                entry["shred"] = json!(!crate::marks::mark(&self.conn, id, "shred")?.is_null());
                entries.push(entry);
            }
            groups.insert(d.as_str().into(), json!(entries));
        }
        Ok(json!({ "disposals": groups }))
    }

    pub fn mark_lost(&mut self, reference: &str) -> Result<Value> {
        self.mark_lost_qty(reference, None)
    }

    /// `mark_lost` for `qty` of a record's units: those are missing, the rest are where they
    /// were (spec/portions.md §4.1).
    pub fn mark_lost_qty(&mut self, reference: &str, qty: Option<i64>) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let node = load(&tx, resolve(&tx, reference, false)?)?;
        let node = crate::portions::take(&tx, node, qty)?;
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
                    "{} was never seen anywhere; say where it turned up with `ev found <ref> --in <place>`",
                    label(&node)
                ),
                Value::Null,
            ));
        }
        tx.execute("UPDATE nodes SET lost = 0 WHERE id = ?1", [node.id])?;
        touch(&tx, node.id)?;
        event(&tx, node.id, "found", json!({ "at": node.parent_id }))?;
        // Found where the rest of the same thing is: the units join them.
        let holder = crate::portions::join_here(&tx, node.id)?;
        tx.commit()?;
        // Back in hand: the moment to ask which purchase it was, as for a new record, and where
        // the things that waited for it go now.
        let v = offer_purchases(&self.conn, holder, show(&self.conn, holder)?)?;
        with_waiting(&self.conn, node.id, v)
    }

    /// A lost node turned up somewhere else than where it was last seen: it moves there, which
    /// clears the lost mark.
    pub fn found_in(&mut self, reference: &str, place: &str) -> Result<Value> {
        let node = load(&self.conn, resolve(&self.conn, reference, false)?)?;
        if !node.lost {
            return Err(refused(
                format!("{} is not lost", label(&node)),
                Value::Null,
            ));
        }
        let v = self.move_to(&format!("#{}", node.id), place, false)?;
        let v = offer_purchases(&self.conn, node.id, v)?;
        with_waiting(&self.conn, node.id, v)
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

    /// A place's history as seen from it: its own events, and the events of things that came
    /// in, went out or were added there. Those carry the thing as `item` and a `relation`:
    /// `in` (moved or planned to here), `out` (moved away from here) or `added` (created
    /// here). Oldest first, like `history`.
    pub fn history_with_contents(&self, reference: &str) -> Result<Value> {
        let id = resolve(&self.conn, reference, true)?;
        let mut stmt = self.conn.prepare(
            "SELECT node_id, at, type, data FROM events
             WHERE node_id = ?1
                OR (type IN ('move', 'done', 'plan')
                    AND (json_extract(data, '$.to') = ?1 OR json_extract(data, '$.from') = ?1))
                OR (type = 'create' AND json_extract(data, '$.parent') = ?1)
             ORDER BY id",
        )?;
        let rows = stmt
            .query_map([id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut events = Vec::with_capacity(rows.len());
        for (node, at, kind, data) in rows {
            let data = serde_json::from_str::<Value>(&data).unwrap_or(Value::Null);
            let mut e = json!({ "at": at, "type": kind, "data": data });
            if node != id {
                let relation = if kind == "create" {
                    "added"
                } else if e["data"]["to"].as_i64() == Some(id) {
                    "in"
                } else {
                    "out"
                };
                e["item"] = json!(brief(&self.conn, node)?);
                e["relation"] = json!(relation);
            }
            events.push(e);
        }
        Ok(json!({ "node": brief(&self.conn, id)?, "events": events }))
    }
}

// ---------- reading ----------

fn db_enum<T: std::str::FromStr<Err = Error>>(idx: usize, s: String) -> rusqlite::Result<T> {
    s.parse::<T>().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
    })
}

/// A record from a row of `SELECT {NODE_COLUMNS}`, without its tags and photos.
fn node_row(r: &rusqlite::Row) -> rusqlite::Result<Node> {
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
        size: r.get(19)?,
        temporary: r.get(20)?,
        make: r.get(21)?,
        model: r.get(22)?,
        serial: r.get(23)?,
        thing: r.get(24)?,
        waits_for: r.get(25)?,
        came_at: r.get(26)?,
    })
}

pub(crate) fn load(conn: &Connection, id: i64) -> Result<Node> {
    let node = conn
        .query_row(
            &format!("SELECT {NODE_COLUMNS} FROM nodes WHERE id = ?1"),
            [id],
            node_row,
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
        "SELECT ev_file(path) FROM photos WHERE node_id = ?1 ORDER BY position",
        id,
    )?;
    Ok(node)
}

/// Every record that has not left the home, in id order, with its tags and photos: three
/// queries in all, however many records there are (loading them one by one took a few each).
pub(crate) fn load_live(conn: &Connection) -> Result<Vec<Node>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {NODE_COLUMNS} FROM nodes WHERE state != 'gone' ORDER BY id"
    ))?;
    let mut all: Vec<Node> = stmt
        .query_map([], node_row)?
        .collect::<rusqlite::Result<_>>()?;
    let at: HashMap<i64, usize> = all.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    let mut stmt = conn.prepare("SELECT node_id, tag FROM tags ORDER BY node_id, tag")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (id, tag) = row?;
        if let Some(&i) = at.get(&id) {
            all[i].tags.push(tag);
        }
    }
    let mut stmt =
        conn.prepare("SELECT node_id, ev_file(path) FROM photos ORDER BY node_id, position")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (id, path) = row?;
        if let Some(&i) = at.get(&id) {
            all[i].photos.push(path);
        }
    }
    Ok(all)
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

pub(crate) fn label(n: &Node) -> String {
    match &n.code {
        Some(c) => format!("{c} ({})", n.name),
        None => format!("#{} {}", n.id, n.name),
    }
}

/// One move inside the caller's transaction: `qty` of the record's units (all by default) to
/// `to`, or planned there with `plan`. Returns the record that now holds them (a portion that
/// joined one already there) or, planned, the one waiting.
fn move_in(
    tx: &Connection,
    reference: &str,
    to: &str,
    plan: bool,
    qty: Option<i64>,
) -> Result<i64> {
    let mut node = load(tx, resolve(tx, reference, false)?)?;
    let target = resolve(tx, to, false)?;
    // A move to where it already is says nothing, and as a plan it waits forever. A lost
    // thing moved to where it was last seen is found there, so that one goes on.
    if !node.lost && node.parent_id == Some(target) {
        return Err(refused(
            format!(
                "{} is already in {}; a place inside it (a compartment) is a grid cell \
                 (`ev grid`, `ev cell`) or a holder of its own",
                label(&node),
                label(&load(tx, target)?)
            ),
            Value::Null,
        ));
    }
    if let Some(count) = crate::portions::part_of(&node, qty)? {
        let id = crate::portions::split_off(tx, &node, count)?;
        node = load(tx, id)?;
    }
    if plan {
        if let Some(p) = node.pending_to {
            return Err(refused(
                format!(
                    "{} already has a pending move; cancel it first",
                    label(&node)
                ),
                json!({ "pending": brief(tx, p)? }),
            ));
        }
        tx.execute(
            "UPDATE nodes SET pending_to = ?1 WHERE id = ?2",
            params![target, node.id],
        )?;
        touch(tx, node.id)?;
        event(tx, node.id, "plan", json!({ "to": target }))?;
        return Ok(node.id);
    }
    apply_move(tx, &node, target, "move")?;
    crate::portions::join_here(tx, node.id)
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
    })
}

pub(crate) fn show(conn: &Connection, id: i64) -> Result<Value> {
    let n = load(conn, id)?;
    let segments = path(conn, id)?;
    let mut node = serde_json::to_value(&n).map_err(|e| Error::Internal(e.to_string()))?;
    node["path_text"] = json!(path_text(&segments));
    node["path"] = json!(segments);
    let children = sorted_children(conn, id, true)?;
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
    // A placed box is read against the grid it stands in.
    let parent_grid = match (&cells, n.parent_id) {
        (Some(_), Some(p)) => crate::grid::grid_json(conn, p)?,
        _ => None,
    };
    let room = n
        .fill
        .is_some()
        .then(|| crate::placement::room(conn, &n))
        .transpose()?;
    // What proves, covers and values it is the thing's: read across its portions (spec/
    // portions.md §3), each marked with the one it was linked on.
    let (_, proposal) = crate::coverage::coverages_of(conn, id)?;
    let coverages =
        crate::portions::across(conn, &n, |m| Ok(crate::coverage::coverages_of(conn, m)?.0))?;
    Ok(json!({
        "cells": cells,
        "grid": grid,
        "parent_grid": parent_grid,
        "room": room,
        "tasks": tasks,
        "marks": marks,
        "needs": needs,
        "node": node,
        "thing": crate::portions::thing_json(conn, &n)?,
        // What its place waits for, and what waits for it (spec/waits-for.md).
        "waits_for": n.waits_for.map(|w| brief(conn, w)).transpose()?,
        "waited_for_by": waited_for_by(conn, id)?,
        "empty": empty_of(conn, &n)?,
        // When it came and, gone, how it left (spec/past-belongings.md).
        "came": conn.query_row("SELECT came_at FROM nodes WHERE id = ?1", [id], |r| {
            r.get::<_, Option<String>>(0)
        })?,
        "departure": past::departure_json(conn, id)?,
        "traded_from": past::traded_from(conn, id)?,
        "children": children,
        "pending": pending,
        "last_seen": last_seen,
        "review": review,
        "observations": observations,
        "kits": crate::kits::kits_of(conn, id)?,
        "documents": crate::portions::across(conn, &n, |m| crate::docs::docs_of(conn, m))?,
        "purchases": crate::portions::across(conn, &n, |m| crate::purchases::purchases_of(conn, m))?,
        "valuations": crate::portions::across(conn, &n, |m| crate::valuations::valuations_of(conn, m))?,
        "links": crate::portions::across(conn, &n, |m| crate::links::links_of(conn, m))?,
        "coverages": coverages,
        "coverage_proposal": proposal,
        "tracking": {
            "value": crate::coverage::decision(conn, id, "value")?
                .map(|(v, why, on)| json!({ "decision": v, "why": why, "on": on })),
            "coverage": crate::coverage::decision(conn, id, "coverage")?
                .map(|(v, why, on)| json!({ "decision": v, "why": why, "on": on })),
        },
    }))
}

/// That a box nothing is in is known to be empty (see `known_empty`), and how: `{from: "said",
/// at, note}` when the person called it empty (`ev empty`), else `{from: "tour"}`. None for
/// anything else.
fn empty_of(conn: &Connection, n: &Node) -> Result<Option<Value>> {
    if n.kind != Kind::Container {
        return Ok(None);
    }
    let inside: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE parent_id = ?1 AND state != 'gone' AND lost = 0",
        [n.id],
        |r| r.get(0),
    )?;
    if inside > 0 || !known_empty(conn, n.id)? {
        return Ok(None);
    }
    let said = conn
        .query_row(
            "SELECT at, json_extract(data, '$.note') FROM events
              WHERE node_id = ?1 AND type = 'empty' ORDER BY at DESC, id DESC LIMIT 1",
            [n.id],
            |r| {
                Ok(json!({
                    "from": "said",
                    "at": r.get::<_, String>(0)?,
                    "note": r.get::<_, Option<String>>(1)?,
                }))
            },
        )
        .optional()?;
    Ok(Some(said.unwrap_or_else(|| json!({ "from": "tour" }))))
}

/// The live records whose place waits for `id` (spec/waits-for.md).
pub(crate) fn waited_for_by(conn: &Connection, id: i64) -> Result<Vec<NodeRef>> {
    ids(
        conn,
        "SELECT id FROM nodes WHERE waits_for = ?1 AND state != 'gone' ORDER BY id",
        [id],
    )?
    .into_iter()
    .map(|w| brief(conn, w))
    .collect()
}

/// Items anywhere below `id` that are not gone, counting each item's quantity.
pub(crate) fn item_total(conn: &Connection, id: i64) -> Result<i64> {
    Ok(conn.query_row(
        "WITH RECURSIVE d(id) AS (
             SELECT id FROM nodes WHERE parent_id = ?1 AND state != 'gone' AND lost = 0
             UNION ALL
             SELECT n.id FROM nodes n JOIN d ON n.parent_id = d.id
              WHERE n.state != 'gone' AND n.lost = 0
         )
         SELECT COALESCE(SUM(COALESCE(qty, 1)), 0) FROM nodes WHERE kind = 'item' AND id IN d",
        [id],
        |r| r.get(0),
    )?)
}

/// Whether a container nothing is in is known to be empty, not just never counted: its place has
/// been counted (`toured`, its own review or inherited; `kept` is left uncounted), or something was once
/// recorded in it (created there or moved there) and has left, or the person said it is empty
/// (`ev empty`). An uncounted carton in a room
/// never toured has no records inside because nobody looked, not because it is empty. The box set
/// back to `raw` (`ev review <box> --as raw`: it holds things never counted) forgets all of that
/// up to then.
pub(crate) fn known_empty(conn: &Connection, id: i64) -> Result<bool> {
    let reset: Option<String> = conn.query_row(
        "SELECT MAX(at) FROM events
          WHERE node_id = ?1 AND type = 'review' AND json_extract(data, '$.as') = 'raw'",
        [id],
        |r| r.get(0),
    )?;
    let after = |at: &str| reset.as_deref().is_none_or(|r| at > r);
    let review = crate::plan::review_inherited(conn, id)?;
    // A tour of the place it is in only saw it if it was there then: a box found or moved into
    // a counted drawer later was never opened on that tour.
    // Events in the order they were written: times only have seconds.
    let seen_on_tour = match review["from"].as_i64() {
        Some(from) if from != id => {
            let (arrived, toured): (Option<i64>, Option<i64>) = conn.query_row(
                "SELECT
                   (SELECT MAX(id) FROM events
                     WHERE node_id = ?1 AND type IN ('create', 'move', 'done', 'found')),
                   (SELECT MAX(id) FROM events
                     WHERE node_id = ?2 AND type = 'review'
                       AND json_extract(data, '$.as') = 'toured')",
                params![id, from],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            match (arrived, toured) {
                (Some(a), Some(t)) => a < t,
                _ => true,
            }
        }
        _ => true,
    };
    if review["status"] == "toured" && review["at"].as_str().is_none_or(after) && seen_on_tour {
        return Ok(true);
    }
    let held: Option<String> = conn
        .query_row(
            "SELECT MAX(at) FROM events
              WHERE (type = 'create' AND json_extract(data, '$.parent') = ?1)
                 OR json_extract(data, '$.to') = ?1
                 OR (type = 'empty' AND node_id = ?1)",
            [id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(held.as_deref().is_some_and(after))
}

/// A slot of a piece of furniture (a drawer, a compartment) rather than a box that moves: one
/// in furniture or in such a slot, with no code or a positional code extending its holder's
/// (`K2x2-03` in `K2x2`, `K2x2-01-A` in `K2x2-01`). A box with a code of its own is a box
/// wherever it stands: `S5-01` in compartment `K2x2-02`, `G1x1-003` on a desk.
pub(crate) fn is_slot(conn: &Connection, n: &Node) -> Result<bool> {
    let Some(parent) = n.parent_id else {
        return Ok(false);
    };
    let p = load(conn, parent)?;
    let positional = match (n.code.as_deref(), p.code.as_deref()) {
        (None, _) => true,
        (Some(c), Some(pc)) => fold(c).starts_with(&format!("{}-", fold(pc))),
        (Some(_), None) => false,
    };
    Ok(positional && (p.kind == Kind::Furniture || is_slot(conn, &p)?))
}

/// A node's live children in the order a person reads a shelf: rooms, furniture, containers,
/// then things; within each, the coded ones by their code read naturally (`S5-2` before
/// `S5-10`, the `S5` series before `S45`), then the rest by name. Creation order split a series
/// whenever something else was recorded between its boxes. `with_lost` keeps the lost ones.
pub(crate) fn sorted_children(conn: &Connection, parent: i64, with_lost: bool) -> Result<Vec<i64>> {
    let sql = format!(
        "SELECT id, kind, code, name FROM nodes WHERE parent_id = ?1 AND state != 'gone'{}",
        if with_lost { "" } else { " AND lost = 0" }
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows: Vec<(i64, String, Option<String>, String)> = stmt
        .query_map([parent], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<std::result::Result<_, _>>()?;
    rows.sort_by_cached_key(|(id, kind, code, name)| sibling_key(kind, code.as_deref(), name, *id));
    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// Where a record stands among its siblings (see `sorted_children`).
type SiblingKey = (u8, bool, Vec<(bool, u64, String)>, String, i64);

fn sibling_key(kind: &str, code: Option<&str>, name: &str, id: i64) -> SiblingKey {
    let rank = match kind {
        "room" => 0,
        "furniture" => 1,
        "container" => 2,
        _ => 3,
    };
    let code_key = code.map(natural_key).unwrap_or_default();
    (rank, code.is_none(), code_key, fold(name), id)
}

/// Everything `ev tree` shows of every live record, read in a few queries: built per record it
/// took thousands (each record loaded again, its count found by walking up its chain, its item
/// total by a recursive query), and `ev ui` waited for them on every start.
struct TreeIndex {
    nodes: HashMap<i64, Node>,
    /// The children that are not lost, in reading order.
    kids: HashMap<i64, Vec<i64>>,
    /// The units of everything below a record, lost ones left out.
    items: HashMap<i64, i64>,
    /// How far each place gone through on its own has been counted.
    count: HashMap<i64, String>,
    /// The counted places whose contents changed after their tour.
    changed: HashSet<i64>,
    /// The boxes known to be empty: nothing in them, and counted, on their own or with the
    /// place they are in (`ev empty` counts a box too).
    empty: HashSet<i64>,
}

impl TreeIndex {
    fn build(conn: &Connection) -> Result<Self> {
        let all = load_live(conn)?;
        let mut kids: HashMap<i64, Vec<i64>> = HashMap::new();
        for n in all.iter().filter(|n| !n.lost) {
            if let Some(p) = n.parent_id {
                kids.entry(p).or_default().push(n.id);
            }
        }
        let parent: HashMap<i64, Option<i64>> = all.iter().map(|n| (n.id, n.parent_id)).collect();
        let reviews = crate::plan::all_reviews(conn)?;
        let changes = crate::marks::ContentChanges::load(conn)?;
        let mut changed = HashSet::new();
        let count = crate::plan::units(&all)
            .into_iter()
            .map(|u| {
                let s = match crate::plan::effective_review(u, &parent, &reviews) {
                    Some((_, s, at)) => {
                        // As `ev progress` says it: only a change to what it holds dates a tour.
                        if s == "toured" && changes.at(u).is_some_and(|c| c > at) {
                            changed.insert(u);
                        }
                        s
                    }
                    None => "raw".to_string(),
                };
                (u, s)
            })
            .collect();
        let empty = all
            .iter()
            .filter(|n| n.kind == Kind::Container && !n.lost && !kids.contains_key(&n.id))
            .filter(|n| {
                crate::plan::effective_review(n.id, &parent, &reviews)
                    .is_some_and(|(_, s, _)| s == "toured")
            })
            .map(|n| n.id)
            .collect();
        let nodes: HashMap<i64, Node> = all.into_iter().map(|n| (n.id, n)).collect();
        for list in kids.values_mut() {
            list.sort_by_cached_key(|id| {
                let n = &nodes[id];
                sibling_key(&n.kind.to_string(), n.code.as_deref(), &n.name, n.id)
            });
        }
        let mut items = HashMap::new();
        let ids: Vec<i64> = nodes.keys().copied().collect();
        for id in ids {
            item_total_in(id, &nodes, &kids, &mut items);
        }
        Ok(TreeIndex {
            nodes,
            kids,
            items,
            count,
            changed,
            empty,
        })
    }
}

/// The units of the items below `id` (not lost), memoised in `out`.
fn item_total_in(
    id: i64,
    nodes: &HashMap<i64, Node>,
    kids: &HashMap<i64, Vec<i64>>,
    out: &mut HashMap<i64, i64>,
) -> i64 {
    if let Some(t) = out.get(&id) {
        return *t;
    }
    let mut total = 0;
    for k in kids.get(&id).into_iter().flatten() {
        let n = &nodes[k];
        if n.kind == Kind::Item {
            total += n.qty.unwrap_or(1);
        }
        total += item_total_in(*k, nodes, kids, out);
    }
    out.insert(id, total);
    total
}

/// A code cut into runs of digits and of the rest, digits compared as numbers.
fn natural_key(s: &str) -> Vec<(bool, u64, String)> {
    let mut out: Vec<(bool, u64, String)> = Vec::new();
    let mut run = String::new();
    let mut digits = false;
    let flush = |run: &mut String, digits: bool, out: &mut Vec<(bool, u64, String)>| {
        if run.is_empty() {
            return;
        }
        if digits {
            out.push((false, run.parse().unwrap_or(u64::MAX), String::new()));
        } else {
            out.push((true, 0, fold(run)));
        }
        run.clear();
    };
    for c in s.chars() {
        if c.is_ascii_digit() != digits {
            flush(&mut run, digits, &mut out);
            digits = c.is_ascii_digit();
        }
        run.push(c);
    }
    flush(&mut run, digits, &mut out);
    out
}
fn subtree(t: &TreeIndex, id: i64, depth: usize) -> Value {
    let Some(n) = t.nodes.get(&id) else {
        return Value::Null;
    };
    let mut v = json!({
        "id": n.id, "code": n.code, "name": n.name, "kind": n.kind, "state": n.state,
    });
    // As in a row: only when true (spec/output.md).
    if n.lost {
        v["lost"] = json!(true);
    }
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
    if n.temporary {
        v["temporary"] = json!(true);
    }
    // How far a place gone through on its own has been counted.
    if let Some(s) = t.count.get(&id) {
        v["count"] = json!(s);
        if t.changed.contains(&id) {
            v["changed_since"] = json!(true);
        }
    }
    if t.empty.contains(&id) {
        v["empty"] = json!(true);
    }
    // What a holder is for and how much room it has, so one `ev tree` reads as a layout.
    for (k, val) in [("theme", &n.theme), ("size", &n.size)] {
        if let Some(x) = val {
            v[k] = json!(x);
        }
    }
    if let Some(f) = n.fill {
        v["fill"] = json!(f);
    }
    if !n.tags.is_empty() {
        v["tags"] = json!(n.tags);
    }
    v["updated_at"] = json!(n.updated_at);
    v["items"] = json!(t.items.get(&id).copied().unwrap_or(0));
    // A lost thing is not where it was last seen: the tree lists it apart.
    let children: Vec<Value> = if depth == 0 {
        Vec::new()
    } else {
        t.kids
            .get(&id)
            .into_iter()
            .flatten()
            .map(|k| subtree(t, *k, depth - 1))
            .collect()
    };
    v["children"] = json!(children);
    v
}

// ---------- references (spec §4, §11.4) ----------

/// A record that history may be added to: a live one by any reference, a gone one by its id
/// only (`512`, `#512`), as for `ev edit`. Documents and photos of a thing that left are its
/// history too (spec/past-belongings.md).
pub(crate) fn resolve_for_history(conn: &Connection, reference: &str) -> Result<i64> {
    match resolve(conn, reference, false) {
        Err(Error::NotFound(_))
            if reference
                .trim()
                .trim_start_matches('#')
                .chars()
                .all(|c| c.is_ascii_digit()) =>
        {
            resolve(conn, reference, true)
        }
        other => other,
    }
}

pub(crate) fn resolve(conn: &Connection, reference: &str, include_gone: bool) -> Result<i64> {
    let r = reference.trim();
    if r.is_empty() {
        return Err(Error::Usage("empty reference".into()));
    }
    // `#534`, as `ev ui` and the readable output print ids, is the id 534.
    let r = match r.strip_prefix('#') {
        Some(d) if !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()) => d,
        _ => r,
    };
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

    // A code as printed: `S3_11` finds `S3-11`, `S03` finds `S3` (spec/codes.md).
    let wanted_code = crate::fold::fold_code(r);
    let code_hits: Vec<_> = visible
        .iter()
        .filter(|x| x.1.as_deref().map(crate::fold::fold_code) == Some(wanted_code.clone()))
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

/// Today where the person is: what "today" means to them. Moments are kept in UTC (`now`), but
/// a day said or shown is the local one, or anything done between midnight and the UTC offset
/// lands on the day before.
pub(crate) fn today() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
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
    crate::plan::mark_work(conn, id, kind)
}

pub(crate) fn brief_json(conn: &Connection, id: i64) -> Result<Value> {
    serde_json::to_value(brief(conn, id)?).map_err(|e| Error::Internal(e.to_string()))
}

/// A code ending in `*` is the next free number of its series: `GF1x1-*` after `GF1x1-007`
/// is `GF1x1-008`, padded like the series (3 digits for a new one). Numbers are never reused:
/// gone nodes count too, since a printed label may still be around. Any other code is
/// returned as it is.
fn expand_code(conn: &Connection, code: &str) -> Result<String> {
    let c = code.trim();
    let Some(prefix) = c.strip_suffix('*') else {
        return Ok(c.to_string());
    };
    if prefix.is_empty() || prefix.contains('*') {
        return Err(Error::Usage(format!(
            "`{c}`: a series is a prefix followed by one `*`, like GF1x1-*"
        )));
    }
    // One series whatever its labels' separator and padding (spec/codes.md): `S3_*` continues
    // after `S3-11` and `S03_12`; the padding is the printed one, the widest in the series.
    let folded_prefix = crate::fold::fold_code(prefix);
    let mut stmt = conn.prepare("SELECT code FROM nodes WHERE code IS NOT NULL")?;
    let codes = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let (mut last, mut width) = (0u64, None::<usize>);
    for code in codes {
        let folded = crate::fold::fold_code(&code);
        let Some(rest) = folded.strip_prefix(&folded_prefix) else {
            continue;
        };
        if rest.is_empty() || !rest.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        if let Ok(n) = rest.parse::<u64>() {
            last = last.max(n);
            let printed = code.chars().rev().take_while(char::is_ascii_digit).count();
            width = Some(width.unwrap_or(0).max(printed));
        }
    }
    let width = width.unwrap_or(3);
    Ok(format!("{prefix}{:0width$}", last + 1))
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
    let folded = crate::fold::fold_code(c);
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
    if let Some(of) = non_empty(&new.of) {
        return add_of(conn, new, &of, parent);
    }
    // A past thing is a thing, unless said otherwise.
    let kind: Kind = match new.kind.trim() {
        "" if non_empty(&new.gone).is_some() => Kind::Item,
        k => k.parse()?,
    };
    let name = new.name.trim();
    if name.is_empty() {
        return Err(Error::Usage("name is empty".into()));
    }
    check_ranges(new.qty, new.fill)?;
    let address = non_empty(&new.address);
    if address.is_some() && kind != Kind::Home {
        return Err(refused("only a home has an address", Value::Null));
    }
    // A past thing is recorded already gone, in no holder (spec/past-belongings.md).
    let gone: Option<Disposition> = non_empty(&new.gone).map(|g| g.parse()).transpose()?;
    if let Some(g) = gone {
        if matches!(
            g,
            Disposition::Mistake | Disposition::Merged | Disposition::Digitize
        ) {
            return Err(Error::Usage(format!(
                "a past thing is not added as {}: give how it left (sell, give, trash, used, trade, left, stolen, unknown)",
                g.as_str()
            )));
        }
        if kind == Kind::Home || parent.is_some() || new.lost {
            return Err(Error::Usage(
                "a past thing is added on its own: no --in, --lost, or home".into(),
            ));
        }
        // Where a thing stands, or is to go, says nothing of one that left.
        if non_empty(&new.to).is_some() || new.temporary || non_empty(&new.code).is_some() {
            return Err(Error::Usage(
                "a past thing has no --to, --temporary or --code: it is no longer here".into(),
            ));
        }
        if non_empty(&new.place).is_some_and(|p| p.starts_with('#')) {
            return Err(Error::Usage(
                "--where names a place (a former home), not a record".into(),
            ));
        }
    } else if non_empty(&new.at).is_some() || non_empty(&new.place).is_some() {
        return Err(Error::Usage(
            "--at and --where say how a thing left: add it with --gone".into(),
        ));
    }
    let came = non_empty(&new.came)
        .map(|c| past::partial_date(&c))
        .transpose()?;
    if let (Some(c), Some(a)) = (&came, non_empty(&new.at)) {
        past::came_before_left(c, &past::partial_date(&a)?)?;
    }
    if gone.is_none() {
        check_placement(conn, kind, parent, new.lost, None)?;
    }
    let code = non_empty(&new.code)
        .map(|c| expand_code(conn, &c))
        .transpose()?;
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
    if new.temporary {
        conn.execute("UPDATE nodes SET temporary = 1 WHERE id = ?1", [id])?;
    }
    for (column, text) in [
        ("make", &new.make),
        ("model", &new.model),
        ("serial", &new.serial),
    ] {
        if let Some(t) = non_empty(text) {
            conn.execute(
                &format!("UPDATE nodes SET {column} = ?1 WHERE id = ?2"),
                params![t, id],
            )?;
        }
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
            "INSERT INTO photos (node_id, position, path) VALUES (?1, ?2, ev_store(?3))",
            params![id, i as i64, p],
        )?;
    }
    event(
        conn,
        id,
        "create",
        json!({ "name": name, "kind": kind, "parent": parent, "code": code, "lost": new.lost }),
    )?;
    if let Some(c) = came {
        conn.execute(
            "UPDATE nodes SET came_at = ?1 WHERE id = ?2",
            params![c, id],
        )?;
    }
    if let Some(g) = gone {
        conn.execute(
            "UPDATE nodes SET state = 'gone', disposition = ?1 WHERE id = ?2",
            params![g.as_str(), id],
        )?;
        event(conn, id, "gone", json!({ "as": g, "past": true }))?;
        past::set_departure(
            conn,
            id,
            non_empty(&new.at).as_deref(),
            non_empty(&new.place).as_deref(),
        )?;
    }
    Ok(id)
}

/// `ev add --of`: more units of a thing already recorded, in `parent`. What the thing is comes
/// from it; what is said of these units (count, note, photos) from `new`. They are a portion of
/// the same thing, and join a portion already in that place. Returns the record that holds them.
/// The record may be gone: a cassette used up and replaced with the same one is the same thing,
/// and the used-up one is often its only record.
fn add_of(conn: &Connection, new: &NewNode, of: &str, parent: Option<i64>) -> Result<i64> {
    let src = load(conn, resolve(conn, of, true)?)?;
    if let Some(e) = crate::portions::not_a_portion(&src) {
        return Err(e);
    }
    if parent.is_none() {
        return Err(Error::Usage("say where they are with --in".into()));
    }
    let made = NewNode {
        name: src.name.clone(),
        kind: Kind::Item.to_string(),
        make: src.make.clone(),
        model: src.model.clone(),
        size: src.size.clone(),
        tags: src.tags.clone(),
        of: None,
        ..new.clone()
    };
    let id = add_one(conn, &made, parent)?;
    let thing = src.thing.unwrap_or(src.id);
    conn.execute(
        "UPDATE nodes SET thing = ?1 WHERE id IN (?2, ?3)",
        params![thing, src.id, id],
    )?;
    event(conn, id, "more_of", json!({ "of": src.id }))?;
    crate::portions::join_here(conn, id)
}

fn apply_move(conn: &Connection, node: &Node, target: i64, kind: &str) -> Result<()> {
    check_placement(conn, node.kind, Some(target), false, Some(node.id))?;
    conn.execute(
        "UPDATE nodes SET parent_id = ?1, pending_to = NULL, lost = 0 WHERE id = ?2",
        params![target, node.id],
    )?;
    // A thing parked on its own (`temporary`) that moves has left its stop: like `lost`, the
    // mark goes with the move. Moved into another parking place, that place's mark says it.
    // A place keeps its own mark: a parking shelf moved is still a parking shelf.
    let unparked = node.temporary && node.kind == Kind::Item;
    if unparked {
        conn.execute("UPDATE nodes SET temporary = 0 WHERE id = ?1", [node.id])?;
    }
    // A thing that waited for another has found its place by moving (spec/waits-for.md).
    if node.waits_for.is_some() {
        conn.execute("UPDATE nodes SET waits_for = NULL WHERE id = ?1", [node.id])?;
    }
    // Cells are positions in the old holder's grid; they mean nothing anywhere else. A sketch
    // is in the old holder's frame too, but it can be carried: it is translated so the node
    // stays where it lies on the map.
    if node.parent_id != Some(target) {
        conn.execute("DELETE FROM cells WHERE node_id = ?1", [node.id])?;
        if let Some(from) = node.parent_id {
            crate::map::carry_sketch(conn, node.id, from, target)?;
        }
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
        json!({
            "from": node.parent_id, "to": target, "dropped_pending": dropped,
            "was_lost": node.lost, "was_temporary": unparked, "waited_for": node.waits_for,
        }),
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

/// Only what goes in the bin is shredded; a thing given away or sold leaves whole.
fn check_shred(d: Disposition, shred: bool) -> Result<()> {
    if shred && !matches!(d, Disposition::Trash | Disposition::Digitize) {
        return Err(Error::Usage(format!(
            "--shred is for what goes in the bin (trash, digitize), not `{d}`"
        )));
    }
    Ok(())
}

/// The short side, in pixels, below which a copy may not read: a ticket's small print needs
/// about this much. A guess to warn on, never to refuse on.
const COPY_SHORT_SIDE: u32 = 800;

/// `merged` is ev's own word for a portion that joined another; nobody says a thing left so.
fn not_merged(disposition: Option<Disposition>) -> Result<()> {
    if disposition == Some(Disposition::Merged) {
        return Err(Error::Usage(
            "`merged` is not a way to leave: ev sets it when a portion joins another".into(),
        ));
    }
    Ok(())
}

/// Whether an edit sets a make or a model: the fields a purchase is matched by best.
fn sets_identity(assignments: &[String]) -> bool {
    assignments
        .iter()
        .any(|a| a.starts_with("make=") || a.starts_with("model="))
}

/// Adds the purchase lines a node could be (purchases spec §5) to a result, when there are any:
/// asked while the thing is in hand, after `add`, `split`, `found` and a new make or model.
/// A found thing's answer names what waited for it (spec/waits-for.md), so the agent asks
/// where those go now.
fn with_waiting(conn: &Connection, id: i64, mut v: Value) -> Result<Value> {
    let waiting = waited_for_by(conn, id)?;
    if !waiting.is_empty() {
        v["waiting"] = serde_json::to_value(waiting).map_err(|e| Error::Internal(e.to_string()))?;
        // Said once: `waiting` is the same list as the node's `waited_for_by`.
        if let Some(o) = v["node"].as_object_mut() {
            o.remove("waited_for_by");
        }
        if let Some(o) = v.as_object_mut() {
            o.remove("waited_for_by");
        }
    }
    Ok(v)
}

fn offer_purchases(conn: &Connection, id: i64, mut v: Value) -> Result<Value> {
    // A portion of a thing its purchases already account for asks nothing (spec/portions.md
    // §6); a line linked to the thing is no question either.
    let n = load(conn, id)?;
    if n.thing.is_some()
        && let Some(bought) = crate::portions::bought(conn, &n)?
        && crate::portions::here(conn, &n)? <= bought
    {
        return Ok(v);
    }
    // A part of a kit bought as one line has that line (spec/kit-purchase.md).
    if !crate::purchases::kit_purchases_of(conn, id)?.is_empty() {
        return Ok(v);
    }
    let offered: Vec<Value> =
        crate::purchase_match::candidates_for(conn, id, crate::purchase_match::OFFER_AT, 3)?
            .into_iter()
            .filter(|c| c["linked"] != true)
            .collect();
    if !offered.is_empty() {
        v["purchase_candidates"] = json!(offered);
    }
    Ok(v)
}

/// A digitized thing leaves only with a copy on its own record: a photo, or a document linked
/// to it. Refuses when there is none; returns a warning when every copy is an image too small
/// to be sure it reads (a PDF or any other file counts as readable).
fn require_copy(conn: &Connection, node: &Node) -> Result<Option<String>> {
    let files: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT ev_file(path) FROM photos WHERE node_id = ?1
             UNION ALL
             SELECT ev_file(d.file) FROM documents d JOIN document_links l ON l.document_id = d.id
              WHERE l.target = 'node' AND l.target_id = ?1",
        )?;
        stmt.query_map([node.id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    if files.is_empty() {
        return Err(refused(
            format!(
                "{} has no copy yet; attach one with `ev photo add` or `ev doc add --for` before \
                 it leaves as digitized",
                label(node)
            ),
            Value::Null,
        ));
    }
    let sides: Vec<Option<u32>> = files
        .iter()
        .map(|f| crate::photo::short_side(std::path::Path::new(f)))
        .collect();
    if sides.iter().any(Option::is_none) {
        return Ok(None);
    }
    let best = sides.into_iter().flatten().max().unwrap_or(0);
    Ok((best < COPY_SHORT_SIDE).then(|| {
        format!(
            "the sharpest copy of {} is {best} px on its short side; check that it reads \
             before the paper is thrown out",
            label(node)
        )
    }))
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
        crate::marks::clear_shred(&tx, node.id)?;
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
        inv.edit("Ev", &["owner=Annemler".into()]).unwrap();
        assert_eq!(inv.node(1).unwrap().owner.as_deref(), Some("Annemler"));
    }
}
