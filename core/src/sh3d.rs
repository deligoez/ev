//! Reading a Sweet Home 3D floor plan (spec §31): its rooms' outlines, so a home is sketched
//! from the plan the person already drew instead of by hand.
//!
//! A `.sh3d` file is a zip whose `Home.xml` (Sweet Home 3D 5.3 and later) lists every room as
//! its corners, every wall by its ends and thickness, and every piece of furniture by its
//! centre, size and angle, in centimetres with y growing downwards — the frame the map uses.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension};
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::map::{Sketch, sketch_of, write_sketch};
use crate::model::{Kind, State};
use crate::store::{Inventory, brief_json, event, load, resolve};

struct PlanRoom {
    name: String,
    points: Vec<[f64; 2]>,
}

struct Wall {
    a: [f64; 2],
    b: [f64; 2],
    thickness: f64,
}

/// A piece of the plan's furniture: `Table#2` for the second one named Table, and its
/// footprint, turned as it stands: x, y, w, d.
struct PlanPiece {
    label: String,
    rect: [f64; 4],
}

type Plan = (Vec<PlanRoom>, Vec<PlanPiece>, Vec<Wall>);

fn read_plan(file: &Path) -> Result<Plan> {
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
    let mut walls = Vec::new();
    for n in doc.descendants() {
        match n.tag_name().name() {
            "wall" => {
                if let (Some(xs), Some(ys), Some(xe), Some(ye)) = (
                    num(n, "xStart"),
                    num(n, "yStart"),
                    num(n, "xEnd"),
                    num(n, "yEnd"),
                ) {
                    walls.push(Wall {
                        a: [xs, ys],
                        b: [xe, ye],
                        thickness: num(n, "thickness").unwrap_or(10.0),
                    });
                }
            }
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
            "pieceOfFurniture" => {
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
                let k = seen.entry(name.clone()).or_default();
                *k += 1;
                pieces.push(PlanPiece {
                    label: format!("{name}#{k}"),
                    rect: [x - bw / 2.0, y - bd / 2.0, bw, bd],
                });
            }
            _ => {}
        }
    }
    Ok((rooms, pieces, walls))
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

/// How far a point is from the segment a–b.
fn to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx * dx + dy * dy;
    let t = if len == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len).clamp(0.0, 1.0)
    };
    let (cx, cy) = (a[0] + t * dx, a[1] + t * dy);
    ((p[0] - cx).powi(2) + (p[1] - cy).powi(2)).sqrt()
}

/// A room the plan did not draw, found from its walls: the space around `seed` that walls and
/// the plan's rooms close in, as the corners of its outline. Refused when the space is not
/// closed (it would run out of the plan) or the point is in a wall or a room.
fn space_around(seed: [f64; 2], walls: &[Wall], rooms: &[PlanRoom]) -> Result<Vec<[f64; 2]>> {
    const CELL: f64 = 5.0;
    let ends: Vec<[f64; 2]> = walls.iter().flat_map(|w| [w.a, w.b]).collect();
    if ends.is_empty() {
        return Err(Error::Usage(
            "the plan has no walls to find a space by".into(),
        ));
    }
    let [bx, by, bw, bd] = crate::map::bbox(&ends);
    let (x0, y0) = (bx - 50.0, by - 50.0);
    let cols = ((bw + 100.0) / CELL).ceil() as usize;
    let rows = ((bd + 100.0) / CELL).ceil() as usize;
    let centre = |i: usize, j: usize| [x0 + (i as f64 + 0.5) * CELL, y0 + (j as f64 + 0.5) * CELL];
    let blocked = |p: [f64; 2]| {
        walls
            .iter()
            .any(|w| to_segment(p, w.a, w.b) <= w.thickness / 2.0)
            || rooms.iter().any(|r| inside(p, &r.points))
    };
    let at = |v: f64, o: f64, n: usize| {
        let k = ((v - o) / CELL).floor();
        (k >= 0.0 && (k as usize) < n).then_some(k as usize)
    };
    let (Some(si), Some(sj)) = (at(seed[0], x0, cols), at(seed[1], y0, rows)) else {
        return Err(Error::Usage(format!(
            "{},{} is outside the plan",
            seed[0], seed[1]
        )));
    };
    if blocked(centre(si, sj)) {
        return Err(Error::Usage(format!(
            "{},{} is in a wall or in a room of the plan",
            seed[0], seed[1]
        )));
    }
    let mut filled = vec![false; cols * rows];
    let mut seen = vec![false; cols * rows];
    let mut queue = std::collections::VecDeque::from([(si, sj)]);
    seen[sj * cols + si] = true;
    while let Some((i, j)) = queue.pop_front() {
        if i == 0 || j == 0 || i + 1 == cols || j + 1 == rows {
            return Err(Error::Usage(format!(
                "the space around {},{} is not closed by walls",
                seed[0], seed[1]
            )));
        }
        filled[j * cols + i] = true;
        for (ni, nj) in [(i + 1, j), (i - 1, j), (i, j + 1), (i, j - 1)] {
            if !seen[nj * cols + ni] {
                seen[nj * cols + ni] = true;
                if !blocked(centre(ni, nj)) {
                    queue.push_back((ni, nj));
                }
            }
        }
    }
    // The outline: every cell side between the space and the rest, walked clockwise.
    let is = |i: isize, j: isize| {
        i >= 0
            && j >= 0
            && (i as usize) < cols
            && (j as usize) < rows
            && filled[j as usize * cols + i as usize]
    };
    let mut next: HashMap<(isize, isize), Vec<(isize, isize)>> = HashMap::new();
    for j in 0..rows as isize {
        for i in 0..cols as isize {
            if !is(i, j) {
                continue;
            }
            let mut side =
                |a: (isize, isize), b: (isize, isize)| next.entry(a).or_default().push(b);
            if !is(i, j - 1) {
                side((i, j), (i + 1, j));
            }
            if !is(i + 1, j) {
                side((i + 1, j), (i + 1, j + 1));
            }
            if !is(i, j + 1) {
                side((i + 1, j + 1), (i, j + 1));
            }
            if !is(i - 1, j) {
                side((i, j + 1), (i, j));
            }
        }
    }
    let mut best: Vec<(isize, isize)> = Vec::new();
    while let Some(&start) = next.keys().next() {
        let mut ring = vec![start];
        let mut cur = start;
        while let Some(n) = next.get_mut(&cur).and_then(Vec::pop) {
            if next.get(&cur).is_some_and(Vec::is_empty) {
                next.remove(&cur);
            }
            if n == start {
                break;
            }
            ring.push(n);
            cur = n;
        }
        next.remove(&start)
            .filter(|v| !v.is_empty())
            .map(|v| next.insert(start, v));
        if ring.len() > best.len() {
            best = ring;
        }
    }
    // Only the corners: drop points in line with their neighbours.
    let n = best.len();
    let corners: Vec<[f64; 2]> = (0..n)
        .filter(|&k| {
            let (a, b, c) = (best[(k + n - 1) % n], best[k], best[(k + 1) % n]);
            (b.0 - a.0) * (c.1 - b.1) != (b.1 - a.1) * (c.0 - b.0)
        })
        .map(|k| [x0 + best[k].0 as f64 * CELL, y0 + best[k].1 as f64 * CELL])
        .collect();
    Ok(corners)
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
    /// the room of the same name (or the one `--room "<plan name>=<ref>"` names), and a piece
    /// of the plan's furniture named with `--piece "<Table#2>=<ref>"` gives its place and size
    /// to that record. Nothing else of the plan is drawn. `--space "<ref>@x,y"` gives a room
    /// the plan did not draw the space around that point closed in by walls and the plan's
    /// rooms. Rooms of the plan with no record are listed, not made.
    pub fn sketch_import(
        &mut self,
        file: &Path,
        rooms: &[String],
        pieces: &[String],
        spaces: &[String],
        dry_run: bool,
    ) -> Result<Value> {
        let (plan_rooms, plan_pieces, walls) = read_plan(file)?;
        let room_map = pairs(rooms, "--room")?;
        let piece_map = pairs(pieces, "--piece")?;
        let mut found = Vec::new();
        for s in spaces {
            let bad = || {
                Error::Usage(format!(
                    "--space is `<ref>@x,y` in the plan's centimetres; got `{s}`"
                ))
            };
            let (reference, at) = s.rsplit_once('@').ok_or_else(bad)?;
            let (x, y) = at.split_once(',').ok_or_else(bad)?;
            let seed = [
                x.trim().parse::<f64>().map_err(|_| bad())?,
                y.trim().parse::<f64>().map_err(|_| bad())?,
            ];
            let points = space_around(seed, &walls, &plan_rooms)?;
            found.push((
                reference.trim().to_string(),
                PlanRoom {
                    name: s.clone(),
                    points,
                },
            ));
        }
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
        for (reference, r) in &found {
            let id = resolve(&tx, reference, false)?;
            let depth = crate::store::path(&tx, id)?.len();
            matched.push((id, r, depth));
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
        // The plan's furniture is listed to choose from, and placed only on the record it is
        // named for: what the plan shows that is no record here is not drawn.
        let mut out_pieces = Vec::new();
        for p in &plan_pieces {
            let centre = [p.rect[0] + p.rect[2] / 2.0, p.rect[1] + p.rect[3] / 2.0];
            let room = matched
                .iter()
                .find(|(_, r, _)| inside(centre, &r.points))
                .map(|m| m.0);
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
            out_pieces.push(json!({
                "ref": p.label,
                "room": match room { Some(r) => brief_json(&tx, r)?, None => Value::Null },
                "linked": match linked { Some(l) => brief_json(&tx, l)?, None => Value::Null },
                "size": [p.rect[2].round(), p.rect[3].round()],
            }));
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
            }),
        )?;
        let out = json!({
            "rooms": out_rooms,
            "unmatched": unmatched,
            "not_in_plan": missing,
            "pieces": out_pieces,
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
    fn a_point_is_inside_an_l_and_near_a_wall() {
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
        assert!((to_segment([8.0, 6.0], [4.0, 4.0], [10.0, 4.0]) - 2.0).abs() < 1e-9);
    }
}
