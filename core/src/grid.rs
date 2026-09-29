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

fn parse_cell(s: &str) -> Result<(i64, i64)> {
    let s = s.trim();
    let bad = || Error::Usage(format!("`{s}` is not a cell like A3"));
    let mut chars = s.chars();
    let letter = chars.next().ok_or_else(bad)?.to_ascii_uppercase();
    if !letter.is_ascii_uppercase() {
        return Err(bad());
    }
    let row: i64 = chars.as_str().parse().map_err(|_| bad())?;
    if row < 1 {
        return Err(bad());
    }
    Ok((i64::from(letter as u8 - b'A'), row - 1))
}

impl Cells {
    /// `A3`, or a range between two opposite corners: `A3-B4`, `A3:B4` or `A3–B4`.
    pub fn parse(s: &str) -> Result<Cells> {
        let s = s.trim();
        let (a, b) = match s.split_once(['-', ':', '–']) {
            Some((a, b)) => (parse_cell(a)?, parse_cell(b)?),
            None => {
                let c = parse_cell(s)?;
                (c, c)
            }
        };
        Ok(Cells {
            col: a.0.min(b.0),
            row: a.1.min(b.1),
            width: (a.0 - b.0).abs() + 1,
            depth: (a.1 - b.1).abs() + 1,
        })
    }

    /// The back-left cell, which a box's code is usually named after.
    pub fn anchor(&self) -> String {
        cell_name(self.col, self.row)
    }

    pub fn name(&self) -> String {
        if self.width == 1 && self.depth == 1 {
            self.anchor()
        } else {
            format!(
                "{}-{}",
                self.anchor(),
                cell_name(self.col + self.width - 1, self.row + self.depth - 1)
            )
        }
    }

    fn contains(&self, col: i64, row: i64) -> bool {
        col >= self.col
            && col < self.col + self.width
            && row >= self.row
            && row < self.row + self.depth
    }

    fn overlaps(&self, o: &Cells) -> bool {
        self.col < o.col + o.width
            && o.col < self.col + self.width
            && self.row < o.row + o.depth
            && o.row < self.row + self.depth
    }
}

pub(crate) fn grid_of(conn: &Connection, id: i64) -> Result<Option<(i64, i64)>> {
    Ok(conn
        .query_row(
            "SELECT cols, rows FROM grids WHERE node_id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?)
}

pub(crate) fn cells_of(conn: &Connection, id: i64) -> Result<Option<Cells>> {
    Ok(conn
        .query_row(
            "SELECT col, row, width, depth FROM cells WHERE node_id = ?1",
            [id],
            |r| {
                Ok(Cells {
                    col: r.get(0)?,
                    row: r.get(1)?,
                    width: r.get(2)?,
                    depth: r.get(3)?,
                })
            },
        )
        .optional()?)
}

