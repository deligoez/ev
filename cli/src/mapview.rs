//! The map in `ev ui` (`M`): a place's contents drawn where they are — on its grid, on its
//! sketch, or as a stack of furniture front on — to walk with the arrows and go into with Enter.
//!
//! Like the rest of `ev ui` it never writes: each level is `ev map` of the place shown.

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
        let Some(&(_, id)) = hit else {
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
        let [top, main, bottom] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
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
        let [area, strip] = if unplaced.is_empty() {
            [main, Rect::default()]
        } else {
            Layout::vertical([Constraint::Min(0), Constraint::Length(3)]).areas(main)
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
        if tiles.is_empty() && unplaced.is_empty() {
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
            let fits = (inner.width / 16).max(1) as usize;
            let shown = unplaced.len().min(fits);
            let cells =
                Layout::horizontal(vec![Constraint::Ratio(1, shown as u32); shown]).split(inner);
            for (tile, cell) in unplaced.iter().zip(cells.iter()) {
                let mut style = Style::new();
                if tile["id"].as_i64() == selected {
                    style = style.add_modifier(Modifier::REVERSED | Modifier::BOLD);
                }
                let text = format!("{}  {}", title_of(tile), tf("{} items", &[&tile["items"]]));
                f.render_widget(Paragraph::new(Span::styled(text, style)), *cell);
                if let Some(id) = tile["id"].as_i64() {
                    self.hits.push((*cell, id));
                }
            }
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
