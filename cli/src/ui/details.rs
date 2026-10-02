//! The details pane: what is loaded for the selected node, its tabs and what each draws.

use super::*;

/// `text` cut into lines of at most `width` characters at spaces; a word longer than a line
/// is cut where the line ends.
fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let mut word = word.to_string();
            let fits = line.chars().count() + usize::from(!line.is_empty()) + word.chars().count();
            if fits > width && !line.is_empty() {
                out.push(std::mem::take(&mut line));
            }
            while word.chars().count() > width {
                let head: String = word.chars().take(width).collect();
                word = word.chars().skip(width).collect();
                out.push(head);
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(&word);
        }
        out.push(line);
    }
    out
}

impl App {
    pub(super) fn load_details(&mut self) -> Result<()> {
        let before = self.details.as_ref().map(|d| d["node"]["id"].clone());
        let now = self.selected_id().map(|i| serde_json::json!(i));
        if before != now {
            // The newest photo is the one that shows the place as it is now; older ones stay a
            // step back with `[`.
            self.photo_idx = usize::MAX;
            self.doc_idx = 0;
            self.detail_scroll = 0;
        }
        self.details = match self.selected_id() {
            Some(id) if id > 0 => Some(self.inv.show(&id.to_string(), true)?),
            _ => None,
        };
        self.hints = match self.details.clone() {
            Some(v) if self.tab != Tab::Settings => {
                let mut lines = self.placement_hints(&v);
                lines.extend(self.theme_hints(&v));
                lines
            }
            _ => Vec::new(),
        };
        self.history = match self.selected_id() {
            Some(id) if id > 0 && self.tab != Tab::Settings => {
                self.inv.history_with_contents(&id.to_string()).ok()
            }
            _ => None,
        };
        self.photos = match self.selected_id() {
            Some(id) if id > 0 && self.photo_count() > 0 => self
                .inv
                .photo_list(&id.to_string())
                .ok()
                .and_then(|v| v["photos"].as_array().cloned())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        Ok(())
    }

    /// Whether the selected node has anything for a details tab.
    pub(super) fn tab_available(&self, tab: DetailTab) -> bool {
        let Some(v) = &self.details else {
            return tab == DetailTab::Summary;
        };
        match tab {
            DetailTab::Summary => true,
            DetailTab::Photos => self.photo_count() > 0,
            DetailTab::Documents => self.tab_badge(DetailTab::Documents).is_some(),
            DetailTab::Grid => v["grid"].is_object() || v["parent_grid"].is_object(),
            DetailTab::Contents => v["children"].as_array().is_some_and(|c| !c.is_empty()),
            DetailTab::Suggestions => !self.hints.is_empty(),
            DetailTab::History => self.tab_badge(DetailTab::History).is_some(),
        }
    }

    /// The count a tab title carries: things inside, suggested moves, events.
    pub(super) fn tab_badge(&self, tab: DetailTab) -> Option<usize> {
        let n = match tab {
            DetailTab::Photos => self.photo_count(),
            DetailTab::Documents => self.document_targets().len(),
            DetailTab::Contents => self.details.as_ref()?["children"].as_array()?.len(),
            DetailTab::Suggestions => self
                .hints
                .iter()
                .filter(|l| {
                    l.spans
                        .first()
                        .is_some_and(|s| s.content.starts_with("  →"))
                })
                .count(),
            DetailTab::History => self.history.as_ref()?["events"].as_array()?.len(),
            _ => 0,
        };
        (n > 0).then_some(n)
    }

    /// The chosen tab when the node has something for it, else the summary. The choice is
    /// kept, so stepping through a drawer's boxes stays on their grid.
    pub(super) fn shown_detail_tab(&self) -> DetailTab {
        if self.tab_available(self.detail_tab) {
            self.detail_tab
        } else {
            DetailTab::Summary
        }
    }

    /// `H` / `L`: the previous or next tab the node has something for.
    pub(super) fn step_detail_tab(&mut self, delta: isize) {
        let open: Vec<DetailTab> = DetailTab::ALL
            .into_iter()
            .filter(|&t| self.tab_available(t))
            .collect();
        let at = open
            .iter()
            .position(|&t| t == self.shown_detail_tab())
            .unwrap_or(0) as isize;
        let n = open.len() as isize;
        self.detail_tab = open[((at + delta) % n + n) as usize % open.len()];
        self.detail_scroll = 0;
    }

    /// For a place with things in it and no theme: what `ev themes` reads from its contents,
    /// so a theme can be written while looking at it.
    pub(super) fn theme_hints(&self, v: &Value) -> Vec<Line<'static>> {
        let n = &v["node"];
        let placeish = matches!(n["kind"].as_str(), Some("container" | "furniture"));
        if !placeish || n["theme"].is_string() {
            return Vec::new();
        }
        let Some(id) = n["id"].as_i64() else {
            return Vec::new();
        };
        let Ok(r) = self.inv.themes(Some(&id.to_string())) else {
            return Vec::new();
        };
        let Some(e) = r["themes"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["holder"]["id"].as_i64() == Some(id))
        else {
            return Vec::new();
        };
        let mut lines = vec![
            Line::raw(""),
            Line::from(t("No theme yet (ev themes)")).bold(),
            Line::from(vec![
                Span::styled(format!("  {}", t("words: ")), Style::new().fg(pal().muted)),
                Span::raw(crate::render::theme_words(e)),
            ]),
        ];
        if e["like"].is_object() {
            let code = e["like"]["code"]
                .as_str()
                .map_or_else(|| str_of(&e["like"], "name"), str::to_string);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {}", t("reads like: ")),
                    Style::new().fg(pal().muted),
                ),
                Span::styled(code, Style::new().fg(pal().code)),
                Span::raw(format!("  {}", str_of(&e["like"], "theme"))),
            ]));
        }
        lines
    }

    /// What `ev regroup` says about the selected holder: things in it that would fit better
    /// elsewhere, and whether it is mixed. A placed box is judged among its drawer's boxes; a
    /// drawer among its own. Results are kept per drawer until the data changes.
    pub(super) fn placement_hints(&mut self, v: &Value) -> Vec<Line<'static>> {
        let n = &v["node"];
        let Some(id) = n["id"].as_i64() else {
            return Vec::new();
        };
        let holds = v["children"].as_array().is_some_and(|c| !c.is_empty());
        let scope = if v["parent_grid"].is_object() {
            n["parent_id"].as_i64()
        } else if holds && n["kind"] != "item" {
            Some(id)
        } else {
            None
        };
        let Some(scope) = scope else {
            return Vec::new();
        };
        let fresh = matches!(self.regroups.get(&scope), Some((ver, _)) if *ver == self.version);
        if !fresh {
            let Ok(r) = self.inv.regroup(Some(&scope.to_string())) else {
                return Vec::new();
            };
            self.regroups.insert(scope, (self.version, r));
        }
        let r = &self.regroups[&scope].1;
        let label = |h: &Value| {
            h["code"]
                .as_str()
                .map_or_else(|| str_of(h, "name"), str::to_string)
        };
        let mut lines = Vec::new();
        let sure = r["elsewhere"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| (e, true));
        let guess = r["alone"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| (e, false));
        for (e, sure) in sure.chain(guess) {
            if scope != id && e["now"]["holder"]["id"].as_i64() != Some(id) {
                continue;
            }
            // The destination first: a long name wraps, a code does not. A thing that shares no
            // word with its neighbours is only a guess, and says so.
            let mut spans = vec![
                Span::styled("  → ", Style::new().fg(pal().muted)),
                Span::styled(label(&e["better"]["holder"]), Style::new().fg(pal().code)),
                Span::raw(format!("  {}", str_of(&e["item"], "name"))),
            ];
            if !sure {
                spans.push(Span::styled(t("  (a guess)"), Style::new().fg(pal().muted)));
            }
            lines.push(Line::from(spans));
        }
        if r["mixed"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|m| m["holder"]["id"].as_i64() == Some(id))
        {
            lines.push(Line::from(Span::styled(
                format!("  {}", t("mixed: half or more fit better elsewhere")),
                Style::new().fg(pal().mark),
            )));
        }
        if !lines.is_empty() {
            lines.insert(0, Line::from(t("Suggestions (ev regroup)")).bold());
            lines.insert(0, Line::raw(""));
        }
        lines
    }

    /// The lines above every tab: the `#id` and the name, then the place it is in. The id comes
    /// first: it is how any node, with or without a label, is named to the agent.
    fn details_head(n: &Value) -> Vec<Line<'static>> {
        let path = n["path_text"].as_str().unwrap_or_default();
        let (place, _) = path.rsplit_once(" › ").unwrap_or(("", path));
        let mut place = path_spans(place);
        for s in &mut place {
            *s = s.clone().patch_style(Style::new().fg(pal().muted));
        }
        vec![
            Line::from(vec![
                Span::styled(
                    format!("#{}  ", n["id"]),
                    Style::new().fg(pal().code).bold(),
                ),
                Span::styled(str_of(n, "name"), Style::new().bold()),
            ]),
            Line::from(place),
            Line::raw(""),
        ]
    }

    /// The width the details text is laid out for: the pane's inside, as last drawn.
    fn detail_width(&self) -> usize {
        (self.details_area.width.saturating_sub(2) as usize).max(30)
    }

    pub(super) fn details_text(&self) -> Text<'static> {
        let Some(v) = &self.details else {
            return Text::from(t("(empty)"));
        };
        let n = &v["node"];
        let mut lines = Self::details_head(n);
        match self.shown_detail_tab() {
            DetailTab::Grid => {
                if v["grid"].is_object() {
                    lines.extend(Self::grid_text(&v["grid"], None));
                } else {
                    // A box is shown where it stands in its drawer.
                    lines.extend(Self::grid_text(&v["parent_grid"], n["id"].as_i64()));
                }
                return Text::from(lines);
            }
            DetailTab::Contents => {
                for c in v["children"].as_array().into_iter().flatten() {
                    lines.push(Line::from(node_spans(c, &self.snap)));
                }
                return Text::from(lines);
            }
            DetailTab::Suggestions => {
                // The hints open with a blank line for when they followed the fields.
                let hints = self.hints.iter().skip_while(|l| l.width() == 0);
                lines.extend(hints.cloned());
                return Text::from(lines);
            }
            DetailTab::History => {
                lines.extend(self.history_lines().into_iter().map(|l| l.0));
                return Text::from(lines);
            }
            DetailTab::Photos => {
                lines.extend(self.photo_lines().into_iter().map(|l| l.0));
                return Text::from(lines);
            }
            DetailTab::Documents => {
                lines.extend(self.document_lines().into_iter().map(|l| l.0));
                return Text::from(lines);
            }
            DetailTab::Summary => {}
        }
        lines.extend(self.summary_lines(v));
        Text::from(lines)
    }

    /// The Summary tab, in the order a person asks about a thing: what state it is in, what it
    /// is, what it cost and what covers it, what proves it, what is waiting on it, and what was
    /// said about it. Empty fields are not drawn; long values wrap under their own column.
    fn summary_lines(&self, v: &Value) -> Vec<Line<'static>> {
        let n = &v["node"];
        let width = self.detail_width();
        let mut lines = Vec::new();

        // State first, as badges: only what is true is drawn, and only state has colour.
        let mut badges: Vec<Span<'static>> = vec![Span::raw(kind_name(&str_of(n, "kind")))];
        if let Some(q) = n["qty"].as_i64() {
            badges.push(Span::styled(format!("×{q}"), Style::new().fg(pal().qty)));
        }
        let d = disposition_tr(n["disposition"].as_str().unwrap_or_default());
        match n["state"].as_str() {
            Some("candidate") => badges.push(Span::styled(
                tf("candidate ({})", &[&d]),
                Style::new().fg(pal().mark),
            )),
            Some("gone") => badges.push(Span::styled(
                tf("gone ({})", &[&d]),
                Style::new().fg(pal().muted),
            )),
            _ => {}
        }
        if n["lost"] == true {
            badges.push(Span::styled(t("lost"), Style::new().fg(pal().lost)));
        }
        let m = &v["marks"];
        if m["broken"].is_object() {
            badges.push(Span::styled(t("broken"), Style::new().fg(pal().lost)));
        }
        if let Some(st) = m["sale"]["value"].as_str() {
            let st = if st == "listed" {
                t("listed")
            } else {
                t("reserved")
            };
            badges.push(Span::styled(st, Style::new().fg(pal().qty)));
        }
        if m["label"]["value"] == "needed" {
            badges.push(Span::styled(
                t("label to print"),
                Style::new().fg(pal().code),
            ));
        }
        let mut status = Vec::new();
        for (i, b) in badges.into_iter().enumerate() {
            if i > 0 {
                status.push(Span::styled("  ·  ", Style::new().fg(pal().muted)));
            }
            status.push(b);
        }
        lines.push(Line::from(status));

        // What it is and where it stands: plain fields, keys in one column.
        let mut fields: Vec<(String, String, Style)> = Vec::new();
        let plain = Style::new();
        let mut field =
            |k: &str, val: String, style: Style| fields.push((k.to_string(), val, style));
        if let Some(c) = n["code"].as_str() {
            field(t("code"), c.to_string(), Style::new().fg(pal().code));
        }
        // With `E`, the identity fields still empty show as “—”, so the gaps are in view.
        let empty = Style::new().fg(pal().muted);
        for (k, key, identity) in [
            (t("make"), "make", true),
            (t("model"), "model", true),
            (t("serial"), "serial", true),
            (t("size"), "size", false),
            (t("theme"), "theme", false),
        ] {
            match n[key].as_str() {
                Some(x) => field(k, x.to_string(), plain),
                None if identity && self.show_empty => field(k, "—".into(), empty),
                None => {}
            }
        }
        if let Some(c) = v["cells"].as_str() {
            field(t("cells"), c.to_string(), Style::new().fg(pal().code));
        }
        if let Some(fill) = n["fill"].as_i64() {
            let style = if v["room"]["stale"] == true {
                Style::new().fg(pal().muted)
            } else {
                Style::new().fg(fill_color(fill))
            };
            field(
                t("fill"),
                format!(
                    "{}  {}",
                    fill_bar(fill),
                    crate::render::room_text(&v["room"])
                ),
                style,
            );
        }
        let tags: Vec<&str> = n["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !tags.is_empty() {
            field(t("tags"), tags.join(", "), plain);
        }
        if n["lost"] == true {
            let seen = v["last_seen"]["path_text"]
                .as_str()
                .unwrap_or(t("never known"));
            field(
                t("last seen"),
                seen.to_string(),
                Style::new().fg(pal().lost),
            );
        }
        if let Some(p) = v["pending"]["path_text"].as_str() {
            field(t("moving to"), p.to_string(), Style::new().fg(pal().mark));
        }
        for (k, key) in [
            (t("to take to"), "to"),
            (t("owner"), "owner"),
            (t("lent to"), "with"),
            (t("address"), "address"),
        ] {
            if let Some(x) = n[key].as_str() {
                field(k, x.to_string(), plain);
            }
        }
        if let Some(note) = m["broken"]["note"].as_str() {
            field(t("broken"), note.to_string(), Style::new().fg(pal().lost));
        }
        if let Some(d) = m["expires"]["value"].as_str() {
            field(t("use-by"), d.to_string(), Style::new().fg(pal().furniture));
        }
        if m["sale"].is_object() {
            let price = m["sale"]["amount"]
                .as_i64()
                .map(|a| crate::render::amount(&format!("{a}.00"), "TRY"));
            let sale = [price, m["sale"]["note"].as_str().map(str::to_string)]
                .into_iter()
                .flatten()
                .chain(
                    m["condition"]["value"]
                        .as_str()
                        .map(|c| crate::render::condition(c).to_string()),
                )
                .collect::<Vec<_>>()
                .join(" · ");
            if !sale.is_empty() {
                field(t("sale"), sale, Style::new().fg(pal().qty));
            }
        }
        Self::push_fields(&mut lines, fields, width);

        // What it cost and what it is worth: one compact line per purchase, the shop's own
        // long title dimmed under it, cut to one line.
        let purchases = v["purchases"].as_array().cloned().unwrap_or_default();
        let value = v["valuations"].as_array().and_then(|l| l.first()).cloned();
        if !purchases.is_empty() || value.is_some() {
            Self::section(&mut lines, t("Money").to_string(), width);
            let mut fields = Vec::new();
            for p in &purchases {
                fields.push((
                    t("bought").to_string(),
                    crate::render::purchase_brief(p),
                    plain,
                ));
            }
            if let Some(x) = &value {
                fields.push((
                    t("value").to_string(),
                    crate::render::valuation_brief(x),
                    plain,
                ));
            }
            Self::push_fields(&mut lines, fields, width);
            for p in &purchases {
                let title = format!("  {}", str_of(p, "name"));
                lines.push(Line::from(fit(
                    vec![Span::styled(title, Style::new().fg(pal().muted))],
                    width,
                )));
            }
        }

        // What still covers it, its status in colour; a proposal is only a proposal.
        let coverages = v["coverages"].as_array().cloned().unwrap_or_default();
        let proposal = v["coverage_proposal"].as_object().cloned();
        if !coverages.is_empty() || proposal.is_some() {
            Self::section(&mut lines, t("Coverage").to_string(), width);
            for cv in &coverages {
                let color = match cv["status"].as_str() {
                    Some("active") => pal().code,
                    Some("ending") => pal().mark,
                    Some("ended") => pal().muted,
                    _ => pal().furniture,
                };
                lines.push(Line::from(Span::styled(
                    crate::render::coverage_line(cv),
                    Style::new().fg(color),
                )));
            }
            if let Some(p) = proposal {
                let p = Value::Object(p);
                lines.push(Line::from(Span::styled(
                    tf(
                        "proposed: statutory warranty until {} (2 years from delivery {}), not recorded",
                        &[&str_of(&p, "end"), &str_of(&p, "start")],
                    ),
                    Style::new().fg(pal().muted),
                )));
            }
        }

        // What proves it: counts and kinds here, the files themselves on the Documents tab.
        let docs = v["documents"].as_array().cloned().unwrap_or_default();
        let links = self.detail_links();
        if !docs.is_empty() || !links.is_empty() {
            Self::section(
                &mut lines,
                tf("Documents ({}) · Links ({})", &[&docs.len(), &links.len()]),
                width,
            );
            let mut kinds: Vec<String> = Vec::new();
            for d in &docs {
                let k = crate::render::doc_kind(&str_of(d, "kind")).to_string();
                if !kinds.contains(&k) {
                    kinds.push(k);
                }
            }
            let mut spans = Vec::new();
            if !kinds.is_empty() {
                spans.push(Span::raw(kinds.join(", ")));
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(
                t("→ Documents tab (H/L), O opens"),
                Style::new().fg(pal().muted),
            ));
            lines.push(Line::from(spans));
        }

        // What is waiting on it: its own tasks and needs in full, those of the places it is in
        // only counted, since every thing in a drawer would repeat them.
        let own_id = n["id"].clone();
        let tasks = v["tasks"].as_array().cloned().unwrap_or_default();
        let (own, inherited): (Vec<_>, Vec<_>) = tasks.iter().partition(|t| t["via"] == own_id);
        let needs = v["needs"].as_array().cloned().unwrap_or_default();
        if !own.is_empty() || !inherited.is_empty() || !needs.is_empty() {
            Self::section(&mut lines, t("To do").to_string(), width);
            for task in &own {
                Self::push_wrapped(
                    &mut lines,
                    &format!("□ {}. {}", task["position"], str_of(task, "title")),
                    "  ",
                    Style::new().fg(pal().mark),
                    width,
                );
            }
            for nd in &needs {
                let q = nd["qty"]
                    .as_i64()
                    .map(|q| format!("{q} × "))
                    .unwrap_or_default();
                Self::push_wrapped(
                    &mut lines,
                    &format!("{}: {q}{}", t("to get"), str_of(nd, "text")),
                    "  ",
                    Style::new().fg(pal().qty),
                    width,
                );
            }
            if !inherited.is_empty() {
                lines.push(Line::from(Span::styled(
                    tf("{} more on the places it is in", &[&inherited.len()]),
                    Style::new().fg(pal().muted),
                )));
            }
        }

        // What was said about it, in full: the reason a decision can be made later.
        if let Some(note) = n["note"].as_str() {
            Self::section(&mut lines, t("Note").to_string(), width);
            Self::push_wrapped(&mut lines, note, "", Style::new(), width);
        }

        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            tf(
                "updated {}",
                &[&when(
                    &str_of(n, "updated_at"),
                    chrono::Utc::now().timestamp(),
                )],
            ),
            Style::new().fg(pal().muted),
        )));
        lines
    }

    /// A section heading: its title, then a rule to the edge.
    fn section(lines: &mut Vec<Line<'static>>, title: String, width: usize) {
        let rule = width.saturating_sub(title.chars().count() + 1);
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled(title, Style::new().bold()),
            Span::styled(
                format!(" {}", "─".repeat(rule)),
                Style::new().fg(pal().muted),
            ),
        ]));
    }

    /// Key–value lines, keys padded to one column; a long value wraps under its own column.
    fn push_fields(
        lines: &mut Vec<Line<'static>>,
        fields: Vec<(String, String, Style)>,
        width: usize,
    ) {
        let keys = fields
            .iter()
            .map(|(k, _, _)| k.chars().count())
            .max()
            .unwrap_or(0);
        let room = width.saturating_sub(keys + 2).max(10);
        for (k, val, style) in fields {
            let pad = keys - k.chars().count();
            for (i, chunk) in wrap_words(&val, room).into_iter().enumerate() {
                let key = if i == 0 {
                    format!("{k}{}  ", " ".repeat(pad))
                } else {
                    " ".repeat(keys + 2)
                };
                lines.push(Line::from(vec![
                    Span::styled(key, Style::new().fg(pal().muted)),
                    Span::styled(chunk, style),
                ]));
            }
        }
    }

    /// A paragraph wrapped to the width, each line after the first indented by `indent`.
    fn push_wrapped(
        lines: &mut Vec<Line<'static>>,
        text: &str,
        indent: &str,
        style: Style,
        width: usize,
    ) {
        let room = width.saturating_sub(indent.chars().count()).max(10);
        for (i, chunk) in wrap_words(text, room).into_iter().enumerate() {
            let lead = if i == 0 {
                String::new()
            } else {
                indent.to_string()
            };
            lines.push(Line::from(Span::styled(format!("{lead}{chunk}"), style)));
        }
    }

    /// Everything the Documents tab can open: the thing's own links, then the order and
    /// product pages of its purchases, each with a label to show.
    pub(super) fn detail_links(&self) -> Vec<(String, String, Option<String>)> {
        let Some(v) = &self.details else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for l in v["links"].as_array().into_iter().flatten() {
            let kind = match l["kind"].as_str() {
                Some("info") => t("info page"),
                Some("manual") => t("manual"),
                Some("support") => t("support"),
                Some("driver") => t("driver"),
                _ => t("other"),
            };
            out.push((
                kind.to_string(),
                str_of(l, "url"),
                l["archive"].as_str().map(str::to_string),
            ));
        }
        for p in v["purchases"].as_array().into_iter().flatten() {
            let shop = p["shop"].as_str().unwrap_or_default();
            for (key, label) in [
                ("order_url", t("order page")),
                ("product_url", t("product page")),
            ] {
                if let Some(u) = p[key].as_str() {
                    out.push((format!("{label} · {shop}"), u.to_string(), None));
                }
            }
        }
        out
    }

    /// What `[` `]` step through and `O` opens on the Documents tab, in the order drawn.
    pub(super) fn document_targets(&self) -> Vec<Target> {
        let docs = self
            .details
            .as_ref()
            .and_then(|v| v["documents"].as_array())
            .map_or(0, Vec::len);
        (0..docs)
            .map(Target::Document)
            .chain((0..self.detail_links().len()).map(Target::Link))
            .collect()
    }

    pub(super) fn step_document(&mut self, delta: isize) {
        let n = self.document_targets().len();
        if n > 0 {
            self.doc_idx = (self.doc_idx as isize + delta).clamp(0, n as isize - 1) as usize;
        }
    }

    /// Opens a document or a link in the program the system gives it.
    pub(super) fn open_target(&mut self, target: Option<Target>) {
        let what = match target {
            Some(Target::Document(i)) => self
                .details
                .as_ref()
                .and_then(|v| v["documents"][i]["file"].as_str().map(str::to_string)),
            Some(Target::Link(i)) => self.detail_links().get(i).map(|l| l.1.clone()),
            _ => None,
        };
        let Some(what) = what else { return };
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        self.status = match std::process::Command::new(opener).arg(&what).spawn() {
            Ok(_) => tf("opened {}", &[&what]),
            Err(e) => tf("could not open {}: {}", &[&what, &e]),
        };
    }

    /// The Documents tab: the documents, newest first, then the links; the picked one is
    /// marked, a click or `O` opens it.
    pub(super) fn document_lines(&self) -> Vec<(Line<'static>, Option<Target>)> {
        let Some(v) = &self.details else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut k = 0;
        let mark = |k: usize| {
            if k == self.doc_idx {
                Span::styled("▶ ", Style::new().fg(pal().code).bold())
            } else {
                Span::raw("  ")
            }
        };
        let docs = v["documents"].as_array().cloned().unwrap_or_default();
        if !docs.is_empty() {
            out.push((
                Line::from(tf("Documents ({})", &[&docs.len()])).bold(),
                None,
            ));
            for (i, d) in docs.iter().enumerate() {
                let mut what = vec![crate::render::doc_kind(&str_of(d, "kind")).to_string()];
                for key in ["issuer", "issued_at"] {
                    if let Some(x) = d[key].as_str() {
                        what.push(x.to_string());
                    }
                }
                if let Some(no) = d["number"].as_str() {
                    what.push(tf("no {}", &[&no]));
                }
                let mut spans = vec![mark(k), Span::raw(what.join(" · "))];
                if let Some(p) = d["via_purchase"].as_i64() {
                    spans.push(Span::styled(
                        tf("  (purchase #{})", &[&p]),
                        Style::new().fg(pal().muted),
                    ));
                }
                if let Some(name) = d["original_name"].as_str() {
                    spans.push(Span::styled(
                        format!("  {name}"),
                        Style::new().fg(pal().muted),
                    ));
                }
                out.push((Line::from(spans), Some(Target::Document(i))));
                k += 1;
            }
        }
        let links = self.detail_links();
        if !links.is_empty() {
            if !out.is_empty() {
                out.push((Line::raw(""), None));
            }
            out.push((Line::from(tf("Links ({})", &[&links.len()])).bold(), None));
            for (i, (label, url, archive)) in links.iter().enumerate() {
                let mut spans = vec![
                    mark(k),
                    Span::raw(format!("↗ {label}  ")),
                    Span::styled(url.clone(), Style::new().fg(pal().muted)),
                ];
                if archive.is_some() {
                    spans.push(Span::styled(
                        t("  · archived"),
                        Style::new().fg(pal().muted),
                    ));
                }
                out.push((Line::from(spans), Some(Target::Link(i))));
                k += 1;
            }
        }
        out.push((Line::raw(""), None));
        out.push((
            Line::from(Span::styled(
                t("[ ] pick · O or a click opens it"),
                Style::new().fg(pal().muted),
            )),
            None,
        ));
        out
    }

    /// The History tab: newest first, under a heading per day, each event in words. A place's
    /// history includes what came in, went out and was added there.
    pub(super) fn history_lines(&self) -> Vec<(Line<'static>, Option<Target>)> {
        let Some(events) = self.history.as_ref().and_then(|h| h["events"].as_array()) else {
            return Vec::new();
        };
        // A place by its code when it has one, else its name; one gone since by its id.
        let place = |v: &Value| -> String {
            match v {
                Value::Number(n) => n
                    .as_i64()
                    .map(|i| {
                        self.snap.label.get(&i).map_or_else(
                            || format!("#{i}"),
                            |l| l.split("  ").next().unwrap_or(l).to_string(),
                        )
                    })
                    .unwrap_or_default(),
                Value::String(s) => s.clone(),
                _ => "—".into(),
            }
        };
        let today = chrono::Local::now().date_naive();
        let mut lines = Vec::new();
        let mut day = None;
        for e in events.iter().rev() {
            let at = chrono::DateTime::parse_from_rfc3339(e["at"].as_str().unwrap_or_default())
                .map(|d| d.with_timezone(&chrono::Local))
                .ok();
            let this = at.map(|a| a.date_naive());
            if this != day {
                day = this;
                let head = match this {
                    Some(d) if d == today => t("Today").to_string(),
                    Some(d) if Some(d) == today.pred_opt() => t("Yesterday").to_string(),
                    Some(d) => d.format("%Y-%m-%d").to_string(),
                    None => "?".into(),
                };
                if !lines.is_empty() {
                    lines.push((Line::raw(""), None));
                }
                lines.push((Line::from(head).bold(), None));
            }
            let time = at.map_or_else(String::new, |a| a.format("%H:%M").to_string());
            let d = &e["data"];
            let (verb, detail, style) = if e["item"].is_object() {
                let name = str_of(&e["item"], "name");
                match (e["relation"].as_str(), e["type"].as_str()) {
                    (Some("added"), _) => (t("added here"), name, pal().code),
                    (Some("in"), Some("plan")) => (t("planned to come"), name, pal().mark),
                    (Some("in"), _) => (
                        t("came in"),
                        format!("{name}  ← {}", place(&d["from"])),
                        pal().code,
                    ),
                    _ => (
                        t("went out"),
                        format!("{name}  → {}", place(&d["to"])),
                        pal().muted,
                    ),
                }
            } else {
                let own = |v: &'static str, s: String| (t(v), s, pal().furniture);
                match e["type"].as_str().unwrap_or_default() {
                    "create" => own("created", place(&d["parent"])),
                    "move" => own(
                        "moved",
                        format!("{} → {}", place(&d["from"]), place(&d["to"])),
                    ),
                    "done" => own(
                        "moved as planned",
                        format!("{} → {}", place(&d["from"]), place(&d["to"])),
                    ),
                    "plan" => own("move planned", format!("→ {}", place(&d["to"]))),
                    "cancel" => own("plan cancelled", String::new()),
                    "edit" => own("changed", edit_text(d)),
                    "photo" => own(
                        "photo added",
                        if d["crop"].is_string() {
                            t("(a crop)").to_string()
                        } else {
                            String::new()
                        },
                    ),
                    "split" => own(
                        "split into",
                        d["into"]
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .map(|p| format!("#{} {}", p["id"], str_of(p, "name")))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_default(),
                    ),
                    "split_from" => own(
                        "split from",
                        format!("#{} {}", d["from"], str_of(d, "name")),
                    ),
                    "kit_link" => own(
                        "linked to kit",
                        format!(
                            "{} · {}. {}",
                            str_of(d, "kit"),
                            d["part"],
                            str_of(d, "text")
                        ),
                    ),
                    "kit_unlink" => own(
                        "unlinked from kit",
                        format!(
                            "{} · {}. {}",
                            str_of(d, "kit"),
                            d["part"],
                            str_of(d, "text")
                        ),
                    ),
                    "photo_remove" => own(
                        "photo removed",
                        [str_of(d, "note"), str_of(d, "crop")]
                            .into_iter()
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>()
                            .join("  "),
                    ),
                    "observe" => own("observed", str_of(d, "text")),
                    "unobserve" => own("observation removed", str_of(d, "text")),
                    "review" => own("reviewed", {
                        let status = crate::render::count_label(d["as"].as_str().unwrap_or("raw"))
                            .to_string();
                        format!("{status}  {}", str_of(d, "note"))
                    }),
                    "dispose" => own(
                        "set aside",
                        disposition_tr(d["as"].as_str().unwrap_or_default()).to_string(),
                    ),
                    "gone" => own("gone", {
                        let why = str_of(d, "why");
                        format!(
                            "{}  {why}",
                            disposition_tr(d["as"].as_str().unwrap_or_default())
                        )
                    }),
                    "restore" => own("restored", str_of(d, "correction")),
                    "cell" => own(
                        "cells",
                        format!("{} → {}", place(&d["before"]), place(&d["after"])),
                    ),
                    "grid" => own("grid set", format!("{}×{}", d["after"][0], d["after"][1])),
                    "decline" => own("move declined", str_of(d, "why")),
                    "decline_cleared" => own("decline taken back", String::new()),
                    "grid_face" => own(
                        "grid seen from",
                        t(if d["after"] == "front" {
                            "the front"
                        } else {
                            "above"
                        })
                        .to_string(),
                    ),
                    "sketch" => {
                        let a = &d["after"];
                        let mut parts = Vec::new();
                        if !a["x"].is_null() {
                            parts.push(tf(
                                "at {},{} cm",
                                &[&crate::render::cm(&a["x"]), &crate::render::cm(&a["y"])],
                            ));
                        }
                        if !a["w"].is_null() {
                            parts.push(tf(
                                "{}×{} cm",
                                &[&crate::render::cm(&a["w"]), &crate::render::cm(&a["d"])],
                            ));
                        }
                        if !a["on"].is_null() {
                            parts.push(tf("on #{}", &[&a["on"]]));
                        }
                        if a.is_null() {
                            own("sketch removed", String::new())
                        } else {
                            own("sketched", parts.join(" · "))
                        }
                    }
                    "lost" => own("lost", String::new()),
                    "found" => own("found", place(&d["at"])),
                    "back" => own("returned", place(&d["from"])),
                    "lend" => own("lent", place(&d["to"])),
                    "sketch_import" => own(
                        "plan imported",
                        tf(
                            "{} · {} rooms",
                            &[&str_of(d, "file"), &d["rooms"].as_i64().unwrap_or(0)],
                        ),
                    ),
                    "broken" => own("broken", str_of(d, "note")),
                    "fixed" => own("fixed", String::new()),
                    "purchase_linked" => own(
                        "linked to purchase",
                        tf("#{} ×{}", &[&d["purchase"], &d["qty"]]),
                    ),
                    "purchase_unlinked" => {
                        own("unlinked from purchase", format!("#{}", d["purchase"]))
                    }
                    "doc_linked" => own(
                        "document added",
                        format!(
                            "#{} {}",
                            d["document"],
                            crate::render::doc_kind(&str_of(d, "kind"))
                        ),
                    ),
                    "doc_unlinked" => own(
                        "document removed",
                        format!(
                            "#{} {}",
                            d["document"],
                            crate::render::doc_kind(&str_of(d, "kind"))
                        ),
                    ),
                    "coverage_added" => own(
                        "coverage added",
                        format!(
                            "#{} {}",
                            d["coverage"],
                            crate::render::coverage_kind(&str_of(d, "kind"))
                        ),
                    ),
                    "coverage_removed" => own(
                        "coverage removed",
                        format!(
                            "#{} {}",
                            d["coverage"],
                            crate::render::coverage_kind(&str_of(d, "kind"))
                        ),
                    ),
                    "track" => own("decided", {
                        let subject = if d["subject"] == "value" {
                            t("value")
                        } else {
                            t("coverage")
                        };
                        let decision = match d["decision"].as_str() {
                            Some("later") => t("not now"),
                            Some("yes") => t("tracked again"),
                            _ => t("not tracked"),
                        };
                        let why = str_of(d, "why");
                        format!("{subject}: {decision}  {why}")
                            .trim_end()
                            .to_string()
                    }),
                    other => (t("event"), format!("{other} {d}"), pal().muted),
                }
            };
            // A thing that came, went or was added opens with a click.
            let target = e["item"]["id"].as_i64().map(Target::Node);
            lines.push((
                Line::from(vec![
                    Span::styled(format!("  {time}  "), Style::new().fg(pal().muted)),
                    Span::styled(verb.to_string(), Style::new().fg(style)),
                    Span::raw(format!("  {detail}")),
                ]),
                target,
            ));
        }
        lines
    }

    /// The Photos tab: every photo, newest first, with when it was added, whether it is a crop,
    /// and its note; the one shown above is marked. A click shows that one.
    pub(super) fn photo_lines(&self) -> Vec<(Line<'static>, Option<Target>)> {
        let current = self.photo_idx.min(self.photos.len().saturating_sub(1));
        let count = self.photos.len();
        (0..count)
            .rev()
            .map(|i| {
                let p = &self.photos[i];
                let at = chrono::DateTime::parse_from_rfc3339(p["added_at"].as_str().unwrap_or(""))
                    .map(|d| {
                        d.with_timezone(&chrono::Local)
                            .format("%Y-%m-%d %H:%M")
                            .to_string()
                    })
                    .unwrap_or_else(|_| "\u{2014}".repeat(16));
                let kind = if p["crop"].is_string() {
                    t("crop")
                } else {
                    t("whole")
                };
                let mark = if i == current { "\u{25b6} " } else { "  " };
                let style = if i == current {
                    Style::new().bold()
                } else {
                    Style::new()
                };
                let line = Line::from(vec![
                    Span::styled(format!("{mark}{:>2}  ", i + 1), style.fg(pal().code)),
                    Span::styled(format!("{at}  {kind:<5}  "), Style::new().fg(pal().muted)),
                    Span::styled(str_of(p, "note"), style),
                ]);
                (line, Some(Target::Photo(i)))
            })
            .collect()
    }

    /// What each line of the details points at, in the order `details_text` draws them: the
    /// Photos, Contents and History tabs have lines to click, the others none.
    pub(super) fn detail_targets(&self) -> Vec<Option<Target>> {
        let Some(v) = &self.details else {
            return Vec::new();
        };
        // The head: the id and name, the place, a blank line.
        let mut out = vec![None, None, None];
        match self.shown_detail_tab() {
            DetailTab::Photos => out.extend(self.photo_lines().into_iter().map(|l| l.1)),
            DetailTab::Documents => out.extend(self.document_lines().into_iter().map(|l| l.1)),
            DetailTab::Contents => out.extend(
                v["children"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|c| c["id"].as_i64().map(Target::Node)),
            ),
            DetailTab::History => out.extend(self.history_lines().into_iter().map(|l| l.1)),
            _ => return Vec::new(),
        }
        out
    }

    /// A holder's grid for the details pane: title, the map with free cells muted, and the
    /// free cells by name. It goes right under the path, above the fields and the contents,
    /// since a drawer with a photo and many boxes leaves little room below.
    pub(super) fn grid_text(g: &Value, mark: Option<i64>) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from(crate::render::grid_title(g)).bold()];
        // The boxes are drawn as frames on the plate, the way they sit in the drawer: a box
        // over several cells is one frame, and a free cell is a bare dot on the plate.
        const W: usize = 6;
        let map = grid_map(g);
        let rows = map.len() as isize;
        let cols = map.first().map_or(0, Vec::len) as isize;
        let at = |r: isize, c: isize| -> Option<i64> {
            if r < 0 || c < 0 || r >= rows || c >= cols {
                return None;
            }
            map[r as usize][c as usize]
        };
        // An edge runs between two cells that belong to different boxes, one of them a box.
        let differ = |a: Option<i64>, b: Option<i64>| a != b && (a.is_some() || b.is_some());
        let hseg = |r: isize, c: isize| c >= 0 && c < cols && differ(at(r - 1, c), at(r, c));
        let vseg = |r: isize, c: isize| r >= 0 && r < rows && differ(at(r, c - 1), at(r, c));
        let style = |ids: [Option<i64>; 4]| match mark {
            Some(m) if ids.contains(&Some(m)) => Style::new().fg(pal().code).bold(),
            Some(_) => Style::new().fg(pal().muted),
            None => Style::new(),
        };
        let corner = |r: isize, c: isize| -> Span<'static> {
            let (u, d, l, rt) = (vseg(r - 1, c), vseg(r, c), hseg(r, c - 1), hseg(r, c));
            let ch = match (u, d, l, rt) {
                (false, false, false, false) => " ",
                (true, true, false, false)
                | (true, false, false, false)
                | (false, true, false, false) => "│",
                (false, false, true, true)
                | (false, false, true, false)
                | (false, false, false, true) => "─",
                (false, true, false, true) => "┌",
                (false, true, true, false) => "┐",
                (true, false, false, true) => "└",
                (true, false, true, false) => "┘",
                (true, true, false, true) => "├",
                (true, true, true, false) => "┤",
                (false, true, true, true) => "┬",
                (true, false, true, true) => "┴",
                (true, true, true, true) => "┼",
            };
            let ids = [at(r - 1, c - 1), at(r - 1, c), at(r, c - 1), at(r, c)];
            Span::styled(ch, style(ids))
        };
        let mut head = String::from("   ");
        for c in 0..cols {
            head.push_str(&format!("{:^W$}", ((b'A' + c as u8) as char).to_string()));
        }
        lines.push(Line::from(head));
        for r in 0..=rows {
            // The edge line above cell row `r`.
            let mut spans = vec![Span::raw("   ")];
            for c in 0..cols {
                spans.push(corner(r, c));
                let fill = if hseg(r, c) { "─" } else { " " };
                spans.push(Span::styled(
                    fill.repeat(W - 1),
                    style([at(r - 1, c), at(r, c), None, None]),
                ));
            }
            spans.push(corner(r, cols));
            lines.push(Line::from(spans));
            if r == rows {
                break;
            }
            // The cell row itself: a box names its back-left cell once, and the box being
            // shown stands out while the others step back.
            let mut spans = vec![Span::raw(format!("{:>2} ", r + 1))];
            for c in 0..cols {
                let edge = if vseg(r, c) { "│" } else { " " };
                spans.push(Span::styled(
                    edge,
                    style([at(r, c - 1), at(r, c), None, None]),
                ));
                let id = at(r, c);
                let label = format!("{}{}", (b'A' + c as u8) as char, r + 1);
                let span = match id {
                    None => Span::styled(
                        format!("{:^w$}", "·", w = W - 1),
                        Style::new().fg(pal().muted),
                    ),
                    Some(_) if at(r - 1, c) == id || at(r, c - 1) == id => {
                        Span::raw(" ".repeat(W - 1))
                    }
                    Some(i) if Some(i) == mark => Span::styled(
                        format!("{:^w$}", label, w = W - 1),
                        Style::new().fg(pal().code).bold().reversed(),
                    ),
                    Some(_) if mark.is_some() => Span::styled(
                        format!("{:^w$}", label, w = W - 1),
                        Style::new().fg(pal().muted),
                    ),
                    Some(_) => Span::styled(
                        format!("{:^w$}", label, w = W - 1),
                        Style::new().fg(pal().code),
                    ),
                };
                spans.push(span);
            }
            let edge = if vseg(r, cols) { "│" } else { " " };
            spans.push(Span::styled(
                edge,
                style([at(r, cols - 1), None, None, None]),
            ));
            lines.push(Line::from(spans));
        }
        if mark.is_none() {
            let names: Vec<&str> = g["free"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            lines.push(Line::from(Span::styled(
                tf("free ({}): {}", &[&names.len(), &names.join(" ")]),
                Style::new().fg(pal().muted),
            )));
        }
        lines.push(Line::raw(""));
        lines
    }
}
