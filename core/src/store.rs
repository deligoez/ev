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
