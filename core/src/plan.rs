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

/// The kinds of picture `ev` draws on a copy of a photo, named `<photo>-<kind>-<ms>.jpg`.
const MARKED_KINDS: [&str; 4] = ["marked", "numbered", "preview", "sheet"];

/// Which photo a marked picture shows, for its place in the stack on screen: its folder and
/// file name without a trailing `-<kind>-<ms>` — so a photo marked again, or cut after it was
/// marked, replaces its earlier copy. Any other picture is its own path.
fn stack_key(path: &str) -> String {
    let p = std::path::Path::new(path);
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    let photo = stem.rsplit_once('-').and_then(|(head, ms)| {
        let (photo, kind) = head.rsplit_once('-')?;
        (!ms.is_empty() && ms.bytes().all(|b| b.is_ascii_digit()) && MARKED_KINDS.contains(&kind))
            .then_some(photo)
    });
    match photo {
        Some(photo) => p.with_file_name(photo).to_string_lossy().into_owned(),
        None => path.to_string(),
    }
}

/// One picture of the series on screen.
struct Marked {
    file: String,
    note: Value,
    frames: Vec<Value>,
    /// The photo it was drawn on, unmarked (a mark's or a cut's photo), else the picture
    /// itself: what `f12` names when a command takes a photo.
    source: String,
}

/// The series a focus request holds, without the pictures no longer on disk. A request written
/// before series existed (no `frames`) holds none: its pictures carry numbers the series never
/// registered, so counting on from them would draw the same number twice.
fn series_of(req: &Value) -> Vec<Marked> {
    if !req["frames"].is_array() {
        return Vec::new();
    }
    req["files"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(i, f)| {
            let note = match req["notes"].get(i) {
                Some(n) => n.clone(),
                None => req["note"].clone(),
            };
            let frames = req["frames"]
                .get(i)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let file = f.as_str()?.to_string();
            let source = req["sources"]
                .get(i)
                .and_then(Value::as_str)
                .map_or_else(|| file.clone(), str::to_string);
            Some(Marked {
                file,
                note,
                frames,
                source,
            })
        })
        .filter(|m| std::path::Path::new(&m.file).is_file())
        .collect()
}

/// Whole minutes from `from` to `to` (RFC 3339 times), when both read.
fn minutes_between(from: &str, to: &str) -> Option<i64> {
    let parse = |t: &str| chrono::DateTime::parse_from_rfc3339(t).ok();
    Some((parse(to)? - parse(from)?).num_minutes())
}

/// The number in a series reference: `f12` (or `F12`) is the series' twelfth picture.
pub fn series_number(text: &str) -> Option<usize> {
    let rest = text.strip_prefix(['f', 'F'])?;
    (!rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
        .then(|| rest.parse().ok())
        .flatten()
        .filter(|n| *n > 0)
}

/// The key a marked copy of `file` would have: copies go to `<temp>/ev-marks`.
fn source_key(file: &std::path::Path) -> String {
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    let dir = std::path::absolute(std::env::temp_dir().join("ev-marks")).unwrap_or_default();
    dir.join(stem.as_ref()).to_string_lossy().into_owned()
}

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
/// children is labelled as a place of its own; everything below a unit is gone through with it.
/// `K4x4-08-A` is a unit and the boxes in it are not; `K4x4-08` is not, because its drawers are
/// labelled as its parts (see `own_place`).
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

/// Whether a child labelled `child` is a place of its own inside a holder labelled `holder`. A
/// code is a physical label, so a labelled child in an unlabelled holder is a place. In a
/// labelled one it is when its code goes on from the holder's: `K2-01-A` in `K2-01` is a
/// drawer of that unit, opened on its own. A label of another series is a labelled box in the
/// place (`B1_007` in `K2-01-A`), gone through with it: the drawer stays on the list.
fn own_place(holder: Option<&str>, child: Option<&str>) -> bool {
    match (holder, child) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(h), Some(c)) => {
            let (h, c) = (crate::fold::fold_code(h), crate::fold::fold_code(c));
            c.strip_prefix(&h).is_some_and(|rest| rest.starts_with('-'))
        }
    }
}

/// Whether a node is a unit, given its live children: a holder with no child labelled as a
/// place of its own, or a room with nothing in it that holds things (an empty kitchen is
/// counted as one place).
fn unit_by_children(n: &Node, children: &[&Node]) -> bool {
    if n.state != State::Active || n.lost {
        return false;
    }
    match n.kind {
        Kind::Home | Kind::Item => false,
        Kind::Room => !children
            .iter()
            .any(|c| matches!(c.kind, Kind::Room | Kind::Furniture | Kind::Container)),
        _ => !children
            .iter()
            .any(|c| own_place(n.code.as_deref(), c.code.as_deref())),
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
        let blocked = if n.kind == Kind::Room {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM nodes WHERE parent_id = ?1 AND state != 'gone'
                   AND kind IN ('room', 'furniture', 'container')",
                [n.id],
                |r| r.get(0),
            )?;
            count > 0
        } else {
            let mut stmt = conn.prepare(
                "SELECT code FROM nodes WHERE parent_id = ?1 AND state != 'gone'
                   AND code IS NOT NULL",
            )?;
            let codes = stmt
                .query_map([n.id], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            codes.iter().any(|c| own_place(n.code.as_deref(), Some(c)))
        };
        if !blocked {
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
        // The marked photos waiting on screen stay beside the node, for `m` to open.
        let mut sent = value.clone();
        let req = self.focus_request()?;
        for k in ["files", "notes", "frames", "next", "since"] {
            if !req[k].is_null() {
                sent[k] = req[k].clone();
            }
        }
        self.send_focus(&sent)?;
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
        self.focus_marked(files, note, &[])
    }

    /// Like `focus_file`, for pictures whose numbered `frames` ev drew (`[{n, …}]`): they join
    /// the series on screen instead of replacing it (spec/focus-stack.md), the newest shown,
    /// and a new copy of a photo in the series takes its place. The series keeps each picture's
    /// frames and the next free number, so a batch's numbers never repeat (`focus_numbers`).
    /// The person ends it in `ev ui`; `ev focus --clear` does too.
    pub fn focus_marked(
        &mut self,
        files: &[std::path::PathBuf],
        note: Option<&str>,
        frames: &[Value],
    ) -> Result<Value> {
        let each: Vec<(std::path::PathBuf, Option<String>)> =
            files.iter().map(|f| (f.clone(), None)).collect();
        self.focus_in(&each, note, frames, None)
    }

    /// `focus_marked` for pictures drawn on `source` (a mark's or a cut's photo): `f12` then
    /// names that photo, unmarked, to a command that takes one.
    pub fn focus_drawn(
        &mut self,
        files: &[std::path::PathBuf],
        note: Option<&str>,
        frames: &[Value],
        source: &std::path::Path,
    ) -> Result<Value> {
        let each: Vec<(std::path::PathBuf, Option<String>)> =
            files.iter().map(|f| (f.clone(), None)).collect();
        self.focus_in(&each, note, frames, Some(source))
    }

    /// `focus_file` with a note of its own on each picture that has one (`ev focus --file
    /// a.jpg=<note>`); the others take `note`.
    pub fn focus_noted(
        &mut self,
        files: &[(std::path::PathBuf, Option<String>)],
        note: Option<&str>,
    ) -> Result<Value> {
        self.focus_in(files, note, &[], None)
    }

    /// The photo `f12` names: the series' twelfth picture, as the photo it was drawn on (see
    /// `Marked::source`). Refused when the series has no such picture.
    pub fn series_photo(&self, n: usize) -> Result<std::path::PathBuf> {
        let series = series_of(&self.focus_request()?);
        series
            .get(n.wrapping_sub(1))
            .map(|m| std::path::PathBuf::from(&m.source))
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "the marked photo series has no f{n} ({} picture(s))",
                    series.len()
                ))
            })
    }

    /// The note the series' `n`th picture was sent with, if any: the note a photo attached from
    /// it (`ev photo add <ref> f12`) takes by default.
    pub fn series_note(&self, n: usize) -> Result<Option<String>> {
        let series = series_of(&self.focus_request()?);
        Ok(series
            .get(n.wrapping_sub(1))
            .and_then(|m| m.note.as_str().map(str::to_string)))
    }

    /// `ev focus f12`: the series' twelfth picture on the person's screen again.
    pub fn focus_picture(&mut self, n: usize) -> Result<Value> {
        let mut req = self.focus_request()?;
        let series = series_of(&req);
        let count = series.len();
        if n == 0 || n > count {
            return Err(Error::NotFound(format!(
                "the marked photo series has no f{n} ({count} picture(s))"
            )));
        }
        let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        // Written back as the series reads, so `f12` counts what is on screen.
        req["files"] = json!(series.iter().map(|m| &m.file).collect::<Vec<_>>());
        req["notes"] = json!(series.iter().map(|m| &m.note).collect::<Vec<_>>());
        req["sources"] = json!(series.iter().map(|m| &m.source).collect::<Vec<_>>());
        req["frames"] = json!(series.iter().map(|m| &m.frames).collect::<Vec<_>>());
        req["show"] = json!(n - 1);
        req["at"] = json!(at);
        // A node asked for before leaves its id here; the picture is what is shown now.
        if let Some(o) = req.as_object_mut() {
            o.remove("id");
            o.remove("photo");
        }
        self.send_focus(&req)?;
        Ok(json!({ "focus": {
            "files": [req["files"][n - 1].clone()], "note": req["notes"][n - 1].clone(),
            "series": count, "at": at,
        } }))
    }

    fn focus_in(
        &mut self,
        files: &[(std::path::PathBuf, Option<String>)],
        note: Option<&str>,
        frames: &[Value],
        source: Option<&std::path::Path>,
    ) -> Result<Value> {
        if files.is_empty() {
            return Err(Error::Usage("name at least one picture".into()));
        }
        let mut paths = Vec::new();
        for (file, own) in files {
            if !file.is_file() {
                return Err(Error::NotFound(format!("no file {}", file.display())));
            }
            let abs = std::path::absolute(file).unwrap_or_else(|_| file.clone());
            paths.push((abs.to_string_lossy().into_owned(), own.as_deref().or(note)));
        }
        let req = self.focus_request()?;
        let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let mut series = series_of(&req);
        // Only a picture sent before is replaced, and once: a cut's preview and its numbered
        // copy are two pictures of the same photo.
        let mut taken = vec![false; series.len()];
        let mut show = None;
        let source = source
            .map(|s| std::path::absolute(s).unwrap_or_else(|_| s.to_path_buf()))
            .map(|s| s.to_string_lossy().into_owned());
        for (p, own) in &paths {
            let entry = Marked {
                file: p.clone(),
                note: json!(own),
                frames: frames.to_vec(),
                source: source.clone().unwrap_or_else(|| p.clone()),
            };
            let found =
                (0..taken.len()).find(|&i| !taken[i] && stack_key(&series[i].file) == stack_key(p));
            let i = match found {
                Some(i) => {
                    taken[i] = true;
                    series[i] = entry;
                    i
                }
                None => {
                    series.push(entry);
                    series.len() - 1
                }
            };
            show = Some(show.map_or(i, |s: usize| s.min(i)));
        }
        let last = frames.iter().filter_map(|f| f["n"].as_u64()).max();
        let next = req["next"].as_u64().max(last.map(|l| l + 1));
        let since = match req["since"].as_str() {
            Some(s) if !req["files"].is_null() => json!(s),
            _ => json!(at),
        };
        let count = series.len();
        let mut sent = json!({
            "files": series.iter().map(|m| m.file.clone()).collect::<Vec<_>>(),
            "notes": series.iter().map(|m| m.note.clone()).collect::<Vec<_>>(),
            "sources": series.iter().map(|m| m.source.clone()).collect::<Vec<_>>(),
            "frames": series.into_iter().map(|m| m.frames).collect::<Vec<_>>(),
            "show": show, "note": note, "next": next, "since": since, "at": at,
        });
        if next.is_none() {
            sent.as_object_mut().map(|o| o.remove("next"));
        }
        self.send_focus(&sent)?;
        let files: Vec<&String> = paths.iter().map(|(p, _)| p).collect();
        Ok(json!({ "focus": {
            "files": files, "note": note, "series": count, "next": next, "at": at,
        } }))
    }

    /// The numbers `count` frames of a picture of `file` take in the series, so every number on
    /// screen means one frame until the person closes the series: the photo's own numbers first
    /// when it is in the series already (marked again, or cut after it was marked), in order,
    /// and the series' next free numbers for frames beyond them. Nothing is renumbered.
    pub fn focus_numbers(&self, file: &std::path::Path, count: usize) -> Result<Vec<usize>> {
        let req = self.focus_request()?;
        let key = source_key(file);
        let mut own: Vec<usize> = series_of(&req)
            .into_iter()
            .find(|m| stack_key(&m.file) == key)
            .map(|m| {
                // Not the numbers a mark only pointed at (`--keep-numbers`): they are another
                // photo's frames.
                m.frames
                    .iter()
                    .filter(|f| f["kept"] != true)
                    .filter_map(|f| f["n"].as_u64().map(|n| n as usize))
                    .collect()
            })
            .unwrap_or_default();
        own.sort_unstable();
        own.dedup();
        let next = req["next"].as_u64().map_or(1, |n| n as usize);
        Ok((0..count)
            .map(|i| match own.get(i) {
                Some(n) => *n,
                None => next + i - own.len().min(i),
            })
            .collect())
    }

    /// The series of marked photos on screen: each picture with its note and frames, and the
    /// next free number (`ev focus --list`), so the agent quotes the numbers ev drew.
    pub fn focus_list(&self) -> Result<Value> {
        let req = self.focus_request()?;
        let series = series_of(&req);
        if series.is_empty() {
            return Ok(json!({ "series": null }));
        }
        // `f` is how the person and the agent name a picture (`f12`); `#12` stays a record and
        // a bare number a frame.
        let pictures: Vec<Value> = series
            .into_iter()
            .enumerate()
            .map(|(i, m)| {
                json!({
                    "n": i + 1, "f": format!("f{}", i + 1), "file": m.file, "source": m.source,
                    "note": m.note, "frames": m.frames,
                })
            })
            .collect();
        Ok(json!({ "series": {
            "since": req["since"], "next": req["next"].as_u64().unwrap_or(1), "pictures": pictures,
        } }))
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
                            "SELECT COUNT(*) FROM nodes
                              WHERE parent_id = ?1 AND state != 'gone' AND lost = 0",
                            [n],
                            |r| r.get(0),
                        )?;
                        // An empty place never photographed needs no photo; one whose older photo
                        // still shows what left does, as `todo` lists it: the person decided an
                        // emptied place is photographed empty, so its picture does not mislead.
                        if let Some((reason, photo_at, changed)) =
                            crate::marks::photo_stale(&tx, n)?
                            && !(holds == 0 && reason == "none")
                        {
                            // How long after the photo the records changed: minutes mean the
                            // records caught up with what the photo already shows.
                            let minutes = match (&photo_at, &changed) {
                                (Some(p), Some(c)) => minutes_between(p, c),
                                _ => None,
                            };
                            stale.push(json!({
                                "node": brief(&tx, n)?,
                                "reason": reason,
                                "photo_at": photo_at,
                                "changed_at": changed,
                                "minutes_after": minutes,
                            }));
                        }
                    }
                    if !stale.is_empty() {
                        let recent = stale
                            .iter()
                            .all(|s| s["minutes_after"].as_i64().is_some_and(|m| m <= 120));
                        let msg = if recent {
                            format!(
                                "{} photo(s) are older than what they show, by minutes: the \
                                 records may have caught up with what the photo already shows. \
                                 If it does, say so (`ev photo current <ref>`); else attach a \
                                 current photo",
                                stale.len()
                            )
                        } else {
                            format!(
                                "{} photo(s) are older than what they show; attach a current \
                                 photo (`ev photo cut <photo> --place <ref> --grid …`) or say \
                                 an old one still holds (`ev photo current <ref>`)",
                                stale.len()
                            )
                        };
                        return Err(refused(msg, json!({ "stale": stale })));
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
            // What is in it: a lost thing only keeps it as where it was last seen.
            let direct = kids
                .get(&u)
                .map_or(0, |k| k.iter().filter(|n| !n.lost).count());
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
