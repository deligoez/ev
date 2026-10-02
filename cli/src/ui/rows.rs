//! The rows of the list on the left: the tree, the lists and the to-do sections.

use super::*;

impl App {
    /// Recomputes the rows of the current tab, keeping the selection on the same node.
    pub(super) fn rebuild(&mut self) -> Result<()> {
        let keep = self.selected_id();
        self.rows = match self.tab {
            Tab::Tree => {
                let mut out = Vec::new();
                for r in &self.snap.roots {
                    self.flatten(r, 0, &mut out);
                }
                // Lost things under their own heading, each with where it was last seen.
                if !self.snap.lost.is_empty() {
                    let open = !self.collapsed.contains(&LOST_SECTION);
                    out.push(Row {
                        id: LOST_SECTION,
                        depth: 0,
                        spans: vec![Span::styled(
                            format!("{} ({})", t("Unknown place"), self.snap.lost.len()),
                            Style::new().fg(pal().lost).bold(),
                        )],
                        expandable: true,
                        expanded: open,
                    });
                    if open {
                        for u in &self.snap.lost {
                            let at = out.len();
                            self.flatten(u, 1, &mut out);
                            out[at].spans.push(Span::styled(
                                crate::render::last_seen(u),
                                Style::new().fg(pal().muted),
                            ));
                        }
                    }
                }
                out
            }
            Tab::Pending => {
                let v = self.inv.pending()?;
                v["pending"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|m| {
                        let to = m["to"]["path_text"].as_str().unwrap_or_default();
                        self.list_row(
                            &m["node"],
                            vec![Span::styled(
                                format!("  → {to}"),
                                Style::new().fg(pal().mark),
                            )],
                        )
                    })
                    .collect()
            }
            Tab::Disposals => {
                let v = self.inv.disposals(None)?;
                let mut out = Vec::new();
                for (d, list) in v["disposals"].as_object().into_iter().flatten() {
                    for n in list.as_array().into_iter().flatten() {
                        let mut extra = vec![Span::styled(
                            format!("  [{}]", disposition_tr(d)),
                            Style::new().fg(pal().mark),
                        )];
                        let parts = n["parts"].as_array().map_or(0, Vec::len);
                        if parts > 0 {
                            extra.push(Span::styled(
                                tf("  (+{} parts)", &[&parts]),
                                Style::new().fg(pal().muted),
                            ));
                        }
                        out.push(self.list_row(n, extra));
                    }
                }
                out
            }
            Tab::Lost => {
                let v = self.inv.lost_list()?;
                v["lost"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|m| {
                        let seen = m["last_seen"]["path_text"]
                            .as_str()
                            .unwrap_or(t("never known"));
                        self.list_row(
                            &m["node"],
                            vec![Span::styled(
                                tf("  (last seen: {})", &[&seen]),
                                Style::new().fg(pal().lost),
                            )],
                        )
                    })
                    .collect()
            }
            Tab::Places => {
                let v = self.inv.errands(None)?;
                let mut out = Vec::new();
                for e in v["errands"].as_array().into_iter().flatten() {
                    let place = str_of(&e["place"], "name");
                    for (key, what, color) in [
                        ("take", t("take"), pal().mark),
                        ("return", t("return"), pal().blue),
                        ("collect", t("collect"), pal().blue),
                    ] {
                        for n in e[key].as_array().into_iter().flatten() {
                            let mut row = self.list_row(n, Vec::new());
                            let mut head = vec![
                                Span::styled(place.clone(), Style::new().bold()),
                                Span::styled(format!(" · {what}  "), Style::new().fg(color)),
                            ];
                            head.append(&mut row.spans);
                            row.spans = head;
                            out.push(row);
                        }
                    }
                }
                out
            }
            Tab::Search => self.search_rows.clone(),
            Tab::Plan => self.todo_rows()?,
            Tab::Settings => self.settings_rows(),
        };
        let idx = keep
            .and_then(|id| self.rows.iter().position(|r| r.id == id))
            // Start on the first real line, not on a section header.
            .or_else(|| {
                self.rows
                    .iter()
                    .position(|r| r.id >= 0)
                    .or(if self.rows.is_empty() { None } else { Some(0) })
            })
            .map(|i| i.min(self.rows.len().saturating_sub(1)));
        self.state.select(idx);
        self.load_details()
    }

    /// The Yapılacak tab: one section per kind of waiting work, each headed by its count and
    /// collapsible; a section with nothing in it is left out. Every line points at the node it
    /// is about, so the right pane shows it; a header's id is negative (its section index).
    pub(super) fn todo_rows(&mut self) -> Result<Vec<Row>> {
        let v = self.inv.todo()?;
        let c = &v["counts"];
        let p = &v["progress"];
        self.plan_title = tf(
            " To do · {}/{} counted · {} tasks · {} moves ",
            &[&p["toured"], &p["units"], &c["tasks"], &c["moves"]],
        );
        let mut out = Vec::new();
        let short = |path: &str| {
            let parts: Vec<&str> = path.split(" › ").collect();
            parts[parts.len().saturating_sub(2)..].join(" › ")
        };
        // Where a thing is: the last two steps of its parent's path (its own name is already
        // on the line).
        let within = |path: &str| {
            let parts: Vec<&str> = path.split(" › ").collect();
            let parent = &parts[..parts.len().saturating_sub(1)];
            parent[parent.len().saturating_sub(2)..].join(" › ")
        };
        let item = |id: i64, spans: Vec<Span<'static>>| Row {
            id,
            depth: 1,
            spans,
            expandable: false,
            expanded: false,
        };
        let muted = |s: String| Span::styled(s, Style::new().fg(pal().muted));
        type Section<'a> = (&'static str, Color, Vec<Row>);
        let mut sections: Vec<Section> = Vec::new();

        let tasks = v["tasks"].as_array().cloned().unwrap_or_default();
        sections.push((
            t("TASKS"),
            pal().mark,
            tasks
                .iter()
                .map(|t| {
                    let mut spans = vec![muted(format!("{}. ", t["position"]))];
                    if t["status"] == "doing" {
                        spans.push(Span::styled("▶ ", Style::new().fg(pal().mark).bold()));
                    }
                    spans.push(Span::styled(str_of(t, "title"), Style::new().bold()));
                    spans.push(muted(format!("  — {}", str_of(t, "why"))));
                    item(t["nodes"][0]["id"].as_i64().unwrap_or(0), spans)
                })
                .collect(),
        ));
        sections.push((
            t("MOVES"),
            pal().blue,
            v["moves"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|m| {
                    item(
                        m["node"]["id"].as_i64().unwrap_or(0),
                        vec![
                            Span::raw(str_of(&m["node"], "name")),
                            Span::styled("  → ", Style::new().fg(pal().mark)),
                            Span::styled(
                                short(&str_of(&m["to"], "path_text")),
                                Style::new().fg(pal().code),
                            ),
                        ],
                    )
                })
                .collect(),
        ));
        let mut errands = Vec::new();
        for e in v["errands"].as_array().into_iter().flatten() {
            let place = str_of(&e["place"], "name");
            for (key, what) in [
                ("take", t("take")),
                ("return", t("return")),
                ("collect", t("collect")),
            ] {
                for n in e[key].as_array().into_iter().flatten() {
                    errands.push(item(
                        n["id"].as_i64().unwrap_or(0),
                        vec![
                            Span::styled(place.clone(), Style::new().bold()),
                            Span::styled(format!(" · {what}  "), Style::new().fg(pal().mark)),
                            Span::raw(str_of(n, "name")),
                        ],
                    ));
                }
            }
        }
        sections.push((t("TAKE / RETURN"), pal().blue, errands));
        let mut leaving = Vec::new();
        for (d, list) in v["disposals"].as_object().into_iter().flatten() {
            for n in list.as_array().into_iter().flatten() {
                let mut spans = vec![
                    Span::styled(
                        format!("{}  ", disposition_tr(d)),
                        Style::new().fg(pal().mark),
                    ),
                    Span::raw(str_of(n, "name")),
                ];
                if let Some(st) = n["sale"]["value"].as_str() {
                    let st = if st == "listed" {
                        t("listed")
                    } else {
                        t("reserved")
                    };
                    let price = n["sale"]["amount"]
                        .as_i64()
                        .map(|a| format!(" {a} TL"))
                        .unwrap_or_default();
                    spans.push(Span::styled(
                        format!("  [{st}{price}]"),
                        Style::new().fg(pal().qty).bold(),
                    ));
                }
                spans.push(muted(format!("  {}", within(&str_of(n, "path_text")))));
                leaving.push(item(n["id"].as_i64().unwrap_or(0), spans));
            }
        }
        sections.push((t("LEAVING"), pal().mark, leaving));
        sections.push((
            t("LABELS TO PRINT"),
            pal().code,
            v["labels"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|n| {
                    item(
                        n["id"].as_i64().unwrap_or(0),
                        vec![
                            Span::styled(str_of(n, "code"), Style::new().fg(pal().code).bold()),
                            muted(format!("  {}", n["theme"].as_str().unwrap_or(""))),
                        ],
                    )
                })
                .collect(),
        ));
        sections.push((
            t("TO GET"),
            pal().qty,
            v["needs"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|n| {
                    let mut spans = Vec::new();
                    if let Some(q) = n["qty"].as_i64() {
                        spans.push(Span::styled(format!("{q} × "), Style::new().fg(pal().qty)));
                    }
                    spans.push(Span::raw(str_of(n, "text")));
                    if n["make"] == true {
                        spans.push(muted(t("  (print / make)").into()));
                    }
                    if let Some(p) = n["for"]["path_text"].as_str() {
                        spans.push(muted(format!("  → {}", short(p))));
                    }
                    item(n["for"]["id"].as_i64().unwrap_or(0), spans)
                })
                .collect(),
        ));
        let plain = |key: &str, extra: &dyn Fn(&Value) -> Option<Span<'static>>| -> Vec<Row> {
            v[key]
                .as_array()
                .into_iter()
                .flatten()
                .map(|n| {
                    let node = if n["node"].is_object() { &n["node"] } else { n };
                    let mut spans = vec![Span::raw(str_of(node, "name"))];
                    if let Some(s) = extra(n) {
                        spans.push(s);
                    }
                    let place = match n["last_seen"]["path_text"].as_str() {
                        Some(p) => tf("  last seen: {}", &[&short(p)]),
                        None if n["node"].is_object() => t("  (never known)").into(),
                        None => format!("  {}", within(&str_of(node, "path_text"))),
                    };
                    spans.push(muted(place));
                    item(node["id"].as_i64().unwrap_or(0), spans)
                })
                .collect()
        };
        sections.push((
            t("REPAIRS"),
            pal().lost,
            plain("repairs", &|n| {
                n["note"]
                    .as_str()
                    .map(|x| Span::styled(format!("  ({x})"), Style::new().fg(pal().lost)))
            }),
        ));
        sections.push((
            t("USE-BY"),
            pal().lost,
            plain("expiring", &|n| {
                let days = n["days_left"].as_i64().unwrap_or(0);
                let (text, color) = if days < 0 {
                    (tf("  {} · past", &[&str_of(n, "expires")]), pal().lost)
                } else {
                    (
                        tf("  {} · {} days", &[&str_of(n, "expires"), &days]),
                        pal().furniture,
                    )
                };
                Some(Span::styled(text, Style::new().fg(color).bold()))
            }),
        ));
        sections.push((t("LOST"), pal().lost, plain("lost", &|_| None)));
        sections.push((
            t("NOT COUNTED YET"),
            pal().furniture,
            plain("uncounted", &|_| None),
        ));
        sections.push((
            t("CHANGED SINCE COUNTED"),
            pal().furniture,
            plain("stale", &|_| None),
        ));
        sections.push((
            t("PHOTO NEEDED"),
            pal().code,
            plain("photos", &|n| {
                let text = if n["photo_reason"] == "none" {
                    t("  no photo at all").to_string()
                } else if n["photo_reason"] == "marked" {
                    t("  photo marked out of date").to_string()
                } else {
                    tf(
                        "  changed after the photo ({})",
                        &[&n["changed_at"]
                            .as_str()
                            .unwrap_or("")
                            .get(..10)
                            .unwrap_or("")],
                    )
                };
                Some(Span::styled(text, Style::new().fg(pal().code)))
            }),
        ));
        let mut shared = Vec::new();
        for s in v["shared_photos"].as_array().into_iter().flatten() {
            let names: Vec<String> = s["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|n| str_of(n, "name"))
                .collect();
            let first = s["nodes"][0]["id"].as_i64().unwrap_or(0);
            shared.push(item(
                first,
                vec![
                    Span::styled(
                        tf("same whole photo on {} records  ", &[&names.len()]),
                        Style::new().fg(pal().lost).bold(),
                    ),
                    muted(names.join(", ")),
                ],
            ));
        }
        sections.push((t("UNCUT SHARED PHOTO"), pal().lost, shared));
        sections.push((
            t("UNCLEAR RECORDS"),
            pal().muted,
            plain("unclear", &|_| None),
        ));

        for (i, (title, color, rows)) in sections.into_iter().enumerate() {
            if rows.is_empty() {
                continue;
            }
            let id = -(i as i64) - 1;
            let open = !self.collapsed.contains(&id);
            out.push(Row {
                id,
                depth: 0,
                spans: vec![
                    Span::styled(title, Style::new().fg(color).bold()),
                    Span::styled(format!("  {}", rows.len()), Style::new().fg(pal().muted)),
                ],
                expandable: true,
                expanded: open,
            });
            if open {
                out.extend(rows);
            }
        }
        Ok(out)
    }

    pub(super) fn toggle_section(&mut self, id: i64) -> Result<()> {
        if !self.collapsed.remove(&id) {
            self.collapsed.insert(id);
        }
        self.rebuild()
    }

    pub(super) fn tree_row(&self, n: &Value, depth: usize) -> Row {
        let id = n["id"].as_i64().unwrap_or_default();
        let kids = children(n).len();
        let expanded = self.expanded.contains(&id);
        let mut spans = node_spans(n, &self.snap);
        let total = n["items"].as_i64().unwrap_or(0);
        if total > 0 {
            spans.push(Span::styled(
                tf("  {} items", &[&total]),
                Style::new().fg(pal().muted),
            ));
        }
        if let Some(fill) = n["fill"].as_i64() {
            spans.push(Span::styled(
                format!("  {}", fill_bar(fill)),
                Style::new().fg(fill_color(fill)),
            ));
        }
        Row {
            id,
            depth,
            spans,
            expandable: kids > 0,
            expanded,
        }
    }

    pub(super) fn list_row(&self, n: &Value, extra: Vec<Span<'static>>) -> Row {
        let mut spans = vec![kind_mark(n)];
        spans.extend(path_spans(n["path_text"].as_str().unwrap_or_default()));
        spans.extend(extra);
        Row {
            id: n["id"].as_i64().unwrap_or_default(),
            depth: 0,
            spans,
            expandable: false,
            expanded: false,
        }
    }

    pub(super) fn flatten(&self, n: &Value, depth: usize, out: &mut Vec<Row>) {
        let row = self.tree_row(n, depth);
        let open = row.expanded;
        out.push(row);
        if open {
            for c in children(n) {
                self.flatten(c, depth + 1, out);
            }
        }
    }
}
