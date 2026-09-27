//! `ev ui`: a read-only terminal browser that follows the database as it changes.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use ev_core::{Error, Inventory, Result};
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Tabs, Wrap};
use ratatui::{DefaultTerminal, Frame};
use serde_json::Value;

const POLL: Duration = Duration::from_millis(500);
const HIGHLIGHT_FOR: Duration = Duration::from_secs(6);
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const TABS: [&str; 6] = ["Ağaç", "Bekleyen", "Çıkış", "Kayıp", "Yerler", "Ara"];

// Named colours follow the terminal's own palette, so light and dark themes both work.
const CODE: Color = Color::Cyan;
const FURNITURE: Color = Color::Yellow;
const QTY: Color = Color::Green;
const MARK: Color = Color::Magenta;
const LOST: Color = Color::Red;
const MUTED: Color = Color::DarkGray;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Tree,
    Pending,
    Disposals,
    Lost,
    Search,
}

impl Tab {
    fn index(self) -> usize {
        self as usize
    }
    fn from_index(i: usize) -> Self {
        [
            Tab::Tree,
            Tab::Pending,
            Tab::Disposals,
            Tab::Lost,
            Tab::Search,
        ][i % TABS.len()]
    }
}

/// One visible line of a list: the node it points at and how to draw it.
#[derive(Clone)]
struct Row {
    id: i64,
    depth: usize,
    spans: Vec<Span<'static>>,
    expandable: bool,
    expanded: bool,
}

/// Everything learned from one full read of the tree.
#[derive(Default)]
struct Snapshot {
    roots: Vec<Value>,
    unplaced: Vec<Value>,
    parent: HashMap<i64, i64>,
    label: HashMap<i64, String>,
    signature: HashMap<i64, String>,
}

impl Snapshot {
    fn load(inv: &Inventory) -> Result<Self> {
        let v = inv.tree(None, None)?;
        let mut s = Snapshot {
            roots: v["tree"].as_array().cloned().unwrap_or_default(),
            unplaced: v["unplaced"].as_array().cloned().unwrap_or_default(),
            ..Default::default()
        };
        for r in s.roots.clone() {
            s.index(&r, None);
        }
        for u in s.unplaced.clone() {
            let id = u["id"].as_i64().unwrap_or_default();
            s.label.insert(id, label(&u));
            s.signature.insert(id, signature(&u, None));
        }
        Ok(s)
    }

    fn index(&mut self, n: &Value, parent: Option<i64>) {
        let id = n["id"].as_i64().unwrap_or_default();
        if let Some(p) = parent {
            self.parent.insert(id, p);
        }
        self.label.insert(id, label(n));
        self.signature.insert(id, signature(n, parent));
        for c in children(n) {
            self.index(c, Some(id));
        }
    }
}

fn children(n: &Value) -> &[Value] {
    n["children"].as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn str_of(n: &Value, key: &str) -> String {
    n[key].as_str().unwrap_or_default().to_string()
}

fn label(n: &Value) -> String {
    match n["code"].as_str() {
        Some(c) => format!("{c}  {}", str_of(n, "name")),
        None => str_of(n, "name"),
    }
}

fn signature(n: &Value, parent: Option<i64>) -> String {
    format!(
        "{parent:?}|{}|{}|{}|{}",
        n["updated_at"], n["state"], n["lost"], n["pending_to"]
    )
}

fn disposition_tr(d: &str) -> &'static str {
    match d {
        "trash" => "çöp",
        "give" => "ver",
        "sell" => "sat",
        "return" => "iade",
        _ => "?",
    }
}

fn name_style(n: &Value) -> Style {
    let s = match n["kind"].as_str() {
        Some("home" | "room") => Style::new().bold(),
        Some("furniture") => Style::new().fg(FURNITURE),
        _ => Style::new(),
    };
    if n["state"] == "candidate" {
        s.fg(MUTED)
    } else {
        s
    }
}

/// Code, name and state markers of a node as coloured spans.
fn node_spans(n: &Value, snap: &Snapshot) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    if let Some(c) = n["code"].as_str() {
        out.push(Span::styled(c.to_string(), Style::new().fg(CODE)));
        out.push(Span::raw("  "));
    }
    out.push(Span::styled(str_of(n, "name"), name_style(n)));
    out.extend(marker_spans(n, snap));
    out
}

fn marker_spans(n: &Value, snap: &Snapshot) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    if let Some(q) = n["qty"].as_i64() {
        out.push(Span::styled(format!("  ×{q}"), Style::new().fg(QTY)));
    }
    if n["state"] == "candidate" {
        let d = disposition_tr(n["disposition"].as_str().unwrap_or_default());
        out.push(Span::styled(format!("  [{d}]"), Style::new().fg(MARK)));
    }
    if n["lost"] == true {
        out.push(Span::styled("  [kayıp]", Style::new().fg(LOST)));
    }
    if let Some(x) = n["to"].as_str() {
        out.push(Span::styled(format!("  ⇒ {x}"), Style::new().fg(MARK)));
    }
    if let Some(x) = n["owner"].as_str() {
        out.push(Span::styled(
            format!("  [sahibi: {x}]"),
            Style::new().fg(Color::Blue),
        ));
    }
    if let Some(x) = n["with"].as_str() {
        out.push(Span::styled(
            format!("  [{x}'de]"),
            Style::new().fg(Color::Blue),
        ));
    }
    if let Some(p) = n["pending_to"].as_i64() {
        let to = snap
            .label
            .get(&p)
            .cloned()
            .unwrap_or_else(|| format!("#{p}"));
        out.push(Span::styled(format!("  → {to}"), Style::new().fg(MARK)));
    }
    out
}

/// A path with its ancestors muted and its last segment prominent.
fn path_spans(path_text: &str) -> Vec<Span<'static>> {
    match path_text.rsplit_once(" › ") {
        Some((head, last)) => vec![
            Span::styled(format!("{head} › "), Style::new().fg(MUTED)),
            Span::raw(last.to_string()),
        ],
        None => vec![Span::raw(path_text.to_string())],
    }
}

struct App {
    inv: Inventory,
    snap: Snapshot,
    version: i64,
    tab: Tab,
    expanded: HashSet<i64>,
    rows: Vec<Row>,
    state: ListState,
    details: Option<Value>,
    changed: HashMap<i64, Instant>,
    searching: bool,
    query: String,
    search_rows: Vec<Row>,
    status: String,
    quit: bool,
    list_area: Rect,
    tabs_area: Rect,
    last_click: Option<(usize, Instant)>,
}

impl App {
    fn new(inv: Inventory) -> Result<Self> {
        let snap = Snapshot::load(&inv)?;
        let version = inv.data_version()?;
        let mut expanded = HashSet::new();
        // Open homes and rooms so the first screen shows the furniture.
        fn open(n: &Value, out: &mut HashSet<i64>) {
            if matches!(n["kind"].as_str(), Some("home" | "room")) {
                out.insert(n["id"].as_i64().unwrap_or_default());
                for c in children(n) {
                    open(c, out);
                }
            }
        }
        for r in &snap.roots {
            open(r, &mut expanded);
        }
        let mut app = App {
            inv,
            snap,
            version,
            tab: Tab::Tree,
            expanded,
            rows: Vec::new(),
            state: ListState::default().with_selected(Some(0)),
            details: None,
            changed: HashMap::new(),
            searching: false,
            query: String::new(),
            search_rows: Vec::new(),
            status: String::new(),
            quit: false,
            list_area: Rect::default(),
            tabs_area: Rect::default(),
            last_click: None,
        };
        app.rebuild()?;
        Ok(app)
    }

    fn selected_id(&self) -> Option<i64> {
        self.state
            .selected()
            .and_then(|i| self.rows.get(i))
            .map(|r| r.id)
    }

    /// Recomputes the rows of the current tab, keeping the selection on the same node.
    fn rebuild(&mut self) -> Result<()> {
        let keep = self.selected_id();
        self.rows = match self.tab {
            Tab::Tree => {
                let mut out = Vec::new();
                for r in &self.snap.roots {
                    self.flatten(r, 0, &mut out);
                }
                for u in &self.snap.unplaced {
                    let mut row = self.tree_row(u, 0);
                    row.spans
                        .insert(0, Span::styled("(yeri bilinmiyor) ", Style::new().fg(LOST)));
                    out.push(row);
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
                            vec![Span::styled(format!("  → {to}"), Style::new().fg(MARK))],
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
                            Style::new().fg(MARK),
                        )];
                        let parts = n["parts"].as_array().map_or(0, Vec::len);
                        if parts > 0 {
                            extra.push(Span::styled(
                                format!("  (+{parts} parça)"),
                                Style::new().fg(MUTED),
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
                            .unwrap_or("hiç bilinmiyor");
                        self.list_row(
                            &m["node"],
                            vec![Span::styled(
                                format!("  (son görüldüğü: {seen})"),
                                Style::new().fg(LOST),
                            )],
                        )
                    })
                    .collect()
            }
            Tab::Search => self.search_rows.clone(),
        };
        let idx = keep
            .and_then(|id| self.rows.iter().position(|r| r.id == id))
            .or(if self.rows.is_empty() { None } else { Some(0) })
            .map(|i| i.min(self.rows.len().saturating_sub(1)));
        self.state.select(idx);
        self.load_details()
    }

    fn tree_row(&self, n: &Value, depth: usize) -> Row {
        let id = n["id"].as_i64().unwrap_or_default();
        let kids = children(n).len();
        let expanded = self.expanded.contains(&id);
        let mut spans = node_spans(n, &self.snap);
        let total = n["items"].as_i64().unwrap_or(0);
        if total > 0 {
            spans.push(Span::styled(
                format!("  {total} eşya"),
                Style::new().fg(MUTED),
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

    fn list_row(&self, n: &Value, extra: Vec<Span<'static>>) -> Row {
        let mut spans = path_spans(n["path_text"].as_str().unwrap_or_default());
        spans.extend(extra);
        Row {
            id: n["id"].as_i64().unwrap_or_default(),
            depth: 0,
            spans,
            expandable: false,
            expanded: false,
        }
    }

    fn flatten(&self, n: &Value, depth: usize, out: &mut Vec<Row>) {
        let row = self.tree_row(n, depth);
        let open = row.expanded;
        out.push(row);
        if open {
            for c in children(n) {
                self.flatten(c, depth + 1, out);
            }
        }
    }

    fn load_details(&mut self) -> Result<()> {
        self.details = match self.selected_id() {
            Some(id) => Some(self.inv.show(&id.to_string(), true)?),
            None => None,
        };
        Ok(())
    }

    /// Re-reads everything when another process has written, and marks what changed.
    fn refresh_if_changed(&mut self) -> Result<()> {
        let v = self.inv.data_version()?;
        if v == self.version {
            return Ok(());
        }
        self.version = v;
        let next = Snapshot::load(&self.inv)?;
        let now = Instant::now();
        for (id, sig) in &next.signature {
            if self.snap.signature.get(id) != Some(sig) {
                self.changed.insert(*id, now);
                // Reveal what changed so it is visible without hunting for it.
                let mut p = next.parent.get(id).copied();
                while let Some(x) = p {
                    self.expanded.insert(x);
                    p = next.parent.get(&x).copied();
                }
            }
        }
        self.snap = next;
        self.status = format!("güncellendi {}", clock_now());
        if self.tab == Tab::Search && !self.query.is_empty() {
            self.run_search()?;
        }
        self.rebuild()
    }

    fn run_search(&mut self) -> Result<()> {
        let v = match self.inv.find(&self.query, None, None, false) {
            Ok(v) => v,
            Err(Error::Usage(m)) => {
                self.status = m;
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        self.search_rows = v["results"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|n| self.list_row(n, marker_spans(n, &self.snap)))
            .collect();
        self.status = format!("\"{}\": {} sonuç", self.query, self.search_rows.len());
        Ok(())
    }

    /// Opens the tree tab on `id`, expanding every ancestor.
    fn reveal(&mut self, id: i64) -> Result<()> {
        let mut p = self.snap.parent.get(&id).copied();
        while let Some(x) = p {
            self.expanded.insert(x);
            p = self.snap.parent.get(&x).copied();
        }
        self.tab = Tab::Tree;
        self.rows.clear();
        self.state.select(None);
        self.rebuild()?;
        if let Some(i) = self.rows.iter().position(|r| r.id == id) {
            self.state.select(Some(i));
        }
        self.load_details()
    }

    fn select(&mut self, i: usize) -> Result<()> {
        if self.rows.is_empty() {
            return Ok(());
        }
        self.state.select(Some(i.min(self.rows.len() - 1)));
        self.load_details()
    }

    fn step(&mut self, delta: isize) -> Result<()> {
        let cur = self.state.selected().unwrap_or(0) as isize;
        self.select((cur + delta).max(0) as usize)
    }

    fn switch(&mut self, tab: Tab) -> Result<()> {
        self.tab = tab;
        self.state.select(None);
        self.rebuild()
    }

    /// Expands or collapses in the tree; in a list, jumps to the node in the tree.
    fn activate(&mut self) -> Result<()> {
        let Some(id) = self.selected_id() else {
            return Ok(());
        };
        if self.tab != Tab::Tree {
            return self.reveal(id);
        }
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
        self.rebuild()
    }

    fn key(&mut self, k: KeyEvent) -> Result<()> {
        if self.searching {
            match k.code {
                KeyCode::Esc => self.searching = false,
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
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => self.quit = true,
            KeyCode::Down | KeyCode::Char('j') => self.step(1)?,
            KeyCode::Up | KeyCode::Char('k') => self.step(-1)?,
            KeyCode::PageDown => self.step(15)?,
            KeyCode::PageUp => self.step(-15)?,
            KeyCode::Home | KeyCode::Char('g') => self.select(0)?,
            KeyCode::End | KeyCode::Char('G') => self.select(usize::MAX)?,
            KeyCode::Tab => self.switch(Tab::from_index(self.tab.index() + 1))?,
            KeyCode::BackTab => self.switch(Tab::from_index(self.tab.index() + TABS.len() - 1))?,
            KeyCode::Char(c @ '1'..='5') => {
                self.switch(Tab::from_index(c as usize - '1' as usize))?
            }
            KeyCode::Char('/') => {
                self.searching = true;
                self.query.clear();
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => {
                if let Some(id) = self.selected_id() {
                    if self.tab == Tab::Tree {
                        self.expanded.insert(id);
                        self.rebuild()?;
                    } else {
                        self.reveal(id)?;
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

    fn mouse(&mut self, m: MouseEvent) -> Result<()> {
        let inside = |r: Rect| {
            m.column >= r.x && m.column < r.x + r.width && m.row >= r.y && m.row < r.y + r.height
        };
        match m.kind {
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

    fn draw(&mut self, f: &mut Frame) {
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
            Paragraph::new(Span::styled(
                brand,
                Style::new().bold().fg(Color::Black).bg(Color::Cyan),
            )),
            brand_area,
        );
        self.tabs_area = top;
        let tabs = Tabs::new(
            TABS.iter()
                .enumerate()
                .map(|(i, t)| format!("{} {t}", i + 1)),
        )
        .select(self.tab.index())
        .highlight_style(Style::new().bold().reversed());
        f.render_widget(tabs, top);

        let [left, right] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(body);
        self.list_area = left;
        let now = Instant::now();
        self.changed
            .retain(|_, t| now.duration_since(*t) < HIGHLIGHT_FOR);
        let flash = Style::new().bg(Color::Yellow).fg(Color::Black);
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
                    Span::styled(marker, Style::new().fg(MUTED)),
                ];
                spans.extend(r.spans.iter().cloned());
                if self.changed.contains_key(&r.id) {
                    spans = spans.into_iter().map(|s| s.patch_style(flash)).collect();
                }
                ListItem::new(Line::from(spans))
            })
            .collect();
        let list = List::new(items)
            .block(Block::bordered().title(format!(" {} ", TABS[self.tab.index()])))
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
        f.render_stateful_widget(list, left, &mut self.state);

        let details = Paragraph::new(self.details_text())
            .block(Block::bordered().title(" Ayrıntı "))
            .wrap(Wrap { trim: false });
        f.render_widget(details, right);

        let help = if self.searching {
            format!("Ara: {}▏  (Enter ara · Esc vazgeç)", self.query)
        } else {
            format!(
                "↑↓ gez · → aç · ← kapat · Enter/çift tık git · Tab/1-5 sekme · / ara · q çık    {}",
                self.status
            )
        };
        f.render_widget(Paragraph::new(help).fg(MUTED), bottom);
    }

    fn details_text(&self) -> Text<'static> {
        let Some(v) = &self.details else {
            return Text::from("(boş)");
        };
        let n = &v["node"];
        let mut title = path_spans(n["path_text"].as_str().unwrap_or_default());
        if let Some(last) = title.last_mut() {
            *last = last.clone().bold();
        }
        let mut lines = vec![Line::from(title), Line::raw("")];
        let mut field = |k: &str, val: Span<'static>| {
            lines.push(Line::from(vec![
                Span::styled(format!("{k}: "), Style::new().fg(MUTED)),
                val,
            ]))
        };
        field("tür", Span::raw(str_of(n, "kind")));
        field("id", Span::raw(n["id"].to_string()));
        if let Some(c) = n["code"].as_str() {
            field("kod", Span::styled(c.to_string(), Style::new().fg(CODE)));
        }
        if let Some(q) = n["qty"].as_i64() {
            field("adet", Span::styled(q.to_string(), Style::new().fg(QTY)));
        }
        let d = disposition_tr(n["disposition"].as_str().unwrap_or_default());
        match n["state"].as_str() {
            Some("candidate") => field(
                "durum",
                Span::styled(format!("aday ({d})"), Style::new().fg(MARK)),
            ),
            Some("gone") => field(
                "durum",
                Span::styled(format!("gitti ({d})"), Style::new().fg(MUTED)),
            ),
            _ => {}
        }
        if n["lost"] == true {
            let seen = v["last_seen"]["path_text"]
                .as_str()
                .unwrap_or("hiç bilinmiyor");
            field(
                "kayıp",
                Span::styled(format!("son görüldüğü: {seen}"), Style::new().fg(LOST)),
            );
        }
        if let Some(p) = v["pending"]["path_text"].as_str() {
            field(
                "gidecek",
                Span::styled(p.to_string(), Style::new().fg(MARK)),
            );
        }
        for (k, key) in [
            ("götürülecek", "to"),
            ("sahibi", "owner"),
            ("ödünçte", "with"),
            ("tema", "theme"),
            ("not", "note"),
            ("adres", "address"),
        ] {
            if let Some(x) = n[key].as_str() {
                field(k, Span::raw(x.to_string()));
            }
        }
        if let Some(fill) = n["fill"].as_i64() {
            field("doluluk", Span::raw(format!("%{fill}")));
        }
        let tags: Vec<&str> = n["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !tags.is_empty() {
            field("etiketler", Span::raw(tags.join(", ")));
        }
        for p in n["photos"].as_array().into_iter().flatten() {
            field(
                "foto",
                Span::raw(p.as_str().unwrap_or_default().to_string()),
            );
        }
        field(
            "güncellendi",
            Span::styled(str_of(n, "updated_at"), Style::new().fg(MUTED)),
        );
        let kids = v["children"].as_array().cloned().unwrap_or_default();
        if !kids.is_empty() {
            lines.push(Line::raw(""));
            lines.push(Line::from(format!("İçindekiler ({})", kids.len())).bold());
            for c in &kids {
                let mut spans = vec![Span::raw("  ")];
                spans.extend(node_spans(c, &self.snap));
                lines.push(Line::from(spans));
            }
        }
        Text::from(lines)
    }

    fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let io = |e: std::io::Error| Error::Internal(format!("terminal: {e}"));
        while !self.quit {
            terminal.draw(|f| self.draw(f)).map_err(io)?;
            if event::poll(POLL).map_err(io)? {
                match event::read().map_err(io)? {
                    Event::Key(k) if k.kind == KeyEventKind::Press => self.key(k)?,
                    Event::Mouse(m) => self.mouse(m)?,
                    _ => {}
                }
            } else {
                self.refresh_if_changed()?;
            }
        }
        Ok(())
    }
}

/// Which tab title sits at `x`, given ratatui's default padding of one space each side
/// and a one-cell divider between titles.
fn tab_at(x: u16) -> Option<Tab> {
    let mut start = 0u16;
    for (i, t) in TABS.iter().enumerate() {
        let width = format!("{} {t}", i + 1).chars().count() as u16 + 2;
        if x >= start && x < start + width {
            return Some(Tab::from_index(i));
        }
        start += width + 1;
    }
    None
}

fn clock_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let s = secs % 86_400;
    format!("{:02}:{:02}:{:02} UTC", s / 3600, (s % 3600) / 60, s % 60)
}

pub fn run(inv: Inventory) -> Result<()> {
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        return Err(Error::Usage("`ev ui` needs a terminal".into()));
    }
    let mut app = App::new(inv)?;
    let mut terminal = ratatui::init();
    let _ = execute!(std::io::stdout(), EnableMouseCapture);
    let result = app.run(&mut terminal);
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

#[cfg(test)]
mod tests {
    use super::{Tab, tab_at};

    #[test]
    fn tab_titles_are_hit_by_their_columns() {
        // " 1 Ağaç " spans columns 0..8, then a divider, then " 2 Bekleyen ".
        assert!(tab_at(0) == Some(Tab::Tree));
        assert!(tab_at(7) == Some(Tab::Tree));
        assert!(tab_at(8).is_none());
        assert!(tab_at(9) == Some(Tab::Pending));
    }
}
