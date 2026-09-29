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

/// How far a place has been gone through. `raw` is the absence of a row.
const REVIEWS: [&str; 2] = ["toured", "kept"];

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
fn units(all: &[Node]) -> Vec<i64> {
    let mut kids: HashMap<Option<i64>, Vec<&Node>> = HashMap::new();
    for n in all {
        kids.entry(n.parent_id).or_default().push(n);
    }
    let mut out = Vec::new();
    let mut stack: Vec<&Node> = kids.get(&None).cloned().unwrap_or_default();
    while let Some(n) = stack.pop() {
        let children = kids.get(&Some(n.id)).cloned().unwrap_or_default();
        let structural = matches!(n.kind, Kind::Home | Kind::Room);
        let is_unit = !structural
            && n.kind != Kind::Item
            && n.state == State::Active
            && !children.iter().any(|c| c.code.is_some());
        if is_unit {
            out.push(n.id);
        } else {
            stack.extend(children);
        }
    }
    out.sort_unstable();
    out
}

/// The review a unit inherits: its own, or the nearest reviewed ancestor's.
fn effective_review(
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

fn all_reviews(conn: &Connection) -> Result<HashMap<i64, (String, String)>> {
    let mut stmt = conn.prepare("SELECT node_id, status, at FROM reviews")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?
        .collect::<rusqlite::Result<HashMap<_, _>>>()?;
    Ok(rows)
}

/// Latest `updated_at` anywhere below and at `id`, to tell a toured place that changed since.
fn last_change(id: i64, kids: &HashMap<i64, Vec<&Node>>, by_id: &HashMap<i64, &Node>) -> String {
    let mut latest = by_id
        .get(&id)
        .map(|n| n.updated_at.clone())
        .unwrap_or_default();
    let mut stack = vec![id];
    while let Some(c) = stack.pop() {
        for k in kids.get(&c).into_iter().flatten() {
            if k.updated_at > latest {
                latest = k.updated_at.clone();
            }
            stack.push(k.id);
        }
    }
    latest
}

fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
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

fn task_json(conn: &Connection, id: i64) -> Result<Value> {
    let (title, why, status, note, created, updated, closed): (
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT title, why, status, note, created_at, updated_at, closed_at FROM tasks WHERE id = ?1",
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
    Ok(json!({
        "id": id,
        "position": position,
        "title": title,
        "why": why,
        "status": status,
        "note": note,
        "nodes": nodes,
        "created_at": created,
        "updated_at": updated,
        "closed_at": closed,
    }))
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
            self.conn
                .execute("DELETE FROM settings WHERE key = 'focus'", [])?;
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
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES ('focus', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [value.to_string()],
        )?;
        Ok(json!({ "focus": value }))
    }

    /// The pending focus request, if any.
    pub fn focus_request(&self) -> Result<Value> {
        Ok(get_setting(&self.conn, "focus")?
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
                    "SELECT path FROM photos WHERE node_id = ?1 ORDER BY position LIMIT 1 OFFSET ?2",
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

    /// Removes one observation that turned out wrong or no longer holds.
    pub fn unobserve(&mut self, observation: i64) -> Result<Value> {
        let node: Option<i64> = self
            .conn
            .query_row(
                "SELECT node_id FROM observations WHERE id = ?1",
                [observation],
                |r| r.get(0),
            )
            .optional()?;
        let node = node.ok_or_else(|| Error::NotFound(format!("no observation {observation}")))?;
        self.conn
            .execute("DELETE FROM observations WHERE id = ?1", [observation])?;
        show(&self.conn, node)
    }

    /// Marks how far a place has been gone through: `toured` (every thing in it looked at and
    /// the person said it is done), `kept` (the person wants it left as it is), or `raw` to
    /// clear it. Everything below the place inherits the mark.
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
                    "review must be toured, kept or raw".to_string(),
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
        let by_id: HashMap<i64, &Node> = all.iter().map(|n| (n.id, n)).collect();
        let parent: HashMap<i64, Option<i64>> = all.iter().map(|n| (n.id, n.parent_id)).collect();
        let mut kids: HashMap<i64, Vec<&Node>> = HashMap::new();
        for n in &all {
            if let Some(p) = n.parent_id {
                kids.entry(p).or_default().push(n);
            }
        }
        let reviews = all_reviews(&self.conn)?;
        let planned: HashSet<i64> = ids(
            &self.conn,
            "SELECT DISTINCT tn.node_id FROM task_nodes tn JOIN tasks t ON t.id = tn.task_id
              WHERE t.status IN ('open','doing')",
            [],
        )?
        .into_iter()
        .collect();
        let mut list = Vec::new();
        let (mut toured, mut kept, mut raw, mut stale) = (0, 0, 0, 0);
        for u in units(&all) {
            let n = by_id[&u];
            let mut v = serde_json::to_value(brief(&self.conn, u)?)
                .map_err(|e| Error::Internal(e.to_string()))?;
            let direct = kids.get(&u).map_or(0, Vec::len);
            v["children"] = json!(direct);
            v["unknown"] = json!(n.unknown);
            v["observations"] = json!(observations_of(&self.conn, u)?.len());
            v["planned"] = json!(planned.contains(&u));
            match effective_review(u, &parent, &reviews) {
                Some((from, status, at)) => {
                    let changed = last_change(u, &kids, &by_id) > at;
                    if status == "toured" {
                        toured += 1;
                    } else {
                        kept += 1;
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
        let title = non_empty("title", title)?;
        let why = non_empty("why", why)?;
        let tx = self.conn.transaction()?;
        let nodes = on
            .iter()
            .map(|r| resolve(&tx, r, false))
            .collect::<Result<Vec<_>>>()?;
        let t = now();
        tx.execute(
            "INSERT INTO tasks (title, why, rank, status, created_at, updated_at)
             VALUES (?1, ?2, 1000000, 'open', ?3, ?3)",
            params![title, why, t],
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

    /// Where to pick up: the task in progress (or the first open one) with everything needed
    /// to work on it — each of its places as `show` gives it, what is planned to move in or
    /// out of them, and the rules — plus progress, and under `organize` the untoured places no
    /// task covers yet, so gaps in the plan are visible.
    pub fn next(&self) -> Result<Value> {
        let goal = get_setting(&self.conn, "goal")?;
        let current = ids(
            &self.conn,
            "SELECT id FROM tasks WHERE status IN ('open','doing')
              ORDER BY CASE status WHEN 'doing' THEN 0 ELSE 1 END, rank, id LIMIT 1",
            [],
        )?;
        let open_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE status IN ('open','doing')",
            [],
            |r| r.get(0),
        )?;
        let task = match current.first() {
            None => Value::Null,
            Some(t) => {
                let mut v = task_json(&self.conn, *t)?;
                let mut places = Vec::new();
                for n in task_nodes(&self.conn, *t)? {
                    let mut p = show(&self.conn, n)?;
                    p["arriving"] = json!(self.pending_into(n)?);
                    places.push(p);
                }
                v["places"] = json!(places);
                v
            }
        };
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
            "open_tasks": open_count,
            "progress": {
                "units": progress["units"],
                "toured": progress["toured"],
                "kept": progress["kept"],
                "raw": progress["raw"],
                "changed_since_tour": progress["changed_since_tour"],
            },
            "unplanned": unplanned,
            "rules": rules_json(&self.conn)?,
        }))
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
