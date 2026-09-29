//! Grids (spec §22): a holder laid out in cells, like a gridfinity drawer, and the rectangle of
//! cells each box in it covers. Columns read A…Z from the left, rows 1… from the back.

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::error::{Error, Result, refused};
use crate::store::{Inventory, apply_edit, brief_json, event, load, resolve, touch};

const MAX_COLS: i64 = 26;
const MAX_ROWS: i64 = 99;

/// A rectangle of cells: its back-left cell and how many columns and rows it covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cells {
    pub col: i64,
    pub row: i64,
    pub width: i64,
    pub depth: i64,
}

fn cell_name(col: i64, row: i64) -> String {
    format!("{}{}", (b'A' + col as u8) as char, row + 1)
}

