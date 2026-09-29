//! `ev ui`: a read-only terminal browser that follows the database as it changes.
//!
//! It never writes the database. The one thing it writes is the display settings file, from
//! the Settings tab.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime};

use ev_core::{Error, Inventory, Result};
use ratatui::crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, KeyCode, KeyEvent, KeyModifiers, MouseButton,
    MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Tabs, Wrap};
use ratatui::{DefaultTerminal, Frame};
use ratatui_image::picker::Picker;
use ratatui_image::protocol::Protocol;
use ratatui_image::{Image, Resize};
use serde_json::Value;

use crate::i18n::{self, Lang, t, tf};
use crate::input::{self, Input};
use crate::settings::{self, LangPref, Settings, ThemePref};
use crate::theme::{self, Mode, pal};

const POLL: Duration = Duration::from_millis(500);
const HIGHLIGHT_FOR: Duration = Duration::from_secs(6);
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// How long a lone ESC waits for the rest of a sequence before it counts as the Esc key.
const ESC_WAIT: Duration = Duration::from_millis(30);
/// How often a terminal without mode 2031 is asked for its background again.
const BACKGROUND_POLL: Duration = Duration::from_secs(3);

/// Tab titles in the current language.
fn tab_titles() -> [&'static str; 8] {
    [
        t("Tree"),
        t("Pending"),
        t("Leaving"),
        t("Lost"),
        t("Errands"),
        t("Search"),
        t("To do"),
        t("Settings"),
    ]
}

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
    Places,
    Search,
    Plan,
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
            Tab::Places,
            Tab::Search,
            Tab::Plan,
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
        "mistake" => "kayıt hatası",
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
    if n["unknown"] == true {
        out.push(Span::styled("  [içi sayılmadı]", Style::new().fg(LOST)));
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
    picker: Option<Picker>,
    photo_idx: usize,
    decoded: HashMap<String, Option<image::DynamicImage>>,
    /// The photo on screen, keyed by path, quarter turns and area.
    shown: Option<(String, u8, Rect, Protocol)>,
    /// Clockwise quarter turns per photo path (`r`/`R`), for this session only: the UI never
    /// writes, so the files and the database keep their orientation.
    rotation: HashMap<String, u8>,
    /// The selected node's current photo fills the screen (`o`); `O` hands it to the system.
    fullscreen: bool,
    photo_area: Rect,
    plan_title: String,
    /// Collapsed Yapılacak sections, by their (negative) header id.
    collapsed: HashSet<i64>,
    /// The time of the last `ev focus` request shown, so each is shown once.
    focus_seen: Option<String>,
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
            picker: None,
            photo_idx: 0,
            decoded: HashMap::new(),
            shown: None,
            rotation: HashMap::new(),
            fullscreen: false,
            photo_area: Rect::default(),
            plan_title: String::new(),
            collapsed: HashSet::from([UNCLEAR_SECTION]),
            focus_seen: None,
        };
        // A request made before this UI started is old news.
        app.focus_seen = app.inv.focus_request()?["at"].as_str().map(str::to_string);
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
            Tab::Places => {
                let v = self.inv.errands(None)?;
                let mut out = Vec::new();
                for e in v["errands"].as_array().into_iter().flatten() {
                    let place = str_of(&e["place"], "name");
                    for (key, what, color) in [
                        ("take", "götür", MARK),
                        ("return", "iade", Color::Blue),
                        ("collect", "geri al", Color::Blue),
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
    fn todo_rows(&mut self) -> Result<Vec<Row>> {
        let v = self.inv.todo()?;
        let c = &v["counts"];
        let p = &v["progress"];
        self.plan_title = format!(
            " Yapılacak · {}/{} gezildi · {} iş · {} taşıma ",
            p["toured"], p["units"], c["tasks"], c["moves"]
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
        let muted = |s: String| Span::styled(s, Style::new().fg(MUTED));
        type Section<'a> = (&'static str, Color, Vec<Row>);
        let mut sections: Vec<Section> = Vec::new();

        let tasks = v["tasks"].as_array().cloned().unwrap_or_default();
        sections.push((
            "İŞLER",
            MARK,
            tasks
                .iter()
                .map(|t| {
                    let mut spans = vec![muted(format!("{}. ", t["position"]))];
                    if t["status"] == "doing" {
                        spans.push(Span::styled("▶ ", Style::new().fg(MARK).bold()));
                    }
                    spans.push(Span::styled(str_of(t, "title"), Style::new().bold()));
                    spans.push(muted(format!("  — {}", str_of(t, "why"))));
                    item(t["nodes"][0]["id"].as_i64().unwrap_or(0), spans)
                })
                .collect(),
        ));
        sections.push((
            "TAŞIMALAR",
            Color::Blue,
            v["moves"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|m| {
                    item(
                        m["node"]["id"].as_i64().unwrap_or(0),
                        vec![
                            Span::raw(str_of(&m["node"], "name")),
                            Span::styled("  → ", Style::new().fg(MARK)),
                            Span::styled(
                                short(&str_of(&m["to"], "path_text")),
                                Style::new().fg(CODE),
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
                ("take", "götür"),
                ("return", "iade"),
                ("collect", "geri al"),
            ] {
                for n in e[key].as_array().into_iter().flatten() {
                    errands.push(item(
                        n["id"].as_i64().unwrap_or(0),
                        vec![
                            Span::styled(place.clone(), Style::new().bold()),
                            Span::styled(format!(" · {what}  "), Style::new().fg(MARK)),
                            Span::raw(str_of(n, "name")),
                        ],
                    ));
                }
            }
        }
        sections.push(("GÖTÜR / İADE", Color::Blue, errands));
        let mut leaving = Vec::new();
        for (d, list) in v["disposals"].as_object().into_iter().flatten() {
            for n in list.as_array().into_iter().flatten() {
                let mut spans = vec![
                    Span::styled(format!("{}  ", disposition_tr(d)), Style::new().fg(MARK)),
                    Span::raw(str_of(n, "name")),
                ];
                if let Some(st) = n["sale"]["value"].as_str() {
                    let st = if st == "listed" {
                        "ilanda"
                    } else {
                        "ayrıldı"
                    };
                    let price = n["sale"]["amount"]
                        .as_i64()
                        .map(|a| format!(" {a} TL"))
                        .unwrap_or_default();
                    spans.push(Span::styled(
                        format!("  [{st}{price}]"),
                        Style::new().fg(QTY).bold(),
                    ));
                }
                spans.push(muted(format!("  {}", within(&str_of(n, "path_text")))));
                leaving.push(item(n["id"].as_i64().unwrap_or(0), spans));
            }
        }
        sections.push(("ÇIKIŞ", MARK, leaving));
        sections.push((
            "ETİKET BASILACAK",
            CODE,
            v["labels"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|n| {
                    item(
                        n["id"].as_i64().unwrap_or(0),
                        vec![
                            Span::styled(str_of(n, "code"), Style::new().fg(CODE).bold()),
                            muted(format!("  {}", n["theme"].as_str().unwrap_or(""))),
                        ],
                    )
                })
                .collect(),
        ));
        sections.push((
            "ALINACAK",
            QTY,
            v["needs"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|n| {
                    let mut spans = Vec::new();
                    if let Some(q) = n["qty"].as_i64() {
                        spans.push(Span::styled(format!("{q} × "), Style::new().fg(QTY)));
                    }
                    spans.push(Span::raw(str_of(n, "text")));
                    if n["make"] == true {
                        spans.push(muted("  (bas / yap)".into()));
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
                        Some(p) => format!("  son görüldüğü: {}", short(p)),
                        None if n["node"].is_object() => "  (hiç bilinmiyor)".into(),
                        None => format!("  {}", within(&str_of(node, "path_text"))),
                    };
                    spans.push(muted(place));
                    item(node["id"].as_i64().unwrap_or(0), spans)
                })
                .collect()
        };
        sections.push((
            "TAMİR",
            LOST,
            plain("repairs", &|n| {
                n["note"]
                    .as_str()
                    .map(|x| Span::styled(format!("  ({x})"), Style::new().fg(LOST)))
            }),
        ));
        sections.push((
            "SON KULLANMA",
            LOST,
            plain("expiring", &|n| {
                let days = n["days_left"].as_i64().unwrap_or(0);
                let (text, color) = if days < 0 {
                    (format!("  {} · geçti", str_of(n, "expires")), LOST)
                } else {
                    (
                        format!("  {} · {days} gün", str_of(n, "expires")),
                        FURNITURE,
                    )
                };
                Some(Span::styled(text, Style::new().fg(color).bold()))
            }),
        ));
        sections.push(("KAYIP", LOST, plain("lost", &|_| None)));
        sections.push(("İÇİ BİLİNMİYOR", FURNITURE, plain("unknown", &|_| None)));
        sections.push((
            "GEZİLDİKTEN SONRA DEĞİŞTİ",
            FURNITURE,
            plain("stale", &|_| None),
        ));
        sections.push((
            "FOTOĞRAF GEREKLİ",
            CODE,
            plain("photos", &|n| {
                let text = if n["photo_reason"] == "none" {
                    "  hiç fotoğrafı yok".to_string()
                } else {
                    format!(
                        "  fotoğraftan sonra değişti ({})",
                        n["changed_at"]
                            .as_str()
                            .unwrap_or("")
                            .get(..10)
                            .unwrap_or("")
                    )
                };
                Some(Span::styled(text, Style::new().fg(CODE)))
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
                        format!("{} kayıtta aynı tam fotoğraf  ", names.len()),
                        Style::new().fg(LOST).bold(),
                    ),
                    muted(names.join(", ")),
                ],
            ));
        }
        sections.push(("KESİLMEMİŞ ORTAK FOTOĞRAF", LOST, shared));
        sections.push(("BELİRSİZ KAYITLAR", MUTED, plain("unclear", &|_| None)));

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
                    Span::styled(format!("  {}", rows.len()), Style::new().fg(MUTED)),
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

    fn toggle_section(&mut self, id: i64) -> Result<()> {
        if !self.collapsed.remove(&id) {
            self.collapsed.insert(id);
        }
        self.rebuild()
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
        let before = self.details.as_ref().map(|d| d["node"]["id"].clone());
        let now = self.selected_id().map(|i| serde_json::json!(i));
        if before != now {
            self.photo_idx = 0;
        }
        self.details = match self.selected_id() {
            Some(id) if id > 0 => Some(self.inv.show(&id.to_string(), true)?),
            _ => None,
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
        self.rebuild()?;
        self.apply_focus()
    }

    /// Shows what `ev focus` asked for: the node in the tree and, when a photo was named, that
    /// photo full screen. Each request is shown once; the UI never writes, so it remembers the
    /// request's time instead of clearing it.
    fn apply_focus(&mut self) -> Result<()> {
        let req = self.inv.focus_request()?;
        let at = req["at"].as_str().map(str::to_string);
        if at.is_none() || at == self.focus_seen {
            return Ok(());
        }
        self.focus_seen = at;
        let Some(id) = req["id"].as_i64() else {
            return Ok(());
        };
        self.fullscreen = false;
        self.reveal(id)?;
        if let Some(n) = req["photo"].as_i64() {
            self.photo_idx = (n - 1).max(0) as usize;
            self.fullscreen = true;
        }
        self.status = format!(
            "gösteriliyor: {}",
            self.snap.label.get(&id).cloned().unwrap_or_default()
        );
        Ok(())
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
        if id <= 0 {
            return Ok(());
        }
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
        if id < 0 {
            return self.toggle_section(id);
        }
        if self.tab != Tab::Tree {
            return self.reveal(id);
        }
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
        self.rebuild()
    }

    /// Forgets the query and its results; the Ara tab is left empty rather than stale.
    fn clear_search(&mut self) -> Result<()> {
        self.query.clear();
        self.search_rows.clear();
        self.status = "arama temizlendi".into();
        if self.tab == Tab::Search {
            self.rebuild()?;
        }
        Ok(())
    }

    fn has_search(&self) -> bool {
        !self.query.is_empty() || !self.search_rows.is_empty()
    }

    fn step_photo(&mut self, delta: isize) {
        let last = self.photo_count().saturating_sub(1);
        let cur = self.photo_idx.min(last) as isize;
        self.photo_idx = (cur + delta).clamp(0, last as isize) as usize;
    }

    fn open_external(&mut self) {
        if let Some(p) = self.current_photo() {
            let _ = std::process::Command::new("open").arg(p).spawn();
        }
    }

    /// Turns the current photo on screen by `quarters` clockwise quarter turns.
    fn rotate(&mut self, quarters: u8) {
        if let Some(p) = self.current_photo() {
            let r = self.rotation.entry(p).or_default();
            *r = (*r + quarters) % 4;
        }
    }

    fn key(&mut self, k: KeyEvent) -> Result<()> {
        if self.fullscreen {
            match k.code {
                KeyCode::Esc | KeyCode::Char('q' | 'o') => self.fullscreen = false,
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.quit = true
                }
                KeyCode::Char(']') | KeyCode::Right | KeyCode::Char('l') => self.step_photo(1),
                KeyCode::Char('[') | KeyCode::Left | KeyCode::Char('h') => self.step_photo(-1),
                KeyCode::Char('O') => self.open_external(),
                KeyCode::Char('r') => self.rotate(1),
                KeyCode::Char('R') => self.rotate(3),
                _ => {}
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
            KeyCode::PageDown => self.step(15)?,
            KeyCode::PageUp => self.step(-15)?,
            KeyCode::Home | KeyCode::Char('g') => self.select(0)?,
            KeyCode::End | KeyCode::Char('G') => self.select(usize::MAX)?,
            KeyCode::Tab => self.switch(Tab::from_index(self.tab.index() + 1))?,
            KeyCode::BackTab => self.switch(Tab::from_index(self.tab.index() + TABS.len() - 1))?,
            KeyCode::Char(c @ '1'..='7') => {
                self.switch(Tab::from_index(c as usize - '1' as usize))?
            }
            KeyCode::Char('/') => {
                self.searching = true;
                self.query.clear();
            }
            KeyCode::Char(']') => self.step_photo(1),
            KeyCode::Char('[') => self.step_photo(-1),
            KeyCode::Char('o') => self.fullscreen = self.current_photo().is_some(),
            KeyCode::Char('O') => self.open_external(),
            KeyCode::Char('r') => self.rotate(1),
            KeyCode::Char('R') => self.rotate(3),
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => {
                if let Some(id) = self.selected_id() {
                    if id < 0 {
                        if self.collapsed.contains(&id) || k.code == KeyCode::Enter {
                            self.toggle_section(id)?;
                        }
                    } else if self.tab == Tab::Tree {
                        self.expanded.insert(id);
                        self.rebuild()?;
                    } else {
                        self.reveal(id)?;
                    }
                }
            }
            KeyCode::Left | KeyCode::Char('h') if self.tab == Tab::Plan => {
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

    fn mouse(&mut self, m: MouseEvent) -> Result<()> {
        let inside = |r: Rect| {
            m.column >= r.x && m.column < r.x + r.width && m.row >= r.y && m.row < r.y + r.height
        };
        if self.fullscreen {
            match m.kind {
                MouseEventKind::ScrollDown => self.step_photo(1),
                MouseEventKind::ScrollUp => self.step_photo(-1),
                MouseEventKind::Down(MouseButton::Left) => self.fullscreen = false,
                _ => {}
            }
            return Ok(());
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

    /// The current photo over the whole screen, titled with the node, its place in the node's
    /// photos and the photo's own note.
    fn draw_fullscreen(&mut self, f: &mut Frame, path: &str) {
        let [main, bottom] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(f.area());
        let count = self.photo_count();
        let idx = self.photo_idx.min(count.saturating_sub(1));
        let node = self.details.as_ref().map(|d| d["node"].clone());
        let name = node.as_ref().map(|n| str_of(n, "name")).unwrap_or_default();
        let note = node
            .as_ref()
            .and_then(|n| n["id"].as_i64())
            .and_then(|id| self.inv.photo_list(&id.to_string()).ok())
            .and_then(|v| v["photos"][idx]["note"].as_str().map(str::to_string));
        let mut title = format!(" {name} · Fotoğraf {}/{count} ", idx + 1);
        if let Some(n) = note {
            title.push_str(&format!("· {n} "));
        }
        let block = Block::bordered().title(title);
        let inner = block.inner(main);
        f.render_widget(block, main);
        if self.picker.is_some() {
            self.render_photo(f, path, inner);
        } else {
            f.render_widget(
                Paragraph::new("(bu terminal resim gösteremiyor — O ile dışarıda aç)").fg(MUTED),
                inner,
            );
        }
        f.render_widget(
            Paragraph::new("[ ] ← → teker gez · r/R döndür · O dışarıda aç · Esc/o/tık kapat")
                .fg(MUTED),
            bottom,
        );
    }

    fn draw(&mut self, f: &mut Frame) {
        if self.fullscreen {
            if let Some(path) = self.current_photo() {
                return self.draw_fullscreen(f, &path);
            }
            self.fullscreen = false;
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
        let title = if self.tab == Tab::Search && self.has_search() {
            format!(" Ara: \"{}\" · ✕ temizle (x) ", self.query)
        } else if self.tab == Tab::Plan {
            self.plan_title.clone()
        } else {
            format!(" {} ", TABS[self.tab.index()])
        };
        let list = List::new(items)
            .block(Block::bordered().title(title))
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
        f.render_stateful_widget(list, left, &mut self.state);

        self.photo_area = Rect::default();
        let text_area = match (self.current_photo(), self.picker.is_some()) {
            (Some(path), true) => {
                let count = self.photo_count();
                let [img_area, rest] =
                    Layout::vertical([Constraint::Percentage(55), Constraint::Min(6)]).areas(right);
                let block = Block::bordered().title(format!(
                    " Fotoğraf {}/{}  ([ ] gez · r döndür · o tam ekran · O dışarıda aç) ",
                    self.photo_idx.min(count - 1) + 1,
                    count
                ));
                let inner = block.inner(img_area);
                self.photo_area = img_area;
                f.render_widget(block, img_area);
                self.render_photo(f, &path, inner);
                rest
            }
            _ => right,
        };
        let details = Paragraph::new(self.details_text())
            .block(Block::bordered().title(" Ayrıntı "))
            .wrap(Wrap { trim: false });
        f.render_widget(details, text_area);

        let help = if self.searching {
            format!(
                "Ara: {}▏  (Enter ara · Esc sil/vazgeç · Ctrl+U sil)",
                self.query
            )
        } else {
            format!(
                "↑↓ gez · → aç · ← kapat · Enter/çift tık git · Tab/1-7 sekme · / ara · q çık    {}",
                self.status
            )
        };
        f.render_widget(Paragraph::new(help).fg(MUTED), bottom);
    }

    fn photo_count(&self) -> usize {
        self.details
            .as_ref()
            .and_then(|d| d["node"]["photos"].as_array())
            .map_or(0, Vec::len)
    }

    fn current_photo(&self) -> Option<String> {
        let photos = self.details.as_ref()?["node"]["photos"].as_array()?.clone();
        if photos.is_empty() {
            return None;
        }
        photos[self.photo_idx.min(photos.len() - 1)]
            .as_str()
            .map(str::to_string)
    }

    /// Decodes once per path (downscaled), and re-encodes for the terminal only when the photo,
    /// its rotation or its area changes.
    fn render_photo(&mut self, f: &mut Frame, path: &str, area: Rect) {
        let Some(picker) = &self.picker else { return };
        let turns = self.rotation.get(path).copied().unwrap_or(0);
        let fresh =
            !matches!(&self.shown, Some((p, t, a, _)) if p == path && *t == turns && *a == area);
        if fresh {
            let img = self
                .decoded
                .entry(path.to_string())
                .or_insert_with(|| {
                    ev_core::open_upright(std::path::Path::new(path))
                        .ok()
                        .map(|i| i.thumbnail(1600, 1600))
                })
                .clone();
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
            Some((p, _, _, proto)) if p == path => f.render_widget(Image::new(proto), area),
            _ => f.render_widget(Paragraph::new("(fotoğraf açılamadı)").fg(MUTED), area),
        }
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
        let m = &v["marks"];
        if m["label"]["value"] == "needed" {
            field(
                "etiket",
                Span::styled("basılacak", Style::new().fg(CODE).bold()),
            );
        }
        if m["broken"].is_object() {
            let note = m["broken"]["note"].as_str().unwrap_or("");
            field(
                "bozuk",
                Span::styled(format!("tamir bekliyor {note}"), Style::new().fg(LOST)),
            );
        }
        if let Some(d) = m["expires"]["value"].as_str() {
            field(
                "son kullanma",
                Span::styled(d.to_string(), Style::new().fg(FURNITURE)),
            );
        }
        if let Some(st) = m["sale"]["value"].as_str() {
            let st = if st == "listed" {
                "ilanda"
            } else {
                "ayrıldı"
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
                "satış",
                Span::styled(format!("{st}{price}{at}"), Style::new().fg(QTY)),
            );
        }
        for nd in v["needs"].as_array().into_iter().flatten() {
            let q = nd["qty"]
                .as_i64()
                .map(|q| format!("{q} × "))
                .unwrap_or_default();
            field(
                "alınacak",
                Span::styled(format!("{q}{}", str_of(nd, "text")), Style::new().fg(QTY)),
            );
        }
        for t in v["tasks"].as_array().into_iter().flatten() {
            let via = if t["via"] == n["id"] {
                String::new()
            } else {
                let place = t["via"]
                    .as_i64()
                    .and_then(|i| self.snap.label.get(&i).cloned())
                    .unwrap_or_default();
                format!("  ({place} üzerinden)")
            };
            field(
                "görev",
                Span::styled(
                    format!("{}. {}{via}", t["position"], str_of(t, "title")),
                    Style::new().fg(MARK),
                ),
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
    app.picker = Some(Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks()));
    let _ = execute!(std::io::stdout(), EnableMouseCapture);
    let result = app.run(&mut terminal);
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

#[cfg(test)]
mod tests {
    use super::{App, Tab, tab_at};
    use ev_core::{Inventory, NewNode};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};
    use ratatui_image::picker::Picker;

    fn screen(term: &Terminal<TestBackend>) -> String {
        let buf = term.backend().buffer();
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn the_selected_node_shows_its_photo_panel() {
        let dir = tempfile::tempdir().unwrap();
        let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
        inv.add(NewNode {
            name: "Ev".into(),
            kind: "home".into(),
            ..Default::default()
        })
        .unwrap();
        let img = image::RgbImage::from_pixel(64, 32, image::Rgb([200, 50, 50]));
        let photo = dir.path().join("p.png");
        img.save(&photo).unwrap();
        inv.photo_add("Ev", &photo, None, None).unwrap();
        let mut app = App::new(inv).unwrap();
        app.picker = Some(Picker::halfblocks());
        let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        let s = screen(&term);
        assert!(s.contains("Fotoğraf 1/1"), "{s}");
        assert!(s.contains("Ayrıntı"));
    }

    fn press(app: &mut App, c: KeyCode) {
        app.key(KeyEvent::new(c, KeyModifiers::NONE)).unwrap();
    }

    #[test]
    fn o_opens_the_photo_full_screen_and_esc_closes_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
        inv.add(NewNode {
            name: "Ev".into(),
            kind: "home".into(),
            ..Default::default()
        })
        .unwrap();
        let photo = dir.path().join("p.png");
        image::RgbImage::from_pixel(64, 32, image::Rgb([200, 50, 50]))
            .save(&photo)
            .unwrap();
        inv.photo_add("Ev", &photo, None, Some("ilk")).unwrap();
        inv.photo_add("Ev", &photo, None, Some("ikinci")).unwrap();
        let mut app = App::new(inv).unwrap();
        app.picker = Some(Picker::halfblocks());
        let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();

        press(&mut app, KeyCode::Char('o'));
        press(&mut app, KeyCode::Char(']'));
        press(&mut app, KeyCode::Char(']'));
        term.draw(|f| app.draw(f)).unwrap();
        let s = screen(&term);
        assert!(s.contains("Ev · Fotoğraf 2/2 · ikinci"), "{s}");
        assert!(!s.contains("Ayrıntı"), "{s}");

        press(&mut app, KeyCode::Esc);
        assert!(!app.quit);
        term.draw(|f| app.draw(f)).unwrap();
        assert!(screen(&term).contains("Ayrıntı"));
    }

    #[test]
    fn r_and_shift_r_rotate_the_full_screen_photo_for_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
        inv.add(NewNode {
            name: "Ev".into(),
            kind: "home".into(),
            ..Default::default()
        })
        .unwrap();
        let photo = dir.path().join("p.png");
        image::RgbImage::from_pixel(64, 32, image::Rgb([200, 50, 50]))
            .save(&photo)
            .unwrap();
        inv.photo_add("Ev", &photo, None, None).unwrap();
        let mut app = App::new(inv).unwrap();
        app.picker = Some(Picker::halfblocks());
        let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();

        press(&mut app, KeyCode::Char('o'));
        let path = app.current_photo().unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        press(&mut app, KeyCode::Char('r'));
        assert_eq!(app.rotation.get(&path), Some(&1));
        term.draw(|f| app.draw(f)).unwrap();
        assert!(matches!(&app.shown, Some((_, 1, _, _))));

        press(&mut app, KeyCode::Char('R'));
        press(&mut app, KeyCode::Char('R'));
        assert_eq!(app.rotation.get(&path), Some(&3));
        term.draw(|f| app.draw(f)).unwrap();
        assert!(matches!(&app.shown, Some((_, 3, _, _))));
        assert!(screen(&term).contains("r/R döndür"));

        // Closing and reopening keeps the turn for the rest of the session.
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('o'));
        assert_eq!(app.rotation.get(&path), Some(&3));
    }

    #[test]
    fn the_plan_tab_lists_tasks_with_progress() {
        let dir = tempfile::tempdir().unwrap();
        let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
        for (name, kind, parent) in [
            ("Ev", "home", None),
            ("Oda", "room", Some("Ev")),
            ("Kutu", "container", Some("Oda")),
        ] {
            inv.add(NewNode {
                name: name.into(),
                kind: kind.into(),
                parent: parent.map(Into::into),
                ..Default::default()
            })
            .unwrap();
        }
        inv.task_add("Kutuyu aç", "hiç açılmadı", &["Kutu".into()], None)
            .unwrap();
        let mut app = App::new(inv).unwrap();
        press(&mut app, KeyCode::Char('7'));
        let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        let s = screen(&term);
        assert!(s.contains("0/1 gezildi"), "{s}");
        assert!(s.contains("İŞLER"), "{s}");
        assert!(s.contains("1. Kutuyu aç"), "{s}");
        // The task's place is shown on the right.
        assert!(s.contains("Ev › Oda › Kutu"), "{s}");
    }

    #[test]
    fn ev_focus_from_another_process_shows_the_node_and_photo() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ev.db");
        let mut inv = Inventory::open(&db).unwrap();
        for (name, kind, parent) in [
            ("Ev", "home", None),
            ("Oda", "room", Some("Ev")),
            ("Kutu", "container", Some("Oda")),
            ("Çekiç", "item", Some("Kutu")),
        ] {
            inv.add(NewNode {
                name: name.into(),
                kind: kind.into(),
                parent: parent.map(Into::into),
                ..Default::default()
            })
            .unwrap();
        }
        let photo = dir.path().join("p.png");
        image::RgbImage::from_pixel(16, 16, image::Rgb([9, 9, 9]))
            .save(&photo)
            .unwrap();
        inv.photo_add("Çekiç", &photo, None, Some("yakın")).unwrap();
        let mut app = App::new(inv).unwrap();
        press(&mut app, KeyCode::Char('3'));
        assert!(!app.fullscreen);

        // The agent points at the hammer from its own connection.
        let mut agent = Inventory::open(&db).unwrap();
        agent.focus(Some("Çekiç"), None).unwrap();
        app.refresh_if_changed().unwrap();
        assert!(app.tab == Tab::Tree);
        let hammer = agent.resolve("Çekiç", false).unwrap();
        assert_eq!(app.selected_id(), Some(hammer));
        assert!(app.fullscreen);

        // Shown once: closing it does not bring it back on the next refresh.
        press(&mut app, KeyCode::Esc);
        agent.observe("Kutu", "x", None).unwrap();
        app.refresh_if_changed().unwrap();
        assert!(!app.fullscreen);
    }

    #[test]
    fn a_search_can_be_cleared_without_quitting() {
        let dir = tempfile::tempdir().unwrap();
        let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
        inv.add(NewNode {
            name: "Ev".into(),
            kind: "home".into(),
            ..Default::default()
        })
        .unwrap();
        let mut app = App::new(inv).unwrap();
        let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Char('e'));
        press(&mut app, KeyCode::Char('v'));
        press(&mut app, KeyCode::Enter);
        term.draw(|f| app.draw(f)).unwrap();
        assert!(screen(&term).contains("✕ temizle"));

        press(&mut app, KeyCode::Esc);
        assert!(!app.quit);
        assert!(app.query.is_empty() && app.rows.is_empty());
        term.draw(|f| app.draw(f)).unwrap();
        assert!(!screen(&term).contains("✕ temizle"));
        // With nothing left to clear, Esc quits as before.
        press(&mut app, KeyCode::Esc);
        assert!(app.quit);
    }

    #[test]
    fn tab_titles_are_hit_by_their_columns() {
        // " 1 Ağaç " spans columns 0..8, then a divider, then " 2 Bekleyen ".
        assert!(tab_at(0) == Some(Tab::Tree));
        assert!(tab_at(7) == Some(Tab::Tree));
        assert!(tab_at(8).is_none());
        assert!(tab_at(9) == Some(Tab::Pending));
    }
}
