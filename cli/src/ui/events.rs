//! What the keyboard, the mouse and the terminal's own answers do.

use super::*;

impl App {
    pub(super) fn key(&mut self, k: KeyEvent) -> Result<()> {
        if self.fullscreen {
            let photos = self.overlay.is_none();
            match k.code {
                KeyCode::Esc | KeyCode::Char('q' | 'o') => self.close_fullscreen(),
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.quit = true
                }
                KeyCode::Char(']') | KeyCode::Right | KeyCode::Char('l') if photos => {
                    self.step_photo(1)
                }
                KeyCode::Char('[') | KeyCode::Left | KeyCode::Char('h') if photos => {
                    self.step_photo(-1)
                }
                KeyCode::Char(']') | KeyCode::Right | KeyCode::Char('l') => self.step_overlay(1),
                KeyCode::Char('[') | KeyCode::Left | KeyCode::Char('h') => self.step_overlay(-1),
                KeyCode::Char('O') => self.open_external(),
                KeyCode::Char('r') => self.rotate(1),
                KeyCode::Char('R') => self.rotate(3),
                _ => {}
            }
            return Ok(());
        }
        if let Some(m) = self.map_view.as_mut() {
            match m.key(&self.inv, k)? {
                Outcome::Stay => {}
                Outcome::Close => self.map_view = None,
                Outcome::Quit => self.quit = true,
                Outcome::Reveal(id) => {
                    self.map_view = None;
                    self.reveal(id)?;
                }
            }
            return Ok(());
        }
        if self.searching {
            match k.code {
                // Esc empties a half-typed query first, and closes the box on an empty one.
                KeyCode::Esc if !self.query.is_empty() => self.query.clear(),
                KeyCode::Esc => self.searching = false,
                KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.query.clear()
                }
                KeyCode::Enter => {
                    self.searching = false;
                    self.run_search()?;
                    self.switch(Tab::Search)?;
                }
                KeyCode::Backspace => {
                    self.query.pop();
                }
                KeyCode::Char(c) => self.query.push(c),
                _ => {}
            }
            return Ok(());
        }
        match k.code {
            // On the Ara tab, Esc and `x` clear the search instead of quitting.
            KeyCode::Esc | KeyCode::Char('x') if self.tab == Tab::Search && self.has_search() => {
                self.clear_search()?
            }
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => self.quit = true,
            KeyCode::Down | KeyCode::Char('j') => self.step(1)?,
            KeyCode::Up | KeyCode::Char('k') => self.step(-1)?,
            KeyCode::Char('J') => self.scroll_details(3),
            KeyCode::Char('K') => self.scroll_details(-3),
            KeyCode::Char('+') => self.toggle_wide(),
            KeyCode::Char('y') => self.copy(),
            KeyCode::Char('p') => self.next_portion()?,
            KeyCode::Char('E') => self.show_empty = !self.show_empty,
            KeyCode::Char('H') => self.step_detail_tab(-1),
            KeyCode::Char('L') => self.step_detail_tab(1),
            KeyCode::Char('<') => self.resize(Drag::Columns, -5),
            KeyCode::Char('>') => self.resize(Drag::Columns, 5),
            KeyCode::Char('{') => self.resize(Drag::Photo, -5),
            KeyCode::Char('}') => self.resize(Drag::Photo, 5),
            KeyCode::PageDown => self.step(15)?,
            KeyCode::PageUp => self.step(-15)?,
            KeyCode::Home | KeyCode::Char('g') => self.select(0)?,
            KeyCode::End | KeyCode::Char('G') => self.select(usize::MAX)?,
            KeyCode::Tab => self.switch(Tab::from_index(self.tab.index() + 1))?,
            KeyCode::BackTab => {
                self.switch(Tab::from_index(self.tab.index() + Tab::ALL.len() - 1))?
            }
            KeyCode::Char(c @ '1'..='9') => {
                self.switch(Tab::from_index(c as usize - '1' as usize))?
            }
            KeyCode::Char('m') => self.reopen_marked(),
            KeyCode::Char('M') => self.open_map(),
            KeyCode::Char('/') => {
                self.searching = true;
                self.query.clear();
            }
            // On the Documents tab, [ ] pick a document or link and O opens it outside.
            KeyCode::Char(']') if self.shown_detail_tab() == DetailTab::Documents => {
                self.step_document(1)
            }
            KeyCode::Char('[') if self.shown_detail_tab() == DetailTab::Documents => {
                self.step_document(-1)
            }
            KeyCode::Char('O' | 'o') if self.shown_detail_tab() == DetailTab::Documents => {
                self.open_target(self.document_targets().get(self.doc_idx).copied())
            }
            KeyCode::Char(']') => self.step_photo(1),
            KeyCode::Char('[') => self.step_photo(-1),
            KeyCode::Char('o') => self.fullscreen = self.current_photo().is_some(),
            KeyCode::Char('O') => self.open_external(),
            KeyCode::Char('r') => self.rotate(1),
            KeyCode::Char('R') => self.rotate(3),
            KeyCode::Right | KeyCode::Char('l' | ' ') | KeyCode::Enter
                if self.tab == Tab::Settings =>
            {
                if let Some(id) = self.selected_id() {
                    self.cycle_setting(id, true)?;
                }
            }
            KeyCode::Left | KeyCode::Char('h') if self.tab == Tab::Settings => {
                if let Some(id) = self.selected_id() {
                    self.cycle_setting(id, false)?;
                }
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => {
                if let Some(id) = self.selected_id() {
                    if id < 0 {
                        if self.collapsed.contains(&id) || k.code == KeyCode::Enter {
                            self.toggle_section(id)?;
                        }
                    } else if id == 0 {
                        // A line of the Statistics tab that names no record.
                    } else if self.tab == Tab::Tree {
                        self.expanded.insert(id);
                        self.rebuild()?;
                    } else {
                        self.reveal(id)?;
                    }
                }
            }
            KeyCode::Left | KeyCode::Char('h') if matches!(self.tab, Tab::Plan | Tab::Stats) => {
                // On an item, go up to its section; on an open section, close it.
                let Some(i) = self.state.selected() else {
                    return Ok(());
                };
                if let Some(h) = (0..=i).rev().find(|&j| self.rows[j].id < 0) {
                    let id = self.rows[h].id;
                    if h == i {
                        if !self.collapsed.contains(&id) {
                            self.toggle_section(id)?;
                        }
                    } else {
                        self.select(h)?;
                    }
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if let (Tab::Tree, Some(id)) = (self.tab, self.selected_id()) {
                    if self.expanded.remove(&id) {
                        self.rebuild()?;
                    } else if let Some(p) = self.snap.parent.get(&id).copied() {
                        self.reveal(p)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn mouse(&mut self, m: MouseEvent) -> Result<()> {
        let inside = |r: Rect| {
            m.column >= r.x && m.column < r.x + r.width && m.row >= r.y && m.row < r.y + r.height
        };
        if self.fullscreen {
            match m.kind {
                MouseEventKind::ScrollDown if self.overlay.is_none() => self.step_photo(1),
                MouseEventKind::ScrollUp if self.overlay.is_none() => self.step_photo(-1),
                // A marked photo closes with Esc only: a stray click would lose what was shown.
                MouseEventKind::Down(MouseButton::Left) if self.overlay.is_none() => {
                    self.close_fullscreen()
                }
                _ => {}
            }
            return Ok(());
        }
        if let Some(v) = self.map_view.as_mut() {
            if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                v.click(&self.inv, m.column, m.row)?;
            }
            return Ok(());
        }
        // The dividers: the column between the list and the right side, and the row under the
        // photo. Dragging one resizes; a double click puts it back.
        let body = self.body_area;
        let right = self.right_area;
        let on_columns = m.row >= body.y
            && m.row < body.y + body.height
            && right.x > 0
            && (m.column == right.x || m.column + 1 == right.x);
        let photo = self.photo_area;
        let on_photo = photo.height > 0
            && m.row + 1 == photo.y + photo.height
            && m.column >= right.x
            && m.column < right.x + right.width;
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) if on_columns || on_photo => {
                let which = if on_columns {
                    Drag::Columns
                } else {
                    Drag::Photo
                };
                let now = Instant::now();
                if self
                    .divider_click
                    .is_some_and(|t| now.duration_since(t) < DOUBLE_CLICK)
                {
                    match which {
                        Drag::Columns => self.split = SPLIT,
                        Drag::Photo => self.photo_split = PHOTO_SPLIT,
                    }
                    self.divider_click = None;
                    self.drag = None;
                } else {
                    self.divider_click = Some(now);
                    self.drag = Some(which);
                }
                return Ok(());
            }
            MouseEventKind::Drag(MouseButton::Left) if self.drag.is_some() => {
                // A press that moved was a drag, so it cannot be half of a double click.
                self.divider_click = None;
                match self.drag {
                    Some(Drag::Columns) if body.width > 0 => {
                        let left = (m.column.saturating_sub(body.x) + 1) as u32;
                        self.split = ((left * 100 / body.width as u32) as u16).clamp(20, 80);
                    }
                    Some(Drag::Photo) if right.height > 0 => {
                        let top = (m.row.saturating_sub(right.y) + 1) as u32;
                        self.photo_split = ((top * 100 / right.height as u32) as u16).clamp(15, 85);
                    }
                    _ => {}
                }
                return Ok(());
            }
            MouseEventKind::Up(MouseButton::Left) if self.drag.is_some() => {
                self.drag = None;
                return Ok(());
            }
            MouseEventKind::Down(MouseButton::Left) if m.row == self.detail_tab_hits.0 => {
                let hit = self
                    .detail_tab_hits
                    .1
                    .iter()
                    .find(|(a, b, _)| m.column >= *a && m.column < *b)
                    .map(|h| h.2);
                if let Some(tab) = hit {
                    if self.tab_available(tab) {
                        self.detail_tab = tab;
                        self.detail_scroll = 0;
                    }
                    return Ok(());
                }
            }
            // A box on the grid opens in the tree. The grid's first frame line is
            // the sixth line of the pane: name, place, blank, title, column letters.
            MouseEventKind::Down(MouseButton::Left)
                if inside(self.details_area)
                    && m.row > self.details_area.y
                    && self.grid_hit.is_some() =>
            {
                let line = (m.row - self.details_area.y - 1) as usize + self.detail_scroll as usize;
                let x = (m.column - self.details_area.x - 1) as usize;
                let hit = line
                    .checked_sub(5)
                    .and_then(|y| grid_box_at(self.grid_hit.as_deref().unwrap_or_default(), x, y));
                if let Some(id) = hit {
                    return self.reveal(id);
                }
                return Ok(());
            }
            // A line of the Photos, Contents or History tab: show that photo, or open that
            // thing in the tree.
            MouseEventKind::Down(MouseButton::Left)
                if inside(self.details_area)
                    && m.row > self.details_area.y
                    && !self.detail_targets.is_empty() =>
            {
                let line = (m.row - self.details_area.y - 1) as usize + self.detail_scroll as usize;
                match self.detail_targets.get(line).copied().flatten() {
                    Some(Target::Node(id)) if self.snap.label.contains_key(&id) => {
                        return self.reveal(id);
                    }
                    Some(Target::Node(id)) => {
                        self.status = tf("#{} is no longer in the tree (gone)", &[&id]);
                        return Ok(());
                    }
                    Some(Target::Photo(i)) => {
                        self.photo_idx = i;
                        return Ok(());
                    }
                    Some(t @ (Target::Document(_) | Target::Link(_))) => {
                        self.doc_idx = self
                            .document_targets()
                            .iter()
                            .position(|x| *x == t)
                            .unwrap_or(0);
                        self.open_target(Some(t));
                        return Ok(());
                    }
                    None => {}
                }
            }
            _ => {}
        }
        match m.kind {
            MouseEventKind::ScrollDown if inside(self.photo_area) => {
                self.step_photo(1);
                Ok(())
            }
            MouseEventKind::ScrollUp if inside(self.photo_area) => {
                self.step_photo(-1);
                Ok(())
            }
            MouseEventKind::Down(MouseButton::Left) if inside(self.photo_area) => {
                self.fullscreen = true;
                Ok(())
            }
            // The list's top border carries the "✕ temizle" of a search.
            MouseEventKind::Down(MouseButton::Left)
                if self.tab == Tab::Search
                    && self.has_search()
                    && m.row == self.list_area.y
                    && inside(self.list_area) =>
            {
                self.clear_search()
            }
            MouseEventKind::ScrollDown if inside(self.details_area) => {
                self.scroll_details(3);
                Ok(())
            }
            MouseEventKind::ScrollUp if inside(self.details_area) => {
                self.scroll_details(-3);
                Ok(())
            }
            MouseEventKind::ScrollDown if inside(self.list_area) => self.step(3),
            MouseEventKind::ScrollUp if inside(self.list_area) => self.step(-3),
            MouseEventKind::Down(MouseButton::Left) if inside(self.tabs_area) => {
                match tab_at(m.column.saturating_sub(self.tabs_area.x)) {
                    Some(t) => self.switch(t),
                    None => Ok(()),
                }
            }
            MouseEventKind::Down(MouseButton::Left) if inside(self.list_area) => {
                // The list block has a one-cell border on every side.
                if m.row == self.list_area.y
                    || m.row + 1 >= self.list_area.y + self.list_area.height
                {
                    return Ok(());
                }
                let i = self.state.offset() + (m.row - self.list_area.y - 1) as usize;
                if i >= self.rows.len() {
                    return Ok(());
                }
                let now = Instant::now();
                let again = self.state.selected() == Some(i)
                    || self
                        .last_click
                        .is_some_and(|(j, t)| j == i && now.duration_since(t) < DOUBLE_CLICK);
                self.last_click = Some((i, now));
                if again {
                    self.activate()
                } else {
                    self.select(i)
                }
            }
            _ => Ok(()),
        }
    }

    pub(super) fn handle(&mut self, input: Input) -> Result<()> {
        match input {
            Input::Key(k) => self.key(k),
            Input::Mouse(m) => self.mouse(m),
            Input::Appearance { mode, notified } => self.on_appearance(mode, notified),
            Input::Graphics(g) => {
                self.on_graphics(g);
                Ok(())
            }
        }
    }

    /// Gathers the picture-protocol answers; the status report ends them, and the picker built
    /// from them replaces the half-block one the screen started with.
    pub(super) fn on_graphics(&mut self, g: Graphics) {
        let Some(probe) = self.probe.as_mut() else {
            return;
        };
        match g {
            Graphics::Kitty => probe.kitty = true,
            Graphics::Attributes { sixel } => probe.sixel |= sixel,
            Graphics::CellSize { width, height } => probe.cell = Some((width, height)),
            Graphics::Done => {
                self.picker = Some(probe.picker());
                self.probe = None;
                self.shown = None;
            }
        }
    }

    /// Asks a terminal that never sent a mode 2031 report for its background again, so
    /// Automatic still follows a switch there, a few seconds late.
    pub(super) fn poll_background(&mut self) {
        if self.notified
            || self.prefs.theme != ThemePref::Auto
            || self.last_background_query.elapsed() < BACKGROUND_POLL
        {
            return;
        }
        self.last_background_query = Instant::now();
        send(input::ASK_BACKGROUND);
    }
}
