//! The map in `ev ui` (`M`): a place's contents drawn where they are — on its grid, on its
//! sketch, or as a stack of furniture front on — to walk with the arrows and go into with Enter.
//!
//! Like the rest of `ev ui` it never writes: each level is `ev map` of the place shown.

use std::collections::{HashMap, HashSet};

use ev_core::{Inventory, Result};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use serde_json::Value;

use crate::i18n::{t, tf};
use crate::theme::pal;

/// What the map asks of the window around it after a key.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Stay,
    Close,
    /// Close the map and select this node in the tree.
    Reveal(i64),
    Quit,
}

pub struct MapView {
    /// `ev map` of the place shown.
    pub map: Value,
    /// Its tiles in reading order, the unplaced ones last; `sel` indexes it.
    order: Vec<i64>,
    sel: usize,
    /// Where each tile was drawn, for the mouse.
    hits: Vec<(Rect, i64)>,
    status: String,
    /// The way from the home down to the node selected in the tree when the map was opened:
    /// on each level the tile on it is chosen, so Enter after Enter leads to that node.
    trail: Vec<i64>,
    /// A floor plan's cells and the room each belongs to, for the mouse.
    raster: Option<(Rect, Vec<Option<i64>>)>,
}

/// A rectangle given in fractions of `area`, in whole cells. Neighbours share an edge exactly,
/// since both ends are rounded the same way.
fn frac_rect(area: Rect, r: &[f64]) -> Rect {
    let (w, h) = (area.width as f64, area.height as f64);
    let x0 = ((r[0] * w).round() as u16).min(area.width);
    let x1 = (((r[0] + r[2]) * w).round() as u16).min(area.width);
    let y0 = ((r[1] * h).round() as u16).min(area.height);
    let y1 = (((r[1] + r[3]) * h).round() as u16).min(area.height);
    Rect::new(
        area.x + x0,
        area.y + y0,
        x1.saturating_sub(x0).max(1).min(area.width - x0),
        y1.saturating_sub(y0).max(1).min(area.height - y0),
    )
}

/// The largest part of `area` with a sketch's proportions, a cell being about twice as tall as
/// it is wide.
fn fit_aspect(area: Rect, w: f64, d: f64) -> Rect {
    if w <= 0.0 || d <= 0.0 || area.width == 0 || area.height == 0 {
        return area;
    }
    let ratio = w / d * 2.0; // columns per row
    let (aw, ah) = (area.width as f64, area.height as f64);
    let (cw, ch) = if aw / ah > ratio {
        (ah * ratio, ah)
    } else {
        (aw, aw / ratio)
    };
    Rect::new(
        area.x,
        area.y,
        (cw.round() as u16).clamp(1, area.width),
        (ch.round() as u16).clamp(1, area.height),
    )
}

fn rect_of(t: &Value) -> Option<[f64; 4]> {
    let r = t["rect"].as_array()?;
    Some([
        r.first()?.as_f64()?,
        r.get(1)?.as_f64()?,
        r.get(2)?.as_f64()?,
        r.get(3)?.as_f64()?,
    ])
}

/// Corners given as `[[x, y], …]`.
fn polygon(v: &Value) -> Vec<[f64; 2]> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some([p[0].as_f64()?, p[1].as_f64()?]))
        .collect()
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

/// One line on the chosen tile: its label, what it is for, how much it holds and what is in it.
fn summary(t: &Value) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!(" {} ", title_of(t)),
        Style::new().bold(),
    )];
    if t["code"].is_string() {
        let what = t["theme"]
            .as_str()
            .or(t["name"].as_str())
            .unwrap_or_default();
        spans.push(Span::raw(format!("{what}  ")));
    }
    spans.push(Span::styled(
        tf("{} items", &[&t["items"]]),
        Style::new().fg(pal().qty),
    ));
    if t["unknown"] == true {
        spans.push(Span::styled(
            format!("  {}", crate::i18n::t("contents unknown")),
            Style::new().fg(pal().mark),
        ));
    }
    let names: Vec<&str> = t["contents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if !names.is_empty() {
        spans.push(Span::styled(
            format!("  · {}", names.join(" · ")),
            Style::new().fg(pal().muted),
        ));
    }
    Line::from(spans)
}

fn title_of(n: &Value) -> String {
    n["code"]
        .as_str()
        .or(n["name"].as_str())
        .unwrap_or_default()
        .to_string()
}

impl MapView {
    /// The map of `place` (the home when none) with `select` chosen: that tile, or the one it
    /// stands on.
    pub fn open(inv: &Inventory, place: Option<i64>, select: Option<i64>) -> Result<Self> {
        let map = inv.map(place.map(|p| p.to_string()).as_deref())?;
        let mut v = Self {
            order: ev_core::reading_order(&map),
            map,
            sel: 0,
            hits: Vec::new(),
            status: String::new(),
            trail: Vec::new(),
            raster: None,
        };
        if let Some(id) = select {
            v.select_id(id);
        }
        Ok(v)
    }

    /// Keeps `trail` (ancestors first) and, when nothing else was chosen, chooses the tile on it.
    pub fn with_trail(mut self, trail: Vec<i64>, follow: bool) -> Self {
        if follow && let Some(&id) = trail.iter().find(|&&id| self.index_of(id).is_some()) {
            self.select_id(id);
        }
        self.trail = trail;
        self
    }

    /// The place shown.
    pub fn place(&self) -> Option<i64> {
        self.map["node"]["id"].as_i64()
    }

    pub fn selected(&self) -> Option<i64> {
        self.order.get(self.sel).copied()
    }

    fn all(&self) -> impl Iterator<Item = &Value> {
        self.map["tiles"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(self.map["unplaced"].as_array().into_iter().flatten())
    }

    fn tile(&self, id: i64) -> Option<&Value> {
        self.all().find(|t| t["id"].as_i64() == Some(id))
    }

    fn select_id(&mut self, id: i64) {
        if let Some(i) = self.index_of(id) {
            self.sel = i;
        }
    }

    /// Where `id` is in the reading order: its own tile, or the one it stands on.
    fn index_of(&self, id: i64) -> Option<usize> {
        if let Some(i) = self.order.iter().position(|&x| x == id) {
            return Some(i);
        }
        // Something standing on a tile is found under that tile.
        self.order.iter().position(|&o| {
            self.tile(o).is_some_and(|t| {
                t["stacked"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|s| s["id"].as_i64() == Some(id))
            })
        })
    }

    /// The same place again after the data changed, keeping the selection; the home when the
    /// place is gone.
    pub fn reload(&mut self, inv: &Inventory) -> Result<()> {
        let keep = self.selected();
        let trail = std::mem::take(&mut self.trail);
        *self = match Self::open(inv, self.place(), keep) {
            Ok(v) => v.with_trail(trail, false),
            Err(_) => Self::open(inv, None, None)?.with_trail(trail, true),
        };
        Ok(())
    }

    /// Where each tile's centre is, the unplaced ones on a row below the map.
    fn centres(&self) -> Vec<(f64, f64)> {
        let unplaced: Vec<i64> = self.map["unplaced"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| t["id"].as_i64())
            .collect();
        self.order
            .iter()
            .map(|&id| {
                if let Some(k) = unplaced.iter().position(|&u| u == id) {
                    return ((k as f64 + 0.5) / unplaced.len() as f64, 1.2);
                }
                let r = self.tile(id).and_then(rect_of).unwrap_or([0.0; 4]);
                (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0)
            })
            .collect()
    }

    /// Moves to the nearest tile in a direction, preferring one in line with this one.
    fn step(&mut self, dx: i8, dy: i8) {
        let c = self.centres();
        let Some(&(x, y)) = c.get(self.sel) else {
            return;
        };
        let best = c
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != self.sel)
            .filter_map(|(i, &(cx, cy))| {
                let (along, across) = if dx == 0 {
                    ((cy - y) * dy as f64, (cx - x).abs())
                } else {
                    ((cx - x) * dx as f64, (cy - y).abs())
                };
                (along > 1e-6).then_some((i, along + across * 2.0))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = best {
            self.sel = i;
        }
    }

    fn go_in(&mut self, inv: &Inventory) -> Result<()> {
        let Some(id) = self.selected() else {
            return Ok(());
        };
        let inside = self
            .tile(id)
            .and_then(|t| t["children"].as_u64())
            .unwrap_or(0);
        if inside == 0 {
            self.status = t("nothing inside").to_string();
            return Ok(());
        }
        let trail = std::mem::take(&mut self.trail);
        *self = Self::open(inv, Some(id), None)?.with_trail(trail, true);
        Ok(())
    }

    fn go_up(&mut self, inv: &Inventory) -> Result<()> {
        let (Some(here), Some(up)) = (self.place(), self.map["parent"].as_i64()) else {
            self.status = t("this is the top").to_string();
            return Ok(());
        };
        let trail = std::mem::take(&mut self.trail);
        *self = Self::open(inv, Some(up), Some(here))?.with_trail(trail, false);
        Ok(())
    }

    pub fn key(&mut self, inv: &Inventory, k: KeyEvent) -> Result<Outcome> {
        self.status.clear();
        match k.code {
            KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(Outcome::Quit);
            }
            KeyCode::Esc | KeyCode::Char('q' | 'M') => return Ok(Outcome::Close),
            KeyCode::Left | KeyCode::Char('h') => self.step(-1, 0),
            KeyCode::Right | KeyCode::Char('l') => self.step(1, 0),
            KeyCode::Up | KeyCode::Char('k') => self.step(0, -1),
            KeyCode::Down | KeyCode::Char('j') => self.step(0, 1),
            KeyCode::Tab => self.sel = (self.sel + 1) % self.order.len().max(1),
            KeyCode::BackTab => {
                let n = self.order.len().max(1);
                self.sel = (self.sel + n - 1) % n;
            }
            KeyCode::Enter => self.go_in(inv)?,
            KeyCode::Backspace | KeyCode::Char('u') => self.go_up(inv)?,
            KeyCode::Char('t') => {
                if let Some(id) = self.selected().or(self.place()) {
                    return Ok(Outcome::Reveal(id));
                }
            }
            _ => {}
        }
        Ok(Outcome::Stay)
    }

    /// A click selects a tile; a click on the selected one goes into it.
    pub fn click(&mut self, inv: &Inventory, column: u16, row: u16) -> Result<()> {
        self.status.clear();
        let hit = self.hits.iter().find(|(r, _)| {
            column >= r.x && column < r.x + r.width && row >= r.y && row < r.y + r.height
        });
        let from_plan = || {
            let (a, cells) = self.raster.as_ref()?;
            if column < a.x || row < a.y || column >= a.x + a.width || row >= a.y + a.height {
                return None;
            }
            cells[(row - a.y) as usize * a.width as usize + (column - a.x) as usize]
        };
        let Some(id) = hit.map(|h| h.1).or_else(from_plan) else {
            return Ok(());
        };
        if self.selected() == Some(id) {
            self.go_in(inv)
        } else {
            self.select_id(id);
            Ok(())
        }
    }

    fn layout_text(&self) -> String {
        let m = &self.map;
        match m["layout"].as_str().unwrap_or_default() {
            "grid" => tf("grid {}×{}", &[&m["size"]["cols"], &m["size"]["rows"]]),
            "sketch" => tf("sketch {}×{} cm", &[&m["size"]["w"], &m["size"]["d"]]),
            "stack" => t("stack, front on, top first").to_string(),
            _ => t("tiles (no layout yet)").to_string(),
        }
    }

    pub fn draw(&mut self, f: &mut Frame) {
        self.hits.clear();
        self.raster = None;
        let [top, main, info, bottom] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(f.area());
        let path = self.map["path_text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!(" {path} "), pal().brand.bold()),
                Span::styled(format!("  {}", self.layout_text()), pal().muted),
            ])),
            top,
        );
        let unplaced: Vec<Value> = self.map["unplaced"].as_array().cloned().unwrap_or_default();
        // The things with no place yet, as many rows as they need, three at most.
        let per_row = ((main.width.saturating_sub(2)) / 18).max(1) as usize;
        let strip_rows = unplaced.len().div_ceil(per_row).min(3) as u16;
        let [area, strip] = if unplaced.is_empty() {
            [main, Rect::default()]
        } else {
            Layout::vertical([Constraint::Min(0), Constraint::Length(strip_rows + 2)]).areas(main)
        };
        let area = if self.map["layout"] == "sketch" {
            let s = &self.map["size"];
            fit_aspect(
                area,
                s["w"].as_f64().unwrap_or(1.0),
                s["d"].as_f64().unwrap_or(1.0),
            )
        } else {
            area
        };
        let selected = self.selected();
        let tiles: Vec<Value> = self.map["tiles"].as_array().cloned().unwrap_or_default();
        if tiles.is_empty() && unplaced.is_empty() && self.map["size"]["floor"].is_null() {
            f.render_widget(
                Paragraph::new(t("(nothing here yet)")).fg(pal().muted),
                area,
            );
        }
        if self.map["layout"] == "stack" {
            for band in self.map["bands"].as_array().cloned().unwrap_or_default() {
                let Some(br) = rect_of(&band) else { continue };
                let outer = frac_rect(area, &br);
                let block = Block::bordered()
                    .title(format!(" {} ", title_of(&band)))
                    .border_style(Style::new().fg(pal().furniture));
                let inner = block.inner(outer);
                f.render_widget(block, outer);
                for tile in tiles.iter().filter(|t| t["band"] == band["id"]) {
                    let Some(r) = rect_of(tile) else { continue };
                    let local = [r[0], (r[1] - br[1]) / br[3], r[2], r[3] / br[3]];
                    self.draw_tile(f, frac_rect(inner, &local), tile, selected);
                }
            }
        } else if self.map["layout"] == "sketch" {
            self.draw_plan(f, area, &tiles, selected);
        } else {
            for tile in &tiles {
                let Some(r) = rect_of(tile) else { continue };
                self.draw_tile(f, frac_rect(area, &r), tile, selected);
            }
        }
        if !unplaced.is_empty() {
            let block = Block::bordered()
                .title(t(" Not on the map yet "))
                .border_style(Style::new().fg(pal().muted));
            let inner = block.inner(strip);
            f.render_widget(block, strip);
            let cell_w = inner.width / per_row as u16;
            for (k, tile) in unplaced.iter().enumerate().take(per_row * 3) {
                let cell = Rect::new(
                    inner.x + (k % per_row) as u16 * cell_w,
                    inner.y + (k / per_row) as u16,
                    cell_w.saturating_sub(1),
                    1,
                );
                if cell.y >= inner.y + inner.height {
                    break;
                }
                let mut style = Style::new();
                if tile["id"].as_i64() == selected {
                    style = style.add_modifier(Modifier::REVERSED | Modifier::BOLD);
                }
                f.render_widget(Paragraph::new(Span::styled(title_of(tile), style)), cell);
                if let Some(id) = tile["id"].as_i64() {
                    self.hits.push((cell, id));
                }
            }
        }
        // What the chosen tile holds, since a plan has no room to say it inside.
        if let Some(t) = selected.and_then(|id| self.tile(id)) {
            f.render_widget(Paragraph::new(summary(t)), info);
        }
        let hints = crate::ui::fit_hints(
            vec![
                (0, t("←↑↓→ move")),
                (1, t("Enter go in")),
                (1, t("Backspace go up")),
                (2, t("t show in tree")),
                (3, t("click: select, again: go in")),
                (0, t("Esc close")),
            ],
            bottom.width as usize,
            &self.status,
        );
        f.render_widget(Paragraph::new(hints).fg(pal().muted), bottom);
    }

    /// A floor plan: rooms as floors of their own shape and tone, the place's own floor, what
    /// the plan shows that is no record (doors, windows, a bed), and the things with a place
    /// in it as frames.
    fn draw_plan(&mut self, f: &mut Frame, area: Rect, tiles: &[Value], selected: Option<i64>) {
        let (w, h) = (area.width as usize, area.height as usize);
        if w == 0 || h == 0 {
            return;
        }
        const FLOOR: usize = usize::MAX;
        let shaped: Vec<(usize, Vec<Vec<[f64; 2]>>)> = tiles
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let p: Vec<Vec<[f64; 2]>> = t["shapes"]
                    .as_array()?
                    .iter()
                    .map(polygon)
                    .filter(|p| p.len() >= 3)
                    .collect();
                (!p.is_empty()).then_some((i, p))
            })
            .collect();
        let floor = polygon(&self.map["size"]["floor"]);
        let mut owner: Vec<Option<usize>> = vec![None; w * h];
        for y in 0..h {
            for x in 0..w {
                let p = [(x as f64 + 0.5) / w as f64, (y as f64 + 0.5) / h as f64];
                owner[y * w + x] = shaped
                    .iter()
                    .find(|(_, ps)| ps.iter().any(|q| inside(p, q)))
                    .map(|s| s.0)
                    .or_else(|| (floor.len() >= 3 && inside(p, &floor)).then_some(FLOOR));
            }
        }
        // Tones: rooms that meet across a wall get different ones. A wall is a gap of up to
        // three cells across and two down (a cell is about twice as tall as it is wide).
        let mut tone: HashMap<usize, usize> = HashMap::new();
        for (i, _) in &shaped {
            let mut used = HashSet::new();
            for y in 0..h {
                for x in 0..w {
                    if owner[y * w + x] != Some(*i) {
                        continue;
                    }
                    for (dx, dy) in [(3isize, 0isize), (0, 2), (-3, 0), (0, -2), (2, 0), (-2, 0)] {
                        let (nx, ny) = (x as isize + dx, y as isize + dy);
                        if nx >= 0
                            && ny >= 0
                            && (nx as usize) < w
                            && (ny as usize) < h
                            && let Some(j) = owner[ny as usize * w + nx as usize]
                            && j != *i
                            && let Some(k) = tone.get(&j)
                        {
                            used.insert(*k);
                        }
                    }
                }
            }
            tone.insert(*i, (0..4).find(|k| !used.contains(k)).unwrap_or(0));
        }
        let buf = f.buffer_mut();
        for y in 0..h {
            for x in 0..w {
                let bg = match owner[y * w + x] {
                    None => continue,
                    Some(FLOOR) => pal().floor,
                    Some(i) if tiles[i]["id"].as_i64() == selected && selected.is_some() => {
                        pal().room_chosen
                    }
                    Some(i) => pal().rooms[tone.get(&i).copied().unwrap_or(0)],
                };
                buf[(area.x + x as u16, area.y + y as u16)]
                    .set_char(' ')
                    .set_bg(bg);
            }
        }
        // What the plan shows that is no record here.
        for m in self.map["size"]["marks"]
            .as_array()
            .cloned()
            .unwrap_or_default()
        {
            let Some(fr) = rect_of(&m) else { continue };
            let r = frac_rect(area, &fr);
            match m["kind"].as_str().unwrap_or_default() {
                "door" | "window" => {
                    // A line along the wall it sits in, at the wall's middle: the wall runs
                    // the long way of its footprint, in centimetres.
                    let s = &self.map["size"];
                    let across = fr[2] * s["w"].as_f64().unwrap_or(1.0)
                        >= fr[3] * s["d"].as_f64().unwrap_or(1.0);
                    let (c, fg) = match (m["kind"] == "door", across) {
                        (true, true) => ('┄', pal().furniture),
                        (true, false) => ('┆', pal().furniture),
                        (false, true) => ('═', pal().blue),
                        (false, false) => ('║', pal().blue),
                    };
                    let buf = f.buffer_mut();
                    if across {
                        let y = r.y + r.height / 2;
                        for x in r.x..r.x + r.width {
                            buf[(x, y)].set_char(c).set_fg(fg);
                        }
                    } else {
                        let x = r.x + r.width / 2;
                        for y in r.y..r.y + r.height {
                            buf[(x, y)].set_char(c).set_fg(fg);
                        }
                    }
                }
                _ if r.width >= 4 && r.height >= 3 => {
                    let name = m["name"].as_str().unwrap_or_default();
                    f.render_widget(
                        Block::bordered()
                            .border_style(Style::new().fg(pal().muted))
                            .title(Span::styled(name.to_string(), Style::new().fg(pal().muted))),
                        r,
                    );
                }
                _ => {
                    let buf = f.buffer_mut();
                    for y in r.y..r.y + r.height {
                        for x in r.x..r.x + r.width {
                            buf[(x, y)].set_char('░').set_fg(pal().muted);
                        }
                    }
                }
            }
        }
        // Things with a place in it (furniture in a room) as frames on the floor.
        for tile in tiles {
            if tile["shapes"].is_array() {
                continue;
            }
            let Some(r) = rect_of(tile) else { continue };
            self.draw_tile(f, frac_rect(area, &r), tile, selected);
        }
        // Each room's name where it is widest.
        let buf = f.buffer_mut();
        for (i, _) in &shaped {
            let owned = |x: usize, y: usize| owner[y * w + x] == Some(*i);
            let mut best: Option<(usize, usize, usize)> = None;
            for y in 0..h {
                for x in 0..w {
                    if !owned(x, y) {
                        continue;
                    }
                    let run = |dx: isize, dy: isize| {
                        let (mut n, mut cx, mut cy) = (0, x as isize, y as isize);
                        while cx >= 0
                            && cy >= 0
                            && (cx as usize) < w
                            && (cy as usize) < h
                            && owned(cx as usize, cy as usize)
                        {
                            n += 1;
                            cx += dx;
                            cy += dy;
                        }
                        n
                    };
                    let score = run(-1, 0).min(run(1, 0)).min(2 * run(0, -1).min(run(0, 1)));
                    if best.is_none_or(|b| score > b.2) {
                        best = Some((x, y, score));
                    }
                }
            }
            let Some((bx, by, _)) = best else { continue };
            let t = &tiles[*i];
            let mut lines = vec![(title_of(t), Style::new().bold())];
            if t["items"].as_i64().unwrap_or(0) > 0 {
                lines.push((tf("{} items", &[&t["items"]]), Style::new().fg(pal().qty)));
            }
            for (k, (text, style)) in lines.iter().enumerate() {
                let y = by + k;
                if y >= h || !owned(bx, y) {
                    break;
                }
                // The run of the room on this row, to centre the text in and cut it to.
                let (mut x0, mut x1) = (bx, bx);
                while x0 > 0 && owned(x0 - 1, y) {
                    x0 -= 1;
                }
                while x1 + 1 < w && owned(x1 + 1, y) {
                    x1 += 1;
                }
                let room = x1 - x0 + 1;
                let len = text.chars().count().min(room);
                let start = x0 + (room - len) / 2;
                buf.set_stringn(area.x + start as u16, area.y + y as u16, text, len, *style);
            }
        }
        self.raster = Some((
            area,
            owner
                .iter()
                .map(|o| {
                    o.and_then(|i| {
                        if i == FLOOR {
                            None
                        } else {
                            tiles[i]["id"].as_i64()
                        }
                    })
                })
                .collect(),
        ));
    }

    fn draw_tile(&mut self, f: &mut Frame, r: Rect, tile: &Value, selected: Option<i64>) {
        let id = tile["id"].as_i64();
        let is_sel = id.is_some() && id == selected;
        if let Some(id) = id {
            self.hits.push((r, id));
        }
        let title = title_of(tile);
        let mut body: Vec<Line> = Vec::new();
        // Under a label, what the thing is for: its theme when it has one, else its name.
        if tile["code"].is_string() {
            let what = tile["theme"].as_str().or(tile["name"].as_str());
            body.push(Line::from(what.unwrap_or_default().to_string()));
        }
        let mut facts = vec![Span::styled(
            tf("{} items", &[&tile["items"]]),
            Style::new().fg(pal().qty),
        )];
        if let Some(fill) = tile["fill"].as_i64() {
            facts.push(Span::styled(
                format!("  {fill}%"),
                Style::new().fg(pal().muted),
            ));
        }
        body.push(Line::from(facts));
        if tile["temporary"] == true {
            body.push(Line::styled(
                t("temporary place"),
                Style::new().fg(pal().mark),
            ));
        }
        if tile["unknown"] == true {
            body.push(Line::styled(
                t("contents unknown"),
                Style::new().fg(pal().mark),
            ));
        }
        for s in tile["stacked"].as_array().into_iter().flatten() {
            body.push(Line::styled(
                tf("{} on it", &[&title_of(s)]),
                Style::new().fg(pal().furniture),
            ));
        }
        // Then what is inside, as far as the tile has room.
        let names: Vec<&str> = tile["contents"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !names.is_empty() {
            let more =
                (tile["children"].as_u64().unwrap_or(0) as usize).saturating_sub(names.len());
            let mut text = names.join(" · ");
            if more > 0 {
                text.push_str(&format!(" · +{more}"));
            }
            body.push(Line::from(text));
        }
        let title_style = if is_sel {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::new().fg(pal().code).bold()
        };
        // Too small for a frame: the label alone.
        if r.width < 4 || r.height < 3 {
            f.render_widget(Paragraph::new(Span::styled(title, title_style)), r);
            return;
        }
        let border = if is_sel {
            Style::new().fg(pal().mark).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(pal().muted)
        };
        let block = Block::bordered()
            .border_style(border)
            .title(Span::styled(format!(" {title} "), title_style));
        f.render_widget(
            Paragraph::new(body).wrap(Wrap { trim: true }).block(block),
            r,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractions_become_cells_that_meet_without_gaps() {
        let a = Rect::new(0, 0, 10, 4);
        let l = frac_rect(a, &[0.0, 0.0, 1.0 / 3.0, 1.0]);
        let m = frac_rect(a, &[1.0 / 3.0, 0.0, 1.0 / 3.0, 1.0]);
        assert_eq!(l.x + l.width, m.x);
        assert_eq!(frac_rect(a, &[0.0, 0.5, 1.0, 0.5]), Rect::new(0, 2, 10, 2));
    }

    #[test]
    fn a_sketch_keeps_its_proportions() {
        // 4 m by 2 m in a 100×20 area: 20 rows allow 80 columns.
        let r = fit_aspect(Rect::new(0, 0, 100, 20), 400.0, 200.0);
        assert_eq!((r.width, r.height), (80, 20));
    }
}
