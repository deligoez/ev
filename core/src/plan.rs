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

