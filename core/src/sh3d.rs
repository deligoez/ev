//! Reading a Sweet Home 3D floor plan (spec §31): its rooms' outlines and what stands in them,
//! so a home is sketched from the plan the person already drew instead of by hand.
//!
//! A `.sh3d` file is a zip whose `Home.xml` (Sweet Home 3D 5.3 and later) lists every room as
//! its corners and every piece of furniture, door and window by its centre, size and angle, in
//! centimetres with y growing downwards — the frame the map uses.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::map::{Sketch, sketch_of, write_sketch};
use crate::model::{Kind, State};
use crate::store::{Inventory, brief_json, event, load, resolve};

/// How near a door or window must be to a room's outline to be drawn in it: a wall's width.
const WALL: f64 = 30.0;

struct PlanRoom {
    name: String,
    points: Vec<[f64; 2]>,
}

struct PlanPiece {
    /// `Table#2` for the second piece named Table; a door or window keeps its name.
    label: String,
    name: String,
    kind: &'static str,
    /// Its footprint, turned as it stands: x, y, w, d.
    rect: [f64; 4],
}

fn read_plan(file: &Path) -> Result<(Vec<PlanRoom>, Vec<PlanPiece>)> {
    let f = std::fs::File::open(file)
        .map_err(|e| Error::NotFound(format!("{}: {e}", file.display())))?;
    let not_plan = |why: String| {
        Error::Usage(format!(
            "{} is not a Sweet Home 3D plan ({why})",
            file.display()
        ))
    };
    let mut zip = zip::ZipArchive::new(f).map_err(|e| not_plan(e.to_string()))?;
    let mut xml = String::new();
    zip.by_name("Home.xml")
        .map_err(|_| not_plan("no Home.xml in it; save it with Sweet Home 3D 5.3 or later".into()))?
        .read_to_string(&mut xml)
        .map_err(|e| not_plan(e.to_string()))?;
    let doc = roxmltree::Document::parse(&xml).map_err(|e| not_plan(e.to_string()))?;
    let num = |n: roxmltree::Node, a: &str| n.attribute(a).and_then(|v| v.parse::<f64>().ok());
    let mut rooms = Vec::new();
    let mut pieces = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for n in doc.descendants() {
        match n.tag_name().name() {
            "room" => {
                let points: Vec<[f64; 2]> = n
                    .children()
                    .filter(|c| c.has_tag_name("point"))
                    .filter_map(|p| Some([num(p, "x")?, num(p, "y")?]))
                    .collect();
                if points.len() >= 3 {
                    rooms.push(PlanRoom {
                        name: n.attribute("name").unwrap_or_default().trim().to_string(),
                        points,
                    });
                }
            }
            tag @ ("pieceOfFurniture" | "doorOrWindow") => {
                if n.attribute("visible") == Some("false") {
                    continue;
                }
                let (Some(x), Some(y), Some(w), Some(d)) =
                    (num(n, "x"), num(n, "y"), num(n, "width"), num(n, "depth"))
                else {
                    continue;
                };
                let a = num(n, "angle").unwrap_or(0.0);
                let (s, c) = (a.sin().abs(), a.cos().abs());
                let (bw, bd) = (w * c + d * s, w * s + d * c);
                let name = n.attribute("name").unwrap_or("?").trim().to_string();
                let what = format!(
                    "{} {}",
                    name.to_lowercase(),
                    n.attribute("catalogId").unwrap_or_default().to_lowercase()
                );
                let kind = if tag == "pieceOfFurniture" {
                    "piece"
                } else if what.contains("door") || what.contains("kapı") {
                    "door"
                } else {
                    "window"
                };
                let label = if kind == "piece" {
                    let k = seen.entry(name.clone()).or_default();
                    *k += 1;
                    format!("{name}#{k}")
                } else {
                    name.clone()
                };
                pieces.push(PlanPiece {
                    label,
                    name,
                    kind,
                    rect: [x - bw / 2.0, y - bd / 2.0, bw, bd],
                });
            }
            _ => {}
        }
    }
    Ok((rooms, pieces))
}

fn inside(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let mut odd = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            odd = !odd;
        }
        j = i;
    }
    odd
}

/// How far a point is from a polygon: 0 inside, else the distance to its nearest edge.
fn distance(p: [f64; 2], poly: &[[f64; 2]]) -> f64 {
    if inside(p, poly) {
        return 0.0;
    }
    let mut best = f64::MAX;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = dx * dx + dy * dy;
        let t = if len == 0.0 {
            0.0
        } else {
            (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len).clamp(0.0, 1.0)
        };
        let (cx, cy) = (a[0] + t * dx, a[1] + t * dy);
        best = best.min(((p[0] - cx).powi(2) + (p[1] - cy).powi(2)).sqrt());
    }
    best
}

/// Where a node's own frame starts in the home's: its place plus its holders' places.
fn origin(conn: &Connection, id: i64) -> Result<[f64; 2]> {
    let mut at = [0.0, 0.0];
    let mut cur = Some(id);
    while let Some(c) = cur {
        let n = load(conn, c)?;
        if n.kind == Kind::Home {
            break;
        }
        let s = sketch_of(conn, c)?;
        at = [at[0] + s.x.unwrap_or(0.0), at[1] + s.y.unwrap_or(0.0)];
        cur = n.parent_id;
    }
    Ok(at)
}

/// The origin of the frame a node's own place is given in: its holder's.
fn holder_origin(conn: &Connection, id: i64) -> Result<[f64; 2]> {
    match load(conn, id)?.parent_id {
        Some(p) => origin(conn, p),
        None => Ok([0.0, 0.0]),
    }
}

/// `plan name=ref` pairs, keyed by the folded plan name.
fn pairs(list: &[String], what: &str) -> Result<HashMap<String, String>> {
    list.iter()
        .map(|p| {
            let (a, b) = p.split_once('=').ok_or_else(|| {
                Error::Usage(format!("{what} is `<name in the plan>=<ref>`; got `{p}`"))
            })?;
            Ok((crate::fold(a), b.trim().to_string()))
        })
        .collect()
}

impl Inventory {
    /// Sketches the home from a Sweet Home 3D plan: each room of the plan gives its outline to
    /// the room of the same name (or the one `--room "<plan name>=<ref>"` names), a piece named
    /// with `--piece "<Table#2>=<ref>"` gives its place and size to that record, and every other
    /// piece, door and window becomes a mark in the room it stands in, drawn on the map to find
    /// one's way. All marks are replaced; rooms of the plan with no record are listed, not made.
    pub fn sketch_import(
        &mut self,
        file: &Path,
        rooms: &[String],
        pieces: &[String],
        dry_run: bool,
    ) -> Result<Value> {
        let (plan_rooms, plan_pieces) = read_plan(file)?;
        let room_map = pairs(rooms, "--room")?;
        let piece_map = pairs(pieces, "--piece")?;
        let tx = self.conn.transaction()?;
        let home: i64 = tx
            .query_row(
                "SELECT id FROM nodes WHERE kind = 'home' AND state != 'gone' ORDER BY id LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::NotFound("no home yet".into()))?;
        // Every room of the home, by folded name.
        let mut by_name: HashMap<String, Vec<i64>> = HashMap::new();
        for id in crate::store::ids(
            &tx,
            "SELECT id FROM nodes WHERE kind = 'room' AND state != 'gone' ORDER BY id",
            [],
        )? {
            by_name
                .entry(crate::fold(&load(&tx, id)?.name))
                .or_default()
                .push(id);
        }
        let mut matched: Vec<(i64, &PlanRoom, usize)> = Vec::new();
        let mut unmatched = Vec::new();
        for r in &plan_rooms {
            let key = crate::fold(&r.name);
            let id = match room_map.get(&key) {
                Some(reference) => Some(resolve(&tx, reference, false)?),
                None => match by_name.get(&key).map(Vec::as_slice) {
                    Some([one]) => Some(*one),
                    _ => None,
                },
            };
            match id {
                Some(id) => {
                    let depth = crate::store::path(&tx, id)?.len();
                    matched.push((id, r, depth));
                }
                None => unmatched.push(r.name.clone()),
            }
        }
        // Holders first, so a balcony is placed in its room's new frame.
        matched.sort_by_key(|m| m.2);
        let mut out_rooms = Vec::new();
        for (id, r, _) in &matched {
            let o = holder_origin(&tx, *id)?;
            let mut s: Sketch = sketch_of(&tx, *id)?;
            s.set_outline(
                r.points
                    .iter()
                    .map(|p| [p[0] - o[0], p[1] - o[1]])
                    .collect(),
            );
            let changed = write_sketch(&tx, *id, &s)?;
            out_rooms.push(json!({
                "plan": r.name,
                "node": brief_json(&tx, *id)?,
                "changed": changed,
            }));
        }
        tx.execute("DELETE FROM sketch_marks", [])?;
        let mut out_pieces = Vec::new();
        let mut marks = 0;
        for p in &plan_pieces {
            let centre = [p.rect[0] + p.rect[2] / 2.0, p.rect[1] + p.rect[3] / 2.0];
            let near: Vec<i64> = matched
                .iter()
                .filter(|(_, r, _)| {
                    let d = distance(centre, &r.points);
                    if p.kind == "piece" {
                        d == 0.0
                    } else {
                        d <= WALL
                    }
                })
                .map(|m| m.0)
                .collect();
            let linked = match piece_map.get(&crate::fold(&p.label)) {
                Some(reference) => {
                    let id = resolve(&tx, reference, false)?;
                    let o = holder_origin(&tx, id)?;
                    let mut s = sketch_of(&tx, id)?;
                    (s.x, s.y, s.w, s.d) = (
                        Some(p.rect[0] - o[0]),
                        Some(p.rect[1] - o[1]),
                        Some(p.rect[2]),
                        Some(p.rect[3]),
                    );
                    s.points = None;
                    write_sketch(&tx, id, &s)?;
                    Some(id)
                }
                None => None,
            };
            if linked.is_none() {
                // A piece in no room (a hall the plan did not draw) is marked on the home.
                let holders = if near.is_empty() {
                    vec![home]
                } else {
                    near.clone()
                };
                for h in holders {
                    let o = origin(&tx, h)?;
                    tx.execute(
                        "INSERT INTO sketch_marks (node_id, kind, name, x, y, w, d)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            h,
                            p.kind,
                            p.name,
                            p.rect[0] - o[0],
                            p.rect[1] - o[1],
                            p.rect[2],
                            p.rect[3]
                        ],
                    )?;
                    marks += 1;
                }
            }
            if p.kind == "piece" {
                out_pieces.push(json!({
                    "ref": p.label,
                    "room": match near.first() { Some(r) => brief_json(&tx, *r)?, None => Value::Null },
                    "linked": match linked { Some(l) => brief_json(&tx, l)?, None => Value::Null },
                    "size": [p.rect[2].round(), p.rect[3].round()],
                }));
            }
        }
        // Rooms of the home the plan does not have.
        let mut missing = Vec::new();
        for ids in by_name.values() {
            for id in ids {
                if !matched.iter().any(|m| m.0 == *id)
                    && load(&tx, *id)?.state != State::Gone
                    && sketch_of(&tx, *id)?.points.is_none()
                {
                    missing.push(brief_json(&tx, *id)?);
                }
            }
        }
        missing.sort_by_key(|m| m["id"].as_i64());
        event(
            &tx,
            home,
            "sketch_import",
            json!({
                "file": file.file_name().map(|f| f.to_string_lossy().to_string()),
                "rooms": out_rooms.len(),
                "marks": marks,
            }),
        )?;
        let out = json!({
            "rooms": out_rooms,
            "unmatched": unmatched,
            "not_in_plan": missing,
            "pieces": out_pieces,
            "marks": marks,
            "dry_run": dry_run,
        });
        if !dry_run {
            tx.commit()?;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_is_inside_an_l_and_near_its_wall() {
        let l = [
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 4.0],
            [4.0, 4.0],
            [4.0, 10.0],
            [0.0, 10.0],
        ];
        assert!(inside([2.0, 8.0], &l));
        assert!(!inside([8.0, 8.0], &l));
        assert!((distance([8.0, 6.0], &l) - 2.0).abs() < 1e-9);
    }
}
