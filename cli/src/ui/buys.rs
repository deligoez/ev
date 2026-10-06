//! The purchase lists (spec/ui-sidebar.md, phase 3): every line or one bucket's, grouped by
//! order, filtered by where a line stands and by words, with the totals of what is shown.

use super::*;

/// Which lines a purchase list shows by where they stand (`f` steps through them).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum BuyState {
    #[default]
    All,
    /// Something left to link and not dismissed.
    Open,
    /// Linked to a thing, all of it.
    Linked,
    Dismissed,
}

impl BuyState {
    pub(super) fn next(self) -> Self {
        match self {
            BuyState::All => BuyState::Open,
            BuyState::Open => BuyState::Linked,
            BuyState::Linked => BuyState::Dismissed,
            BuyState::Dismissed => BuyState::All,
        }
    }

    fn title(self) -> &'static str {
        match self {
            BuyState::All => t("all"),
            BuyState::Open => t("open"),
            BuyState::Linked => t("linked"),
            BuyState::Dismissed => t("dismissed"),
        }
    }

    fn keeps(self, p: &Value) -> bool {
        let dismissed = p["dismissed"].is_string();
        let open = p["open_qty"].as_i64().unwrap_or(0) > 0;
        match self {
            BuyState::All => true,
            BuyState::Open => !dismissed && open,
            BuyState::Linked => p["linked"].as_array().is_some_and(|l| !l.is_empty()),
            BuyState::Dismissed => dismissed,
        }
    }
}

/// The headings of orders count down from here, by their first line's id, clear of every
/// other heading.
const ORDER_SECTION: i64 = -1_000_000;

/// An amount as hundredths, so totals add up exactly; `None` for one ev cannot read.
fn cents(paid: &str) -> Option<i64> {
    let (sign, digits) = paid.strip_prefix('-').map_or((1, paid), |d| (-1, d));
    let (whole, frac) = digits.split_once('.').unwrap_or((digits, ""));
    let frac = format!("{frac:0<2}");
    let n = whole.parse::<i64>().ok()? * 100 + frac.get(..2)?.parse::<i64>().ok()?;
    Some(sign * n)
}

/// Totals per currency, in the reader's way of writing amounts, joined with `+`.
fn totals<'a>(lines: impl Iterator<Item = &'a Value>) -> String {
    let mut sums: std::collections::BTreeMap<String, i64> = Default::default();
    for p in lines {
        if let (Some(paid), Some(c)) = (p["paid"].as_str(), p["currency"].as_str())
            && let Some(n) = cents(paid)
        {
            *sums.entry(c.to_string()).or_default() += n;
        }
    }
    sums.iter()
        .map(|(c, n)| {
            let sign = if *n < 0 { "-" } else { "" };
            let n = n.abs();
            crate::render::amount(&format!("{sign}{}.{:02}", n / 100, n % 100), c)
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

impl App {
    /// Every purchase line, read once per change of the data and shared by the five lists and
    /// their counts.
    pub(super) fn purchase_lines(&mut self) -> Result<&Vec<Value>> {
        if self.purchases.is_none() {
            let v = self.inv.buy_list(false, None, None, None)?;
            self.purchases = Some(v["purchases"].as_array().cloned().unwrap_or_default());
        }
        Ok(self.purchases.as_ref().expect("read above"))
    }

    /// The sidebar's purchase counts: every line of the bucket, whatever the filter. Counted in
    /// the database: reading every line took most of a second, at start and on every change.
    pub(super) fn count_purchases(&mut self) -> Result<()> {
        let counts = self.inv.buy_counts()?;
        for tab in sidebar_lists() {
            if let Some(bucket) = tab.bucket() {
                let n = match bucket {
                    Some(b) => counts[b].as_u64().unwrap_or(0),
                    None => counts
                        .as_object()
                        .into_iter()
                        .flatten()
                        .filter_map(|(_, n)| n.as_u64())
                        .sum(),
                };
                self.counts.insert(tab, n as usize);
            }
        }
        Ok(())
    }

    /// The lines of a purchase list, newest first; the lines of one order under a heading
    /// with its total, which opens and closes. The list's title counts them and adds them up.
    pub(super) fn purchase_rows(&mut self, bucket: Option<&str>) -> Result<Vec<Row>> {
        let state = self.buy_state;
        let (year, shop) = (self.buy_year.clone(), self.buy_shop.clone());
        let words: Vec<String> = ev_core::fold(&self.buy_words)
            .split_whitespace()
            .map(str::to_string)
            .collect();
        let shown: Vec<Value> = self
            .purchase_lines()?
            .iter()
            .filter(|p| bucket.is_none_or(|b| p["bucket"] == b))
            .filter(|p| state.keeps(p))
            .filter(|p| {
                year.as_ref()
                    .is_none_or(|y| date_of(p).starts_with(y.as_str()))
            })
            .filter(|p| shop.as_ref().is_none_or(|s| p["shop"] == s.as_str()))
            .filter(|p| {
                let text = ev_core::fold(
                    &["name", "shop", "brand", "order_no", "billed_to"]
                        .iter()
                        .filter_map(|k| p[*k].as_str())
                        .collect::<Vec<_>>()
                        .join(" "),
                );
                words.iter().all(|w| text.contains(w.as_str()))
            })
            .cloned()
            .collect();
        let mut title = tf(
            " {} · {} lines · {} · {} ",
            &[
                &self.tab.title(),
                &shown.len(),
                &totals(shown.iter()),
                &state.title(),
            ],
        );
        // What a figure of the statistics narrowed it to.
        for narrowed in [&year, &shop].into_iter().flatten() {
            title.push_str(&format!("· {narrowed} "));
        }
        if !self.buy_words.is_empty() {
            title.push_str(&tf("· \"{}\" ", &[&self.buy_words]));
        }
        self.purchase_title = title;
        // The lines of one order together, where its first line is.
        let mut orders: Vec<Vec<&Value>> = Vec::new();
        let mut at: HashMap<String, usize> = HashMap::new();
        for p in &shown {
            let key = match p["order_no"].as_str() {
                Some(o) => format!("{}\u{1f}{o}", p["shop"].as_str().unwrap_or_default()),
                None => format!("\u{1f}#{}", p["id"]),
            };
            match at.get(&key) {
                Some(&i) => orders[i].push(p),
                None => {
                    at.insert(key, orders.len());
                    orders.push(vec![p]);
                }
            }
        }
        let mut out = Vec::new();
        for lines in orders {
            if lines.len() == 1 {
                out.push(self.purchase_row(lines[0], 0));
                continue;
            }
            let first = lines
                .iter()
                .filter_map(|p| p["id"].as_i64())
                .min()
                .unwrap_or(0);
            let id = ORDER_SECTION - first;
            let open = !self.collapsed.contains(&id);
            let p = lines[0];
            out.push(Row {
                id,
                depth: 0,
                spans: vec![
                    Span::styled(date_of(p).to_string(), Style::new().fg(pal().muted)),
                    Span::styled(
                        format!("  {}", p["shop"].as_str().unwrap_or_default()),
                        Style::new().fg(pal().code).bold(),
                    ),
                    Span::styled(
                        tf(
                            "  order {} · {} lines · {}",
                            &[
                                &p["order_no"].as_str().unwrap_or_default(),
                                &lines.len(),
                                &totals(lines.iter().copied()),
                            ],
                        ),
                        Style::new().bold(),
                    ),
                ],
                expandable: true,
                expanded: open,
            });
            if open {
                out.extend(lines.iter().map(|p| self.purchase_row(p, 1)));
            }
        }
        Ok(out)
    }

    /// One line: its date, shop, name, quantity, price and where it stands.
    fn purchase_row(&self, p: &Value, depth: usize) -> Row {
        let muted = Style::new().fg(pal().muted);
        let mut spans = Vec::new();
        if depth == 0 {
            spans.push(Span::styled(format!("{}  ", date_of(p)), muted));
            if let Some(s) = p["shop"].as_str() {
                spans.push(Span::styled(format!("{s}  "), Style::new().fg(pal().code)));
            }
        }
        spans.push(Span::raw(
            p["name"].as_str().unwrap_or_default().to_string(),
        ));
        let qty = p["qty"].as_i64().unwrap_or(1);
        if qty > 1 {
            spans.push(Span::styled(format!(" ×{qty}"), Style::new().fg(pal().qty)));
        }
        if let (Some(paid), Some(c)) = (p["paid"].as_str(), p["currency"].as_str()) {
            spans.push(Span::styled(
                format!("  {}", crate::render::amount(paid, c)),
                Style::new().fg(pal().qty),
            ));
        }
        let linked: Vec<String> = p["linked"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|l| format!("→ {}", l["node"]["name"].as_str().unwrap_or_default()))
            .collect();
        if linked.is_empty() {
            spans.push(Span::styled(
                format!("  {}", crate::render::purchase_state(p)),
                if p["dismissed"].is_string() {
                    muted
                } else {
                    Style::new().fg(pal().mark)
                },
            ));
        } else {
            spans.push(Span::styled(
                format!("  {}", linked.join(", ")),
                Style::new().fg(pal().done),
            ));
        }
        Row {
            id: p["id"].as_i64().unwrap_or(0),
            depth,
            spans,
            expandable: false,
            expanded: false,
        }
    }

    /// The thing a selected line is linked to, still here, for `Enter` to open in the tree.
    pub(super) fn purchase_thing(&self) -> Option<i64> {
        self.purchase.as_ref()?["purchase"]["linked"]
            .as_array()?
            .iter()
            .find(|l| l["node"]["state"] != "gone")
            .and_then(|l| l["node"]["id"].as_i64())
    }

    /// The details of the selected line, as `ev buy show` writes them.
    pub(super) fn draw_purchase(&mut self, f: &mut Frame, area: Rect) -> bool {
        let text = match &self.purchase {
            Some(v) => crate::render::human(v),
            None => String::new(),
        };
        let lines: Vec<Line> = text.lines().map(|l| Line::raw(l.to_string())).collect();
        self.details_area = area;
        self.detail_lines = lines.len();
        let inner = area.height.saturating_sub(2) as usize;
        self.detail_scroll = self
            .detail_scroll
            .min(self.detail_lines.saturating_sub(1) as u16);
        let scrolls = self.detail_lines > inner || self.detail_scroll > 0;
        let title = match &self.purchase {
            Some(v) => tf(" Purchase #{} ", &[&v["purchase"]["id"]]),
            None => t(" Details ").to_string(),
        };
        let mut block = Block::bordered()
            .title(title)
            .border_style(self.edge(Pane::Details, Some(Drag::Columns)));
        if scrolls {
            block = block.title_bottom(Line::from(t(" J/K scroll ")).fg(pal().muted));
        }
        f.render_widget(
            Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false })
                .scroll((self.detail_scroll, 0)),
            area,
        );
        scrolls
    }
}

/// A line's date: when it was ordered, else delivered.
fn date_of(p: &Value) -> &str {
    p["ordered_at"]
        .as_str()
        .or(p["delivered_at"].as_str())
        .map_or("", |d| d.get(..10).unwrap_or(d))
}
