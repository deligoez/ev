//! `:` — a palette that finds any list, place or thing by what is typed, and the way back
//! through what was opened (spec/ui-sidebar.md).

use super::*;

/// What a line of the palette opens.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Goal {
    List(Tab),
    Node(i64),
}

/// The palette while open: what is typed, what it found, which line is chosen.
pub(super) struct Palette {
    pub(super) query: String,
    pub(super) hits: Vec<(Goal, String)>,
    pub(super) at: usize,
}

/// How many lines the palette offers.
const PALETTE_HITS: usize = 12;
/// How far back `Esc` can go.
const BACK_KEPT: usize = 50;

/// How well `text` matches the folded query: whole, at its start, at a word's start, inside.
/// Lower is better; `None` is no match.
fn rank(text: &str, q: &str) -> Option<u8> {
    let t = ev_core::fold(text);
    if t == q {
        Some(0)
    } else if t.starts_with(q) {
        Some(1)
    } else if t
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| w.starts_with(q))
    {
        Some(2)
    } else if t.contains(q) {
        Some(3)
    } else {
        None
    }
}

impl App {
    pub(super) fn open_palette(&mut self) {
        self.palette = Some(Palette {
            query: String::new(),
            hits: Vec::new(),
            at: 0,
        });
        self.find_in_palette();
    }

    /// The lists first, then records: an `#id` (or a bare number) exactly, then codes and
    /// names by how well they match, Turkish letters or not (`kayip` finds `Kayıp`).
    pub(super) fn find_in_palette(&mut self) {
        let Some(query) = self.palette.as_ref().map(|p| p.query.clone()) else {
            return;
        };
        let q = ev_core::fold(&query);
        let mut hits: Vec<(Goal, String)> = Vec::new();
        for tab in sidebar_lists() {
            if q.is_empty() || rank(tab.title(), &q).is_some() {
                let label = match tab.section() {
                    Some(s) => format!("{} › {}", s, tab.title()),
                    None => tab.title().to_string(),
                };
                hits.push((Goal::List(tab), label));
            }
        }
        let node_line = |id: i64, label: &str| {
            let place = self
                .snap
                .parent
                .get(&id)
                .and_then(|p| self.snap.label.get(p))
                .map(|p| format!("  · {p}"))
                .unwrap_or_default();
            (Goal::Node(id), format!("#{id}  {label}{place}"))
        };
        if let Ok(id) = q.trim_start_matches('#').parse::<i64>()
            && let Some(label) = self.snap.label.get(&id)
        {
            hits.push(node_line(id, label));
        }
        if !q.is_empty() && !q.starts_with('#') {
            let mut found: Vec<(u8, usize, i64)> = self
                .snap
                .label
                .iter()
                .filter_map(|(id, l)| rank(l, &q).map(|r| (r, l.chars().count(), *id)))
                .collect();
            found.sort_unstable();
            for (_, _, id) in found {
                if hits.len() >= PALETTE_HITS {
                    break;
                }
                if !hits.iter().any(|h| h.0 == Goal::Node(id)) {
                    hits.push(node_line(id, &self.snap.label[&id]));
                }
            }
        }
        hits.truncate(PALETTE_HITS);
        if let Some(p) = self.palette.as_mut() {
            p.at = p.at.min(hits.len().saturating_sub(1));
            p.hits = hits;
        }
    }

    /// The palette's keys: typing finds, ↑ ↓ choose, Enter opens, Esc closes.
    pub(super) fn palette_key(&mut self, k: KeyEvent) -> Result<()> {
        let Some(p) = self.palette.as_mut() else {
            return Ok(());
        };
        match k.code {
            KeyCode::Esc => self.palette = None,
            KeyCode::Enter => {
                let goal = p.hits.get(p.at).map(|h| h.0);
                self.palette = None;
                match goal {
                    Some(Goal::List(tab)) => {
                        self.go(tab)?;
                        self.pane = Pane::List;
                    }
                    Some(Goal::Node(id)) => {
                        self.jump_to(id)?;
                        self.pane = Pane::List;
                    }
                    None => {}
                }
            }
            KeyCode::Down => p.at = (p.at + 1).min(p.hits.len().saturating_sub(1)),
            KeyCode::Up => p.at = p.at.saturating_sub(1),
            KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                p.query.clear();
                p.at = 0;
                self.find_in_palette();
            }
            KeyCode::Backspace => {
                p.query.pop();
                p.at = 0;
                self.find_in_palette();
            }
            KeyCode::Char(c) => {
                p.query.push(c);
                p.at = 0;
                self.find_in_palette();
            }
            _ => {}
        }
        Ok(())
    }

    /// The palette over the top of the screen.
    pub(super) fn draw_palette(&self, f: &mut Frame, screen: Rect) {
        let Some(p) = &self.palette else { return };
        let width = screen.width.saturating_sub(4).min(80);
        let height = (p.hits.len() as u16 + 3).min(screen.height.saturating_sub(2));
        let area = Rect {
            x: screen.x + (screen.width - width) / 2,
            y: screen.y + 1,
            width,
            height,
        };
        f.render_widget(Clear, area);
        let block = Block::bordered()
            .title(t(" Go to: a list, a place, a thing or #id "))
            .border_style(Style::new().fg(pal().code).bold());
        let inner = block.inner(area);
        f.render_widget(block, area);
        let mut lines = vec![Line::from(format!(": {}▏", p.query))];
        for (i, (_, label)) in p.hits.iter().enumerate() {
            let text = fit(vec![Span::raw(label.clone())], inner.width as usize);
            let line = Line::from(text);
            lines.push(if i == p.at {
                line.style(Style::new().reversed())
            } else {
                line
            });
        }
        if p.hits.is_empty() {
            lines.push(Line::styled(
                t("nothing found"),
                Style::new().fg(pal().muted),
            ));
        }
        f.render_widget(Paragraph::new(lines), inner);
    }

    /// Opens a list, remembering where the person was, for `Esc`.
    pub(super) fn go(&mut self, tab: Tab) -> Result<()> {
        if tab != self.tab {
            self.remember();
        }
        self.switch(tab)
    }

    /// Shows a record in the tree, remembering where the person was, for `Esc`.
    pub(super) fn jump_to(&mut self, id: i64) -> Result<()> {
        if id <= 0 {
            return Ok(());
        }
        self.remember();
        self.reveal(id)
    }

    fn remember(&mut self) {
        let here = (self.tab, self.selected_id());
        if self.back.last() != Some(&here) {
            self.back.push(here);
            if self.back.len() > BACK_KEPT {
                self.back.remove(0);
            }
        }
    }

    /// `Esc`: back to the list and record before the last jump; false when there is none.
    pub(super) fn go_back(&mut self) -> Result<bool> {
        let Some((tab, id)) = self.back.pop() else {
            return Ok(false);
        };
        match (tab, id) {
            (Tab::Tree, Some(id)) if id > 0 => self.reveal(id)?,
            _ => {
                self.switch(tab)?;
                if let Some(i) = id.and_then(|id| self.rows.iter().position(|r| r.id == id)) {
                    self.select(i)?;
                }
            }
        }
        Ok(true)
    }
}
