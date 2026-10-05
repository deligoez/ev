//! Drawing the screen: the tabs, the list, the photo, the details pane and the key hints, or a
//! photo over the whole screen.

use super::*;

impl App {
    /// The current picture over the whole screen, titled with the node, its place among the
    /// node's photos or product images and its own note.
    pub(super) fn draw_fullscreen(&mut self, f: &mut Frame, path: &str) {
        let [main, bottom] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(f.area());
        let node = self.details.as_ref().map(|d| d["node"].clone());
        let name = node.as_ref().map(|n| str_of(n, "name")).unwrap_or_default();
        let title = format!(" {name} ·{}", self.picture_title());
        let block = Block::bordered().title(title);
        let inner = block.inner(main);
        f.render_widget(block, main);
        if self.picker.is_some() {
            self.render_photo(f, path, inner);
        } else {
            f.render_widget(
                Paragraph::new(t(
                    "(this terminal cannot show pictures — press O to open it outside)",
                ))
                .fg(pal().muted),
                inner,
            );
        }
        f.render_widget(
            Paragraph::new(t(
                "[ ] ← → step · r/R rotate · O open outside · Esc/o/click close",
            ))
            .fg(pal().muted),
            bottom,
        );
    }

    /// The series of marked photos over the whole screen, each titled with its note and which
    /// one of the series it is. They are no record: `Esc` hides them, `X` ends the series.
    pub(super) fn draw_overlay(
        &mut self,
        f: &mut Frame,
        path: &str,
        note: Option<String>,
        at: (usize, usize),
    ) {
        let [main, bottom] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(f.area());
        let mut title = match note {
            Some(n) => format!(" {n} "),
            None => t(" Marked photo ").to_string(),
        };
        if at.1 > 1 {
            title.push_str(&format!("· {}/{} ", at.0 + 1, at.1));
        }
        let block = Block::bordered()
            .title(title)
            .border_style(Style::new().fg(pal().lost).bold());
        let inner = block.inner(main);
        f.render_widget(block, main);
        if self.picker.is_some() {
            self.render_photo(f, path, inner);
        } else {
            f.render_widget(
                Paragraph::new(t(
                    "(this terminal cannot show pictures — press O to open it outside)",
                ))
                .fg(pal().muted),
                inner,
            );
        }
        let mut parts = vec![(0, t("Esc/o hide"))];
        if at.1 > 1 {
            parts.push((0, t("[ ] ← → step")));
        }
        parts.push((0, t("X close series")));
        parts.push((1, t("m shows it again")));
        parts.push((2, t("r/R rotate")));
        parts.push((3, t("O open outside")));
        let keys = fit_hints(parts, bottom.width as usize, "");
        f.render_widget(Paragraph::new(keys).fg(pal().muted), bottom);
    }

    pub(super) fn draw(&mut self, f: &mut Frame) {
        if self.fullscreen {
            if let Some(s) = self.overlay.clone() {
                let i = s.at.min(s.files.len().saturating_sub(1));
                if let Some(path) = s.files.get(i) {
                    let note = s.notes.get(i).cloned().flatten();
                    return self.draw_overlay(f, path, note, (i, s.files.len()));
                }
            }
            if let Some(path) = self.current_photo() {
                return self.draw_fullscreen(f, &path);
            }
            self.fullscreen = false;
        }
        if let Some(m) = self.map_view.as_mut() {
            return m.draw(f);
        }
        let [top, body, bottom] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(f.area());
        let brand = format!(" EV {} ", env!("CARGO_PKG_VERSION"));
        let [brand_area, top] = Layout::horizontal([
            Constraint::Length(brand.chars().count() as u16 + 1),
            Constraint::Min(0),
        ])
        .areas(top);
        f.render_widget(
            Paragraph::new(Span::styled(brand, pal().brand.bold())),
            brand_area,
        );
        self.tabs_area = top;
        let tabs = Tabs::new(
            tab_titles()
                .iter()
                .enumerate()
                .map(|(i, t)| format!("{} {t}", i + 1)),
        )
        .select(self.tab.index())
        .highlight_style(Style::new().bold().reversed());
        f.render_widget(tabs, top);

        let [left, right] = Layout::horizontal([
            Constraint::Percentage(self.split),
            Constraint::Percentage(100 - self.split),
        ])
        .areas(body);
        self.list_area = left;
        self.body_area = body;
        self.right_area = right;
        // The divider being dragged lights up on both of its borders.
        let edge = |which: Drag| {
            if self.drag == Some(which) {
                Style::new().fg(pal().code).bold()
            } else {
                Style::new()
            }
        };
        let columns_edge = edge(Drag::Columns);
        let photo_edge = edge(Drag::Photo);
        // Inside the list's borders.
        let row_width = left.width.saturating_sub(2) as usize;
        let now = Instant::now();
        self.changed
            .retain(|_, t| now.duration_since(*t) < HIGHLIGHT_FOR);
        let flash = pal().flash;
        let items: Vec<ListItem> = self
            .rows
            .iter()
            .map(|r| {
                let marker = match (r.expandable, r.expanded) {
                    (true, true) => "▾ ",
                    (true, false) => "▸ ",
                    _ => "  ",
                };
                let mut spans = vec![
                    Span::raw("  ".repeat(r.depth)),
                    Span::styled(marker, Style::new().fg(pal().muted)),
                ];
                spans.extend(r.spans.iter().cloned());
                let mut spans = fit(spans, row_width);
                if self.changed.contains_key(&r.id) {
                    spans = spans.into_iter().map(|s| s.patch_style(flash)).collect();
                }
                ListItem::new(Line::from(spans))
            })
            .collect();
        let title = if self.tab == Tab::Search && self.has_search() {
            tf(" Search: \"{}\" · ✕ clear (x) ", &[&self.query])
        } else if self.tab == Tab::Plan {
            self.plan_title.clone()
        } else {
            format!(" {} ", tab_titles()[self.tab.index()])
        };
        let mut block = Block::bordered().title(title).border_style(columns_edge);
        // The tree's keys on its own bottom edge, as the details' H/L are on theirs, at its
        // right end, so they stand apart from the details' keys beside them.
        if self.tab == Tab::Tree {
            block = block.title_bottom(
                Line::from(t(
                    " → ← open/close · d two levels · e/c all below · C close all ",
                ))
                .fg(pal().muted)
                .right_aligned(),
            );
        }
        let list = List::new(items)
            .block(block)
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
        f.render_stateful_widget(list, left, &mut self.state);

        self.photo_area = Rect::default();
        let text_area = match (self.current_photo(), self.picker.is_some()) {
            (Some(path), true) => {
                let [img_area, rest] = Layout::vertical([
                    Constraint::Percentage(self.photo_split),
                    Constraint::Min(6),
                ])
                .areas(right);
                // The picture's own note says what it shows (a drawer's final state, the inside
                // of a bag), which the picture alone may not; it comes before the key hints, so
                // a narrow pane cuts the hints, not the note.
                let block = Block::bordered()
                    .title(format!(
                        "{}{}",
                        self.picture_title(),
                        t("([ ] step · r rotate · o full screen · O open outside) ")
                    ))
                    .border_style(columns_edge.patch(photo_edge));
                let inner = block.inner(img_area);
                self.photo_area = img_area;
                f.render_widget(block, img_area);
                self.render_photo(f, &path, inner);
                rest
            }
            _ => right,
        };
        // Tabs whose lines are clicked keep one line per row (cut with …) so a click lands on
        // the line it points at; the others wrap.
        let clickable = self.tab != Tab::Settings
            && self.details.is_some()
            && matches!(
                self.shown_detail_tab(),
                DetailTab::Photos
                    | DetailTab::Documents
                    | DetailTab::Contents
                    | DetailTab::History
                    | DetailTab::Grid
            );
        self.grid_hit = match &self.details {
            Some(v) if clickable && self.shown_detail_tab() == DetailTab::Grid => {
                let g = if v["grid"].is_object() {
                    &v["grid"]
                } else {
                    &v["parent_grid"]
                };
                g.is_object().then(|| grid_map(g))
            }
            _ => None,
        };
        let text = if self.tab == Tab::Settings {
            self.settings_text()
        } else if clickable {
            let width = text_area.width.saturating_sub(2) as usize;
            let lines = self.details_text().lines.into_iter();
            Text::from(
                lines
                    .map(|l| Line::from(fit(l.spans, width)).style(l.style))
                    .collect::<Vec<_>>(),
            )
        } else {
            self.details_text()
        };
        self.detail_targets = if clickable {
            self.detail_targets()
        } else {
            Vec::new()
        };
        // The pane scrolls (J/K, the mouse wheel); its bottom edge says so when there is more.
        self.details_area = text_area;
        self.detail_lines = text.lines.len();
        let inner = text_area.height.saturating_sub(2) as usize;
        let max = self.detail_lines.saturating_sub(1) as u16;
        self.detail_scroll = self.detail_scroll.min(max);
        let scrolls = self.detail_lines > inner || self.detail_scroll > 0;
        let mut block = Block::bordered().border_style(photo_edge.patch(columns_edge));
        self.detail_tab_hits = (u16::MAX, Vec::new());
        if self.tab == Tab::Settings || self.details.is_none() {
            block = block.title(t(" Details "));
        } else {
            // The tabs are the title: only those this node has something for, the one shown
            // stands out, and a count says how much each holds.
            let shown = self.shown_detail_tab();
            let mut spans = vec![Span::raw(" ")];
            let mut x = text_area.x + 2;
            let mut hits = Vec::new();
            let open = DetailTab::ALL
                .into_iter()
                .filter(|&t| self.tab_available(t));
            for (i, tab) in open.enumerate() {
                if i > 0 {
                    spans.push(Span::styled(" · ", Style::new().fg(pal().muted)));
                    x += 3;
                }
                let images = self.product_images().len();
                let badge = match self.tab_badge(tab).filter(|_| tab != DetailTab::Summary) {
                    // The person's photos and the product images counted apart: `2+3`.
                    Some(n) if tab == DetailTab::Photos && images > 0 => {
                        format!(" {}+{images}", n - images)
                    }
                    Some(n) => format!(" {n}"),
                    None => String::new(),
                };
                let style = if tab == shown {
                    Style::new().bold().reversed()
                } else if !self.tab_available(tab) {
                    Style::new().fg(pal().muted)
                } else if tab == DetailTab::Suggestions && !badge.is_empty() {
                    Style::new().fg(pal().mark)
                } else {
                    Style::new()
                };
                let span = Span::styled(format!("{}{badge}", tab.title()), style);
                let w = span.width() as u16;
                hits.push((x, x + w, tab));
                x += w;
                spans.push(span);
            }
            spans.push(Span::raw(" "));
            self.detail_tab_hits = (text_area.y, hits);
            block = block.title(Line::from(spans));
        }
        let keys = match (self.tab == Tab::Settings, scrolls) {
            (true, false) => String::new(),
            (true, true) => t(" J/K scroll ").to_string(),
            (false, false) => t(" H/L tabs ").to_string(),
            (false, true) => t(" H/L tabs · J/K scroll ").to_string(),
        };
        if !keys.is_empty() {
            block = block.title_bottom(Line::from(keys).fg(pal().muted));
        }
        let mut details = Paragraph::new(text)
            .block(block)
            .scroll((self.detail_scroll, 0));
        if !clickable {
            details = details.wrap(Wrap { trim: false });
        }
        f.render_widget(details, text_area);

        let help = if self.searching {
            tf(
                "Search: {}▏  (Enter search · Esc clear/cancel · Ctrl+U clear)",
                &[&self.query],
            )
        } else if self.tab == Tab::Settings {
            tf(
                "↑↓ move · Enter/→ next option · ← previous option · Tab/1-9 tabs · q quit    {}",
                &[&self.status],
            )
        } else {
            self.help_line(bottom.width as usize, scrolls)
        };
        f.render_widget(Paragraph::new(help).fg(pal().muted), bottom);
    }

    /// The key hints for what is on screen, most useful first: the keys of this tab, the details
    /// keys when they do something here, the photo keys when there is a photo. What does not fit
    /// the width is left out from the least useful end; the status message always stays.
    pub(super) fn help_line(&self, width: usize, scrolls: bool) -> String {
        // (priority, text): 0 always, higher numbers go first when space runs out.
        let mut parts: Vec<(u8, &str)> = vec![(0, t("↑↓ move"))];
        match self.tab {
            // The tree's keys are on its own bottom edge.
            Tab::Tree => {}
            Tab::Search if self.has_search() => {
                parts.push((1, t("Enter show in tree")));
                parts.push((1, t("x clear")));
            }
            Tab::Plan => parts.push((1, t("Enter open/close section"))),
            _ => parts.push((1, t("Enter show in tree"))),
        }
        // The hidden series is one key away; said early, so a narrow screen keeps it.
        if self.last_overlay.is_some() {
            parts.push((1, t("m marked photo series")));
            parts.push((1, t("X close series")));
        }
        parts.push((2, t("/ search")));
        parts.push((2, t("M map")));
        if self.details.as_ref().is_some_and(|d| {
            d["thing"]["elsewhere"]
                .as_array()
                .is_some_and(|e| !e.is_empty())
        }) {
            parts.push((1, t("p next place of this thing")));
        }
        if self.details.is_some() {
            parts.push((2, t("H/L details tabs")));
            parts.push((4, t("E empty · y copy · + wide")));
            if scrolls {
                parts.push((3, t("J/K scroll")));
            }
        }
        if self.details.is_some() && self.shown_detail_tab() == DetailTab::Documents {
            parts.push((1, t("[ ] O open a document")));
        } else if self.picture_count() > 0 {
            parts.push((3, t("[ ] o photos")));
        }
        parts.push((4, t("Tab/1-9 tabs")));
        parts.push((5, t("< > { } or drag: resize")));
        parts.push((0, t("q quit")));
        fit_hints(parts, width, &self.status)
    }

    /// The person's own photos of the selected node: what "as it is now" and a stale photo are
    /// reckoned from.
    pub(super) fn photo_count(&self) -> usize {
        self.details
            .as_ref()
            .and_then(|d| d["node"]["photos"].as_array())
            .map_or(0, Vec::len)
    }

    /// The selected thing's product images (documents of kind `image`), with each one's note
    /// or file name and the purchase it came with: the product as sold, shown after the photos
    /// and never counted as one.
    pub(super) fn product_images(&self) -> Vec<Value> {
        self.details
            .as_ref()
            .and_then(|d| d["documents"].as_array())
            .into_iter()
            .flatten()
            .filter(|d| d["kind"] == "image" && d["file"].is_string())
            .cloned()
            .collect()
    }

    /// What `[` `]` step through: the photos, then the product images.
    pub(super) fn picture_count(&self) -> usize {
        self.photo_count() + self.product_images().len()
    }

    /// The picture shown: the one picked, or the newest photo (the place as it is now) when
    /// none was, or the first product image when there is no photo.
    pub(super) fn picture_idx(&self) -> usize {
        let (own, all) = (self.photo_count(), self.picture_count());
        match self.photo_idx {
            usize::MAX if own > 0 => own - 1,
            usize::MAX => 0,
            i => i.min(all.saturating_sub(1)),
        }
    }

    /// ` Photo 2/3 ` or ` Product image 1/2 `, then the picture's note when it has one.
    pub(super) fn picture_title(&self) -> String {
        let (own, idx) = (self.photo_count(), self.picture_idx());
        if idx < own {
            let note = self
                .photos
                .get(idx)
                .and_then(|p| p["note"].as_str())
                .map(|n| format!("· {n} "))
                .unwrap_or_default();
            return format!("{}{note}", tf(" Photo {}/{} ", &[&(idx + 1), &own]));
        }
        let images = self.product_images();
        let note = images
            .get(idx - own)
            .and_then(|d| d["note"].as_str().or(d["original_name"].as_str()))
            .map(|n| format!("· {n} "))
            .unwrap_or_default();
        format!(
            "{}{note}",
            tf(" Product image {}/{} ", &[&(idx - own + 1), &images.len()])
        )
    }

    pub(super) fn current_photo(&self) -> Option<String> {
        let own = self.details.as_ref()?["node"]["photos"].as_array()?.clone();
        let idx = self.picture_idx();
        match own.get(idx) {
            Some(p) => p.as_str().map(str::to_string),
            None => self
                .product_images()
                .get(idx - own.len())
                .and_then(|d| d["file"].as_str().map(str::to_string)),
        }
    }

    /// Keeps a decoded photo, letting the oldest go past `DECODED_KEPT`.
    fn keep_decoded(&mut self, path: String, img: Option<image::DynamicImage>) {
        if self.decoded.insert(path.clone(), img).is_none() {
            self.decoded_order.push_back(path);
        }
        while self.decoded_order.len() > DECODED_KEPT {
            if let Some(old) = self.decoded_order.pop_front() {
                self.decoded.remove(&old);
            }
        }
    }

    /// Takes the photos the decoder finished; the frame drawn next shows them.
    pub(super) fn take_decoded(&mut self) {
        let Some(d) = self.decoder.as_mut() else {
            return;
        };
        let mut done = Vec::new();
        while let Ok(r) = d.done.try_recv() {
            d.pending.remove(&r.0);
            done.push(r);
        }
        for (path, img) in done {
            self.keep_decoded(path, img);
        }
    }

    /// Decodes once per path (downscaled), and re-encodes for the terminal only when the photo,
    /// its rotation or its area changes. A photo not decoded yet goes to the decoder, and the
    /// pane says so until it is back.
    pub(super) fn render_photo(&mut self, f: &mut Frame, path: &str, area: Rect) {
        if self.picker.is_none() {
            return;
        }
        let turns = self.rotation.get(path).copied().unwrap_or(0);
        let fresh =
            !matches!(&self.shown, Some((p, t, a, _)) if p == path && *t == turns && *a == area);
        if fresh {
            let img = match self.decoded.get(path) {
                Some(img) => img.clone(),
                None => match self.decoder.as_mut() {
                    Some(d) => {
                        if d.pending.insert(path.to_string()) {
                            let _ = d.jobs.send(path.to_string());
                        }
                        f.render_widget(
                            Paragraph::new(t("(opening the photo…)")).fg(pal().muted),
                            area,
                        );
                        return;
                    }
                    None => {
                        let img = crate::ui::decode_photo(path);
                        self.keep_decoded(path.to_string(), img.clone());
                        img
                    }
                },
            };
            let Some(picker) = &self.picker else { return };
            self.shown = img.and_then(|img| {
                let img = match turns {
                    1 => img.rotate90(),
                    2 => img.rotate180(),
                    3 => img.rotate270(),
                    _ => img,
                };
                picker
                    .new_protocol(img, area.into(), Resize::Fit(None))
                    .ok()
                    .map(|p| (path.to_string(), turns, area, p))
            });
        }
        match &self.shown {
            // Fitted pictures keep their shape, so a portrait photo in a wide pane is centred
            // rather than left against the border.
            Some((p, _, _, proto)) if p == path => {
                let size = proto.size();
                let w = size.width.min(area.width);
                let h = size.height.min(area.height);
                let at = Rect::new(area.x + (area.width - w) / 2, area.y, w, h);
                f.render_widget(Image::new(proto), at)
            }
            _ => f.render_widget(
                Paragraph::new(t("(the photo could not be opened)")).fg(pal().muted),
                area,
            ),
        }
    }
}
