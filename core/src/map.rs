//! The map (spec §31): any place drawn as the tiles of what is in it, so the home can be walked
//! from the flat down to a gridfinity cell. A place is drawn by the first of these it has:
//!
//! - a **stack** — things standing on each other (a Kallax on another), drawn front on, top
//!   first, each with its own layout;
//! - a **grid** — cells, as for a gridfinity drawer or the compartments of a Kallax;
//! - a **sketch** — the place's size and where each thing in it lies, in centimetres, seen from
//!   above: rooms in the home, furniture in a room;
//! - **tiles** — laid out automatically, so a place with no layout is still a map.
//!
//! Every tile carries a rectangle in fractions of the place (x, y from the top-left).

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::error::{Error, Result, refused};
use crate::model::{Kind, Node, State};
use crate::store::{
    Inventory, brief_json, event, item_total, load, path, path_text, resolve, touch,
};

/// A node's sketch: where it lies in its parent (x, y, its top-left corner), how big it is
/// (w, d), its outline when it is not a rectangle (a room), and what it stands on, all
/// optional. Positions and outlines are in the parent's frame, whose origin is the parent's own
/// top-left corner.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Sketch {
    pub(crate) x: Option<f64>,
    pub(crate) y: Option<f64>,
    pub(crate) w: Option<f64>,
    pub(crate) d: Option<f64>,
    pub(crate) on: Option<i64>,
    pub(crate) points: Option<Vec<[f64; 2]>>,
}

impl Sketch {
    pub(crate) fn rect(&self) -> Option<[f64; 4]> {
        Some([self.x?, self.y?, self.w?, self.d?])
    }

    /// An outline: the corners, and the rectangle around them as the place and size.
    pub(crate) fn set_outline(&mut self, points: Vec<[f64; 2]>) {
        let [x, y, w, d] = bbox(&points);
        (self.x, self.y, self.w, self.d) = (Some(x), Some(y), Some(w), Some(d));
        self.points = Some(points);
    }
}

/// The rectangle around some points, as x, y, w, d.
pub(crate) fn bbox(points: &[[f64; 2]]) -> [f64; 4] {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in points {
        (x0, y0, x1, y1) = (x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1]));
    }
    [x0, y0, x1 - x0, y1 - y0]
}

fn union(a: Option<[f64; 4]>, b: [f64; 4]) -> [f64; 4] {
    let Some(a) = a else { return b };
    let (x0, y0) = (a[0].min(b[0]), a[1].min(b[1]));
    let (x1, y1) = (
        (a[0] + a[2]).max(b[0] + b[2]),
        (a[1] + a[3]).max(b[1] + b[3]),
    );
    [x0, y0, x1 - x0, y1 - y0]
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

pub(crate) fn sketch_of(conn: &Connection, id: i64) -> Result<Sketch> {
    let row = conn
        .query_row(
            "SELECT x, y, w, d, on_id, points FROM sketches WHERE node_id = ?1",
            [id],
            |r| {
                Ok((
                    Sketch {
                        x: r.get(0)?,
                        y: r.get(1)?,
                        w: r.get(2)?,
                        d: r.get(3)?,
                        on: r.get(4)?,
                        points: None,
                    },
                    r.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .optional()?;
    let Some((mut s, points)) = row else {
        return Ok(Sketch::default());
    };
    s.points = points.and_then(|p| serde_json::from_str(&p).ok());
    Ok(s)
}

pub(crate) fn sketch_json(p: &Sketch) -> Value {
    if *p == Sketch::default() {
        return Value::Null;
    }
    let mut v = json!({ "x": p.x, "y": p.y, "w": p.w, "d": p.d, "on": p.on });
    if let Some(pts) = &p.points {
        v["points"] = json!(pts);
    }
    v
}

/// Stores a node's sketch with a `sketch` event, or nothing when it did not change.
pub(crate) fn write_sketch(conn: &Connection, id: i64, after: &Sketch) -> Result<bool> {
    let before = sketch_of(conn, id)?;
    if before == *after {
        return Ok(false);
    }
    let points = after
        .points
        .as_ref()
        .map(|p| serde_json::to_string(p).unwrap_or_default());
    conn.execute(
        "INSERT INTO sketches (node_id, x, y, w, d, on_id, points) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(node_id) DO UPDATE SET x = excluded.x, y = excluded.y, w = excluded.w,
           d = excluded.d, on_id = excluded.on_id, points = excluded.points",
        params![id, after.x, after.y, after.w, after.d, after.on, points],
    )?;
    touch(conn, id)?;
    event(
        conn,
        id,
        "sketch",
        json!({ "before": sketch_json(&before), "after": sketch_json(after) }),
    )?;
    Ok(true)
}

/// `x,y x,y …`: the corners of an outline, three or more.
fn outline(s: &str) -> Result<Vec<[f64; 2]>> {
    let bad = || {
        Error::Usage(format!(
            "--points is three or more corners in centimetres, like `0,0 400,0 400,300`; got `{s}`"
        ))
    };
    let pts = s
        .split_whitespace()
        .map(|p| {
            let (a, b) = p.split_once(',').ok_or_else(bad)?;
            let n = |t: &str| t.trim().parse::<f64>().ok().filter(|v| v.is_finite());
            Ok([n(a).ok_or_else(bad)?, n(b).ok_or_else(bad)?])
        })
        .collect::<Result<Vec<_>>>()?;
    if pts.len() < 3 {
        return Err(bad());
    }
    Ok(pts)
}

/// `a,b` as two positive (or, for a position, non-negative) numbers of centimetres.
fn pair(s: &str, what: &str, allow_zero: bool) -> Result<(f64, f64)> {
    let bad = || {
        Error::Usage(format!(
            "{what} is two numbers of centimetres, like 120,40; got `{s}`"
        ))
    };
    let (a, b) = s.split_once([',', 'x', '×']).ok_or_else(bad)?;
    let num = |t: &str| t.trim().replace(',', ".").parse::<f64>().map_err(|_| bad());
    let (a, b) = (num(a)?, num(b)?);
    let ok = |v: f64| v.is_finite() && if allow_zero { v >= 0.0 } else { v > 0.0 };
    if !ok(a) || !ok(b) {
        return Err(bad());
    }
    Ok((a, b))
}

fn live_children(conn: &Connection, id: i64) -> Result<Vec<Node>> {
    crate::store::ids(
        conn,
        "SELECT id FROM nodes WHERE parent_id = ?1 AND state != 'gone' ORDER BY id",
        [id],
    )?
    .into_iter()
    .map(|c| load(conn, c))
    .collect()
}

/// What stands on `id`, directly or on something that does, bottom up.
fn standing_on(conn: &Connection, id: i64) -> Result<Vec<i64>> {
    let mut out = Vec::new();
    let mut cur = id;
    for _ in 0..1000 {
        let next: Option<i64> = conn
            .query_row(
                "SELECT p.node_id FROM sketches p JOIN nodes n ON n.id = p.node_id
                  WHERE p.on_id = ?1 AND n.state != 'gone' ORDER BY p.node_id LIMIT 1",
                [cur],
                |r| r.get(0),
            )
            .optional()?;
        match next {
            Some(n) if !out.contains(&n) && n != id => {
                out.push(n);
                cur = n;
            }
            _ => break,
        }
    }
    Ok(out)
}

/// The bottom of the stack `id` belongs to: follow `on` down.
fn stack_base(conn: &Connection, id: i64) -> Result<i64> {
    let mut cur = id;
    for _ in 0..1000 {
        match sketch_of(conn, cur)?.on {
            Some(b) if b != id => cur = b,
            _ => break,
        }
    }
    Ok(cur)
}

/// A tile: the node, its rectangle, and what a map needs to say about it.
fn tile(conn: &Connection, n: &Node, rect: [f64; 4]) -> Result<Value> {
    let mut t = brief_json(conn, n.id)?;
    t["rect"] = json!(rect.map(|v| (v * 10_000.0).round() / 10_000.0));
    t["items"] = json!(item_total(conn, n.id)?);
    let kids = live_children(conn, n.id)?;
    t["children"] = json!(kids.len());
    // What a tile can say about its contents: holders by code first, then things by name.
    let mut names: Vec<(bool, String)> = kids
        .iter()
        .map(|k| {
            let mut s = k.code.clone().unwrap_or_else(|| k.name.clone());
            if let Some(q) = k.qty.filter(|&q| q > 1) {
                s.push_str(&format!(" ×{q}"));
            }
            (k.code.is_none(), s)
        })
        .collect();
    names.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(crate::fold(&a.1).cmp(&crate::fold(&b.1)))
    });
    t["contents"] = json!(
        names
            .into_iter()
            .take(40)
            .map(|(_, s)| s)
            .collect::<Vec<_>>()
    );
    for (k, on) in [("temporary", n.temporary), ("unknown", n.unknown)] {
        if on {
            t[k] = json!(true);
        }
    }
    if let Some(f) = n.fill {
        t["fill"] = json!(f);
    }
    if let Some(th) = &n.theme {
        t["theme"] = json!(th);
    }
    if let Some(c) = crate::grid::cells_of(conn, n.id)? {
        t["cells"] = json!(c.name());
    }
    Ok(t)
}

/// Children laid out on their own: as many columns as keep the tiles about as wide as the page
/// is, filled row by row in the order given.
fn auto_rects(n: usize) -> Vec<[f64; 4]> {
    if n == 0 {
        return Vec::new();
    }
    let cols = ((n as f64 * 1.6).sqrt().ceil() as usize).clamp(1, n);
    let rows = n.div_ceil(cols);
    (0..n)
        .map(|i| {
            let (c, r) = (i % cols, i / cols);
            [
                c as f64 / cols as f64,
                r as f64 / rows as f64,
                1.0 / cols as f64,
                1.0 / rows as f64,
            ]
        })
        .collect()
}

/// One place's own layout (not a stack): its kind, tiles, what has no place in it, and the
/// sketch's size when it has one.
fn layout(conn: &Connection, id: i64) -> Result<(String, Vec<Value>, Vec<Value>, Value)> {
    let children = live_children(conn, id)?;
    // A thing standing on a sibling is drawn with it, not beside it.
    let on_sibling = |n: &Node| -> Result<bool> {
        Ok(sketch_of(conn, n.id)?
            .on
            .is_some_and(|b| children.iter().any(|c| c.id == b)))
    };
    let mut shown = Vec::new();
    for c in &children {
        if !on_sibling(c)? {
            shown.push(c);
        }
    }
    let stacked_on = |base: i64| -> Result<Vec<Value>> {
        standing_on(conn, base)?
            .iter()
            .map(|s| brief_json(conn, *s))
            .collect()
    };
    let with_stack = |mut t: Value, id: i64| -> Result<Value> {
        let s = stacked_on(id)?;
        if !s.is_empty() {
            t["stacked"] = json!(s);
        }
        Ok(t)
    };
    // A grid: each placed box on its cells.
    if let Some((cols, rows)) = crate::grid::grid_of(conn, id)? {
        let placed = crate::grid::placed(conn, id)?;
        let size = json!({ "cols": cols, "rows": rows });
        let (cols, rows) = (cols as f64, rows as f64);
        let mut tiles = Vec::new();
        for (b, c) in &placed {
            let n = load(conn, *b)?;
            let rect = [
                c.col as f64 / cols,
                c.row as f64 / rows,
                c.width as f64 / cols,
                c.depth as f64 / rows,
            ];
            tiles.push(with_stack(tile(conn, &n, rect)?, n.id)?);
        }
        let unplaced = shown
            .iter()
            .filter(|c| !placed.iter().any(|(b, _)| *b == c.id))
            .map(|c| with_stack(tile(conn, c, [0.0; 4])?, c.id))
            .collect::<Result<Vec<_>>>()?;
        return Ok(("grid".into(), tiles, unplaced, size));
    }
    // A sketch: the things in it that say where they lie, seen from above, drawn with the
    // place's own outline.
    let own = sketch_of(conn, id)?;
    let mut placed = Vec::new();
    let mut loose = Vec::new();
    for c in &shown {
        let p = sketch_of(conn, c.id)?;
        match p.rect() {
            Some(r) => placed.push((*c, p, r)),
            None => loose.push(*c),
        }
    }
    // A room with an outline is drawn as its floor plan even before anything in it has a place.
    if !placed.is_empty() || own.points.is_some() {
        let floor: Option<Vec<[f64; 2]>> = match (&own.points, own.x, own.y) {
            (Some(pts), Some(x), Some(y)) => {
                Some(pts.iter().map(|p| [p[0] - x, p[1] - y]).collect())
            }
            _ => None,
        };
        // The view: the place's own size and everything drawn in it.
        let mut view = own.w.zip(own.d).map(|(w, d)| [0.0, 0.0, w, d]);
        for (_, _, r) in &placed {
            view = Some(union(view, *r));
        }
        let [vx, vy, vw, vd] = view.unwrap_or([0.0, 0.0, 1.0, 1.0]);
        let (vw, vd) = (vw.max(1e-9), vd.max(1e-9));
        let point = |p: [f64; 2]| [round4((p[0] - vx) / vw), round4((p[1] - vy) / vd)];
        let frac = |r: [f64; 4]| [(r[0] - vx) / vw, (r[1] - vy) / vd, r[2] / vw, r[3] / vd];
        let mut tiles = Vec::new();
        for (c, p, r) in &placed {
            let mut t = with_stack(tile(conn, c, frac(*r))?, c.id)?;
            if let Some(pts) = &p.points {
                let mut shapes = vec![pts.iter().map(|q| point(*q)).collect::<Vec<_>>()];
                // A room inside it (a balcony) is drawn as part of it.
                for g in live_children(conn, c.id)? {
                    if let Some(gp) = sketch_of(conn, g.id)?.points {
                        shapes.push(
                            gp.iter()
                                .map(|q| point([q[0] + r[0], q[1] + r[1]]))
                                .collect(),
                        );
                    }
                }
                t["shapes"] = json!(shapes);
            }
            tiles.push(t);
        }
        let unplaced = loose
            .iter()
            .map(|c| with_stack(tile(conn, c, [0.0; 4])?, c.id))
            .collect::<Result<Vec<_>>>()?;
        let mut size = json!({ "w": vw.round(), "d": vd.round() });
        if let Some(f) = floor {
            size["floor"] = json!(f.iter().map(|q| point(*q)).collect::<Vec<_>>());
        }
        return Ok(("sketch".into(), tiles, unplaced, size));
    }
    // Tiles on their own: holders first (the places to go into), labelled ones before the
    // rest, then by code and name.
    let mut order: Vec<&&Node> = shown.iter().collect();
    order.sort_by_key(|n| {
        (
            n.kind == Kind::Item,
            n.code.is_none(),
            n.code.clone().unwrap_or_default(),
            crate::fold(&n.name),
        )
    });
    let rects = auto_rects(order.len());
    let tiles = order
        .iter()
        .zip(rects)
        .map(|(n, r)| with_stack(tile(conn, n, r)?, n.id))
        .collect::<Result<Vec<_>>>()?;
    Ok(("tiles".into(), tiles, Vec::new(), Value::Null))
}

impl Inventory {
    /// Sets where a node lies and how big it is, in centimetres, and what it stands on: a room
    /// in the home, a piece of furniture in a room (`--at` its top-left corner seen from above,
    /// `--size` width and depth), or a Kallax on another (`--on`). A place's `--size` alone makes
    /// it a sketch its contents can be placed in.
    pub fn sketch_set(
        &mut self,
        reference: &str,
        at: Option<&str>,
        size: Option<&str>,
        on: Option<&str>,
        points: Option<&str>,
    ) -> Result<Value> {
        if at.is_none() && size.is_none() && on.is_none() && points.is_none() {
            return Err(Error::Usage(
                "give --size w,d, --at x,y, --points or --on <ref>".into(),
            ));
        }
        let tx = self.conn.transaction()?;
        let id = resolve(&tx, reference, false)?;
        let mut p = sketch_of(&tx, id)?;
        if let Some(s) = points {
            p.set_outline(outline(s)?);
        }
        if let Some(s) = size {
            let (w, d) = pair(s, "--size", false)?;
            (p.w, p.d) = (Some(w), Some(d));
        }
        if let Some(a) = at {
            let (x, y) = pair(a, "--at", true)?;
            (p.x, p.y) = (Some(x), Some(y));
        }
        if let Some(o) = on {
            let base = resolve(&tx, o, false)?;
            if base == id || stack_base(&tx, base)? == id {
                return Err(refused(
                    "a thing cannot stand on itself or on what stands on it",
                    json!({ "node": brief_json(&tx, id)?, "on": brief_json(&tx, base)? }),
                ));
            }
            p.on = Some(base);
        }
        write_sketch(&tx, id, &p)?;
        tx.commit()?;
        Ok(json!({ "node": brief_json(&self.conn, id)?, "sketch": sketch_json(&p) }))
    }

    /// Removes a node's sketch: its place, size and what it stands on.
    pub fn sketch_clear(&mut self, reference: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let id = resolve(&tx, reference, false)?;
        let before = sketch_of(&tx, id)?;
        tx.execute("DELETE FROM sketches WHERE node_id = ?1", [id])?;
        event(
            &tx,
            id,
            "sketch",
            json!({ "before": sketch_json(&before), "after": Value::Null }),
        )?;
        tx.commit()?;
        Ok(json!({ "node": brief_json(&self.conn, id)?, "sketch": Value::Null }))
    }

    /// A node's sketch as `{x, y, w, d, on}`, or null.
    pub fn sketch(&self, reference: &str) -> Result<Value> {
        let id = resolve(&self.conn, reference, true)?;
        Ok(sketch_json(&sketch_of(&self.conn, id)?))
    }

    /// The map of a place (the first home without one): how its contents are laid out and the
    /// tiles to draw, each with its rectangle in fractions of the place. A place in a stack is
    /// drawn as the whole stack, front on, top first.
    pub fn map(&self, reference: Option<&str>) -> Result<Value> {
        let id = match reference {
            Some(r) => resolve(&self.conn, r, false)?,
            None => self
                .conn
                .query_row(
                    "SELECT id FROM nodes WHERE kind = 'home' AND state != 'gone' ORDER BY id LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(|| Error::NotFound("no home yet; add one with `ev add <name> --kind home`".into()))?,
        };
        let node = load(&self.conn, id)?;
        if node.state == State::Gone {
            return Err(Error::NotFound(format!("#{id} is gone")));
        }
        let segs = path(&self.conn, id)?;
        let base = stack_base(&self.conn, id)?;
        let above = standing_on(&self.conn, base)?;
        let mut out = json!({
            "node": brief_json(&self.conn, id)?,
            "path_text": path_text(&segs),
            "path": segs,
            "parent": node.parent_id,
            "sketch": sketch_json(&sketch_of(&self.conn, id)?),
        });
        if !above.is_empty() {
            // Front on, top first: each member a band as tall as its rows, its tiles inside.
            let mut members: Vec<i64> = above.iter().rev().copied().collect();
            members.push(base);
            let heights: Vec<f64> = members
                .iter()
                .map(|m| Ok(crate::grid::grid_of(&self.conn, *m)?.map_or(1.0, |(_, r)| r as f64)))
                .collect::<Result<_>>()?;
            let total: f64 = heights.iter().sum();
            let mut y = 0.0;
            let mut bands = Vec::new();
            let mut tiles = Vec::new();
            let mut unplaced = Vec::new();
            for (m, h) in members.iter().zip(&heights) {
                let band = [0.0, y / total, 1.0, h / total];
                let mut b = brief_json(&self.conn, *m)?;
                b["rect"] = json!(band);
                let (kind, ts, un, size) = layout(&self.conn, *m)?;
                b["layout"] = json!(kind);
                b["size"] = size;
                for mut t in ts {
                    let r: Vec<f64> = serde_json::from_value(t["rect"].clone()).unwrap_or_default();
                    if r.len() == 4 {
                        t["rect"] = json!([r[0], band[1] + r[1] * band[3], r[2], r[3] * band[3]]);
                    }
                    t["band"] = json!(m);
                    tiles.push(t);
                }
                for mut u in un {
                    u["band"] = json!(m);
                    unplaced.push(u);
                }
                bands.push(b);
                y += h;
            }
            out["layout"] = json!("stack");
            out["bands"] = json!(bands);
            out["tiles"] = json!(tiles);
            out["unplaced"] = json!(unplaced);
            return Ok(out);
        }
        let (kind, tiles, unplaced, size) = layout(&self.conn, id)?;
        out["layout"] = json!(kind);
        out["size"] = size;
        out["tiles"] = json!(tiles);
        out["unplaced"] = json!(unplaced);
        Ok(out)
    }
}

/// The node ids a map draws, in reading order (top to bottom, then left to right), with the
/// ones that have no place last: the order a keyboard walks them in.
pub fn reading_order(map: &Value) -> Vec<i64> {
    let mut placed: Vec<(i64, f64, f64)> = map["tiles"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| {
            let r = t["rect"].as_array()?;
            Some((t["id"].as_i64()?, r.get(1)?.as_f64()?, r.first()?.as_f64()?))
        })
        .collect();
    placed.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.2.total_cmp(&b.2)));
    let mut ids: Vec<i64> = placed.into_iter().map(|t| t.0).collect();
    ids.extend(
        map["unplaced"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| t["id"].as_i64()),
    );
    ids
}
