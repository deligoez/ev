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

/// A node's sketch: where it lies in its parent (x, y), how big it is (w, d), and what it stands
/// on, all optional.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Sketch {
    x: Option<f64>,
    y: Option<f64>,
    w: Option<f64>,
    d: Option<f64>,
    on: Option<i64>,
}

fn sketch_of(conn: &Connection, id: i64) -> Result<Sketch> {
    Ok(conn
        .query_row(
            "SELECT x, y, w, d, on_id FROM sketches WHERE node_id = ?1",
            [id],
            |r| {
                Ok(Sketch {
                    x: r.get(0)?,
                    y: r.get(1)?,
                    w: r.get(2)?,
                    d: r.get(3)?,
                    on: r.get(4)?,
                })
            },
        )
        .optional()?
        .unwrap_or_default())
}

fn sketch_json(p: &Sketch) -> Value {
    if *p == Sketch::default() {
        return Value::Null;
    }
    json!({ "x": p.x, "y": p.y, "w": p.w, "d": p.d, "on": p.on })
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
            .then(a.1.to_lowercase().cmp(&b.1.to_lowercase()))
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
    // A sketch: the place's size and the things in it that say where they lie.
    let own = sketch_of(conn, id)?;
    if let (Some(w), Some(d)) = (own.w, own.d) {
        let mut tiles = Vec::new();
        let mut unplaced = Vec::new();
        for c in &shown {
            let p = sketch_of(conn, c.id)?;
            match (p.x, p.y, p.w, p.d) {
                (Some(x), Some(y), Some(cw), Some(cd)) => {
                    let rect = [x / w, y / d, cw / w, cd / d].map(|v| v.clamp(0.0, 1.0));
                    tiles.push(with_stack(tile(conn, c, rect)?, c.id)?);
                }
                _ => unplaced.push(with_stack(tile(conn, c, [0.0; 4])?, c.id)?),
            }
        }
        if !tiles.is_empty() {
            return Ok(("sketch".into(), tiles, unplaced, json!({ "w": w, "d": d })));
        }
    }
    // Tiles on their own: holders first (the places to go into), labelled ones before the
    // rest, then by code and name.
    let mut order: Vec<&&Node> = shown.iter().collect();
    order.sort_by_key(|n| {
        (
            n.kind == Kind::Item,
            n.code.is_none(),
            n.code.clone().unwrap_or_default(),
            n.name.to_lowercase(),
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
    ) -> Result<Value> {
        if at.is_none() && size.is_none() && on.is_none() {
            return Err(Error::Usage(
                "give --size w,d, --at x,y or --on <ref>".into(),
            ));
        }
        let tx = self.conn.transaction()?;
        let id = resolve(&tx, reference, false)?;
        let before = sketch_of(&tx, id)?;
        let mut p = before;
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
        tx.execute(
            "INSERT INTO sketches (node_id, x, y, w, d, on_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(node_id) DO UPDATE SET x = excluded.x, y = excluded.y, w = excluded.w,
               d = excluded.d, on_id = excluded.on_id",
            params![id, p.x, p.y, p.w, p.d, p.on],
        )?;
        touch(&tx, id)?;
        event(
            &tx,
            id,
            "sketch",
            json!({ "before": sketch_json(&before), "after": sketch_json(&p) }),
        )?;
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
