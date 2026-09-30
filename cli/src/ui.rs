//! `ev ui`: a read-only terminal browser that follows the database as it changes.
//!
//! It never writes the database. What it writes is the display settings file, from the Settings
//! tab, and on exit the tree position it reopens on (`ui-state.json`, beside the settings).

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
use crate::settings::{self, LangPref, Settings, ThemePref, UiState};
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
const SETTING_RESUME: i64 = -1003;

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

/// The tabs of the details pane. Each shows the node's `#id` and path on top; a tab the
/// selected node has nothing for is dimmed, and choosing it shows the summary instead.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DetailTab {
    Summary,
    Photos,
    Grid,
    Contents,
    Suggestions,
    History,
}

impl DetailTab {
    const ALL: [DetailTab; 6] = [
        DetailTab::Summary,
        DetailTab::Photos,
        DetailTab::Grid,
        DetailTab::Contents,
        DetailTab::Suggestions,
        DetailTab::History,
    ];

    fn title(self) -> &'static str {
        match self {
            DetailTab::Summary => t("Summary"),
            DetailTab::Photos => t("Photos"),
            DetailTab::Grid => t("Grid"),
            DetailTab::Contents => t("Contents"),
            DetailTab::Suggestions => t("Suggestions"),
            DetailTab::History => t("History"),
        }
    }

    /// The name kept in `ui-state.json`, which does not change with the language.
    fn key(self) -> &'static str {
        match self {
            DetailTab::Summary => "summary",
            DetailTab::Photos => "photos",
            DetailTab::Grid => "grid",
            DetailTab::Contents => "contents",
            DetailTab::Suggestions => "suggestions",
            DetailTab::History => "history",
        }
    }

    fn from_key(s: &str) -> Option<Self> {
        DetailTab::ALL.into_iter().find(|t| t.key() == s)
    }
}

/// What a line of the details points at, so a click on it can go there: a node (opened in the
/// tree) or one of the selected node's photos.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Target {
    Node(i64),
    Photo(usize),
}

/// The list's share of the width, and the photo's share of the right column, in percent.
const SPLIT: u16 = 55;
const PHOTO_SPLIT: u16 = 55;

/// Which divider is being dragged with the mouse.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Drag {
    /// Between the list and the right column.
    Columns,
    /// Between the photo and the details.
    Photo,
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

/// Key hints joined with ` · ` in the order given, fitted to `width`: while they do not fit, the
/// least useful part (highest rank, the last of its rank) is left out; rank 0 always stays, and
/// so does the status message, after them.
fn fit_hints(mut parts: Vec<(u8, &str)>, width: usize, status: &str) -> String {
    let status = if status.is_empty() {
        String::new()
    } else {
        format!("    {status}")
    };
    let room = width.saturating_sub(status.chars().count());
    let join = |p: &[(u8, &str)]| p.iter().map(|x| x.1).collect::<Vec<_>>().join(" · ");
    while join(&parts).chars().count() > room {
        let Some(worst) = parts
            .iter()
            .enumerate()
            .filter(|(_, p)| p.0 > 0)
            .max_by_key(|(i, p)| (p.0, *i))
            .map(|(i, _)| i)
        else {
            break;
        };
        parts.remove(worst);
    }
    format!("{}{status}", join(&parts))
}

/// A grid's cells by row (row 1 at the back), each the id of the box on it or `None`.
fn grid_map(g: &Value) -> Vec<Vec<Option<i64>>> {
    g["map"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            row.as_array()
                .into_iter()
                .flatten()
                .map(Value::as_i64)
                .collect()
        })
        .collect()
}

/// Which box of a drawn grid sits at `(x, y)`, counted from the grid's first frame line and
/// its row labels. The frame between two cells of one box belongs to that box; a frame between
/// two boxes, and a free cell, to none.
fn grid_box_at(map: &[Vec<Option<i64>>], x: usize, y: usize) -> Option<i64> {
    const W: usize = 6;
    let at = |r: usize, c: usize| map.get(r).and_then(|row| row.get(c)).copied().flatten();
    let x = x.checked_sub(3)?;
    let (c, on_edge) = (x / W, x.is_multiple_of(W));
    let (r, on_rule) = (y / 2, y.is_multiple_of(2));
    match (on_rule, on_edge) {
        (false, false) => at(r, c),
        (false, true) if c > 0 && at(r, c - 1) == at(r, c) => at(r, c),
        (true, false) if r > 0 && at(r - 1, c) == at(r, c) => at(r, c),
        (true, true) if r > 0 && c > 0 => {
            let id = at(r, c);
            [at(r - 1, c - 1), at(r - 1, c), at(r, c - 1)]
                .iter()
                .all(|&o| o == id)
                .then_some(id)
                .flatten()
        }
        _ => None,
    }
}

/// An edit event in words: each field with what it became, and what it was when both are short
/// enough to read side by side.
fn edit_text(d: &Value) -> String {
    let name = |k: &str| -> String {
        match k {
            "name" => t("name").into(),
            "code" => t("code").into(),
            "note" => t("note").into(),
            "theme" => t("theme").into(),
            "qty" => t("qty").into(),
            "size" => t("size").into(),
            "fill" => t("fill").into(),
            "tags" => t("tags").into(),
            "owner" => t("owner").into(),
            "to" => t("to take to").into(),
            "address" => t("address").into(),
            "photos" => t("photos").into(),
            other => other.into(),
        }
    };
    let text = |v: &Value| match v {
        Value::Null => "—".to_string(),
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .map(|x| x.as_str().map_or_else(|| x.to_string(), str::to_string))
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_string(),
    };
    let Some(fields) = d.as_object() else {
        return d.to_string();
    };
    fields
        .iter()
        .map(|(k, c)| {
            let (before, after) = (text(&c["before"]), text(&c["after"]));
            if c["after"].is_null() {
                format!("{}: {}", name(k), t("cleared"))
            } else if before.chars().count() + after.chars().count() <= 40 {
                format!("{}: {before} → {after}", name(k))
            } else {
                format!("{}: {after}", name(k))
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
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
    // One of a thing is the default; only a count says something.
    if let Some(q) = n["qty"].as_i64().filter(|q| *q != 1) {
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

/// A holder's fill as four blocks, a quarter each: `▮▮▯▯` is about half full.
fn fill_bar(fill: i64) -> String {
    let full = ((fill + 12) / 25).clamp(0, 4) as usize;
    format!("{}{}", "▮".repeat(full), "▯".repeat(4 - full))
}

fn fill_color(fill: i64) -> Color {
    if fill >= 90 {
        pal().lost
    } else if fill >= 70 {
        pal().mark
    } else {
        pal().qty
    }
}

/// Spans cut to `width` columns, ending in `…` when something was cut, so a long name does
/// not stop silently at the pane's edge.
fn fit(spans: Vec<Span<'static>>, width: usize) -> Vec<Span<'static>> {
    let total: usize = spans.iter().map(Span::width).sum();
    if width == 0 || total <= width {
        return spans;
    }
    let budget = width - 1;
    let mut used = 0;
    let mut out = Vec::new();
    for s in spans {
        let w = s.width();
        if used + w <= budget {
            used += w;
            out.push(s);
            continue;
        }
        let mut text = String::new();
        for c in s.content.chars() {
            let cw = Span::raw(c.to_string()).width();
            if used + cw > budget {
                break;
            }
            used += cw;
            text.push(c);
        }
        out.push(Span::styled(text, s.style));
        break;
    }
    out.push(Span::styled("…", Style::new().fg(pal().muted)));
    out
}

/// A stored UTC time for people: how long ago when it is recent, the local date and time
/// otherwise.
fn when(ts: &str, now: i64) -> String {
    let Ok(at) = chrono::DateTime::parse_from_rfc3339(ts) else {
        return ts.to_string();
    };
    let at = at.timestamp();
    let ago = now - at;
    if (0..60).contains(&ago) {
        return t("just now").to_string();
    }
    if (0..3600).contains(&ago) {
        return tf("{} min ago", &[&(ago / 60)]);
    }
    if (0..86_400).contains(&ago) {
        return tf("{} h ago", &[&(ago / 3600)]);
    }
    chrono::DateTime::from_timestamp(at, 0)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| ts.to_string())
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
    /// How far the details pane is scrolled, where it was drawn, and how many lines it had.
    detail_scroll: u16,
    details_area: Rect,
    detail_lines: usize,
    /// Placement hints for the selected holder, and `ev regroup` results by scope and the data
    /// version they were computed at.
    hints: Vec<Line<'static>>,
    regroups: HashMap<i64, (i64, Value)>,
    /// The node selected in the tree when another tab was opened; the tree's position is what
    /// `ev ui` reopens on.
    tree_selected: Option<i64>,
    /// The list's share of the width and the photo's share of the right column (percent), the
    /// divider being dragged, and where they were drawn, to hit-test the mouse.
    split: u16,
    photo_split: u16,
    drag: Option<Drag>,
    divider_click: Option<Instant>,
    body_area: Rect,
    right_area: Rect,
    /// The details tab chosen, where each title was drawn (row, then column ranges), and the
    /// selected place's history with what came, went and was added.
    detail_tab: DetailTab,
    detail_tab_hits: (u16, Vec<(u16, u16, DetailTab)>),
    history: Option<Value>,
    /// The selected node's photos (`ev photo list`), and what each drawn details line points at.
    photos: Vec<Value>,
    detail_targets: Vec<Option<Target>>,
    /// The grid on the Grid tab, whose boxes open with a click: a drawer's own, or the one a
    /// box stands in.
    grid_hit: Option<Vec<Vec<Option<i64>>>>,
    /// Pictures sent together with `ev focus --file`, the one shown, and their note; full
    /// screen until closed.
    overlay: Option<(Vec<String>, usize, Option<String>)>,
    /// The marked photos closed last, for `m` to open again.
    last_overlay: Option<(Vec<String>, usize, Option<String>)>,
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
            detail_scroll: 0,
            details_area: Rect::default(),
            detail_lines: 0,
            hints: Vec::new(),
            regroups: HashMap::new(),
            tree_selected: None,
            split: SPLIT,
            photo_split: PHOTO_SPLIT,
            drag: None,
            divider_click: None,
            body_area: Rect::default(),
            right_area: Rect::default(),
            detail_tab: DetailTab::Summary,
            detail_tab_hits: (0, Vec::new()),
            history: None,
            photos: Vec::new(),
            detail_targets: Vec::new(),
            grid_hit: None,
            overlay: None,
            last_overlay: None,
        };
        app.apply_prefs();
        // A request made before this UI started is old news.
        let req = app.inv.focus_request()?;
        app.focus_seen = req["at"].as_str().map(str::to_string);
        // ...but its marked photos, while still on disk, are one `m` away.
        let files: Vec<String> = req["files"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| f.as_str().map(str::to_string))
            .filter(|f| std::path::Path::new(f).is_file())
            .collect();
        if !files.is_empty() {
            app.last_overlay = Some((files, 0, req["note"].as_str().map(str::to_string)));
        }
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
            row(
                SETTING_RESUME,
                t("Reopen where I left off"),
                if self.prefs.resume { t("On") } else { t("Off") }.to_string(),
            ),
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
            Some(SETTING_RESUME) => {
                lines.push(Line::from(t("Reopen where I left off")).bold());
                lines.push(Line::raw(""));
                lines.push(Line::raw(t(
                    "On: ev ui opens on the node that was selected in the tree when it last closed, for each database on its own. Off: it opens at the top.",
                )));
                lines.push(Line::raw(""));
                for (on, name) in [(true, t("On")), (false, t("Off"))] {
                    let mark = if on == self.prefs.resume {
                        "● "
                    } else {
                        "○ "
                    };
                    lines.push(Line::raw(format!("  {mark}{name}")));
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
            "From the command line: ev settings language en|tr|auto, ev settings theme dark|light|auto, ev settings resume on|off",
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
            SETTING_RESUME => prefs.resume = !prefs.resume,
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
    fn tab_available(&self, tab: DetailTab) -> bool {
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
    fn tab_badge(&self, tab: DetailTab) -> Option<usize> {
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
    fn shown_detail_tab(&self) -> DetailTab {
        if self.tab_available(self.detail_tab) {
            self.detail_tab
        } else {
            DetailTab::Summary
        }
    }

    /// `H` / `L`: the previous or next tab the node has something for.
    fn step_detail_tab(&mut self, delta: isize) {
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

    /// Moves a divider by `delta` percent (keys) within the range that leaves both sides usable.
    fn resize(&mut self, which: Drag, delta: i16) {
        match which {
            Drag::Columns => self.split = (self.split as i16 + delta).clamp(20, 80) as u16,
            Drag::Photo => {
                self.photo_split = (self.photo_split as i16 + delta).clamp(15, 85) as u16
            }
        }
    }

    fn layout_json(&self) -> Value {
        serde_json::json!({
            "split": self.split,
            "photo": self.photo_split,
            "details": self.detail_tab.key(),
        })
    }

    fn apply_layout(&mut self, v: &Value) {
        if let Some(s) = v["split"].as_u64() {
            self.split = (s as u16).clamp(20, 80);
        }
        if let Some(s) = v["photo"].as_u64() {
            self.photo_split = (s as u16).clamp(15, 85);
        }
        if let Some(t) = v["details"].as_str().and_then(DetailTab::from_key) {
            self.detail_tab = t;
        }
    }

    /// For a place with things in it and no theme: what `ev themes` reads from its contents,
    /// so a theme can be written while looking at it.
    fn theme_hints(&self, v: &Value) -> Vec<Line<'static>> {
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
    fn placement_hints(&mut self, v: &Value) -> Vec<Line<'static>> {
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
    /// photo full screen; or a picture that is no record (`--file`, a marked photo) full screen.
    /// Each request is shown once; the UI never writes, so it remembers the request's time
    /// instead of clearing it.
    fn apply_focus(&mut self) -> Result<()> {
        let req = self.inv.focus_request()?;
        let at = req["at"].as_str().map(str::to_string);
        if at.is_none() || at == self.focus_seen {
            return Ok(());
        }
        self.focus_seen = at;
        let files: Vec<String> = match req["files"].as_array() {
            Some(a) => a
                .iter()
                .filter_map(|f| f.as_str().map(str::to_string))
                .collect(),
            None => req["file"]
                .as_str()
                .map(str::to_string)
                .into_iter()
                .collect(),
        };
        if !files.is_empty() {
            let note = req["note"].as_str().map(str::to_string);
            self.status = tf(
                "showing: {}",
                &[&note.clone().unwrap_or_else(|| files[0].clone())],
            );
            self.overlay = Some((files, 0, note));
            self.fullscreen = true;
            return Ok(());
        }
        let Some(id) = req["id"].as_i64() else {
            return Ok(());
        };
        self.close_fullscreen();
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

    /// The node selected in the tree, whichever tab is open.
    fn tree_position(&self) -> Option<i64> {
        let id = if self.tab == Tab::Tree {
            self.selected_id()
        } else {
            self.tree_selected
        };
        id.filter(|&i| i > 0)
    }

    /// Opens on the node `ev ui` was on when it last closed; one that is gone since leaves the
    /// first row selected.
    fn resume_at(&mut self, id: i64) -> Result<()> {
        self.reveal(id)?;
        if self.state.selected().is_none() {
            self.select(0)?;
        }
        Ok(())
    }

    fn select(&mut self, i: usize) -> Result<()> {
        if self.rows.is_empty() {
            return Ok(());
        }
        self.state.select(Some(i.min(self.rows.len() - 1)));
        self.load_details()
    }

    fn scroll_details(&mut self, delta: i32) {
        let max = self.detail_lines.saturating_sub(1) as i32;
        self.detail_scroll = (self.detail_scroll as i32 + delta).clamp(0, max.max(0)) as u16;
    }

    fn step(&mut self, delta: isize) -> Result<()> {
        let cur = self.state.selected().unwrap_or(0) as isize;
        self.select((cur + delta).max(0) as usize)
    }

    fn switch(&mut self, tab: Tab) -> Result<()> {
        if self.tab == Tab::Tree {
            self.tree_selected = self.selected_id();
        }
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

    /// The picture on screen full screen: a marked photo sent with `ev focus --file`, else the
    /// selected node's current photo.
    fn shown_picture(&self) -> Option<String> {
        match &self.overlay {
            Some((files, i, _)) => files.get(*i).cloned(),
            None => self.current_photo(),
        }
    }

    fn close_fullscreen(&mut self) {
        self.fullscreen = false;
        if let Some(o) = self.overlay.take() {
            self.last_overlay = Some(o);
        }
    }

    /// `m`: the marked photos shown last, again — closed in this session, or sent before this
    /// `ev ui` started (it treats that request as seen, but keeps its pictures at hand).
    fn reopen_marked(&mut self) {
        match self.last_overlay.clone() {
            Some(o) => {
                self.overlay = Some(o);
                self.fullscreen = true;
            }
            None => self.status = t("no marked photos to show").to_string(),
        }
    }

    /// `[` `]` over pictures sent together: the previous or next one, stopping at the ends.
    fn step_overlay(&mut self, delta: isize) {
        if let Some((files, i, _)) = &mut self.overlay {
            let last = files.len().saturating_sub(1) as isize;
            *i = (*i as isize + delta).clamp(0, last) as usize;
        }
    }

    fn open_external(&mut self) {
        if let Some(p) = self.shown_picture() {
            let _ = std::process::Command::new("open").arg(p).spawn();
        }
    }

    /// Turns the picture on screen by `quarters` clockwise quarter turns.
    fn rotate(&mut self, quarters: u8) {
        if let Some(p) = self.shown_picture() {
            let r = self.rotation.entry(p).or_default();
            *r = (*r + quarters) % 4;
        }
    }

    fn key(&mut self, k: KeyEvent) -> Result<()> {
        if self.fullscreen {
            let photos = self.overlay.is_none();
            match k.code {
                KeyCode::Esc | KeyCode::Char('q' | 'o') => self.close_fullscreen(),
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.quit = true
                }
                KeyCode::Char(']') | KeyCode::Right | KeyCode::Char('l') if photos => {
                    self.step_photo(1)
                }
                KeyCode::Char('[') | KeyCode::Left | KeyCode::Char('h') if photos => {
                    self.step_photo(-1)
                }
                KeyCode::Char(']') | KeyCode::Right | KeyCode::Char('l') => self.step_overlay(1),
                KeyCode::Char('[') | KeyCode::Left | KeyCode::Char('h') => self.step_overlay(-1),
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
            KeyCode::Char('J') => self.scroll_details(3),
            KeyCode::Char('K') => self.scroll_details(-3),
            KeyCode::Char('H') => self.step_detail_tab(-1),
            KeyCode::Char('L') => self.step_detail_tab(1),
            KeyCode::Char('<') => self.resize(Drag::Columns, -5),
            KeyCode::Char('>') => self.resize(Drag::Columns, 5),
            KeyCode::Char('{') => self.resize(Drag::Photo, -5),
            KeyCode::Char('}') => self.resize(Drag::Photo, 5),
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
            KeyCode::Char('m') => self.reopen_marked(),
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
                MouseEventKind::ScrollDown if self.overlay.is_none() => self.step_photo(1),
                MouseEventKind::ScrollUp if self.overlay.is_none() => self.step_photo(-1),
                // A marked photo closes with Esc only: a stray click would lose what was shown.
                MouseEventKind::Down(MouseButton::Left) if self.overlay.is_none() => {
                    self.close_fullscreen()
                }
                _ => {}
            }
            return Ok(());
        }
        // The dividers: the column between the list and the right side, and the row under the
        // photo. Dragging one resizes; a double click puts it back.
        let body = self.body_area;
        let right = self.right_area;
        let on_columns = m.row >= body.y
            && m.row < body.y + body.height
            && right.x > 0
            && (m.column == right.x || m.column + 1 == right.x);
        let photo = self.photo_area;
        let on_photo = photo.height > 0
            && m.row + 1 == photo.y + photo.height
            && m.column >= right.x
            && m.column < right.x + right.width;
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) if on_columns || on_photo => {
                let which = if on_columns {
                    Drag::Columns
                } else {
                    Drag::Photo
                };
                let now = Instant::now();
                if self
                    .divider_click
                    .is_some_and(|t| now.duration_since(t) < DOUBLE_CLICK)
                {
                    match which {
                        Drag::Columns => self.split = SPLIT,
                        Drag::Photo => self.photo_split = PHOTO_SPLIT,
                    }
                    self.divider_click = None;
                    self.drag = None;
                } else {
                    self.divider_click = Some(now);
                    self.drag = Some(which);
                }
                return Ok(());
            }
            MouseEventKind::Drag(MouseButton::Left) if self.drag.is_some() => {
                // A press that moved was a drag, so it cannot be half of a double click.
                self.divider_click = None;
                match self.drag {
                    Some(Drag::Columns) if body.width > 0 => {
                        let left = (m.column.saturating_sub(body.x) + 1) as u32;
                        self.split = ((left * 100 / body.width as u32) as u16).clamp(20, 80);
                    }
                    Some(Drag::Photo) if right.height > 0 => {
                        let top = (m.row.saturating_sub(right.y) + 1) as u32;
                        self.photo_split = ((top * 100 / right.height as u32) as u16).clamp(15, 85);
                    }
                    _ => {}
                }
                return Ok(());
            }
            MouseEventKind::Up(MouseButton::Left) if self.drag.is_some() => {
                self.drag = None;
                return Ok(());
            }
            MouseEventKind::Down(MouseButton::Left) if m.row == self.detail_tab_hits.0 => {
                let hit = self
                    .detail_tab_hits
                    .1
                    .iter()
                    .find(|(a, b, _)| m.column >= *a && m.column < *b)
                    .map(|h| h.2);
                if let Some(tab) = hit {
                    if self.tab_available(tab) {
                        self.detail_tab = tab;
                        self.detail_scroll = 0;
                    }
                    return Ok(());
                }
            }
            // A box on the grid opens in the tree. The grid's first frame line is
            // the fifth line of the pane: path, blank, title, column letters.
            MouseEventKind::Down(MouseButton::Left)
                if inside(self.details_area)
                    && m.row > self.details_area.y
                    && self.grid_hit.is_some() =>
            {
                let line = (m.row - self.details_area.y - 1) as usize + self.detail_scroll as usize;
                let x = (m.column - self.details_area.x - 1) as usize;
                let hit = line
                    .checked_sub(4)
                    .and_then(|y| grid_box_at(self.grid_hit.as_deref().unwrap_or_default(), x, y));
                if let Some(id) = hit {
                    return self.reveal(id);
                }
                return Ok(());
            }
            // A line of the Photos, Contents or History tab: show that photo, or open that
            // thing in the tree.
            MouseEventKind::Down(MouseButton::Left)
                if inside(self.details_area)
                    && m.row > self.details_area.y
                    && !self.detail_targets.is_empty() =>
            {
                let line = (m.row - self.details_area.y - 1) as usize + self.detail_scroll as usize;
                match self.detail_targets.get(line).copied().flatten() {
                    Some(Target::Node(id)) if self.snap.label.contains_key(&id) => {
                        return self.reveal(id);
                    }
                    Some(Target::Node(id)) => {
                        self.status = tf("#{} is no longer in the tree (gone)", &[&id]);
                        return Ok(());
                    }
                    Some(Target::Photo(i)) => {
                        self.photo_idx = i;
                        return Ok(());
                    }
                    None => {}
                }
            }
            _ => {}
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
            MouseEventKind::ScrollDown if inside(self.details_area) => {
                self.scroll_details(3);
                Ok(())
            }
            MouseEventKind::ScrollUp if inside(self.details_area) => {
                self.scroll_details(-3);
                Ok(())
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
        let note = self
            .photos
            .get(idx)
            .and_then(|p| p["note"].as_str().map(str::to_string));
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

    /// Pictures sent with `ev focus --file` (marked photos) over the whole screen, titled with
    /// their note and, when there are several, which one this is. They are no record: closing
    /// forgets them.
    fn draw_overlay(
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
        let mut parts = vec![(0, t("Esc/o close"))];
        if at.1 > 1 {
            parts.push((0, t("[ ] ← → step")));
        }
        parts.push((1, t("m opens it again later")));
        parts.push((2, t("r/R rotate")));
        parts.push((3, t("O open outside")));
        let keys = fit_hints(parts, bottom.width as usize, "");
        f.render_widget(Paragraph::new(keys).fg(pal().muted), bottom);
    }

    fn draw(&mut self, f: &mut Frame) {
        if self.fullscreen {
            if let Some((files, i, note)) = self.overlay.clone() {
                let i = i.min(files.len().saturating_sub(1));
                if let Some(path) = files.get(i) {
                    return self.draw_overlay(f, path, note, (i, files.len()));
                }
            }
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
        let list = List::new(items)
            .block(Block::bordered().title(title).border_style(columns_edge))
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
        f.render_stateful_widget(list, left, &mut self.state);

        self.photo_area = Rect::default();
        let text_area = match (self.current_photo(), self.picker.is_some()) {
            (Some(path), true) => {
                let count = self.photo_count();
                let [img_area, rest] = Layout::vertical([
                    Constraint::Percentage(self.photo_split),
                    Constraint::Min(6),
                ])
                .areas(right);
                let idx = self.photo_idx.min(count - 1);
                // The photo's own note says what it shows (a drawer's final state, the inside
                // of a bag), which the picture alone may not.
                let note = self
                    .photos
                    .get(idx)
                    .and_then(|p| p["note"].as_str())
                    .map(|n| format!("· {n} "))
                    .unwrap_or_default();
                // The note before the key hints, so a narrow pane cuts the hints, not the note.
                let block = Block::bordered()
                    .title(format!(
                        "{}{note}{}",
                        tf(" Photo {}/{} ", &[&(idx + 1), &count]),
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
                DetailTab::Photos | DetailTab::Contents | DetailTab::History | DetailTab::Grid
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
            // The tabs are the title: the one shown stands out, one with nothing for this
            // node steps back, and a count says how much each holds.
            let shown = self.shown_detail_tab();
            let mut spans = vec![Span::raw(" ")];
            let mut x = text_area.x + 2;
            let mut hits = Vec::new();
            for (i, tab) in DetailTab::ALL.into_iter().enumerate() {
                if i > 0 {
                    spans.push(Span::styled(" · ", Style::new().fg(pal().muted)));
                    x += 3;
                }
                let badge = self
                    .tab_badge(tab)
                    .filter(|_| tab != DetailTab::Summary)
                    .map(|n| format!(" {n}"))
                    .unwrap_or_default();
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
                "↑↓ move · Enter/→ next option · ← previous option · Tab/1-8 tabs · q quit    {}",
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
    fn help_line(&self, width: usize, scrolls: bool) -> String {
        // (priority, text): 0 always, higher numbers go first when space runs out.
        let mut parts: Vec<(u8, &str)> = vec![(0, t("↑↓ move"))];
        match self.tab {
            Tab::Tree => {
                parts.push((1, t("→ ← open/close")));
            }
            Tab::Search if self.has_search() => {
                parts.push((1, t("Enter show in tree")));
                parts.push((1, t("x clear")));
            }
            Tab::Plan => parts.push((1, t("Enter open/close section"))),
            _ => parts.push((1, t("Enter show in tree"))),
        }
        // Marked photos closed are one key away; said early, so a narrow screen keeps it.
        if self.last_overlay.is_some() {
            parts.push((1, t("m marked photos")));
        }
        parts.push((2, t("/ search")));
        if self.details.is_some() {
            parts.push((2, t("H/L details tabs")));
            if scrolls {
                parts.push((3, t("J/K scroll")));
            }
        }
        if self.photo_count() > 0 {
            parts.push((3, t("[ ] o photos")));
        }
        parts.push((4, t("Tab/1-8 tabs")));
        parts.push((5, t("< > { } or drag: resize")));
        parts.push((0, t("q quit")));
        fit_hints(parts, width, &self.status)
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

    fn details_text(&self) -> Text<'static> {
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
    fn history_lines(&self) -> Vec<(Line<'static>, Option<Target>)> {
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
                        let status = match d["as"].as_str() {
                            Some("toured") => t("toured").to_string(),
                            _ => str_of(d, "as"),
                        };
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
    fn photo_lines(&self) -> Vec<(Line<'static>, Option<Target>)> {
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
    fn detail_targets(&self) -> Vec<Option<Target>> {
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
    fn grid_text(g: &Value, mark: Option<i64>) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from(tf(
                "{}×{} grid, row 1 at the back",
                &[&g["cols"], &g["rows"]],
            ))
            .bold(),
        ];
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

pub fn run(inv: Inventory, db: &std::path::Path) -> Result<()> {
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        return Err(Error::Usage("`ev ui` needs a terminal".into()));
    }
    let mut app = App::new(inv)?;
    app.settings_path = Settings::path();
    app.reload_settings()?;
    let db = std::path::absolute(db).unwrap_or_else(|_| db.to_path_buf());
    let state = app.settings_path.as_deref().map(UiState::beside);
    if let Some(s) = &state {
        app.apply_layout(&s.layout());
    }
    if let (true, Some(id)) = (app.prefs.resume, state.as_ref().and_then(|s| s.last(&db))) {
        app.resume_at(id)?;
    }
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
    // The position is kept even with resuming off, so turning it on picks up from the last
    // session; the layout always.
    if let Some(state) = &state {
        let _ = state.save(&db, app.tree_position(), app.layout_json());
    }
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
    use super::{App, DetailTab, Drag, Tab, tab_at};
    use crate::i18n::Lang;
    use crate::input::Input;
    use crate::settings::{LangPref, Settings, ThemePref};
    use crate::theme::{self, Mode};
    use ev_core::{Inventory, NewNode};
    use ratatui::crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::style::Color;
    use ratatui::{Terminal, backend::TestBackend};
    use ratatui_image::picker::Picker;

    fn with_prefs(inv: Inventory, language: LangPref, theme: ThemePref) -> App {
        let mut app = App::new(inv).unwrap();
        // Tests do not depend on the COLORFGBG of whoever runs them.
        app.detected = None;
        app.set_prefs(Settings {
            language,
            theme,
            ..Default::default()
        })
        .unwrap();
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
        assert!(s.contains("1 Layout") && s.contains("8 Settings"), "{s}");
        assert!(s.contains("Summary · Photos · Grid"), "{s}");
        assert!(
            s.lines()
                .any(|l| l.contains("kind ") && l.contains(" home")),
            "{s}"
        );
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

        // The last row turns reopening where I left off off, and back on.
        press(&mut app, KeyCode::Down);
        term.draw(|f| app.draw(f)).unwrap();
        assert!(screen(&term).contains("Reopen where I left off: On"));
        press(&mut app, KeyCode::Enter);
        assert!(!app.prefs.resume);
        assert!(!Settings::load_from(&path).resume);
        term.draw(|f| app.draw(f)).unwrap();
        assert!(screen(&term).contains("Reopen where I left off: Off"));
        press(&mut app, KeyCode::Enter);
        assert!(Settings::load_from(&path).resume);
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
            ..Default::default()
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
    fn it_reopens_on_the_tree_node_it_was_on_and_on_the_top_when_that_is_gone() {
        let (_dir, inv) = led_drawer();
        let led = inv.resolve("Kırmızı LED 10 mm", false).unwrap();
        let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
        // A new session starts on a node inside a collapsed box.
        app.resume_at(led).unwrap();
        assert_eq!(app.selected_id(), Some(led));
        // The tree's position is what is kept, even when another tab is open at exit.
        app.switch(Tab::Search).unwrap();
        assert_eq!(app.tree_position(), Some(led));
        // A node deleted since leaves the first row selected rather than nothing.
        app.resume_at(999_999).unwrap();
        assert_eq!(app.state.selected(), Some(0));
    }

    #[test]
    fn a_grid_holder_shows_its_map_and_a_box_its_cells() {
        let (dir, mut inv) = home();
        let mut boxes: Vec<(String, &str, String, Option<String>)> = vec![
            ("Oda".into(), "room", "Ev".into(), None),
            (
                "Çekmece".into(),
                "container",
                "Oda".into(),
                Some("D".into()),
            ),
            ("Kutu".into(), "container", "D".into(), Some("D-B1".into())),
        ];
        // Enough boxes and photos that anything below the fields would be off screen.
        for i in 0..25 {
            boxes.push((format!("Dolgu {i}"), "container", "D".into(), None));
        }
        for (name, kind, parent, code) in boxes {
            inv.add(NewNode {
                name,
                kind: kind.into(),
                parent: Some(parent),
                code,
                ..Default::default()
            })
            .unwrap();
        }
        let photo = dir.path().join("p.png");
        image::RgbImage::from_pixel(16, 16, image::Rgb([9, 9, 9]))
            .save(&photo)
            .unwrap();
        for i in 0..8 {
            inv.photo_add("D", &photo, None, Some(&format!("{i}")))
                .unwrap();
        }
        inv.grid_set("D", 3, 2).unwrap();
        inv.cells_set(&[("D-B1".into(), "B1-C1".into())], false)
            .unwrap();
        let drawer = inv.resolve("D", false).unwrap();
        let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
        app.picker = Some(Picker::halfblocks());
        app.reveal(drawer).unwrap();
        // `L` steps from the summary through the photos to the grid.
        press(&mut app, KeyCode::Char('L'));
        assert_eq!(app.shown_detail_tab(), DetailTab::Photos);
        press(&mut app, KeyCode::Char('L'));
        let mut term = Terminal::new(TestBackend::new(140, 40)).unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        let s = screen(&term);
        assert!(s.contains("3×2 grid, row 1 at the back"), "{s}");
        assert!(s.contains(" 1    ·  │ B1        │"), "{s}");
        assert!(s.contains("┌───────────┐"), "{s}");
        assert!(s.contains("free (4): A1 A2 B2 C2"), "{s}");

        // On the drawer's own grid a box opens with a click, on its cells or on the frame
        // inside it; a free cell does nothing.
        let (x0, y0) = (app.details_area.x + 1, app.details_area.y + 1 + 4);
        let cell = |col: u16, line: u16| (x0 + 3 + col * 6 + 2, y0 + line);
        let (x, y) = cell(0, 1);
        click(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);
        assert_eq!(app.selected_id(), Some(drawer));
        let (x, y) = cell(2, 1);
        click(&mut app, MouseEventKind::Down(MouseButton::Left), x - 2, y);
        let bx = app.selected_id().unwrap();
        assert_eq!(app.snap.label[&bx], "D-B1  Kutu");
        // On the box, its drawer's grid still opens boxes; a free cell stays put.
        term.draw(|f| app.draw(f)).unwrap();
        assert!(app.grid_hit.is_some());
        let (x, y) = cell(0, 1);
        click(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);
        assert_eq!(app.selected_id(), Some(bx));
    }

    #[test]
    fn a_grid_click_finds_the_box_under_it_frames_included() {
        use super::grid_box_at;
        // Row 1: A1 alone, B1–C1 one box. Row 2: A2 free, B2–C2 another box.
        let map = vec![
            vec![Some(1), Some(2), Some(2)],
            vec![None, Some(3), Some(3)],
        ];
        // Cell lines are odd, frame lines even; columns start after the 3-column row label.
        assert_eq!(grid_box_at(&map, 3 + 2, 1), Some(1));
        assert_eq!(
            grid_box_at(&map, 3 + 6, 1),
            None,
            "the frame between A1 and B1"
        );
        assert_eq!(
            grid_box_at(&map, 3 + 12, 1),
            Some(2),
            "the frame inside B1–C1"
        );
        assert_eq!(
            grid_box_at(&map, 3 + 8, 2),
            None,
            "the frame between rows 1 and 2"
        );
        assert_eq!(grid_box_at(&map, 3 + 2, 3), None, "a free cell");
        assert_eq!(grid_box_at(&map, 1, 1), None, "the row label");
    }

    fn add(inv: &mut Inventory, name: &str, kind: &str, parent: &str, code: Option<&str>) {
        inv.add(NewNode {
            name: name.into(),
            kind: kind.into(),
            parent: Some(parent.into()),
            code: code.map(Into::into),
            ..Default::default()
        })
        .unwrap();
    }

    /// A drawer `D` in a 2×1 grid: a red LED box, and a buzzer box with a red LED in it.
    fn led_drawer() -> (tempfile::TempDir, Inventory) {
        let (dir, mut inv) = home();
        add(&mut inv, "Oda", "room", "Ev", None);
        add(&mut inv, "Çekmece", "container", "Oda", Some("D"));
        add(&mut inv, "Kutu", "container", "D", Some("D-A1"));
        add(&mut inv, "Kutu", "container", "D", Some("D-B1"));
        inv.edit("D-A1", &["theme=Kırmızı LED".into(), "size=1x1x1".into()])
            .unwrap();
        inv.edit("D-B1", &["theme=Buzzer".into(), "fill=50".into()])
            .unwrap();
        for (name, parent) in [
            ("Kırmızı LED 5 mm", "D-A1"),
            ("Kırmızı LED 3 mm", "D-A1"),
            ("Aktif buzzer", "D-B1"),
            ("Pasif buzzer", "D-B1"),
            ("Kırmızı LED 10 mm", "D-B1"),
        ] {
            add(&mut inv, name, "item", parent, None);
        }
        inv.grid_set("D", 2, 1).unwrap();
        inv.cells_set(
            &[("D-A1".into(), "A1".into()), ("D-B1".into(), "B1".into())],
            false,
        )
        .unwrap();
        (dir, inv)
    }

    fn shown(app: &mut App, reference: &str, w: u16, h: u16) -> String {
        let id = app.inv.resolve(reference, false).unwrap();
        app.reveal(id).unwrap();
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        screen(&term)
    }

    #[test]
    fn a_box_shows_its_drawer_map_size_room_and_what_would_fit_better_elsewhere() {
        let (_dir, inv) = led_drawer();
        let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
        let s = shown(&mut app, "D-B1", 150, 40);
        // Room from the fill, not only the percentage; no database id among the fields.
        assert!(s.contains("▮▮▯▯  room (50% full)"), "{s}");
        assert!(!s.contains("id: "), "{s}");
        // The tabs count what they hold: three things inside, one suggested move.
        assert!(s.contains("Contents 3 · Suggestions 1"), "{s}");
        // The drawer's map, with this box among the others.
        app.detail_tab = DetailTab::Grid;
        let s = shown(&mut app, "D-B1", 150, 40);
        assert!(s.contains("2×1 grid, row 1 at the back"), "{s}");
        assert!(s.contains(" 1 │ A1  │ B1  │"), "{s}");
        // A neighbour on it opens with a click, from the box as from the drawer.
        let (x, y) = (app.details_area.x + 1 + 5, app.details_area.y + 1 + 5);
        click(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);
        let a1 = app.inv.resolve("D-A1", false).unwrap();
        assert_eq!(app.selected_id(), Some(a1));
        // The grid stays open, so the drawer can be walked box by box.
        assert_eq!(app.shown_detail_tab(), DetailTab::Grid);
        // The stray LED, with where it would fit better.
        app.detail_tab = DetailTab::Suggestions;
        let s = shown(&mut app, "D-B1", 150, 40);
        assert!(s.contains("Suggestions (ev regroup)"), "{s}");
        assert!(s.contains("→ D-A1  Kırmızı LED 10 mm"), "{s}");
        // A box with nothing to suggest shows its summary, and the choice is kept.
        let s = shown(&mut app, "D-A1", 150, 40);
        assert!(
            s.lines()
                .any(|l| l.contains("size ") && l.contains(" 1x1x1")),
            "{s}"
        );
        assert_eq!(app.detail_tab, DetailTab::Suggestions);
    }

    fn click(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
        app.handle(Input::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }))
        .unwrap();
    }

    #[test]
    fn the_divider_drags_resets_on_a_double_click_and_steps_with_keys() {
        let (_dir, inv) = led_drawer();
        let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
        let _ = shown(&mut app, "D-B1", 100, 30);
        // The list ends at column 54 of 100; the right side starts at 55.
        assert_eq!(app.right_area.x, 55);
        click(&mut app, MouseEventKind::Down(MouseButton::Left), 55, 10);
        assert_eq!(app.drag, Some(Drag::Columns));
        click(&mut app, MouseEventKind::Drag(MouseButton::Left), 39, 12);
        click(&mut app, MouseEventKind::Up(MouseButton::Left), 39, 12);
        assert_eq!((app.split, app.drag), (40, None));
        let s = shown(&mut app, "D-B1", 100, 30);
        assert_eq!(app.right_area.x, 40, "{s}");
        // Too far is held back, so neither side disappears.
        click(&mut app, MouseEventKind::Down(MouseButton::Left), 40, 10);
        click(&mut app, MouseEventKind::Drag(MouseButton::Left), 2, 10);
        assert_eq!(app.split, 20);
        click(&mut app, MouseEventKind::Up(MouseButton::Left), 2, 10);
        // A double click on the divider puts it back.
        let _ = shown(&mut app, "D-B1", 100, 30);
        let x = app.right_area.x;
        app.divider_click = None;
        click(&mut app, MouseEventKind::Down(MouseButton::Left), x, 10);
        click(&mut app, MouseEventKind::Up(MouseButton::Left), x, 10);
        click(&mut app, MouseEventKind::Down(MouseButton::Left), x, 10);
        assert_eq!(app.split, 55);
        press(&mut app, KeyCode::Char('<'));
        assert_eq!(app.split, 50);
        // The layout is what `ui-state.json` keeps, and a stored one comes back clamped.
        assert_eq!(app.layout_json()["split"], 50);
        app.apply_layout(&serde_json::json!({"split": 5, "details": "history"}));
        assert_eq!((app.split, app.detail_tab), (20, DetailTab::History));
    }

    #[test]
    fn the_history_tab_tells_what_happened_here_newest_first() {
        let (_dir, mut inv) = led_drawer();
        inv.move_to("Kırmızı LED 10 mm", "D-A1", false).unwrap();
        inv.edit("D-A1", &["theme=LED".into()]).unwrap();
        let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
        app.detail_tab = DetailTab::History;
        let s = shown(&mut app, "D-A1", 150, 40);
        assert!(s.contains("Today"), "{s}");
        let at = |text: &str| s.find(text).unwrap_or_else(|| panic!("{text} in {s}"));
        // Its own change, what came in and from where, what was added: newest first.
        assert!(at("changed  theme: Kırmızı LED → LED") < at("came in  Kırmızı LED 10 mm  ← D-B1"));
        assert!(at("came in") < at("added here  Kırmızı LED 3 mm"));
        assert!(s.contains("created  D"), "{s}");
        // Clicking a title opens that tab; the summary is first.
        let (row, hits) = app.detail_tab_hits.clone();
        click(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            hits[0].0,
            row,
        );
        assert_eq!(app.detail_tab, DetailTab::Summary);
    }

    #[test]
    fn photos_history_and_contents_lines_open_what_they_name_with_a_click() {
        let (dir, mut inv) = led_drawer();
        let photo = dir.path().join("p.png");
        image::RgbImage::from_pixel(8, 8, image::Rgb([9, 9, 9]))
            .save(&photo)
            .unwrap();
        for note in ["eski", "yeni"] {
            inv.photo_add("D-B1", &photo, None, Some(note)).unwrap();
        }
        let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
        app.detail_tab = DetailTab::Photos;
        let s = shown(&mut app, "D-B1", 150, 40);
        // Newest first, the one shown marked; a click on the older shows it.
        let (new, old) = (s.find("yeni").unwrap(), s.find("eski").unwrap());
        assert!(new < old && s.contains("▶  2"), "{s}");
        let (x, first) = (app.details_area.x + 5, app.details_area.y + 1 + 2);
        click(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            x,
            first + 1,
        );
        assert_eq!(app.photo_idx, 0);
        // The contents open in the tree.
        app.detail_tab = DetailTab::Contents;
        let _ = shown(&mut app, "D-B1", 150, 40);
        click(&mut app, MouseEventKind::Down(MouseButton::Left), x, first);
        let opened = app.selected_id().unwrap();
        assert_eq!(
            app.snap.parent[&opened],
            app.inv.resolve("D-B1", false).unwrap()
        );
        // And so does a thing named in a drawer's history.
        app.detail_tab = DetailTab::History;
        let s = shown(&mut app, "D-A1", 150, 40);
        let line = s
            .lines()
            .position(|l| l.contains("added here  Kırmızı LED 5 mm"))
            .unwrap() as u16;
        click(&mut app, MouseEventKind::Down(MouseButton::Left), x, line);
        assert_eq!(
            app.snap.label[&app.selected_id().unwrap()],
            "Kırmızı LED 5 mm"
        );
    }

    #[test]
    fn a_history_line_naming_something_gone_says_so_instead_of_opening_it() {
        let (_dir, mut inv) = led_drawer();
        let led = inv.resolve("Kırmızı LED 5 mm", false).unwrap();
        inv.gone(&led.to_string(), Some(ev_core::Disposition::Trash))
            .unwrap();
        let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
        app.detail_tab = DetailTab::History;
        let s = shown(&mut app, "D-A1", 150, 40);
        let line = s
            .lines()
            .position(|l| l.contains("added here  Kırmızı LED 5 mm"))
            .unwrap_or_else(|| panic!("{s}")) as u16;
        let (before, x) = (app.selected_id(), app.details_area.x + 5);
        click(&mut app, MouseEventKind::Down(MouseButton::Left), x, line);
        assert_eq!(app.selected_id(), before);
        assert_eq!(
            app.status,
            format!("#{led} is no longer in the tree (gone)")
        );
    }

    #[test]
    fn a_marked_photo_sent_from_another_process_shows_full_screen_until_closed() {
        let (dir, inv) = led_drawer();
        let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
        let marked = dir.path().join("marked.png");
        image::RgbImage::from_pixel(8, 8, image::Rgb([230, 30, 30]))
            .save(&marked)
            .unwrap();
        // Another process asks; the running UI picks it up without restarting.
        let drawer = dir.path().join("drawer.png");
        std::fs::copy(&marked, &drawer).unwrap();
        app.inv
            .focus_file(&[marked.clone(), drawer.clone()], Some("1 → A6"))
            .unwrap();
        app.apply_focus().unwrap();
        let mut term = Terminal::new(TestBackend::new(100, 20)).unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        let s = screen(&term);
        assert!(
            s.contains("1 → A6 · 1/2") && s.contains("Esc/o close · [ ] ← → step"),
            "{s}"
        );
        // A stray click does not close it.
        click(&mut app, MouseEventKind::Down(MouseButton::Left), 10, 5);
        assert!(app.overlay.is_some());
        // Sent together, they are stepped through; the end holds.
        press(&mut app, KeyCode::Char(']'));
        press(&mut app, KeyCode::Char(']'));
        term.draw(|f| app.draw(f)).unwrap();
        assert!(screen(&term).contains("1 → A6 · 2/2"));
        assert_eq!(
            app.shown_picture(),
            Some(drawer.to_string_lossy().into_owned())
        );
        // Esc closes it; the same request is not shown again, but `m` opens it, and the key
        // hints say so.
        press(&mut app, KeyCode::Esc);
        app.apply_focus().unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        let s = screen(&term);
        assert!(
            s.contains("Summary") && s.contains("m marked photos"),
            "{s}"
        );
        assert!(app.overlay.is_none());
        press(&mut app, KeyCode::Char('m'));
        term.draw(|f| app.draw(f)).unwrap();
        assert!(screen(&term).contains("1 → A6 · 2/2"));
        // A restarted `ev ui` treats the request as seen, and still opens it with `m`.
        let inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
        let mut again = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
        again.apply_focus().unwrap();
        assert!(again.overlay.is_none());
        press(&mut again, KeyCode::Char('m'));
        assert_eq!(
            again.shown_picture(),
            Some(marked.to_string_lossy().into_owned())
        );
    }

    #[test]
    fn the_key_hints_fit_the_width_dropping_the_least_useful_first() {
        let (_dir, inv) = led_drawer();
        let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
        let _ = shown(&mut app, "D-B1", 150, 40);
        let wide = app.help_line(300, true);
        assert!(
            wide.contains("J/K scroll") && wide.contains("resize"),
            "{wide}"
        );
        // No photo, no photo keys.
        assert!(!wide.contains("photos"), "{wide}");
        let narrow = app.help_line(60, true);
        assert!(narrow.chars().count() <= 60, "{narrow}");
        assert!(
            narrow.starts_with("↑↓ move") && narrow.ends_with("q quit"),
            "{narrow}"
        );
        assert!(!narrow.contains("resize"), "{narrow}");
        // A status message stays.
        app.status = "saved".into();
        assert!(app.help_line(40, true).ends_with("    saved"));
    }

    #[test]
    fn the_details_pane_scrolls_to_contents_below_its_edge() {
        let (_dir, mut inv) = home();
        add(&mut inv, "Oda", "room", "Ev", None);
        add(&mut inv, "Çekmece", "container", "Oda", Some("D"));
        for i in 0..40 {
            add(&mut inv, &format!("Parça {i:02}"), "item", "D", None);
        }
        let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
        app.detail_tab = DetailTab::Contents;
        let s = shown(&mut app, "D", 150, 24);
        assert!(!s.contains("Parça 39"), "{s}");
        assert!(s.contains("H/L tabs · J/K scroll"), "{s}");
        for _ in 0..20 {
            press(&mut app, KeyCode::Char('J'));
        }
        let mut term = Terminal::new(TestBackend::new(150, 24)).unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        let s = screen(&term);
        assert!(s.contains("Parça 39"), "{s}");
        // Another node starts at the top again.
        let s = shown(&mut app, "Oda", 150, 24);
        assert!(s.contains(" H/L tabs ─"), "{s}");
    }

    #[test]
    fn a_place_without_a_theme_shows_what_its_contents_share() {
        let (_dir, mut inv) = home();
        add(&mut inv, "Oda", "room", "Ev", None);
        add(&mut inv, "Kutu", "container", "Oda", Some("K"));
        add(&mut inv, "RP-SMA çubuk anten", "item", "K", None);
        add(&mut inv, "U.FL anten kablosu", "item", "K", None);
        let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
        app.detail_tab = DetailTab::Suggestions;
        let s = shown(&mut app, "K", 150, 30);
        assert!(s.contains("No theme yet (ev themes)"), "{s}");
        assert!(s.contains("words: anten (2)"), "{s}");
        // A themed place shows no hints.
        app.inv.edit("K", &["theme=Antenler".into()]).unwrap();
        let s = shown(&mut app, "Oda", 150, 30);
        let s2 = shown(&mut app, "K", 150, 30);
        assert!(
            !s.contains("No theme yet") && !s2.contains("No theme yet"),
            "{s2}"
        );
    }

    #[test]
    fn long_rows_end_in_an_ellipsis_and_times_read_as_ago() {
        crate::i18n::set_lang(Lang::En);
        let spans = vec![
            ratatui::text::Span::raw("abcdef"),
            ratatui::text::Span::raw("ghij"),
        ];
        let cut: String = super::fit(spans, 6)
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert_eq!(cut, "abcde…");
        assert_eq!(super::fill_bar(50), "▮▮▯▯");
        assert_eq!(super::fill_bar(95), "▮▮▮▮");
        let at = chrono::DateTime::parse_from_rfc3339("2026-09-29T09:00:00Z")
            .unwrap()
            .timestamp();
        assert_eq!(super::when("2026-09-29T09:00:00Z", at + 30), "just now");
        assert_eq!(super::when("2026-09-29T09:00:00Z", at + 7200), "2 h ago");
        let old = super::when("2026-09-29T09:00:00Z", at + 3 * 86_400);
        // Local date and time: the day holds for any zone within nine hours of UTC.
        assert!(old.starts_with("2026-09-29 "), "{old}");
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
            ..Default::default()
        }
        .save_to(&path)
        .unwrap();
        app.reload_settings().unwrap();
        assert_eq!(crate::i18n::lang(), Lang::Tr);
        assert_eq!(super::tab_titles()[0], "Yerleşim");
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
        assert!(s.contains("Özet · Fotoğraflar 1 · Izgara"), "{s}");
    }

    #[test]
    fn the_newest_photo_and_its_note_show_first_under_the_nodes_id() {
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
        inv.photo_add("Ev", &photo, None, Some("before the tour"))
            .unwrap();
        inv.photo_add("Ev", &photo, None, Some("final state"))
            .unwrap();
        let mut app = with_prefs(inv, LangPref::Fixed(Lang::En), ThemePref::Auto);
        app.picker = Some(Picker::halfblocks());
        let mut term = Terminal::new(TestBackend::new(140, 40)).unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        let s = screen(&term);
        assert!(s.contains("Photo 2/2"), "{s}");
        assert!(s.contains("final state"), "{s}");
        assert!(s.contains("#1  Ev"), "{s}");
        // A step back reaches the older one.
        press(&mut app, KeyCode::Char('['));
        term.draw(|f| app.draw(f)).unwrap();
        assert!(screen(&term).contains("before the tour"));
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
        assert!(!s.contains("Özet"), "{s}");

        press(&mut app, KeyCode::Esc);
        assert!(!app.quit);
        term.draw(|f| app.draw(f)).unwrap();
        assert!(screen(&term).contains("Özet"));
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
        // " 1 Layout " spans columns 0..10, then a divider, then " 2 Pending ".
        assert!(tab_at(0) == Some(Tab::Tree));
        assert!(tab_at(9) == Some(Tab::Tree));
        assert!(tab_at(10).is_none());
        assert!(tab_at(11) == Some(Tab::Pending));
    }
}
