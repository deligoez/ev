//! The rows of the list on the left: the tree, the lists and the to-do sections.

use super::*;

impl App {
    /// Recomputes the rows of the current tab, keeping the selection on the same node, and the
    /// details of what is selected.
    pub(super) fn rebuild(&mut self) -> Result<()> {
        self.rebuild_rows()?;
        self.load_details()
    }

    /// The rows alone, for a caller that selects another row next: the details of a row only
    /// passed through (the home, on every jump into the tree) cost a whole-house regroup.
    pub(super) fn rebuild_rows(&mut self) -> Result<()> {
        let keep = self.selected_id();
        let was = self.state.selected();
        self.rows = self.rows_of(self.tab)?;
        let found = keep.and_then(|id| self.rows.iter().position(|r| r.id == id));
        // A record that left a list (done, found, gone) while it was selected: its neighbour
        // is selected, and the status line says why the selection moved.
        if let (None, Some(id), Some(at)) = (found, keep, was)
            && id > 0
            && self.refreshing
            && self.tab != Tab::Tree
            && !self.rows.is_empty()
        {
            self.status = tf("#{} left this list", &[&id]);
            self.state.select(Some(at.min(self.rows.len() - 1)));
            return Ok(());
        }
        let idx = found
            // Start on the first real line, not on a section header.
            .or_else(|| {
                self.rows
                    .iter()
                    .position(|r| r.id >= 0)
                    .or(if self.rows.is_empty() { None } else { Some(0) })
            })
            .map(|i| i.min(self.rows.len().saturating_sub(1)));
        self.state.select(idx);
        Ok(())
    }

    /// The sidebar's counts: a list of one line per record is counted by building it, so the
    /// count and the list come from one query (spec/ui-sidebar.md); the past by its two lists,
    /// whose years may be closed.
    pub(super) fn count_lists(&mut self) -> Result<()> {
        for tab in [Tab::Pending, Tab::Disposals, Tab::Lost, Tab::Places] {
            let n = self.rows_of(tab)?.len();
            self.counts.insert(tab, n);
        }
        let v = self.inv.past(None, None)?;
        let n = ["remembered", "left_inventory"]
            .iter()
            .map(|k| v[k]["past"].as_array().map_or(0, Vec::len))
            .sum::<usize>()
            + v["homes_and_vehicles"].as_array().map_or(0, Vec::len);
        self.counts.insert(Tab::Past, n);
        self.count_purchases()
    }

    /// The rows of a list.
    pub(super) fn rows_of(&mut self, tab: Tab) -> Result<Vec<Row>> {
        Ok(match tab {
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
            Tab::Stats => self.stats_rows()?,
            Tab::Past => self.past_rows()?,
            Tab::Buys
            | Tab::BuysDurable
            | Tab::BuysClothing
            | Tab::BuysDigital
            | Tab::BuysService => self.purchase_rows(tab.bucket().flatten())?,
        })
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
        // Only a photo needed now: a place not counted yet gets its photo on its tour.
        let now: std::collections::HashSet<i64> = v["photos"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|p| p["when"] == "now")
            .filter_map(|p| p["id"].as_i64())
            .collect();
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
            })
            .into_iter()
            .filter(|r| now.contains(&r.id))
            .collect(),
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

    /// The Statistics tab: `ev stats` as one collapsible section per heading. A line that
    /// names a record opens it on the right; the other lines point at nothing (id 0).
    pub(super) fn stats_rows(&mut self) -> Result<Vec<Row>> {
        let v = self.inv.stats()?;
        let mut out = Vec::new();
        self.stat_drills.clear();
        for (i, (heading, lines)) in crate::render::stats_sections(&v).into_iter().enumerate() {
            let id = STATS_SECTION - i as i64;
            let open = !self.collapsed.contains(&id);
            out.push(Row {
                id,
                depth: 0,
                spans: vec![Span::styled(heading, Style::new().fg(pal().blue).bold())],
                expandable: true,
                expanded: open,
            });
            if open {
                for (node, text, drill) in lines {
                    let mut spans = vec![Span::raw(text.trim_start().to_string())];
                    // A figure with a list behind it, which Enter opens.
                    if let Some(d) = drill {
                        spans.push(Span::styled("  ›", Style::new().fg(pal().code)));
                        self.stat_drills.insert(out.len(), d);
                    }
                    out.push(Row {
                        id: node.unwrap_or(0),
                        depth: 1,
                        spans,
                        expandable: false,
                        expanded: false,
                    });
                }
            }
        }
        Ok(out)
    }

    /// The Past tab: a heading per year things left in, with how many and the money paid for
    /// them and got for them, and under it each thing that left that year.
    pub(super) fn past_rows(&mut self) -> Result<Vec<Row>> {
        let v = self.inv.past(None, None)?;
        let mut out = Vec::new();
        let sums = |m: &Value| -> String {
            m.as_object()
                .into_iter()
                .flatten()
                .map(|(c, a)| crate::render::amount(a.as_str().unwrap_or_default(), c))
                .collect::<Vec<_>>()
                .join(" + ")
        };
        // The homes and vehicles, here now or before, lead under a heading of their own
        // (spec/vehicles-homes.md).
        let homes = v["homes_and_vehicles"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !homes.is_empty() {
            let id = PAST_LIST - 2;
            let open = !self.collapsed.contains(&id);
            out.push(Row {
                id,
                depth: 0,
                spans: vec![Span::styled(
                    tf("Homes and vehicles ({})", &[&homes.len()]),
                    Style::new().fg(pal().blue).bold(),
                )],
                expandable: true,
                expanded: open,
            });
            if open {
                for h in &homes {
                    let line = crate::render::home_line(h);
                    // The id is the row's own, not repeated in it.
                    let line = line
                        .split_once(' ')
                        .map_or(line.as_str(), |(_, r)| r)
                        .trim_start();
                    out.push(Row {
                        id: h["id"].as_i64().unwrap_or_default(),
                        depth: 1,
                        spans: vec![Span::raw(line.to_string())],
                        expandable: false,
                        expanded: false,
                    });
                }
            }
        }
        // Two lists, remembered first (decided with the person, 2026-10-06), each a heading
        // that opens and closes, with its years under it.
        let lists = [
            (&v["remembered"], "Remembered ({})", 0),
            (&v["left_inventory"], "Left the inventory ({})", 1),
        ];
        for (list, label, n) in lists {
            let things = list["past"].as_array().cloned().unwrap_or_default();
            if things.is_empty() {
                continue;
            }
            let id = PAST_LIST - n;
            let open = !self.collapsed.contains(&id);
            out.push(Row {
                id,
                depth: 0,
                spans: vec![Span::styled(
                    tf(label, &[&things.len()]),
                    Style::new().fg(pal().blue).bold(),
                )],
                expandable: true,
                expanded: open,
            });
            if !open {
                continue;
            }
            // Each year, then those nothing says when they left.
            let undated = Some(&list["undated"]).filter(|u| u.is_object());
            for y in list["years"]
                .as_array()
                .into_iter()
                .flatten()
                .chain(undated)
            {
                let year = y["year"].as_i64();
                let id = PAST_SECTION - n * PAST_LIST_SPAN - year.unwrap_or_default();
                let open = !self.collapsed.contains(&id);
                let mut head = vec![tf("{} left", &[&y["left"]])];
                for (key, label) in [("paid", "paid {}"), ("got", "got {}")] {
                    let s = sums(&y[key]);
                    if !s.is_empty() {
                        head.push(tf(label, &[&s]));
                    }
                }
                let title = year.map_or_else(|| t("when not known").to_string(), |y| y.to_string());
                out.push(Row {
                    id,
                    depth: 1,
                    spans: vec![
                        Span::styled(format!("{title}  "), Style::new().fg(pal().blue)),
                        Span::styled(head.join(" · "), Style::new().fg(pal().muted)),
                    ],
                    expandable: true,
                    expanded: open,
                });
                if !open {
                    continue;
                }
                let in_year = |n: &&Value| {
                    n["left"]
                        .as_str()
                        .and_then(|l| l.get(..4)?.parse::<i64>().ok())
                        == year
                };
                for n in things.iter().filter(in_year) {
                    let mut what = vec![
                        crate::history::left_as(n["how"].as_str().unwrap_or_default()).to_string(),
                    ];
                    if let Some(w) = n["where"].as_str() {
                        what.push(tf("was in {}", &[&w]));
                    }
                    if let Some(g) = n["got"].as_object() {
                        what.push(tf(
                            "got {}",
                            &[&crate::render::amount(
                                g["price"].as_str().unwrap_or_default(),
                                g["currency"].as_str().unwrap_or_default(),
                            )],
                        ));
                    }
                    out.push(Row {
                        id: n["id"].as_i64().unwrap_or_default(),
                        depth: 2,
                        spans: vec![
                            Span::raw(str_of(n, "name")),
                            Span::styled(
                                format!("  {}", what.join(" · ")),
                                Style::new().fg(pal().mark),
                            ),
                        ],
                        expandable: false,
                        expanded: false,
                    });
                }
            }
        }
        if out.is_empty() {
            out.push(Row {
                id: 0,
                depth: 0,
                spans: vec![Span::styled(
                    t("(nothing past)"),
                    Style::new().fg(pal().muted),
                )],
                expandable: false,
                expanded: false,
            });
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
