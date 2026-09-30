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
mod draw;
mod events;
mod prefs;
mod rows;

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
    /// Counted, with nothing left to do on it or anything in it.
    settled: HashSet<i64>,
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
        let open = inv.open_nodes()?;
        for r in &s.roots {
            settle(r, None, &open, &mut s.settled);
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

/// Whether `n` is settled, adding every settled node under it to `out`: nothing waits on it or
/// on anything in it, and it has been counted. A place counted on its own carries its count
/// down to the things in it (`counted`); above those places, a holder is settled once
/// everything in it is, and an empty one never is.
fn settle(n: &Value, counted: Option<bool>, open: &HashSet<i64>, out: &mut HashSet<i64>) -> bool {
    let id = n["id"].as_i64().unwrap_or_default();
    let counted = n["count"].as_str().map(|c| c == "toured").or(counted);
    let kids = children(n);
    // Every child is walked, not stopped at the first that is not settled, so a settled one
    // shows even beside one that is not.
    let mut kids_settled = true;
    for c in kids {
        kids_settled &= settle(c, counted, open, out);
    }
    let settled = !open.contains(&id) && kids_settled && counted.unwrap_or(!kids.is_empty());
    if settled {
        out.insert(id);
    }
    settled
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

/// A name's colour tells how far it is, not what kind it is (its mark says that): green once
/// settled, faded while it waits to leave.
fn name_style(n: &Value, snap: &Snapshot) -> Style {
    let s = match n["kind"].as_str() {
        Some("home" | "room") => Style::new().bold(),
        _ => Style::new(),
    };
    if n["state"] == "candidate" {
        s.fg(pal().muted)
    } else if snap.settled.contains(&n["id"].as_i64().unwrap_or_default()) {
        s.fg(pal().done)
    } else {
        s
    }
}

/// A one-cell mark of a node's kind, put before it in the tree and the lists, so kinds read
/// apart at a glance: a box from the things in it most of all, which differed only by a code.
/// Plain geometric shapes, never emoji or font icons: one cell wide in every terminal, so the
/// columns clicks and cut rows count stay right.
fn kind_mark(n: &Value) -> Span<'static> {
    let (mark, style) = match n["kind"].as_str() {
        Some("home") => ("⌂", Style::new().bold()),
        Some("room") => ("◫", Style::new().bold()),
        Some("furniture") => ("▥", Style::new().fg(pal().furniture)),
        Some("container") => ("□", Style::new().fg(pal().code)),
        _ => ("·", Style::new().fg(pal().muted)),
    };
    Span::styled(format!("{mark} "), style)
}

/// Kind mark, code, name and state markers of a node as coloured spans.
fn node_spans(n: &Value, snap: &Snapshot) -> Vec<Span<'static>> {
    let mut out = vec![kind_mark(n)];
    if let Some(c) = n["code"].as_str() {
        out.push(Span::styled(c.to_string(), Style::new().fg(pal().code)));
        out.push(Span::raw("  "));
    }
    out.push(Span::styled(str_of(n, "name"), name_style(n, snap)));
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

    /// Which tree nodes are open and which list headings closed, for `ui-state.json`.
    fn tree_json(&self) -> Value {
        let sorted = |s: &HashSet<i64>| {
            let mut v: Vec<i64> = s.iter().copied().collect();
            v.sort_unstable();
            v
        };
        serde_json::json!({
            "expanded": sorted(&self.expanded),
            "collapsed": sorted(&self.collapsed),
        })
    }

    /// Opens and closes the tree as a kept state says. A node gone since is dropped, so the
    /// list does not grow with every tidy-up; a part never kept stays as `ev ui` opens.
    fn apply_tree(&mut self, v: &Value) -> Result<()> {
        let ids = |k: &str| {
            v[k].as_array()
                .map(|a| a.iter().filter_map(Value::as_i64).collect::<HashSet<i64>>())
        };
        if let Some(e) = ids("expanded") {
            self.expanded = e
                .into_iter()
                .filter(|id| self.snap.label.contains_key(id))
                .collect();
        }
        if let Some(c) = ids("collapsed") {
            self.collapsed = c;
        }
        self.rebuild()
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
    // Resuming reopens the tree as it was left: its open nodes and closed headings, then the
    // node that was selected.
    if let (true, Some(s)) = (app.prefs.resume, &state) {
        app.apply_tree(&s.tree(&db))?;
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
        let _ = state.save_tree(&db, app.tree_json());
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
