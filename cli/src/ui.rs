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
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::Protocol;
use ratatui_image::{Image, Resize};
use serde_json::Value;

use crate::i18n::{self, Lang, t, tf};
use crate::input::{self, Graphics, Input};
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
        t("Layout"),
        t("Pending"),
        t("Leaving"),
        t("Lost"),
        t("Errands"),
        t("Search"),
        t("To do"),
        t("Settings"),
    ]
}

/// The To do section that starts collapsed: unclear records are a long, low-priority list.
const UNCLEAR_SECTION: i64 = -14;
/// Rows of the Settings tab; their ids are negative like section headers, but far below them.
const SETTING_LANGUAGE: i64 = -1001;
const SETTING_THEME: i64 = -1002;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Tree,
    Pending,
    Disposals,
    Lost,
    Places,
    Search,
    Plan,
    Settings,
}

impl Tab {
    const ALL: [Tab; 8] = [
        Tab::Tree,
        Tab::Pending,
        Tab::Disposals,
        Tab::Lost,
        Tab::Places,
        Tab::Search,
        Tab::Plan,
        Tab::Settings,
    ];

    fn index(self) -> usize {
        self as usize
    }
    fn from_index(i: usize) -> Self {
        Tab::ALL[i % Tab::ALL.len()]
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
        "trash" => t("trash"),
        "give" => t("give"),
        "sell" => t("sell"),
        "return" => t("return"),
        "mistake" => t("record error"),
        _ => "?",
    }
}

fn kind_name(k: &str) -> &'static str {
    match k {
        "home" => t("home"),
        "room" => t("room"),
        "furniture" => t("furniture"),
        "container" => t("container"),
        "item" => t("item"),
        _ => "?",
    }
}

fn name_style(n: &Value) -> Style {
    let s = match n["kind"].as_str() {
        Some("home" | "room") => Style::new().bold(),
        Some("furniture") => Style::new().fg(pal().furniture),
        _ => Style::new(),
    };
    if n["state"] == "candidate" {
        s.fg(pal().muted)
    } else {
        s
    }
}

/// Code, name and state markers of a node as coloured spans.
fn node_spans(n: &Value, snap: &Snapshot) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    if let Some(c) = n["code"].as_str() {
        out.push(Span::styled(c.to_string(), Style::new().fg(pal().code)));
        out.push(Span::raw("  "));
    }
    out.push(Span::styled(str_of(n, "name"), name_style(n)));
    out.extend(marker_spans(n, snap));
    out
}

fn marker_spans(n: &Value, snap: &Snapshot) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    if let Some(q) = n["qty"].as_i64() {
        out.push(Span::styled(format!("  ×{q}"), Style::new().fg(pal().qty)));
    }
    if n["state"] == "candidate" {
        let d = disposition_tr(n["disposition"].as_str().unwrap_or_default());
        out.push(Span::styled(
            format!("  [{d}]"),
            Style::new().fg(pal().mark),
        ));
    }
    if n["lost"] == true {
        out.push(Span::styled(t("  [lost]"), Style::new().fg(pal().lost)));
    }
    if n["unknown"] == true {
        out.push(Span::styled(
            t("  [contents unknown]"),
            Style::new().fg(pal().lost),
        ));
    }
    if let Some(x) = n["to"].as_str() {
        out.push(Span::styled(
            format!("  ⇒ {x}"),
            Style::new().fg(pal().mark),
        ));
    }
    if let Some(x) = n["owner"].as_str() {
        out.push(Span::styled(
            tf("  [owner: {}]", &[&x]),
            Style::new().fg(pal().blue),
        ));
    }
    if let Some(x) = n["with"].as_str() {
        out.push(Span::styled(
            tf("  [with {}]", &[&x]),
            Style::new().fg(pal().blue),
        ));
    }
    if let Some(p) = n["pending_to"].as_i64() {
        let to = snap
            .label
            .get(&p)
            .cloned()
            .unwrap_or_else(|| format!("#{p}"));
        out.push(Span::styled(
            format!("  → {to}"),
            Style::new().fg(pal().mark),
        ));
    }
    out
}

/// A path with its ancestors muted and its last segment prominent.
fn path_spans(path_text: &str) -> Vec<Span<'static>> {
    match path_text.rsplit_once(" › ") {
        Some((head, last)) => vec![
            Span::styled(format!("{head} › "), Style::new().fg(pal().muted)),
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
    /// Display settings, their file, and the file's modification time when last read, so a
    /// change made with `ev settings` shows up without restarting.
    prefs: Settings,
    settings_path: Option<PathBuf>,
    settings_stamp: Option<SystemTime>,
    /// What the terminal said about its background, if anything yet.
    detected: Option<Mode>,
    /// The terminal sent a mode 2031 report, so it will say when the appearance changes.
    notified: bool,
    last_background_query: Instant,
    /// Answers to the picture-protocol query so far; `None` when no query was sent.
    probe: Option<ImageProbe>,
}

/// What the terminal said about pictures, gathered until its status report ends the answers.
#[derive(Default)]
struct ImageProbe {
    kitty: bool,
    sixel: bool,
    cell: Option<(u16, u16)>,
}

impl ImageProbe {
    /// The picker the answers call for, with ratatui-image's own preferences: kitty, then
    /// iTerm2 where the terminal is known to speak it, then sixel; half blocks without a cell
    /// size, since pictures are fitted to cells.
    fn picker(&self) -> Picker {
        let program = std::env::var("TERM_PROGRAM").unwrap_or_default();
        let konsole = std::env::var_os("KONSOLE_VERSION").is_some();
        let wezterm = std::env::var_os("WEZTERM_EXECUTABLE").is_some();
        let protocol = if konsole {
            ProtocolType::Halfblocks
        } else if self.kitty && !wezterm {
            ProtocolType::Kitty
        } else if wezterm || program == "iTerm.app" {
            ProtocolType::Iterm2
        } else if self.sixel {
            ProtocolType::Sixel
        } else {
            ProtocolType::Halfblocks
        };
        match self.cell {
            Some((w, h)) if w > 0 && h > 0 && protocol != ProtocolType::Halfblocks => {
                // Deprecated only in favour of the stdio query, which is what this replaces.
                #[allow(deprecated)]
                let mut p = Picker::from_fontsize((w, h).into());
                p.set_protocol_type(protocol);
                p
            }
            _ => Picker::halfblocks(),
        }
    }
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
            prefs: Settings::default(),
            settings_path: None,
            settings_stamp: None,
            detected: std::env::var("COLORFGBG")
                .ok()
                .and_then(|v| theme::from_colorfgbg(&v)),
            notified: false,
            last_background_query: Instant::now(),
            probe: None,
        };
        app.apply_prefs();
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
                    row.spans.insert(
                        0,
                        Span::styled(t("(place unknown) "), Style::new().fg(pal().lost)),
                    );
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
    fn todo_rows(&mut self) -> Result<Vec<Row>> {
        let v = self.inv.todo()?;
        let c = &v["counts"];
        let p = &v["progress"];
        self.plan_title = tf(
            " To do · {}/{} toured · {} tasks · {} moves ",
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
            t("CONTENTS UNKNOWN"),
            pal().furniture,
            plain("unknown", &|_| None),
        ));
        sections.push((
            t("CHANGED SINCE TOURED"),
            pal().furniture,
            plain("stale", &|_| None),
        ));
        sections.push((
            t("PHOTO NEEDED"),
            pal().code,
            plain("photos", &|n| {
                let text = if n["photo_reason"] == "none" {
                    t("  no photo at all").to_string()
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

    fn toggle_section(&mut self, id: i64) -> Result<()> {
        if !self.collapsed.remove(&id) {
            self.collapsed.insert(id);
        }
        self.rebuild()
    }

    /// The appearance in use: the fixed one, or what the terminal reported (dark until it
    /// says otherwise).
    fn effective_mode(&self) -> Mode {
        match self.prefs.theme {
            ThemePref::Fixed(m) => m,
            ThemePref::Auto => self.detected.unwrap_or(Mode::Dark),
        }
    }

    /// Makes the language and palette follow the settings; rows keep their colours and words,
    /// so callers rebuild afterwards.
    fn apply_prefs(&mut self) {
        i18n::set_lang(self.prefs.language.effective());
        theme::set_mode(self.effective_mode());
    }

    fn set_prefs(&mut self, prefs: Settings) -> Result<()> {
        self.prefs = prefs;
        self.apply_prefs();
        self.rebuild()
    }

    /// Reads the settings file again when it changed on disk (`ev settings` from another
    /// terminal, or an edit by hand).
    fn reload_settings(&mut self) -> Result<()> {
        let Some(path) = self.settings_path.clone() else {
            return Ok(());
        };
        let stamp = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if stamp == self.settings_stamp {
            return Ok(());
        }
        self.settings_stamp = stamp;
        let prefs = Settings::load_from(&path);
        if prefs != self.prefs {
            self.set_prefs(prefs)?;
        }
        Ok(())
    }

    /// The terminal told us its appearance; switch palettes if it changed.
    fn on_appearance(&mut self, mode: Mode, notified: bool) -> Result<()> {
        self.notified |= notified;
        if self.detected == Some(mode) {
            return Ok(());
        }
        self.detected = Some(mode);
        let before = theme::mode();
        self.apply_prefs();
        if theme::mode() != before || self.tab == Tab::Settings {
            self.rebuild()?;
        }
        Ok(())
    }

    fn language_label(&self) -> String {
        match self.prefs.language {
            LangPref::Auto => tf(
                "Automatic ({})",
                &[&tf("system: {}", &[&i18n::system_lang().native_name()])],
            ),
            LangPref::Fixed(l) => l.native_name().to_string(),
        }
    }

    fn theme_label(&self) -> String {
        let name = |m: Mode| match m {
            Mode::Dark => t("dark"),
            Mode::Light => t("light"),
        };
        match self.prefs.theme {
            ThemePref::Auto => {
                let seen = self.detected.map_or(t("not reported yet"), name);
                tf("Automatic ({})", &[&tf("terminal: {}", &[&seen])])
            }
            ThemePref::Fixed(Mode::Dark) => t("Dark").to_string(),
            ThemePref::Fixed(Mode::Light) => t("Light").to_string(),
        }
    }

    fn settings_rows(&self) -> Vec<Row> {
        let row = |id: i64, name: &'static str, value: String| Row {
            id,
            depth: 0,
            spans: vec![
                Span::styled(format!("{name}: "), Style::new().bold()),
                Span::styled(value, Style::new().fg(pal().code)),
            ],
            expandable: false,
            expanded: false,
        };
        vec![
            row(SETTING_LANGUAGE, t("Language"), self.language_label()),
            row(SETTING_THEME, t("Appearance"), self.theme_label()),
        ]
    }

    /// The right pane on the Settings tab: what the selected setting does and its options.
    fn settings_text(&self) -> Text<'static> {
        let muted = |s: String| Line::from(Span::styled(s, Style::new().fg(pal().muted)));
        let mut lines = Vec::new();
        match self.selected_id() {
            Some(SETTING_LANGUAGE) => {
                lines.push(Line::from(t("Language")).bold());
                lines.push(Line::raw(""));
                lines.push(Line::raw(t(
                    "Automatic follows the computer's language, and English when that language is not available.",
                )));
                lines.push(Line::raw(""));
                for p in LangPref::ALL {
                    let name = match p {
                        LangPref::Auto => t("Automatic").to_string(),
                        LangPref::Fixed(l) => l.native_name().to_string(),
                    };
                    let mark = if p == self.prefs.language {
                        "● "
                    } else {
                        "○ "
                    };
                    lines.push(Line::raw(format!("  {mark}{name}")));
                }
            }
            Some(SETTING_THEME) => {
                lines.push(Line::from(t("Appearance")).bold());
                lines.push(Line::raw(""));
                lines.push(Line::raw(t(
                    "Automatic follows the terminal's light or dark background and switches with it while ev ui is open.",
                )));
                lines.push(Line::raw(""));
                for p in ThemePref::ALL {
                    let name = match p {
                        ThemePref::Auto => t("Automatic"),
                        ThemePref::Fixed(Mode::Dark) => t("Dark"),
                        ThemePref::Fixed(Mode::Light) => t("Light"),
                    };
                    let mark = if p == self.prefs.theme {
                        "● "
                    } else {
                        "○ "
                    };
                    lines.push(Line::raw(format!("  {mark}{name}")));
                }
                if !self.notified {
                    lines.push(Line::raw(""));
                    lines.push(muted(t(
                        "This terminal does not announce appearance changes, so ev ui asks it every few seconds.",
                    ).to_string()));
                }
            }
            _ => {}
        }
        lines.push(Line::raw(""));
        lines.push(muted(
            t("Enter or → picks the next option, ← the previous one.").to_string(),
        ));
        if let Some(p) = &self.settings_path {
            lines.push(muted(tf("Saved in {}", &[&p.display()])));
        }
        lines.push(muted(t(
            "From the command line: ev settings language en|tr|auto, ev settings theme dark|light|auto",
        ).to_string()));
        Text::from(lines)
    }

    /// Moves a setting to its next (or previous) option, saves it and shows the result.
    fn cycle_setting(&mut self, id: i64, forward: bool) -> Result<()> {
        let mut prefs = self.prefs;
        match id {
            SETTING_LANGUAGE => {
                prefs.language = settings::cycle(&LangPref::ALL, prefs.language, forward)
            }
            SETTING_THEME => prefs.theme = settings::cycle(&ThemePref::ALL, prefs.theme, forward),
            _ => return Ok(()),
        }
        self.set_prefs(prefs)?;
        self.status = match &self.settings_path {
            Some(path) => match prefs.save_to(path) {
                Ok(()) => {
                    self.settings_stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok();
                    t("settings saved").to_string()
                }
                Err(e) => tf("could not save the settings: {}", &[&e]),
            },
            None => String::new(),
        };
        Ok(())
    }

    fn tree_row(&self, n: &Value, depth: usize) -> Row {
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
        self.status = tf("updated {}", &[&clock_now()]);
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
        self.status = tf(
            "showing: {}",
            &[&self.snap.label.get(&id).cloned().unwrap_or_default()],
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
        self.status = tf(
            "\"{}\": {} results",
            &[&self.query, &self.search_rows.len()],
        );
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
        if self.tab == Tab::Settings {
            return self.cycle_setting(id, true);
        }
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
        self.status = t("search cleared").into();
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
            KeyCode::BackTab => {
                self.switch(Tab::from_index(self.tab.index() + Tab::ALL.len() - 1))?
            }
            KeyCode::Char(c @ '1'..='8') => {
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
        let mut title = tf(" {} · Photo {}/{} ", &[&name, &(idx + 1), &count]);
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

        let [left, right] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(body);
        self.list_area = left;
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
                let block = Block::bordered().title(tf(
                    " Photo {}/{}  ([ ] step · r rotate · o full screen · O open outside) ",
                    &[&(self.photo_idx.min(count - 1) + 1), &count],
                ));
                let inner = block.inner(img_area);
                self.photo_area = img_area;
                f.render_widget(block, img_area);
                self.render_photo(f, &path, inner);
                rest
            }
            _ => right,
        };
        let text = if self.tab == Tab::Settings {
            self.settings_text()
        } else {
            self.details_text()
        };
        let details = Paragraph::new(text)
            .block(Block::bordered().title(t(" Details ")))
            .wrap(Wrap { trim: false });
        f.render_widget(details, text_area);

        let help = if self.searching {
            tf(
                "Search: {}▏  (Enter search · Esc clear/cancel · Ctrl+U clear)",
                &[&self.query],
            )
        } else if self.tab == Tab::Settings {
            tf(
                "↑↓ move · Enter/→ next option · ← previous option · Tab/1-8 tabs · q quit    {}",
                &[&self.status],
            )
        } else {
            tf(
                "↑↓ move · → open · ← close · Enter/double-click go · Tab/1-8 tabs · / search · q quit    {}",
                &[&self.status],
            )
        };
        f.render_widget(Paragraph::new(help).fg(pal().muted), bottom);
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
            _ => f.render_widget(
                Paragraph::new(t("(the photo could not be opened)")).fg(pal().muted),
                area,
            ),
        }
    }

    fn details_text(&self) -> Text<'static> {
        let Some(v) = &self.details else {
            return Text::from(t("(empty)"));
        };
        let n = &v["node"];
        let mut title = path_spans(n["path_text"].as_str().unwrap_or_default());
        if let Some(last) = title.last_mut() {
            *last = last.clone().bold();
        }
        let mut lines = vec![Line::from(title), Line::raw("")];
        let mut field = |k: &str, val: Span<'static>| {
            lines.push(Line::from(vec![
                Span::styled(format!("{k}: "), Style::new().fg(pal().muted)),
                val,
            ]))
        };
        field(t("kind"), Span::raw(kind_name(&str_of(n, "kind"))));
        field("id", Span::raw(n["id"].to_string()));
        if let Some(c) = n["code"].as_str() {
            field(
                t("code"),
                Span::styled(c.to_string(), Style::new().fg(pal().code)),
            );
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
            (t("note"), "note"),
            (t("address"), "address"),
        ] {
            if let Some(x) = n[key].as_str() {
                field(k, Span::raw(x.to_string()));
            }
        }
        if let Some(fill) = n["fill"].as_i64() {
            field(t("fill"), Span::raw(tf("{}%", &[&fill])));
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
        for p in n["photos"].as_array().into_iter().flatten() {
            field(
                t("photo"),
                Span::raw(p.as_str().unwrap_or_default().to_string()),
            );
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
            Span::styled(str_of(n, "updated_at"), Style::new().fg(pal().muted)),
        );
        let kids = v["children"].as_array().cloned().unwrap_or_default();
        if !kids.is_empty() {
            lines.push(Line::raw(""));
            lines.push(Line::from(tf("Contents ({})", &[&kids.len()])).bold());
            for c in &kids {
                let mut spans = vec![Span::raw("  ")];
                spans.extend(node_spans(c, &self.snap));
                lines.push(Line::from(spans));
            }
        }
        Text::from(lines)
    }

    fn handle(&mut self, input: Input) -> Result<()> {
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
    fn on_graphics(&mut self, g: Graphics) {
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
    fn poll_background(&mut self) {
        if self.notified
            || self.prefs.theme != ThemePref::Auto
            || self.last_background_query.elapsed() < BACKGROUND_POLL
        {
            return;
        }
        self.last_background_query = Instant::now();
        send(input::ASK_BACKGROUND);
    }

    /// Reads the terminal from a thread of its own, since a read blocks; the loop wakes on
    /// input or every half second to look for changes in the database and the settings.
    fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let io = |e: std::io::Error| Error::Internal(format!("terminal: {e}"));
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut stdin = std::io::stdin();
            let mut buf = [0u8; 4096];
            loop {
                match stdin.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
        });
        let mut parser = input::Parser::default();
        if self.probe.is_some() {
            send(input::IMAGE_QUERY);
        }
        send(input::START);
        while !self.quit {
            terminal.draw(|f| self.draw(f)).map_err(io)?;
            match rx.recv_timeout(POLL) {
                Ok(bytes) => {
                    let mut inputs = parser.feed(&bytes);
                    while parser.pending() {
                        match rx.recv_timeout(ESC_WAIT) {
                            Ok(more) => inputs.extend(parser.feed(&more)),
                            Err(_) => inputs.extend(parser.flush()),
                        }
                    }
                    for i in inputs {
                        self.handle(i)?;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    self.refresh_if_changed()?;
                    self.reload_settings()?;
                    self.poll_background();
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        Ok(())
    }
}

/// Which tab title sits at `x`, given ratatui's default padding of one space each side
/// and a one-cell divider between titles.
fn tab_at(x: u16) -> Option<Tab> {
    let mut start = 0u16;
    for (i, t) in tab_titles().iter().enumerate() {
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
    app.settings_path = Settings::path();
    app.reload_settings()?;
    let mut terminal = ratatui::init();
    // Inside tmux the query has to be wrapped for passthrough, which ratatui-image knows how to
    // do; everywhere else `ev ui` asks itself (see `input::IMAGE_QUERY`) and starts on half
    // blocks until the answer arrives, a few milliseconds later.
    if std::env::var_os("TMUX").is_some() {
        app.picker = Some(Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks()));
    } else {
        app.picker = Some(Picker::halfblocks());
        app.probe = Some(ImageProbe::default());
    }
    let _ = execute!(std::io::stdout(), EnableMouseCapture);
    let result = app.run(&mut terminal);
    send(input::STOP);
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

/// Writes a control sequence to the terminal; a failed write only costs the feature.
fn send(seq: &str) {
    let mut out = std::io::stdout();
    let _ = out.write_all(seq.as_bytes());
    let _ = out.flush();
}

/// The language for everything but `ev ui`, which keeps its own settings live.
pub fn set_language_from_settings() -> Lang {
    let lang = Settings::load().language.effective();
    i18n::set_lang(lang);
    lang
}

#[cfg(test)]
mod tests {
    use super::{App, Tab, tab_at};
    use crate::i18n::Lang;
    use crate::input::Input;
    use crate::settings::{LangPref, Settings, ThemePref};
    use crate::theme::{self, Mode};
    use ev_core::{Inventory, NewNode};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::style::Color;
    use ratatui::{Terminal, backend::TestBackend};
    use ratatui_image::picker::Picker;

    fn with_prefs(inv: Inventory, language: LangPref, theme: ThemePref) -> App {
        let mut app = App::new(inv).unwrap();
        // Tests do not depend on the COLORFGBG of whoever runs them.
        app.detected = None;
        app.set_prefs(Settings { language, theme }).unwrap();
        app
    }

    /// The older screen tests read Turkish words, so they run with Turkish chosen.
    fn app_tr(inv: Inventory) -> App {
        with_prefs(inv, LangPref::Fixed(Lang::Tr), ThemePref::Auto)
    }

    fn home() -> (tempfile::TempDir, Inventory) {
        let dir = tempfile::tempdir().unwrap();
        let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
        inv.add(NewNode {
            name: "Ev".into(),
            kind: "home".into(),
            ..Default::default()
        })
        .unwrap();
        (dir, inv)
    }

    #[test]
    fn english_is_shown_when_english_is_chosen() {
        let (_dir, inv) = home();
        let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
        let mut term = Terminal::new(TestBackend::new(140, 20)).unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        let s = screen(&term);
        assert!(s.contains("1 Tree") && s.contains("8 Settings"), "{s}");
        assert!(s.contains("Details") && s.contains("kind: home"), "{s}");
        assert!(!s.contains("Ayrıntı"), "{s}");
    }

    #[test]
    fn the_settings_tab_switches_language_and_appearance_and_saves_them() {
        let (dir, inv) = home();
        let path = dir.path().join("settings.json");
        let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
        app.settings_path = Some(path.clone());
        let mut term = Terminal::new(TestBackend::new(140, 20)).unwrap();

        press(&mut app, KeyCode::Char('8'));
        term.draw(|f| app.draw(f)).unwrap();
        let s = screen(&term);
        assert!(s.contains("Language: English"), "{s}");
        assert!(s.contains("Appearance: Automatic"), "{s}");

        // Enter moves English to Türkçe, and the whole screen follows at once.
        press(&mut app, KeyCode::Enter);
        term.draw(|f| app.draw(f)).unwrap();
        let s = screen(&term);
        assert!(s.contains("Dil: Türkçe") && s.contains("8 Ayarlar"), "{s}");
        assert!(s.contains("ayarlar kaydedildi"), "{s}");
        assert_eq!(
            Settings::load_from(&path).language,
            LangPref::Fixed(Lang::Tr)
        );

        // ← goes back; the appearance row moves Automatic to Dark.
        press(&mut app, KeyCode::Left);
        assert_eq!(app.prefs.language, LangPref::Fixed(Lang::En));
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.prefs.theme, ThemePref::Fixed(Mode::Dark));
        assert_eq!(
            Settings::load_from(&path).theme,
            ThemePref::Fixed(Mode::Dark)
        );
    }

    #[test]
    fn the_palette_follows_the_terminal_while_running() {
        let (_dir, inv) = home();
        let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
        let mut term = Terminal::new(TestBackend::new(120, 20)).unwrap();
        let brand_bg = |term: &Terminal<TestBackend>| term.backend().buffer()[(1, 0)].bg;

        term.draw(|f| app.draw(f)).unwrap();
        assert_eq!(brand_bg(&term), Color::Cyan);

        // The terminal reports a switch to light (mode 2031): no restart needed.
        app.handle(Input::Appearance {
            mode: Mode::Light,
            notified: true,
        })
        .unwrap();
        assert_eq!(theme::mode(), Mode::Light);
        term.draw(|f| app.draw(f)).unwrap();
        assert_eq!(brand_bg(&term), Color::Rgb(0, 110, 140));

        app.handle(Input::Appearance {
            mode: Mode::Dark,
            notified: true,
        })
        .unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        assert_eq!(brand_bg(&term), Color::Cyan);

        // A fixed appearance ignores the terminal.
        app.set_prefs(Settings {
            language: LangPref::Fixed(Lang::En),
            theme: ThemePref::Fixed(Mode::Light),
        })
        .unwrap();
        app.handle(Input::Appearance {
            mode: Mode::Dark,
            notified: true,
        })
        .unwrap();
        assert_eq!(theme::mode(), Mode::Light);
    }

    #[test]
    fn the_picture_protocol_comes_from_the_answers_read_by_ev_ui() {
        use crate::input::Graphics;
        use ratatui_image::picker::ProtocolType;
        let (_dir, inv) = home();
        let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
        app.picker = Some(Picker::halfblocks());
        app.probe = Some(super::ImageProbe::default());
        for g in [
            Graphics::Kitty,
            Graphics::Attributes { sixel: false },
            Graphics::CellSize {
                width: 10,
                height: 20,
            },
        ] {
            app.handle(Input::Graphics(g)).unwrap();
        }
        // Nothing changes until the status report says the answers are complete.
        assert_eq!(
            app.picker.as_ref().unwrap().protocol_type(),
            ProtocolType::Halfblocks
        );
        app.handle(Input::Graphics(Graphics::Done)).unwrap();
        let p = app.picker.as_ref().unwrap();
        if std::env::var_os("WEZTERM_EXECUTABLE").is_none()
            && std::env::var_os("KONSOLE_VERSION").is_none()
        {
            assert_eq!(p.protocol_type(), ProtocolType::Kitty);
        }
        assert!(app.probe.is_none());
    }

    #[test]
    fn a_settings_change_made_elsewhere_shows_up_without_restarting() {
        let (dir, inv) = home();
        let path = dir.path().join("settings.json");
        let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
        app.settings_path = Some(path.clone());
        app.reload_settings().unwrap();
        assert_eq!(crate::i18n::lang(), Lang::En);
        Settings {
            language: LangPref::Fixed(Lang::Tr),
            theme: ThemePref::Auto,
        }
        .save_to(&path)
        .unwrap();
        app.reload_settings().unwrap();
        assert_eq!(crate::i18n::lang(), Lang::Tr);
        assert_eq!(super::tab_titles()[0], "Ağaç");
    }

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
        let mut app = app_tr(inv);
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
        let mut app = app_tr(inv);
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
        let mut app = app_tr(inv);
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
        let mut app = app_tr(inv);
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
        let mut app = app_tr(inv);
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
        let mut app = app_tr(inv);
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
