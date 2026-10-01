//! The details pane: what is loaded for the selected node, its tabs and what each draws.

use super::*;

impl App {
    pub(super) fn load_details(&mut self) -> Result<()> {
        let before = self.details.as_ref().map(|d| d["node"]["id"].clone());
        let now = self.selected_id().map(|i| serde_json::json!(i));
        if before != now {
            // The newest photo is the one that shows the place as it is now; older ones stay a
            // step back with `[`.
            self.photo_idx = usize::MAX;
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

    pub(super) fn details_text(&self) -> Text<'static> {
        let Some(v) = &self.details else {
            return Text::from(t("(empty)"));
        };
        let n = &v["node"];
        // The id first: it is how any node, with or without a label, is named to the agent.
        let mut title = vec![Span::styled(
            format!("#{}  ", n["id"]),
            Style::new().fg(pal().code).bold(),
        )];
        title.extend(path_spans(n["path_text"].as_str().unwrap_or_default()));
        if let Some(last) = title.last_mut() {
            *last = last.clone().bold();
        }
        let mut lines = vec![Line::from(title), Line::raw("")];
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
            DetailTab::Summary => {}
        }
        let mut fields: Vec<(String, Span<'static>)> = Vec::new();
        let mut field = |k: &str, val: Span<'static>| fields.push((k.to_string(), val));
        field(t("kind"), Span::raw(kind_name(&str_of(n, "kind"))));
        if let Some(c) = n["code"].as_str() {
            field(
                t("code"),
                Span::styled(c.to_string(), Style::new().fg(pal().code)),
            );
        }
        if let Some(s) = n["size"].as_str() {
            field(t("size"), Span::raw(s.to_string()));
        }
        if let Some(q) = n["qty"].as_i64() {
            field(
                t("qty"),
                Span::styled(q.to_string(), Style::new().fg(pal().qty)),
            );
        }
        let d = disposition_tr(n["disposition"].as_str().unwrap_or_default());
        match n["state"].as_str() {
            Some("candidate") => field(
                t("state"),
                Span::styled(tf("candidate ({})", &[&d]), Style::new().fg(pal().mark)),
            ),
            Some("gone") => field(
                t("state"),
                Span::styled(tf("gone ({})", &[&d]), Style::new().fg(pal().muted)),
            ),
            _ => {}
        }
        if n["lost"] == true {
            let seen = v["last_seen"]["path_text"]
                .as_str()
                .unwrap_or(t("never known"));
            field(
                t("lost"),
                Span::styled(tf("last seen: {}", &[&seen]), Style::new().fg(pal().lost)),
            );
        }
        if let Some(p) = v["pending"]["path_text"].as_str() {
            field(
                t("moving to"),
                Span::styled(p.to_string(), Style::new().fg(pal().mark)),
            );
        }
        for (k, key) in [
            (t("to take to"), "to"),
            (t("owner"), "owner"),
            (t("lent to"), "with"),
            (t("theme"), "theme"),
            (t("make"), "make"),
            (t("model"), "model"),
            (t("serial"), "serial"),
            (t("note"), "note"),
            (t("address"), "address"),
        ] {
            if let Some(x) = n[key].as_str() {
                field(k, Span::raw(x.to_string()));
            }
        }
        if let Some(fill) = n["fill"].as_i64() {
            let stale = v["room"]["stale"] == true;
            let style = if stale {
                Style::new().fg(pal().muted)
            } else {
                Style::new().fg(fill_color(fill))
            };
            field(
                t("fill"),
                Span::styled(
                    format!(
                        "{}  {}",
                        fill_bar(fill),
                        crate::render::room_text(&v["room"])
                    ),
                    style,
                ),
            );
        }
        if let Some(c) = v["cells"].as_str() {
            field(
                t("cells"),
                Span::styled(c.to_string(), Style::new().fg(pal().code)),
            );
        }
        let tags: Vec<&str> = n["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !tags.is_empty() {
            field(t("tags"), Span::raw(tags.join(", ")));
        }
        // The photos themselves are on the panel above; their file paths only push the rest
        // of the details out of view.
        let photos = n["photos"].as_array().map_or(0, Vec::len);
        if photos > 0 {
            field(t("photos"), Span::raw(photos.to_string()));
        }
        let m = &v["marks"];
        if m["label"]["value"] == "needed" {
            field(
                t("label"),
                Span::styled(t("to print"), Style::new().fg(pal().code).bold()),
            );
        }
        if m["broken"].is_object() {
            let note = m["broken"]["note"].as_str().unwrap_or("");
            field(
                t("broken"),
                Span::styled(
                    tf("awaiting repair {}", &[&note]),
                    Style::new().fg(pal().lost),
                ),
            );
        }
        if let Some(d) = m["expires"]["value"].as_str() {
            field(
                t("use-by"),
                Span::styled(d.to_string(), Style::new().fg(pal().furniture)),
            );
        }
        if let Some(st) = m["sale"]["value"].as_str() {
            let st = if st == "listed" {
                t("listed")
            } else {
                t("reserved")
            };
            let price = m["sale"]["amount"]
                .as_i64()
                .map(|a| format!(" · {a} TL"))
                .unwrap_or_default();
            let at = m["sale"]["note"]
                .as_str()
                .map(|w| format!(" · {w}"))
                .unwrap_or_default();
            field(
                t("sale"),
                Span::styled(format!("{st}{price}{at}"), Style::new().fg(pal().qty)),
            );
        }
        for nd in v["needs"].as_array().into_iter().flatten() {
            let q = nd["qty"]
                .as_i64()
                .map(|q| format!("{q} × "))
                .unwrap_or_default();
            field(
                t("to get"),
                Span::styled(
                    format!("{q}{}", str_of(nd, "text")),
                    Style::new().fg(pal().qty),
                ),
            );
        }
        for p in v["purchases"].as_array().into_iter().flatten() {
            field(t("bought"), Span::raw(crate::render::purchase_line(p)));
        }
        for d in v["documents"].as_array().into_iter().flatten() {
            field(t("document"), Span::raw(crate::render::doc_line(d)));
        }
        for cv in v["coverages"].as_array().into_iter().flatten() {
            field(t("coverage"), Span::raw(crate::render::coverage_line(cv)));
        }
        for task in v["tasks"].as_array().into_iter().flatten() {
            let via = if task["via"] == n["id"] {
                String::new()
            } else {
                let place = task["via"]
                    .as_i64()
                    .and_then(|i| self.snap.label.get(&i).cloned())
                    .unwrap_or_default();
                tf("  (via {})", &[&place])
            };
            field(
                t("task"),
                Span::styled(
                    format!("{}. {}{via}", task["position"], str_of(task, "title")),
                    Style::new().fg(pal().mark),
                ),
            );
        }
        field(
            t("updated"),
            Span::styled(
                when(&str_of(n, "updated_at"), chrono::Utc::now().timestamp()),
                Style::new().fg(pal().muted),
            ),
        );
        // Labels padded to one width, so the values line up in a column.
        let width = fields
            .iter()
            .map(|(k, _)| k.chars().count())
            .max()
            .unwrap_or(0);
        for (k, val) in fields {
            let pad = width - k.chars().count();
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{k}{}  ", " ".repeat(pad)),
                    Style::new().fg(pal().muted),
                ),
                val,
            ]));
        }
        Text::from(lines)
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
        let mut out = vec![None, None];
        match self.shown_detail_tab() {
            DetailTab::Photos => out.extend(self.photo_lines().into_iter().map(|l| l.1)),
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
