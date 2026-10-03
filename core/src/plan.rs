//! The tidy-up plan (spec §17): which places have been gone through, what was noticed in them,
//! an ordered work list with a reason for every entry, and the household's goal.
//!
//! The judgement — is this drawer a mess, what comes first — stays with the agent and the
//! person. This module only makes the state durable and complete, so the next session starts
//! from a list instead of from memory.

use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::error::refused;
use crate::model::{Kind, Node, State};
use crate::store::{Inventory, brief, event, ids, live_nodes, now, resolve, rules_json, show};
use crate::{Error, Result};

/// What the household wants from ev: `organize` (tidy up, with a plan) or `track` (only keep
/// the records right; no tidy-up proposals).
const GOALS: [&str; 2] = ["organize", "track"];

/// How far a place has been counted: `raw` (not counted, the absence of a row), `counting`
/// (its tour has begun), `toured` (counted), `kept` (left as it is on purpose).
const REVIEWS: [&str; 3] = ["counting", "toured", "kept"];

const TASK_STATES: [&str; 4] = ["open", "doing", "done", "dropped"];

pub(crate) fn review_of(conn: &Connection, id: i64) -> Result<Value> {
    Ok(conn
        .query_row(
            "SELECT status, at, note FROM reviews WHERE node_id = ?1",
            [id],
            |r| {
                Ok(json!({
                    "status": r.get::<_, String>(0)?,
                    "at": r.get::<_, String>(1)?,
                    "note": r.get::<_, Option<String>>(2)?,
                }))
            },
        )
        .optional()?
        .unwrap_or(Value::Null))
}

pub(crate) fn observations_of(conn: &Connection, id: i64) -> Result<Vec<Value>> {
    let mut stmt = conn
        .prepare("SELECT id, text, photo, at FROM observations WHERE node_id = ?1 ORDER BY id")?;
    let rows = stmt
        .query_map([id], |r| {
            Ok(json!({
                "id": r.get::<_, i64>(0)?,
                "text": r.get::<_, String>(1)?,
                "photo": r.get::<_, Option<String>>(2)?,
                "at": r.get::<_, String>(3)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Unfinished tasks that concern a node: those linked to it, and those linked to a place that
/// holds it (a task on a drawer concerns every box in it). `via` names the node the link is on.
pub(crate) fn tasks_of(conn: &Connection, id: i64) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    let mut cur = Some(id);
    while let Some(c) = cur {
        for t in ids(
            conn,
            "SELECT t.id FROM tasks t JOIN task_nodes tn ON tn.task_id = t.id
              WHERE tn.node_id = ?1 AND t.status IN ('open','doing') ORDER BY t.rank, t.id",
            [c],
        )? {
            let full = task_json(conn, t)?;
            out.push(json!({
                "id": t,
                "position": full["position"],
                "title": full["title"],
                "status": full["status"],
                "via": c,
            }));
        }
        cur = conn
            .query_row("SELECT parent_id FROM nodes WHERE id = ?1", [c], |r| {
                r.get(0)
            })
            .optional()?
            .flatten();
    }
    Ok(out)
}

/// The places a person opens one at a time: the innermost labelled holders, plus unlabelled
/// holders standing on their own in a room or on furniture. A holder is a unit when none of its
/// children carries a code (a code is a physical label, so a coded child is a place of its
/// own); everything below a unit is gone through with it. `K4x4-08-A` is a unit and the boxes
/// in it are not; `K4x4-08` is not, because its drawers are labelled.
pub(crate) fn units(all: &[Node]) -> Vec<i64> {
    let mut kids: HashMap<Option<i64>, Vec<&Node>> = HashMap::new();
    for n in all {
        kids.entry(n.parent_id).or_default().push(n);
    }
    let mut out = Vec::new();
    let mut stack: Vec<&Node> = kids.get(&None).cloned().unwrap_or_default();
    while let Some(n) = stack.pop() {
        let children = kids.get(&Some(n.id)).cloned().unwrap_or_default();
        if n.lost {
            continue;
        }
        if unit_by_children(n, &children) {
            out.push(n.id);
        } else {
            stack.extend(children);
        }
    }
    out.sort_unstable();
    out
}

/// Whether a node is a unit, given its live children: a holder with no labelled child, or a
/// room with nothing in it that holds things (an empty kitchen is counted as one place).
fn unit_by_children(n: &Node, children: &[&Node]) -> bool {
    if n.state != State::Active || n.lost {
        return false;
    }
    match n.kind {
        Kind::Home | Kind::Item => false,
        Kind::Room => !children
            .iter()
            .any(|c| matches!(c.kind, Kind::Room | Kind::Furniture | Kind::Container)),
        _ => !children.iter().any(|c| c.code.is_some()),
    }
}

/// How far a place has been counted when it is one of the places gone through one at a time
/// (a unit): `raw`, `counting`, `toured` or `kept`. None for what is inside a unit (a box in
/// a counted drawer is counted with it) and for what is above the units (a room with
/// furniture in it is counted through its furniture).
pub(crate) fn count_state(conn: &Connection, id: i64) -> Result<Option<String>> {
    let Some((unit, status)) = unit_state(conn, id)? else {
        return Ok(None);
    };
    Ok((unit == id).then_some(status))
}

/// The unit a node is or is in, and how far it has been counted.
fn unit_state(conn: &Connection, id: i64) -> Result<Option<(i64, String)>> {
    // The path from the top down; the unit is the first node on it that is one.
    let mut chain = Vec::new();
    let mut cur = Some(id);
    while let Some(c) = cur {
        let n = crate::store::load(conn, c)?;
        cur = n.parent_id;
        chain.push(n);
    }
    chain.reverse();
    let mut unit = None;
    for n in &chain {
        if n.lost {
            // What is lost is not a place anyone goes through.
            return Ok(None);
        }
        if n.state != State::Active || matches!(n.kind, Kind::Home | Kind::Item) {
            continue;
        }
        // The same test as `unit_by_children`, asked of the database.
        let blocking = if n.kind == Kind::Room {
            "kind IN ('room', 'furniture', 'container')"
        } else {
            "code IS NOT NULL"
        };
        let count: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM nodes WHERE parent_id = ?1 AND state != 'gone' AND {blocking}"
            ),
            [n.id],
            |r| r.get(0),
        )?;
        if count == 0 {
            unit = Some(n.id);
            break;
        }
    }
    let Some(u) = unit else { return Ok(None) };
    let r = review_inherited(conn, u)?;
    Ok(Some((u, r["status"].as_str().unwrap_or("raw").to_string())))
}

/// The review a unit inherits: its own, or the nearest reviewed ancestor's.
pub(crate) fn effective_review(
    id: i64,
    parent: &HashMap<i64, Option<i64>>,
    reviews: &HashMap<i64, (String, String)>,
) -> Option<(i64, String, String)> {
    let mut cur = Some(id);
    while let Some(c) = cur {
        if let Some((s, at)) = reviews.get(&c) {
            return Some((c, s.clone(), at.clone()));
        }
        cur = parent.get(&c).copied().flatten();
    }
    None
}

/// The review a place stands under: its own, or its nearest reviewed ancestor's (a toured
/// drawer covers the boxes in it), as `{status, at, from}`; null when none is.
pub(crate) fn review_inherited(conn: &Connection, id: i64) -> Result<Value> {
    let mut cur = Some(id);
    for _ in 0..10_000 {
        let Some(c) = cur else { break };
        let own = review_of(conn, c)?;
        if !own.is_null() {
            let mut v = own;
            v["from"] = json!(c);
            return Ok(v);
        }
        cur = conn.query_row("SELECT parent_id FROM nodes WHERE id = ?1", [c], |r| {
            r.get(0)
        })?;
    }
    Ok(Value::Null)
}

pub(crate) fn all_reviews(conn: &Connection) -> Result<HashMap<i64, (String, String)>> {
    let mut stmt = conn.prepare("SELECT node_id, status, at FROM reviews")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?
        .collect::<rusqlite::Result<HashMap<_, _>>>()?;
    Ok(rows)
}

pub(crate) fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}

fn task_nodes(conn: &Connection, task: i64) -> Result<Vec<i64>> {
    ids(
        conn,
        "SELECT node_id FROM task_nodes WHERE task_id = ?1 ORDER BY node_id",
        [task],
    )
}

/// A `tasks` row: title, why, status, note, created, updated, closed, due.
type TaskRow = (
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    Option<String>,
    Option<String>,
);

fn task_json(conn: &Connection, id: i64) -> Result<Value> {
    let (title, why, status, note, created, updated, closed, due): TaskRow = conn
        .query_row(
            "SELECT title, why, status, note, created_at, updated_at, closed_at, due
               FROM tasks WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("no task with id {id}")))?;
    let nodes = task_nodes(conn, id)?
        .into_iter()
        .map(|n| {
            let b = brief(conn, n)?;
            let mut v = serde_json::to_value(b).map_err(|e| Error::Internal(e.to_string()))?;
            v["review"] = review_of(conn, n)?;
            Ok(v)
        })
        .collect::<Result<Vec<_>>>()?;
    // Position among the unfinished tasks, 1-based, which is what the person answers with.
    let position: Option<i64> = if matches!(status.as_str(), "open" | "doing") {
        Some(conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE status IN ('open','doing')
               AND rank <= (SELECT rank FROM tasks WHERE id = ?1)",
            [id],
            |r| r.get(0),
        )?)
    } else {
        None
    };
    let days_left = due.as_deref().and_then(days_left);
    Ok(json!({
        "id": id,
        "position": position,
        "title": title,
        "why": why,
        "status": status,
        "note": note,
        "due": due,
        "days_left": days_left,
        "nodes": nodes,
        "created_at": created,
        "updated_at": updated,
        "closed_at": closed,
    }))
}

/// A due date this close (or past) puts its task ahead of the order in `next`.
const DUE_SOON_DAYS: i64 = 1;

/// A due date as given: `YYYY-MM-DD`, or nothing for an empty text or `none`.
fn parse_due(due: Option<&str>) -> Result<Option<String>> {
    match due.map(str::trim).filter(|d| !d.is_empty() && *d != "none") {
        None => Ok(None),
        Some(d) => chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .map(|d| Some(d.to_string()))
            .map_err(|_| Error::Usage(format!("due is YYYY-MM-DD, got `{d}`"))),
    }
}

/// Days from today to a `YYYY-MM-DD` date; negative when it is past.
fn days_left(due: &str) -> Option<i64> {
    chrono::NaiveDate::parse_from_str(due, "%Y-%m-%d")
        .ok()
        .map(|d| (d - chrono::Utc::now().date_naive()).num_days())
}

/// The counts of `progress` without its list of places, as `next` and `todo` show them.
pub(crate) fn progress_summary(p: &Value) -> Value {
    json!({
        "units": p["units"],
        "toured": p["toured"],
        "kept": p["kept"],
        "counting": p["counting"],
        "raw": p["raw"],
        "changed_since_tour": p["changed_since_tour"],
    })
}

/// Whether a `todo` entry (a brief node, or one wrapped as `{node}`) is among `under`: a place
/// and everything inside it.
fn inside(entry: &Value, under: &HashSet<i64>) -> bool {
    let node = if entry["path_text"].is_string() {
        entry
    } else {
        &entry["node"]
    };
    node["id"].as_i64().is_some_and(|id| under.contains(&id))
}

/// Everything `todo` lists that sits in `place`: the small jobs done while it is open anyway,
/// so they ride with the tour instead of being ranked on their own. Empty lists are left out.
fn while_there(
    conn: &Connection,
    place: i64,
    todo: &Value,
    uncovered: &[i64],
    unvalued: &[i64],
) -> Result<Value> {
    let under: HashSet<i64> = ids(
        conn,
        "WITH RECURSIVE d(id) AS (
             SELECT ?1 UNION ALL SELECT n.id FROM nodes n JOIN d ON n.parent_id = d.id
         )
         SELECT id FROM d",
        [place],
    )?
    .into_iter()
    .collect();
    let pick = |list: &Value| -> Vec<Value> {
        list.as_array()
            .into_iter()
            .flatten()
            .filter(|e| inside(e, &under))
            .cloned()
            .collect()
    };
    let mut out = serde_json::Map::new();
    for key in ["photos", "labels", "unclear", "parked"] {
        let found = pick(&todo[key]);
        if !found.is_empty() {
            out.insert(key.into(), json!(found));
        }
    }
    // A planned move out of the place: the thing goes along when the person leaves.
    let leaving: Vec<Value> = todo["moves"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| inside(&m["node"], &under) && !inside(&m["to"], &under))
        .cloned()
        .collect();
    if !leaving.is_empty() {
        out.insert("leaving".into(), json!(leaving));
    }
    let mut disposals = Vec::new();
    for (as_, list) in todo["disposals"].as_object().into_iter().flatten() {
        for e in pick(list) {
            let mut e = e;
            e["as"] = json!(as_);
            disposals.push(e);
        }
    }
    if !disposals.is_empty() {
        out.insert("disposals".into(), json!(disposals));
    }
    // A lost thing last seen here is worth a look while the place is open.
    let lost: Vec<Value> = todo["lost"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|l| l["last_seen"].is_object() && inside(&l["last_seen"], &under))
        .cloned()
        .collect();
    if !lost.is_empty() {
        out.insert("lost".into(), json!(lost));
    }
    for (key, list) in [("coverage", uncovered), ("values", unvalued)] {
        let mut found = Vec::new();
        for n in list.iter().filter(|n| under.contains(n)) {
            found.push(
                serde_json::to_value(brief(conn, *n)?)
                    .map_err(|e| Error::Internal(e.to_string()))?,
            );
        }
        if !found.is_empty() {
            out.insert(key.into(), json!(found));
        }
    }
    Ok(Value::Object(out))
}

fn non_empty<'a>(what: &str, s: &'a str) -> Result<&'a str> {
    let t = s.trim();
    if t.is_empty() {
        return Err(Error::Usage(format!("{what} is empty")));
    }
    Ok(t)
}

/// Renumbers the unfinished tasks 1..n in their current order, placing `moving` at `at`.
fn rerank(conn: &Connection, moving: Option<i64>, at: Option<usize>) -> Result<()> {
    let mut order = ids(
        conn,
        "SELECT id FROM tasks WHERE status IN ('open','doing') ORDER BY rank, id",
        [],
    )?;
    if let Some(m) = moving {
        order.retain(|x| *x != m);
        let i = at.unwrap_or(usize::MAX).saturating_sub(1).min(order.len());
        order.insert(i, m);
    }
    for (i, t) in order.iter().enumerate() {
        conn.execute(
            "UPDATE tasks SET rank = ?1 WHERE id = ?2",
            params![i as i64 + 1, t],
        )?;
    }
    Ok(())
}

impl Inventory {
    /// The household's goal, or sets it when `goal` is given.
    pub fn goal(&mut self, goal: Option<&str>) -> Result<Value> {
        if let Some(g) = goal {
            let g = g.trim().to_lowercase();
            if !GOALS.contains(&g.as_str()) {
                return Err(Error::Usage(format!(
                    "goal must be one of {}",
                    GOALS.join(", ")
                )));
            }
            self.conn.execute(
                "INSERT INTO settings (key, value) VALUES ('goal', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [g],
            )?;
        }
        Ok(json!({ "goal": get_setting(&self.conn, "goal")? }))
    }

    /// Asks a running `ev ui` to show a node — and one of its photos full screen, the last one
    /// when `photo` is not given — so the person sees exactly which thing is meant. `None`
    /// clears the request. The UI is read-only; it remembers which request it has shown.
    pub fn focus(&mut self, reference: Option<&str>, photo: Option<usize>) -> Result<Value> {
        let Some(r) = reference else {
            match std::fs::remove_file(&self.focus_file) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(Error::Internal(format!(
                        "cannot clear {}: {e}",
                        self.focus_file.display()
                    )));
                }
                _ => {}
            }
            return Ok(json!({ "focus": null }));
        };
        let id = resolve(&self.conn, r, true)?;
        let photos: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE node_id = ?1",
            [id],
            |r| r.get(0),
        )?;
        let photo = match photo {
            Some(n) if n == 0 || n as i64 > photos => {
                return Err(Error::NotFound(format!("node {id} has no photo {n}")));
            }
            Some(n) => Some(n as i64),
            None if photos > 0 => Some(photos),
            None => None,
        };
        // Milliseconds, so two requests in the same second are still two requests.
        let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let value = json!({ "id": id, "photo": photo, "at": at });
        self.send_focus(&value)?;
        Ok(json!({ "focus": value }))
    }

    /// Asks a running `ev ui` to show pictures that are no record — photos marked with
    /// `photo_mark` — full screen, titled with `note`, stepped through with `[` `]`, until the
    /// person closes them. Sent together, so the parts and where they go arrive as one.
    pub fn focus_file(
        &mut self,
        files: &[std::path::PathBuf],
        note: Option<&str>,
    ) -> Result<Value> {
        if files.is_empty() {
            return Err(Error::Usage("name at least one picture".into()));
        }
        let mut paths = Vec::new();
        for file in files {
            if !file.is_file() {
                return Err(Error::NotFound(format!("no file {}", file.display())));
            }
            let abs = std::path::absolute(file).unwrap_or_else(|_| file.clone());
            paths.push(abs.to_string_lossy().into_owned());
        }
        let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let value = json!({ "files": paths, "note": note, "at": at });
        self.send_focus(&value)?;
        Ok(json!({ "focus": value }))
    }

    /// Writes a focus request beside the database (`ev.db-focus.json`), never into it: it is a
    /// message to a running `ev ui`, not a change to the inventory, so it leaves `ev.db` as it
    /// was. Written whole and renamed into place, so the UI never reads half of one.
    fn send_focus(&self, value: &Value) -> Result<()> {
        let io = |e: std::io::Error| {
            Error::Internal(format!("cannot write {}: {e}", self.focus_file.display()))
        };
        let part = self.focus_file.with_extension("json.part");
        std::fs::write(&part, value.to_string()).map_err(io)?;
        std::fs::rename(&part, &self.focus_file).map_err(io)
    }

    /// The pending focus request, if any.
    pub fn focus_request(&self) -> Result<Value> {
        Ok(std::fs::read_to_string(&self.focus_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Null))
    }

    /// Records something noticed about a place, optionally tied to one of its photos (1-based).
    pub fn observe(&mut self, reference: &str, text: &str, photo: Option<usize>) -> Result<Value> {
        let text = non_empty("observation", text)?;
        let tx = self.conn.transaction()?;
        let id = resolve(&tx, reference, false)?;
        let photo_path = match photo {
            None => None,
            Some(n) => Some(
                tx.query_row(
                    "SELECT ev_file(path) FROM photos WHERE node_id = ?1 ORDER BY position LIMIT 1 OFFSET ?2",
                    params![id, n.saturating_sub(1) as i64],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| Error::NotFound(format!("node {id} has no photo {n}")))?,
            ),
        };
        tx.execute(
            "INSERT INTO observations (node_id, text, photo, at) VALUES (?1, ?2, ?3, ?4)",
            params![id, text, photo_path, now()],
        )?;
        event(&tx, id, "observe", json!({ "text": text }))?;
        tx.commit()?;
        show(&self.conn, id)
    }

    /// Removes one observation that turned out wrong or no longer holds — or whose work is
    /// done. The place's history keeps both ends: the `observe` event and an `unobserve` event
    /// with the same text.
    pub fn unobserve(&mut self, observation: i64) -> Result<Value> {
        let row: Option<(i64, String)> = self
            .conn
            .query_row(
                "SELECT node_id, text FROM observations WHERE id = ?1",
                [observation],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (node, text) =
            row.ok_or_else(|| Error::NotFound(format!("no observation {observation}")))?;
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM observations WHERE id = ?1", [observation])?;
        event(
            &tx,
            node,
            "unobserve",
            json!({ "observation": observation, "text": text }),
        )?;
        tx.commit()?;
        show(&self.conn, node)
    }

    /// Marks how far a place has been counted: `counting` (its tour has begun), `toured` (every
    /// thing in it looked at and the person said it is done), `kept` (the person wants it left
    /// as it is), or `raw` (not counted, the default) to clear it. Everything below the place
    /// inherits the mark.
    pub fn review(&mut self, reference: &str, status: &str, note: Option<&str>) -> Result<Value> {
        let status = status.trim().to_lowercase();
        let tx = self.conn.transaction()?;
        let id = resolve(&tx, reference, false)?;
        match status.as_str() {
            "raw" => {
                tx.execute("DELETE FROM reviews WHERE node_id = ?1", [id])?;
            }
            s if REVIEWS.contains(&s) => {
                // A place is toured with photos that show it as it is now: its own and those of
                // every box in its grid, whenever their contents changed after their newest
                // photo. Fix it with a photo (`ev photo cut … --grid`) or `ev photo current`.
                if s == "toured" {
                    let mut check = vec![id];
                    check.extend(crate::grid::placed(&tx, id)?.into_iter().map(|(b, _)| b));
                    let mut stale = Vec::new();
                    for n in check {
                        let holds: i64 = tx.query_row(
                            "SELECT COUNT(*) FROM nodes WHERE parent_id = ?1 AND state != 'gone'",
                            [n],
                            |r| r.get(0),
                        )?;
                        if holds == 0 {
                            continue;
                        }
                        if let Some((reason, photo_at, changed)) =
                            crate::marks::photo_stale(&tx, n)?
                        {
                            stale.push(json!({
                                "node": brief(&tx, n)?,
                                "reason": reason,
                                "photo_at": photo_at,
                                "changed_at": changed,
                            }));
                        }
                    }
                    if !stale.is_empty() {
                        return Err(refused(
                            format!(
                                "{} photo(s) are older than what they show; attach a current \
                                 photo (`ev photo cut <photo> --place <ref> --grid …`) or say \
                                 an old one still holds (`ev photo current <ref>`)",
                                stale.len()
                            ),
                            json!({ "stale": stale }),
                        ));
                    }
                }
                tx.execute(
                    "INSERT INTO reviews (node_id, status, at, note) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(node_id) DO UPDATE SET status = excluded.status,
                       at = excluded.at, note = excluded.note",
                    params![id, s, now(), note.map(str::trim).filter(|n| !n.is_empty())],
                )?;
            }
            _ => {
                return Err(Error::Usage(
                    "review must be counting, toured, kept or raw".to_string(),
                ));
            }
        }
        event(&tx, id, "review", json!({ "as": status, "note": note }))?;
        tx.commit()?;
        show(&self.conn, id)
    }

    /// Every unit (see `units`) with how far it has been gone through, and which toured ones
    /// changed since.
    pub fn progress(&self) -> Result<Value> {
        let all = live_nodes(&self.conn)?;
        let parent: HashMap<i64, Option<i64>> = all.iter().map(|n| (n.id, n.parent_id)).collect();
        let mut kids: HashMap<i64, Vec<&Node>> = HashMap::new();
        for n in &all {
            if let Some(p) = n.parent_id {
                kids.entry(p).or_default().push(n);
            }
        }
        let reviews = all_reviews(&self.conn)?;
        let changes = crate::marks::ContentChanges::load(&self.conn)?;
        let planned: HashSet<i64> = ids(
            &self.conn,
            "SELECT DISTINCT tn.node_id FROM task_nodes tn JOIN tasks t ON t.id = tn.task_id
              WHERE t.status IN ('open','doing')",
            [],
        )?
        .into_iter()
        .collect();
        let mut list = Vec::new();
        let (mut toured, mut kept, mut raw, mut counting, mut stale) = (0, 0, 0, 0, 0);
        for u in units(&all) {
            let mut v = serde_json::to_value(brief(&self.conn, u)?)
                .map_err(|e| Error::Internal(e.to_string()))?;
            let direct = kids.get(&u).map_or(0, Vec::len);
            v["children"] = json!(direct);
            v["observations"] = json!(observations_of(&self.conn, u)?.len());
            v["planned"] = json!(planned.contains(&u));
            match effective_review(u, &parent, &reviews) {
                Some((from, status, at)) => {
                    // Only a change to what the place holds dates a tour: re-coding a box,
                    // linking a document or editing a note leaves the count as it was.
                    let changed = changes.at(u).is_some_and(|c| c > at);
                    match status.as_str() {
                        "toured" => toured += 1,
                        "counting" => counting += 1,
                        _ => kept += 1,
                    }
                    if changed && status == "toured" {
                        stale += 1;
                    }
                    v["review"] = json!({ "status": status, "at": at, "from": from, "changed_since": changed });
                }
                None => {
                    raw += 1;
                    v["review"] = json!({ "status": "raw" });
                }
            }
            list.push(v);
        }
        Ok(json!({
            "goal": get_setting(&self.conn, "goal")?,
            "units": list.len(),
            "toured": toured,
            "kept": kept,
            "counting": counting,
            "raw": raw,
            "changed_since_tour": stale,
            "places": list,
        }))
    }

    /// Adds a task at position `at` (1-based among unfinished tasks; last when omitted).
    pub fn task_add(
        &mut self,
        title: &str,
        why: &str,
        on: &[String],
        at: Option<usize>,
    ) -> Result<Value> {
        self.task_add_with(title, why, on, at, None)
    }

    /// `task_add` with a due date (`YYYY-MM-DD`), checked before anything is written.
    pub fn task_add_with(
        &mut self,
        title: &str,
        why: &str,
        on: &[String],
        at: Option<usize>,
        due: Option<&str>,
    ) -> Result<Value> {
        let title = non_empty("title", title)?;
        let why = non_empty("why", why)?;
        let due = parse_due(due)?;
        let tx = self.conn.transaction()?;
        let nodes = on
            .iter()
            .map(|r| resolve(&tx, r, false))
            .collect::<Result<Vec<_>>>()?;
        let t = now();
        tx.execute(
            "INSERT INTO tasks (title, why, rank, status, created_at, updated_at, due)
             VALUES (?1, ?2, 1000000, 'open', ?3, ?3, ?4)",
            params![title, why, t, due],
        )?;
        let id = tx.last_insert_rowid();
        for n in nodes {
            tx.execute(
                "INSERT OR IGNORE INTO task_nodes (task_id, node_id) VALUES (?1, ?2)",
                params![id, n],
            )?;
        }
        rerank(&tx, Some(id), at)?;
        tx.commit()?;
        task_json(&self.conn, id)
    }

    /// Unfinished tasks in order; with `all`, finished and dropped ones after them.
    pub fn task_list(&self, all: bool) -> Result<Value> {
        let open = ids(
            &self.conn,
            "SELECT id FROM tasks WHERE status IN ('open','doing') ORDER BY rank, id",
            [],
        )?;
        let mut tasks = open
            .iter()
            .map(|t| task_json(&self.conn, *t))
            .collect::<Result<Vec<_>>>()?;
        if all {
            for t in ids(
                &self.conn,
                "SELECT id FROM tasks WHERE status IN ('done','dropped') ORDER BY closed_at DESC, id DESC",
                [],
            )? {
                tasks.push(task_json(&self.conn, t)?);
            }
        }
        Ok(json!({ "tasks": tasks }))
    }

    pub fn task_show(&self, id: i64) -> Result<Value> {
        task_json(&self.conn, id)
    }

    /// Moves a task to `status`. `done` and `dropped` close it; `open`/`doing` reopen it.
    pub fn task_set(&mut self, id: i64, status: &str, note: Option<&str>) -> Result<Value> {
        if !TASK_STATES.contains(&status) {
            return Err(Error::Usage(format!(
                "status must be one of {}",
                TASK_STATES.join(", ")
            )));
        }
        let tx = self.conn.transaction()?;
        task_json(&tx, id)?;
        let closed = matches!(status, "done" | "dropped");
        let t = now();
        tx.execute(
            "UPDATE tasks SET status = ?1, updated_at = ?2,
               closed_at = CASE WHEN ?3 THEN ?2 ELSE NULL END,
               note = COALESCE(?4, note)
             WHERE id = ?5",
            params![status, t, closed, note.map(str::trim), id],
        )?;
        if status == "doing" {
            // One task at a time: whatever was in progress goes back to open.
            tx.execute(
                "UPDATE tasks SET status = 'open', updated_at = ?1 WHERE status = 'doing' AND id != ?2",
                params![t, id],
            )?;
            // Its places are being counted now, unless they already have a state.
            for n in task_nodes(&tx, id)? {
                if tx.execute(
                    "INSERT OR IGNORE INTO reviews (node_id, status, at) VALUES (?1, 'counting', ?2)",
                    params![n, t],
                )? > 0
                {
                    event(&tx, n, "review", json!({ "as": "counting", "task": id }))?;
                }
            }
        } else {
            // Stopped or finished: what is still being counted was not finished, so not counted.
            for n in task_nodes(&tx, id)? {
                if tx.execute(
                    "DELETE FROM reviews WHERE node_id = ?1 AND status = 'counting'",
                    [n],
                )? > 0
                {
                    event(&tx, n, "review", json!({ "as": "raw", "task": id }))?;
                }
            }
        }
        rerank(&tx, None, None)?;
        tx.commit()?;
        task_json(&self.conn, id)
    }

    /// Changes a task's title, reason, position or the places it is about.
    #[allow(clippy::too_many_arguments)]
    pub fn task_edit(
        &mut self,
        id: i64,
        title: Option<&str>,
        why: Option<&str>,
        add: &[String],
        remove: &[String],
        at: Option<usize>,
    ) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let current = task_json(&tx, id)?;
        if let Some(t) = title {
            tx.execute(
                "UPDATE tasks SET title = ?1 WHERE id = ?2",
                params![non_empty("title", t)?, id],
            )?;
        }
        if let Some(w) = why {
            tx.execute(
                "UPDATE tasks SET why = ?1 WHERE id = ?2",
                params![non_empty("why", w)?, id],
            )?;
        }
        for r in add {
            let n = resolve(&tx, r, false)?;
            tx.execute(
                "INSERT OR IGNORE INTO task_nodes (task_id, node_id) VALUES (?1, ?2)",
                params![id, n],
            )?;
        }
        for r in remove {
            let n = resolve(&tx, r, true)?;
            tx.execute(
                "DELETE FROM task_nodes WHERE task_id = ?1 AND node_id = ?2",
                params![id, n],
            )?;
        }
        if at.is_some() {
            if current["position"].is_null() {
                return Err(refused(
                    format!("task {id} is closed; reopen it before moving it"),
                    Value::Null,
                ));
            }
            rerank(&tx, Some(id), at)?;
        }
        tx.execute(
            "UPDATE tasks SET updated_at = ?1 WHERE id = ?2",
            params![now(), id],
        )?;
        tx.commit()?;
        task_json(&self.conn, id)
    }

    /// Where to pick up: the task in progress, else one due within a day (or overdue), else the
    /// first open one — with why it was picked, each of its places as `show` gives it, what is
    /// planned to move in, and `while_there`: everything else waiting in that place, done while
    /// it is open. Also `hints` on the order (a task due soon, one whose places are all counted,
    /// one that settles planned moves), progress, the rules, and under `organize` the untoured
    /// places no task covers yet. It only informs the order; it never changes it.
    pub fn next(&self) -> Result<Value> {
        let goal = get_setting(&self.conn, "goal")?;
        let open: Vec<(i64, String, Option<String>)> = {
            let mut stmt = self.conn.prepare(
                "SELECT id, status, due FROM tasks WHERE status IN ('open','doing')
                  ORDER BY rank, id",
            )?;
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?
        };
        let due_soon = |d: &Option<String>| {
            d.as_deref()
                .and_then(days_left)
                .is_some_and(|n| n <= DUE_SOON_DAYS)
        };
        let doing = open.iter().find(|t| t.1 == "doing");
        let urgent = open
            .iter()
            .filter(|t| due_soon(&t.2))
            .min_by(|a, b| a.2.cmp(&b.2));
        let (current, picked) = match (doing, urgent, open.first()) {
            (Some(t), _, _) => (Some(t.0), "doing"),
            (None, Some(t), _) => (Some(t.0), "due"),
            (None, None, Some(t)) => (Some(t.0), "order"),
            _ => (None, "none"),
        };
        let todo = self.todo()?;
        let crate::coverage::TodoParts {
            uncovered,
            unvalued,
            ..
        } = crate::coverage::todo_parts(&self.conn)?;
        let task = match current {
            None => Value::Null,
            Some(t) => {
                let mut v = task_json(&self.conn, t)?;
                let mut places = Vec::new();
                for n in task_nodes(&self.conn, t)? {
                    let mut p = show(&self.conn, n)?;
                    p["arriving"] = json!(self.pending_into(n)?);
                    p["while_there"] = while_there(&self.conn, n, &todo, &uncovered, &unvalued)?;
                    places.push(p);
                }
                v["places"] = json!(places);
                v["picked"] = json!(picked);
                v
            }
        };
        let mut hints = Vec::new();
        for (id, _, due) in &open {
            if let Some(d) = due.as_deref().filter(|_| due_soon(due)) {
                hints.push(
                    json!({ "task": id, "kind": "due", "due": d, "days_left": days_left(d) }),
                );
            }
            // No "its places are all counted" note: a task is often work on a counted place (a
            // grid to fit, labels to stick), and on the maintainer's inventory every such note
            // was wrong.
            let mut arriving = 0;
            for n in task_nodes(&self.conn, *id)? {
                arriving += self.pending_into(n)?.len();
            }
            if arriving > 0 {
                hints.push(json!({ "task": id, "kind": "settles_moves", "moves": arriving }));
            }
        }
        let progress = self.progress()?;
        let unplanned: Vec<Value> = if goal.as_deref() == Some("track") {
            Vec::new()
        } else {
            progress["places"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|p| p["review"]["status"] == "raw" && p["planned"] == false)
                .cloned()
                .collect()
        };
        Ok(json!({
            "goal": goal,
            "task": task,
            "open_tasks": open.len(),
            "hints": hints,
            "progress": progress_summary(&progress),
            "unplanned": unplanned,
            "rules": rules_json(&self.conn)?,
        }))
    }

    /// Sets a task's due date (`YYYY-MM-DD`), or clears it with `None`, an empty text or
    /// `none`: a day the person said they want it done by.
    pub fn task_due(&mut self, id: i64, due: Option<&str>) -> Result<Value> {
        task_json(&self.conn, id)?;
        let due = parse_due(due)?;
        self.conn.execute(
            "UPDATE tasks SET due = ?1, updated_at = ?2 WHERE id = ?3",
            params![due, now(), id],
        )?;
        task_json(&self.conn, id)
    }

    /// Nodes planned to move into `id` or anywhere below it.
    fn pending_into(&self, id: i64) -> Result<Vec<Value>> {
        let movers = ids(
            &self.conn,
            "WITH RECURSIVE d(id) AS (
                 SELECT ?1 UNION ALL
                 SELECT n.id FROM nodes n JOIN d ON n.parent_id = d.id WHERE n.state != 'gone'
             )
             SELECT id FROM nodes WHERE pending_to IN d AND state != 'gone' ORDER BY id",
            [id],
        )?;
        movers
            .into_iter()
            .map(|m| {
                serde_json::to_value(brief(&self.conn, m)?)
                    .map_err(|e| Error::Internal(e.to_string()))
            })
            .collect()
    }
}
