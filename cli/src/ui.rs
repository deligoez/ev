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
const TABS: [&str; 5] = ["Ağaç", "Bekleyen", "Çıkış", "Kayıp", "Ara"];

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
        let roots = s.roots.clone();
        for r in &roots {
            s.index(r, None);
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

fn label(n: &Value) -> String {
    let name = n["name"].as_str().unwrap_or_default();
    match n["code"].as_str() {
        Some(c) => format!("{c}  {name}"),
        None => name.to_string(),
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
        _ => "?",
    }
}

fn suffix(n: &Value, snap: &Snapshot) -> String {
    let mut s = String::new();
    if let Some(q) = n["qty"].as_i64() {
        s.push_str(&format!("  ×{q}"));
    }
    if n["state"] == "candidate" {
        s.push_str(&format!(
            "  [{}]",
            disposition_tr(n["disposition"].as_str().unwrap_or_default())
        ));
    }
    if n["lost"] == true {
        s.push_str("  [kayıp]");
    }
    if let Some(p) = n["pending_to"].as_i64() {
        s.push_str(&format!(
            "  → {}",
            snap.label
                .get(&p)
                .cloned()
                .unwrap_or_else(|| format!("#{p}"))
        ));
    }
    s
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
                    out.push(self.row(u, 0, "(yeri bilinmiyor) "));
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
                        self.list_row(
                            &m["node"],
                            &format!("  → {}", m["to"]["path_text"].as_str().unwrap_or_default()),
                        )
                    })
                    .collect()
            }
            Tab::Disposals => {
                let v = self.inv.disposals(None)?;
                let mut out = Vec::new();
                for (d, list) in v["disposals"].as_object().into_iter().flatten() {
                    for n in list.as_array().into_iter().flatten() {
                        let parts = n["parts"].as_array().map_or(0, Vec::len);
                        let extra = if parts > 0 {
                            format!(" (+{parts} parça)")
                        } else {
                            String::new()
                        };
                        out.push(self.list_row(n, &format!("  [{}]{extra}", disposition_tr(d))));
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
                        self.list_row(&m["node"], &format!("  (son görüldüğü: {seen})"))
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

    fn row(&self, n: &Value, depth: usize, prefix: &str) -> Row {
        let id = n["id"].as_i64().unwrap_or_default();
        Row {
            id,
            depth,
            text: format!("{prefix}{}{}", label(n), suffix(n, &self.snap)),
            expandable: !children(n).is_empty(),
            expanded: self.expanded.contains(&id),
            muted: n["state"] == "candidate",
        }
    }

    fn list_row(&self, n: &Value, extra: &str) -> Row {
        Row {
            id: n["id"].as_i64().unwrap_or_default(),
            depth: 0,
            text: format!("{}{extra}", n["path_text"].as_str().unwrap_or_default()),
            expandable: false,
            expanded: false,
            muted: false,
        }
    }

    fn flatten(&self, n: &Value, depth: usize, out: &mut Vec<Row>) {
        let row = self.row(n, depth, "");
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
        self.status = format!("güncellendi {}", chrono_like_now());
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
            .map(|n| self.list_row(n, &suffix(n, &self.snap)))
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

    fn step(&mut self, delta: isize) -> Result<()> {
        if self.rows.is_empty() {
            return Ok(());
        }
        let cur = self.state.selected().unwrap_or(0) as isize;
        let next = (cur + delta).clamp(0, self.rows.len() as isize - 1) as usize;
        self.state.select(Some(next));
        self.load_details()
    }

    fn switch(&mut self, tab: Tab) -> Result<()> {
        self.tab = tab;
        self.state.select(None);
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
            KeyCode::Home | KeyCode::Char('g') => self.step(isize::MIN / 2)?,
            KeyCode::End | KeyCode::Char('G') => self.step(isize::MAX / 2)?,
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

    fn draw(&mut self, f: &mut Frame) {
        let [top, body, bottom] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(f.area());
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
        let now = Instant::now();
        self.changed
            .retain(|_, t| now.duration_since(*t) < HIGHLIGHT_FOR);
        let items: Vec<ListItem> = self
            .rows
            .iter()
            .map(|r| {
                let marker = match (r.expandable, r.expanded) {
                    (true, true) => "▾ ",
                    (true, false) => "▸ ",
                    _ => "  ",
                };
                let mut style = Style::new();
                if r.muted {
                    style = style.fg(Color::DarkGray);
                }
                if self.changed.contains_key(&r.id) {
                    style = style.bg(Color::Yellow).fg(Color::Black);
                }
                ListItem::new(Line::from(Span::styled(
                    format!("{}{marker}{}", "  ".repeat(r.depth), r.text),
                    style,
                )))
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
                "↑↓ gez · → aç · ← kapat · Enter git · Tab/1-5 sekme · / ara · q çık    {}",
                self.status
            )
        };
        f.render_widget(Paragraph::new(help).dim(), bottom);
    }

    fn details_text(&self) -> Text<'static> {
        let Some(v) = &self.details else {
            return Text::from("(boş)");
        };
        let n = &v["node"];
        let mut lines = vec![
            Line::from(n["path_text"].as_str().unwrap_or_default().to_string()).bold(),
            Line::raw(""),
        ];
        let mut field = |k: &str, val: String| {
            lines.push(Line::from(vec![format!("{k}: ").dim(), Span::raw(val)]))
        };
        field("tür", n["kind"].as_str().unwrap_or_default().to_string());
        field("id", n["id"].to_string());
        if let Some(c) = n["code"].as_str() {
            field("kod", c.to_string());
        }
        if let Some(q) = n["qty"].as_i64() {
            field("adet", q.to_string());
        }
        match n["state"].as_str() {
            Some("candidate") => field(
                "durum",
                format!(
                    "aday ({})",
                    disposition_tr(n["disposition"].as_str().unwrap_or_default())
                ),
            ),
            Some("gone") => field(
                "durum",
                format!(
                    "gitti ({})",
                    disposition_tr(n["disposition"].as_str().unwrap_or_default())
                ),
            ),
            _ => {}
        }
        if n["lost"] == true {
            let seen = v["last_seen"]["path_text"]
                .as_str()
                .unwrap_or("hiç bilinmiyor");
            field("kayıp", format!("son görüldüğü: {seen}"));
        }
        if let Some(p) = v["pending"]["path_text"].as_str() {
            field("gidecek", p.to_string());
        }
        for (k, key) in [("tema", "theme"), ("not", "note"), ("adres", "address")] {
            if let Some(x) = n[key].as_str() {
                field(k, x.to_string());
            }
        }
        if let Some(fill) = n["fill"].as_i64() {
            field("doluluk", format!("%{fill}"));
        }
        let tags: Vec<&str> = n["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !tags.is_empty() {
            field("etiketler", tags.join(", "));
        }
        for p in n["photos"].as_array().into_iter().flatten() {
            field("foto", p.as_str().unwrap_or_default().to_string());
        }
        field(
            "güncellendi",
            n["updated_at"].as_str().unwrap_or_default().to_string(),
        );
        let kids = v["children"].as_array().cloned().unwrap_or_default();
        if !kids.is_empty() {
            lines.push(Line::raw(""));
            lines.push(Line::from(format!("İçindekiler ({})", kids.len())).bold());
            for c in &kids {
                lines.push(Line::raw(format!(
                    "  {}{}",
                    label(c),
                    suffix(c, &self.snap)
                )));
            }
        }
        Text::from(lines)
    }

    fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let io = |e: std::io::Error| Error::Internal(format!("terminal: {e}"));
        while !self.quit {
            terminal.draw(|f| self.draw(f)).map_err(io)?;
            if event::poll(POLL).map_err(io)? {
                if let Event::Key(k) = event::read().map_err(io)?
                    && k.kind == KeyEventKind::Press
                {
                    self.key(k)?;
                }
            } else {
                self.refresh_if_changed()?;
            }
        }
        Ok(())
    }
}

fn chrono_like_now() -> String {
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
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}
