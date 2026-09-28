//! Things to do that hang on a node, a list of things to get, and `ev todo`, the one list of
//! everything waiting (spec §18).
//!
//! Most of what is "to do" in a home is already state on a node: a planned move, a thing meant
//! for another household, a disposal, a lost thing. Those stay where they are and close with
//! their own verbs (`done`, `gone`, `back`, `found`); `todo` only gathers them, so nothing is
//! kept twice. This module adds the few kinds that had no state yet — a label to print, broken,
//! a use-by date, a sale in progress — and needs, which are not in the tree at all.

use chrono::{Datelike, NaiveDate};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::error::refused;
use crate::model::{Disposition, Node, State};
use crate::store::{Inventory, brief, ids, live_nodes, now, place_errands, resolve, show};
use crate::{Error, Result};

/// How close a use-by date has to be before it shows in `todo`.
const EXPIRY_WINDOW_DAYS: i64 = 60;

/// Words that mark a record as a guess to clear up with the person.
const UNSURE: [&str; 3] = ["belirsiz", "muhtemelen", "?"];

fn brief_value(conn: &Connection, id: i64) -> Result<Value> {
    serde_json::to_value(brief(conn, id)?).map_err(|e| Error::Internal(e.to_string()))
}

