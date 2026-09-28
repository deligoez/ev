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

