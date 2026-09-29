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

/// The live boxes placed in `holder`'s grid, in reading order.
fn placed(conn: &Connection, holder: i64) -> Result<Vec<(i64, Cells)>> {
    let mut stmt = conn.prepare(
        "SELECT c.node_id, c.col, c.row, c.width, c.depth FROM cells c
           JOIN nodes n ON n.id = c.node_id
          WHERE n.parent_id = ?1 AND n.state != 'gone'
          ORDER BY c.row, c.col",
    )?;
    let rows = stmt.query_map([holder], |r| {
        Ok((
            r.get(0)?,
            Cells {
                col: r.get(1)?,
                row: r.get(2)?,
                width: r.get(3)?,
                depth: r.get(4)?,
            },
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// A holder's grid: size, the boxes in it with their cells, the free cells, and a map with the
/// id of the box in each cell (null where free), row by row from the back.
pub(crate) fn grid_json(conn: &Connection, holder: i64) -> Result<Option<Value>> {
    let Some((cols, rows)) = grid_of(conn, holder)? else {
        return Ok(None);
    };
    let boxes = placed(conn, holder)?;
    let mut map = Vec::new();
    let mut free = Vec::new();
    for row in 0..rows {
        let mut line = Vec::new();
        for col in 0..cols {
            match boxes.iter().find(|(_, c)| c.contains(col, row)) {
                Some((id, _)) => line.push(json!(id)),
                None => {
                    free.push(cell_name(col, row));
                    line.push(Value::Null);
                }
            }
        }
        map.push(line);
    }
    let unplaced = crate::store::ids(
        conn,
        "SELECT n.id FROM nodes n LEFT JOIN cells c ON c.node_id = n.id
          WHERE n.parent_id = ?1 AND n.state != 'gone' AND c.node_id IS NULL ORDER BY n.id",
        [holder],
    )?;
    Ok(Some(json!({
        "cols": cols,
        "rows": rows,
        "boxes": boxes
            .iter()
            .map(|(id, c)| {
                let mut b = brief_json(conn, *id)?;
                b["cells"] = json!(c.name());
                Ok(b)
            })
            .collect::<Result<Vec<_>>>()?,
        "free": free,
        "unplaced": unplaced
            .iter()
            .map(|id| brief_json(conn, *id))
            .collect::<Result<Vec<_>>>()?,
        "map": map,
    })))
}

impl Inventory {
    /// Lays `reference` out in `cols` × `rows` cells. Boxes already placed must still fit.
    pub fn grid_set(&mut self, reference: &str, cols: i64, rows: i64) -> Result<Value> {
        if !(1..=MAX_COLS).contains(&cols) || !(1..=MAX_ROWS).contains(&rows) {
            return Err(Error::Usage(format!(
                "a grid is 1–{MAX_COLS} columns and 1–{MAX_ROWS} rows"
            )));
        }
        let tx = self.conn.transaction()?;
        let id = resolve(&tx, reference, false)?;
        let outside: Vec<Value> = placed(&tx, id)?
            .iter()
            .filter(|(_, c)| c.col + c.width > cols || c.row + c.depth > rows)
            .map(|(b, c)| {
                let mut v = brief_json(&tx, *b)?;
                v["cells"] = json!(c.name());
                Ok(v)
            })
            .collect::<Result<_>>()?;
        if !outside.is_empty() {
            return Err(refused(
                format!(
                    "{} placed box(es) would fall outside a {cols}×{rows} grid",
                    outside.len()
                ),
                json!({ "outside": outside }),
            ));
        }
        let before = grid_of(&tx, id)?;
        tx.execute(
            "INSERT INTO grids (node_id, cols, rows) VALUES (?1, ?2, ?3)
             ON CONFLICT(node_id) DO UPDATE SET cols = excluded.cols, rows = excluded.rows",
            params![id, cols, rows],
        )?;
        touch(&tx, id)?;
        event(
            &tx,
            id,
            "grid",
            json!({ "before": before.map(|(c, r)| [c, r]), "after": [cols, rows] }),
        )?;
        tx.commit()?;
        self.grid(&id.to_string())
    }

    /// Removes a grid; refused while boxes are placed in it.
    pub fn grid_clear(&mut self, reference: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let id = resolve(&tx, reference, false)?;
        let boxes = placed(&tx, id)?;
        if !boxes.is_empty() {
            return Err(refused(
                format!(
                    "{} box(es) are placed in this grid; clear their cells first",
                    boxes.len()
                ),
                Value::Null,
            ));
        }
        let before = grid_of(&tx, id)?;
        if before.is_some() {
            tx.execute("DELETE FROM grids WHERE node_id = ?1", [id])?;
            touch(&tx, id)?;
            event(
                &tx,
                id,
                "grid",
                json!({ "before": before.map(|(c, r)| [c, r]), "after": null }),
            )?;
        }
        tx.commit()?;
        self.grid(&id.to_string())
    }

    /// A holder with its grid (`grid` is null when it has none).
    pub fn grid(&self, reference: &str) -> Result<Value> {
        let id = resolve(&self.conn, reference, false)?;
        Ok(json!({
            "node": brief_json(&self.conn, id)?,
            "grid": grid_json(&self.conn, id)?,
        }))
    }

    /// Places boxes in the cells of their holder's grid, all at once: each pair is a reference
    /// and a cell range (`A3`, `A3-B4`), or an empty range to take a box out of the grid.
    /// Bounds and overlaps are checked against where every box ends up, so boxes can swap
    /// places in one step. With `recode`, each placed box gets the code `<holder code>-<cell>`,
    /// named after its back-left cell.
    pub fn cells_set(&mut self, pairs: &[(String, String)], recode: bool) -> Result<Value> {
        if pairs.is_empty() {
            return Err(Error::Usage("give at least one <ref>=<cells>".into()));
        }
        let tx = self.conn.transaction()?;
        let mut moves: Vec<(crate::Node, i64, Option<Cells>)> = Vec::new();
        for (reference, range) in pairs {
            let n = load(&tx, resolve(&tx, reference, false)?)?;
            if moves.iter().any(|(m, _, _)| m.id == n.id) {
                return Err(Error::Usage(format!("`{reference}` is given twice")));
            }
            let holder = n.parent_id.ok_or_else(|| {
                refused(format!("{} is not inside anything", n.name), Value::Null)
            })?;
            let Some((cols, rows)) = grid_of(&tx, holder)? else {
                let h = load(&tx, holder)?;
                return Err(refused(
                    format!(
                        "{} has no grid; set one with `ev grid <holder> --cols N --rows M`",
                        h.code.unwrap_or(h.name)
                    ),
                    json!({ "holder": brief_json(&tx, holder)? }),
                ));
            };
            let cells = if range.trim().is_empty() {
                None
            } else {
                let c = Cells::parse(range)?;
                if c.col + c.width > cols || c.row + c.depth > rows {
                    return Err(refused(
                        format!(
                            "{} does not fit in a {cols}×{rows} grid (columns A–{}, rows 1–{rows})",
                            c.name(),
                            (b'A' + (cols - 1) as u8) as char
                        ),
                        json!({ "node": brief_json(&tx, n.id)? }),
                    ));
                }
                Some(c)
            };
            moves.push((n, holder, cells));
        }
        // Overlaps, per holder, in the state after the whole call.
        let mut holders: Vec<i64> = moves.iter().map(|(_, h, _)| *h).collect();
        holders.sort_unstable();
        holders.dedup();
        for h in &holders {
            let mut after: Vec<(i64, Cells)> = placed(&tx, *h)?
                .into_iter()
                .filter(|(id, _)| !moves.iter().any(|(m, _, _)| m.id == *id))
                .collect();
            after.extend(
                moves
                    .iter()
                    .filter(|(_, mh, c)| mh == h && c.is_some())
                    .map(|(m, _, c)| (m.id, c.unwrap())),
            );
            for (i, (a, ca)) in after.iter().enumerate() {
                if let Some((b, cb)) = after[i + 1..].iter().find(|(_, cb)| ca.overlaps(cb)) {
                    return Err(refused(
                        format!("{} and {} would share cells", ca.name(), cb.name()),
                        json!({ "boxes": [brief_json(&tx, *a)?, brief_json(&tx, *b)?] }),
                    ));
                }
            }
        }
        if recode {
            for (n, h, c) in &moves {
                if c.is_some() && load(&tx, *h)?.code.is_none() {
                    return Err(refused(
                        format!("--recode needs a code on the holder of {}", n.name),
                        Value::Null,
                    ));
                }
            }
            // Codes may rotate between the boxes, as with `ev recode`.
            for (n, _, c) in &moves {
                if c.is_some() {
                    tx.execute(
                        "UPDATE nodes SET code = NULL, code_folded = NULL WHERE id = ?1",
                        [n.id],
                    )?;
                }
            }
        }
        let mut out = Vec::new();
        for (n, h, c) in &moves {
            let before = cells_of(&tx, n.id)?;
            match c {
                Some(c) => tx.execute(
                    "INSERT INTO cells (node_id, col, row, width, depth) VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(node_id) DO UPDATE SET col = excluded.col, row = excluded.row,
                       width = excluded.width, depth = excluded.depth",
                    params![n.id, c.col, c.row, c.width, c.depth],
                )?,
                None => tx.execute("DELETE FROM cells WHERE node_id = ?1", [n.id])?,
            };
            let mut change = json!({
                "before": before.map(|b| b.name()),
                "after": c.map(|c| c.name()),
            });
            if recode && let Some(c) = c {
                let holder_code = load(&tx, *h)?.code.unwrap_or_default();
                let code = format!("{holder_code}-{}", c.anchor());
                apply_edit(&tx, n, "code", &code)?;
                change["code"] = json!({ "before": n.code, "after": code });
            }
            touch(&tx, n.id)?;
            event(&tx, n.id, "cell", change)?;
            let mut b = brief_json(&tx, n.id)?;
            b["cells"] = json!(c.map(|c| c.name()));
            out.push(b);
        }
        let grids = holders
            .iter()
            .map(|h| {
                Ok(json!({
                    "node": brief_json(&tx, *h)?,
                    "grid": grid_json(&tx, *h)?,
                }))
            })
            .collect::<Result<Vec<_>>>()?;
        tx.commit()?;
        Ok(json!({ "placed": out, "grids": grids }))
    }
}

#[cfg(test)]
mod tests {
    use super::Cells;

    #[test]
    fn cells_read_as_a_corner_or_a_range_in_any_order() {
        let c = Cells::parse("a3").unwrap();
        assert_eq!((c.col, c.row, c.width, c.depth), (0, 2, 1, 1));
        let r = Cells::parse("B4-A3").unwrap();
        assert_eq!((r.col, r.row, r.width, r.depth), (0, 2, 2, 2));
        assert_eq!(r.name(), "A3-B4");
        assert_eq!(Cells::parse("A3–B3").unwrap().name(), "A3-B3");
        assert!(Cells::parse("3A").is_err());
        assert!(Cells::parse("A0").is_err());
        assert!(r.overlaps(&Cells::parse("B4").unwrap()));
        assert!(!r.overlaps(&Cells::parse("C3").unwrap()));
    }
}
