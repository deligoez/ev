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
use crate::mapview::{MapView, Outcome};
use crate::settings::{self, LangPref, Settings, ThemePref, UiState};
use crate::theme::{self, Mode, pal};

mod details;

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
/// The To do section of places not counted yet, which also starts collapsed: every place in
/// the home is on it until it is counted.
const UNCOUNTED_SECTION: i64 = -10;
/// The tree's heading over lost things, whose place is not known.
const LOST_SECTION: i64 = -900;
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
    /// Lost things, apart from where they were last seen.
    lost: Vec<Value>,
    parent: HashMap<i64, i64>,
    label: HashMap<i64, String>,
    signature: HashMap<i64, String>,
}

impl Snapshot {
    fn load(inv: &Inventory) -> Result<Self> {
        let v = inv.tree(None, None)?;
        let mut s = Snapshot {
            roots: v["tree"].as_array().cloned().unwrap_or_default(),
            lost: v["lost"].as_array().cloned().unwrap_or_default(),
            ..Default::default()
        };
        for r in s.roots.clone() {
            s.index(&r, None);
        }
        // A lost thing sits under the "Unknown place" heading, not under where it was seen.
        for u in s.lost.clone() {
            s.index(&u, Some(LOST_SECTION));
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
pub(crate) fn fit_hints(mut parts: Vec<(u8, &str)>, width: usize, status: &str) -> String {
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
    // How far a place gone through on its own has been counted.
    if let Some(c) = n["count"].as_str() {
        let color = match c {
            "toured" => pal().qty,
            "counting" => pal().mark,
            _ => pal().muted,
        };
        out.push(Span::styled(
            format!("  [{}]", crate::render::count_label(c)),
            Style::new().fg(color),
        ));
    }
    if n["temporary"] == true {
        out.push(Span::styled(
            t("  [temporary place]"),
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
    /// The map (`M`) over the whole screen while open.
    map_view: Option<MapView>,
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
            collapsed: HashSet::from([UNCLEAR_SECTION, UNCOUNTED_SECTION]),
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
            map_view: None,
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
    fn todo_rows(&mut self) -> Result<Vec<Row>> {
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
        if let Some(m) = self.map_view.as_mut() {
            m.reload(&self.inv)?;
        }
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
            // A heading on the way (Unknown place) opens too, or the node stays hidden.
            self.collapsed.remove(&x);
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

    /// Opens the map on the home, its rooms first, with the room on the way to the selected node
    /// chosen; Enter after Enter leads down to that node.
    fn open_map(&mut self) {
        let sel = self
            .selected_id()
            .filter(|&i| i > 0)
            .or(self.tree_position());
        // The way from the home down to the selection, home first.
        let mut trail: Vec<i64> = sel.into_iter().collect();
        while let Some(p) = trail.last().and_then(|x| self.snap.parent.get(x)) {
            trail.push(*p);
        }
        trail.reverse();
        match MapView::open(&self.inv, trail.first().copied(), None) {
            Ok(v) => self.map_view = Some(v.with_trail(trail, true)),
            Err(e) => self.status = e.to_string(),
        }
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
        if let Some(m) = self.map_view.as_mut() {
            match m.key(&self.inv, k)? {
                Outcome::Stay => {}
                Outcome::Close => self.map_view = None,
                Outcome::Quit => self.quit = true,
                Outcome::Reveal(id) => {
                    self.map_view = None;
                    self.reveal(id)?;
                }
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
            KeyCode::Char('M') => self.open_map(),
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
        if let Some(v) = self.map_view.as_mut() {
            if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                v.click(&self.inv, m.column, m.row)?;
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
        if let Some(m) = self.map_view.as_mut() {
            return m.draw(f);
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
        parts.push((2, t("M map")));
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
mod tests;
