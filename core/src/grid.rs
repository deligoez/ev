//! Grids (spec §22): a holder laid out in cells, like a gridfinity drawer, and the rectangle of
//! cells each box in it covers. Columns read A…Z from the left, rows 1… from the back.

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::error::{Error, Result, refused};
use crate::store::{Inventory, brief_json, event, load, resolve, touch};

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

/// Where a grid's outer corners are in a photo, as fractions of the upright photo, in the order
/// back-left, back-right, front-right, front-left (row 1 is the back): `x1,y1,…,x4,y4`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridCorners(pub [(f64, f64); 4]);

impl std::str::FromStr for GridCorners {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let bad = || {
            Error::Usage(format!(
                "grid corners `{s}` are eight fractions 0–1: back-left x,y, back-right x,y, \
                 front-right x,y, front-left x,y"
            ))
        };
        let v: Vec<f64> = s
            .split(',')
            .map(|p| p.trim().parse::<f64>())
            .collect::<std::result::Result<_, _>>()
            .map_err(|_| bad())?;
        if v.len() != 8 || v.iter().any(|x| !(0.0..=1.0).contains(x)) {
            return Err(bad());
        }
        Ok(GridCorners([
            (v[0], v[1]),
            (v[2], v[3]),
            (v[4], v[5]),
            (v[6], v[7]),
        ]))
    }
}

impl GridCorners {
    /// The same corners after `turns` clockwise quarter turns of the photo; each keeps its name
    /// (back-left is the furniture's corner, wherever the picture puts it).
    pub(crate) fn turned(&self, turns: u8) -> GridCorners {
        GridCorners(self.0.map(|p| crate::photo::turn_point(p, turns)))
    }
}

impl std::fmt::Display for GridCorners {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let v: Vec<String> = self
            .0
            .iter()
            .flat_map(|(x, y)| [format!("{x:.4}"), format!("{y:.4}")])
            .collect();
        write!(f, "{}", v.join(","))
    }
}

/// Where `cells` of `holder`'s grid are in a photo whose grid corners are known: the four
/// corners of that rectangle of cells (back-left, back-right, front-right, front-left), as
/// fractions of the upright photo.
pub(crate) fn cells_quad(
    conn: &Connection,
    holder: i64,
    corners: &GridCorners,
    cells: &Cells,
) -> Result<[(f64, f64); 4]> {
    let Some((cols, rows)) = grid_of(conn, holder)? else {
        return Err(refused(
            "this place has no grid; set one with `ev grid <ref> --cols N --rows M`",
            json!({ "holder": brief_json(conn, holder)? }),
        ));
    };
    if cells.col + cells.width > cols || cells.row + cells.depth > rows {
        return Err(Error::Usage(format!(
            "{} is outside the {cols}×{rows} grid",
            cells.name()
        )));
    }
    let map = projection(corners);
    let (cols, rows) = (cols as f64, rows as f64);
    let (u0, u1) = (
        cells.col as f64 / cols,
        (cells.col + cells.width) as f64 / cols,
    );
    let (v0, v1) = (
        cells.row as f64 / rows,
        (cells.row + cells.depth) as f64 / rows,
    );
    Ok([map(u0, v0), map(u1, v0), map(u1, v1), map(u0, v1)])
}

/// The projective map from the grid's unit square (u across from the left, v from the back) to
/// the photo, through its four corners. Unlike an even (bilinear) split it keeps a photo's
/// perspective: rows further back come out shorter, as they are in the picture.
fn projection(corners: &GridCorners) -> impl Fn(f64, f64) -> (f64, f64) {
    let [(x0, y0), (x1, y1), (x2, y2), (x3, y3)] = corners.0;
    let (dx1, dx2, dx3) = (x1 - x2, x3 - x2, x0 - x1 + x2 - x3);
    let (dy1, dy2, dy3) = (y1 - y2, y3 - y2, y0 - y1 + y2 - y3);
    let den = dx1 * dy2 - dx2 * dy1;
    let (g, h) = if den.abs() < 1e-12 || (dx3.abs() < 1e-12 && dy3.abs() < 1e-12) {
        (0.0, 0.0)
    } else {
        ((dx3 * dy2 - dx2 * dy3) / den, (dx1 * dy3 - dx3 * dy1) / den)
    };
    let (a, b, c) = (x1 - x0 + g * x1, x3 - x0 + h * x3, x0);
    let (d, e, f) = (y1 - y0 + g * y1, y3 - y0 + h * y3, y0);
    move |u, v| {
        let w = g * u + h * v + 1.0;
        ((a * u + b * v + c) / w, (d * u + e * v + f) / w)
    }
}

/// How far past its cells a box's crop reaches, as a share of a cell: a box's rim stands above
/// the floor the corners are read at, and a little more is better than a cut-off label.
const CROP_MARGIN: f64 = 0.15;

/// How much further a box's crop reaches for each unit of height above 1, as a share of a cell,
/// on the sides away from the photo's centre: a photo taken from above sees a tall box's rim
/// leaning outwards (up at the back, down at the front, out at the sides), not all round.
const LEAN_PER_HEIGHT: f64 = 0.3;

/// A box's height from its `size` (`1x2x1.5` is 1.5 high), 1 when the size says none. A taller
/// box's rim stands further above the floor and leans further out in a photo taken from above.
fn box_height(conn: &Connection, id: i64) -> Result<f64> {
    let size: Option<String> =
        conn.query_row("SELECT size FROM nodes WHERE id = ?1", [id], |r| r.get(0))?;
    Ok(size
        .as_deref()
        .and_then(|s| s.split(['x', '×']).nth(2))
        .and_then(|h| h.trim().replace(',', ".").parse::<f64>().ok())
        .filter(|h| *h > 0.0)
        .unwrap_or(1.0))
}

/// The crop of every box placed in `holder`'s grid, read off one photo from where the grid's
/// four corners are in it. Each box's cells are mapped through the corners (projectively, so a
/// photo taken at an angle still maps) and the crop is the rectangle around them, with a small
/// margin all round and, for a box higher than 1, more on the sides it leans out to.
pub(crate) fn grid_crops(
    conn: &Connection,
    holder: i64,
    corners: &GridCorners,
) -> Result<Vec<(i64, crate::Crop)>> {
    let Some((cols, rows)) = grid_of(conn, holder)? else {
        return Err(refused(
            "this place has no grid; set one with `ev grid <ref> --cols N --rows M`",
            json!({ "holder": brief_json(conn, holder)? }),
        ));
    };
    let map = projection(corners);
    let (cols, rows) = (cols as f64, rows as f64);
    placed(conn, holder)?
        .into_iter()
        .map(|(id, c)| {
            let lean = LEAN_PER_HEIGHT * (box_height(conn, id)? - 1.0).max(0.0);
            let (mu, mv) = (CROP_MARGIN / cols, CROP_MARGIN / rows);
            let (lu, lv) = (lean / cols, lean / rows);
            let mut u0 = c.col as f64 / cols - mu;
            let mut u1 = (c.col + c.width) as f64 / cols + mu;
            let mut v0 = c.row as f64 / rows - mv;
            let mut v1 = (c.row + c.depth) as f64 / rows + mv;
            // Which way is out: from the photo's centre to the box's centre.
            let (cx, cy) = map((u0 + u1) / 2.0, (v0 + v1) / 2.0);
            if cx < 0.5 {
                u0 -= lu;
            } else {
                u1 += lu;
            }
            if cy < 0.5 {
                v0 -= lv;
            } else {
                v1 += lv;
            }
            let pts = [map(u0, v0), map(u1, v0), map(u1, v1), map(u0, v1)];
            let x0 = pts
                .iter()
                .map(|p| p.0)
                .fold(f64::MAX, f64::min)
                .clamp(0.0, 1.0);
            let x1 = pts
                .iter()
                .map(|p| p.0)
                .fold(f64::MIN, f64::max)
                .clamp(0.0, 1.0);
            let y0 = pts
                .iter()
                .map(|p| p.1)
                .fold(f64::MAX, f64::min)
                .clamp(0.0, 1.0);
            let y1 = pts
                .iter()
                .map(|p| p.1)
                .fold(f64::MIN, f64::max)
                .clamp(0.0, 1.0);
            Ok((
                id,
                crate::Crop {
                    x: x0,
                    y: y0,
                    w: x1 - x0,
                    h: y1 - y0,
                },
            ))
        })
        .collect()
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
pub(crate) fn placed(conn: &Connection, holder: i64) -> Result<Vec<(i64, Cells)>> {
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
/// How a holder's grid is seen: `above` (a drawer, row 1 at the back) or `front` (furniture
/// and its compartments, row 1 at the top).
fn face_of(conn: &Connection, holder: i64) -> Result<String> {
    Ok(conn
        .query_row("SELECT face FROM grids WHERE node_id = ?1", [holder], |r| {
            r.get(0)
        })
        .optional()?
        .unwrap_or_else(|| "above".into()))
}

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
        "face": face_of(conn, holder)?,
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

/// Lays `id` out in `cols` × `rows` cells inside the caller's transaction.
fn grid_set_in(conn: &Connection, id: i64, cols: i64, rows: i64) -> Result<()> {
    if !(1..=MAX_COLS).contains(&cols) || !(1..=MAX_ROWS).contains(&rows) {
        return Err(Error::Usage(format!(
            "a grid is 1–{MAX_COLS} columns and 1–{MAX_ROWS} rows"
        )));
    }
    let outside: Vec<Value> = placed(conn, id)?
        .iter()
        .filter(|(_, c)| c.col + c.width > cols || c.row + c.depth > rows)
        .map(|(b, c)| {
            let mut v = brief_json(conn, *b)?;
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
    let before = grid_of(conn, id)?;
    conn.execute(
        "INSERT INTO grids (node_id, cols, rows) VALUES (?1, ?2, ?3)
         ON CONFLICT(node_id) DO UPDATE SET cols = excluded.cols, rows = excluded.rows",
        params![id, cols, rows],
    )?;
    touch(conn, id)?;
    event(
        conn,
        id,
        "grid",
        json!({ "before": before.map(|(c, r)| [c, r]), "after": [cols, rows] }),
    )?;
    Ok(())
}

impl Inventory {
    /// Lays `reference` out in `cols` × `rows` cells. Boxes already placed must still fit.
    pub fn grid_set(&mut self, reference: &str, cols: i64, rows: i64) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let id = resolve(&tx, reference, false)?;
        grid_set_in(&tx, id, cols, rows)?;
        tx.commit()?;
        self.grid(&id.to_string())
    }

    /// Lays several alike holders out at once (the 16 compartments of a Kallax, each an upper
    /// and a lower drawer), all or none.
    pub fn grid_set_many(&mut self, references: &[String], cols: i64, rows: i64) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let mut ids = Vec::new();
        for r in references {
            let id = resolve(&tx, r, false)?;
            grid_set_in(&tx, id, cols, rows)?;
            ids.push(id);
        }
        tx.commit()?;
        let grids = ids
            .iter()
            .map(|id| self.grid(&id.to_string()))
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({ "grids": grids }))
    }

    /// Says how several holders' grids are seen, `above` or `front`, all or none; refused for
    /// a holder without a grid.
    pub fn grid_face(&mut self, references: &[String], face: &str) -> Result<Value> {
        if !matches!(face, "above" | "front") {
            return Err(Error::Usage(format!(
                "a grid is seen from `above` or from the `front`; got `{face}`"
            )));
        }
        let tx = self.conn.transaction()?;
        let mut ids = Vec::new();
        for r in references {
            let id = resolve(&tx, r, false)?;
            if grid_of(&tx, id)?.is_none() {
                return Err(refused(
                    "it has no grid; give it one with --cols and --rows",
                    json!({ "node": brief_json(&tx, id)? }),
                ));
            }
            let before = face_of(&tx, id)?;
            if before != face {
                tx.execute(
                    "UPDATE grids SET face = ?2 WHERE node_id = ?1",
                    params![id, face],
                )?;
                touch(&tx, id)?;
                event(
                    &tx,
                    id,
                    "grid_face",
                    json!({ "before": before, "after": face }),
                )?;
            }
            ids.push(id);
        }
        tx.commit()?;
        let grids = ids
            .iter()
            .map(|id| self.grid(&id.to_string()))
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({ "grids": grids }))
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
    /// places in one step. A box keeps its code: it is the box's serial label, not its place.
    pub fn cells_set(&mut self, pairs: &[(String, String)]) -> Result<Value> {
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
        let mut out = Vec::new();
        for (n, _, c) in &moves {
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
            let change = json!({
                "before": before.map(|b| b.name()),
                "after": c.map(|c| c.name()),
            });
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
    use super::{Cells, GridCorners, projection};

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn grid_corners_turn_with_the_photo_and_keep_their_names() {
        let g: GridCorners = "0.1,0.2,0.9,0.2,0.95,0.9,0.05,0.9".parse().unwrap();
        // A clockwise quarter turn: (x, y) → (1 - y, x); back-left stays first.
        let t = g.turned(1);
        assert!(close(t.0[0], (0.8, 0.1)), "{t}");
        assert!(close(t.0[2], (0.1, 0.95)), "{t}");
        let back = g.turned(1).turned(3);
        assert!(g.0.iter().zip(back.0).all(|(a, b)| close(*a, b)));
    }

    #[test]
    fn grid_corners_read_eight_fractions_and_write_them_back() {
        let g: GridCorners = "0.1,0.2,0.9,0.2,0.95,0.9,0.05,0.9".parse().unwrap();
        assert_eq!(
            g.to_string(),
            "0.1000,0.2000,0.9000,0.2000,0.9500,0.9000,0.0500,0.9000"
        );
        assert_eq!(g.to_string().parse::<GridCorners>().unwrap(), g);
        assert!("0.1,0.2".parse::<GridCorners>().is_err());
        assert!(
            "0.1,0.2,0.9,0.2,1.5,0.9,0.05,0.9"
                .parse::<GridCorners>()
                .is_err()
        );
        assert!("a,b,c,d,e,f,g,h".parse::<GridCorners>().is_err());
    }

    #[test]
    fn the_projection_lands_on_the_corners_and_keeps_perspective() {
        let trap: GridCorners = "0.3,0.1,0.7,0.1,0.9,0.9,0.1,0.9".parse().unwrap();
        let m = projection(&trap);
        assert!(close(m(0.0, 0.0), (0.3, 0.1)));
        assert!(close(m(1.0, 0.0), (0.7, 0.1)));
        assert!(close(m(1.0, 1.0), (0.9, 0.9)));
        assert!(close(m(0.0, 1.0), (0.1, 0.9)));
        // Seen from the front, the back half of the drawer looks shorter: the middle row line
        // sits above the middle of the photo span, where an even split would put it.
        assert!(m(0.5, 0.5).1 < 0.5, "{:?}", m(0.5, 0.5));
        // A square view has no perspective: the middle is the middle.
        let square: GridCorners = "0.1,0.1,0.9,0.1,0.9,0.9,0.1,0.9".parse().unwrap();
        assert!(close(projection(&square)(0.5, 0.5), (0.5, 0.5)));
    }

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
