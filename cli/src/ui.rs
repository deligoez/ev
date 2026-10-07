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
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::Protocol;
use ratatui_image::{Image, Resize};
use serde_json::Value;

use crate::history::disposition_tr;
use crate::i18n::{self, Lang, t, tf};
use crate::input::{self, Graphics, Input};
use crate::mapview::{MapView, Outcome};
use crate::settings::{self, LangPref, Settings, ThemePref, UiState};
use crate::theme::{self, Mode, pal};

mod buys;
mod details;
mod draw;
mod events;
mod palette;
mod prefs;
mod rows;

const POLL: Duration = Duration::from_millis(500);
/// How many decoded photos are kept: each is up to 1600 px a side, several megabytes.
const DECODED_KEPT: usize = 24;

/// Decodes photos on a thread of its own: a phone photo takes about 80 ms to read and scale,
/// which a key press waited for on every new place with a photo. A finished photo wakes the
/// loop through the input channel (an empty read), and the next frame shows it.
struct Decoder {
    jobs: mpsc::Sender<String>,
    done: mpsc::Receiver<(String, Option<image::DynamicImage>)>,
    pending: HashSet<String>,
}

impl Decoder {
    fn start(wake: mpsc::Sender<Vec<u8>>) -> Decoder {
        let (jobs, todo) = mpsc::channel::<String>();
        let (finished, done) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(path) = todo.recv() {
                let img = decode_photo(&path);
                if finished.send((path, img)).is_err() {
                    break;
                }
                let _ = wake.send(Vec::new());
            }
        });
        Decoder {
            jobs,
            done,
            pending: HashSet::new(),
        }
    }
}

/// A photo upright and scaled to fit the largest pane it is drawn in.
fn decode_photo(path: &str) -> Option<image::DynamicImage> {
    ev_core::open_upright(std::path::Path::new(path))
        .ok()
        .map(|i| i.thumbnail(1600, 1600))
}
const HIGHLIGHT_FOR: Duration = Duration::from_secs(6);
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// How long a lone ESC waits for the rest of a sequence before it counts as the Esc key.
const ESC_WAIT: Duration = Duration::from_millis(30);
/// How often a terminal without mode 2031 is asked for its background again.
const BACKGROUND_POLL: Duration = Duration::from_secs(3);

/// The To do section that starts collapsed: unclear records are a long, low-priority list.
const UNCLEAR_SECTION: i64 = -14;
/// The Statistics tab's sections count down from here, clear of the To do sections, the lost
/// heading and the settings.
const STATS_SECTION: i64 = -3000;
/// The Past tab's two lists' headings: this, and one less.
const PAST_LIST: i64 = -3500;
/// The Past tab's year headings: this less the list's span and the year, clear of every other
/// heading.
const PAST_SECTION: i64 = -4000;
const PAST_LIST_SPAN: i64 = 3000;
/// The To do section of places not counted yet, which also starts collapsed: every place in
/// the home is on it until it is counted.
const UNCOUNTED_SECTION: i64 = -10;
/// The tree's heading over lost things, whose place is not known.
const LOST_SECTION: i64 = -900;
/// Rows of the Settings tab; their ids are negative like section headers, but far below them.
const SETTING_LANGUAGE: i64 = -1001;
const SETTING_THEME: i64 = -1002;
const SETTING_RESUME: i64 = -1003;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Tab {
    Tree,
    Pending,
    Disposals,
    Lost,
    Places,
    Search,
    Plan,
    Settings,
    /// `ev stats` on one page (spec/stats.md).
    Stats,
    /// `ev past`: what was ours and left, by year, apart from the inventory
    /// (spec/past-belongings.md).
    Past,
    /// The purchase lines (`ev buy list`), all of them or one bucket's.
    Buys,
    BuysDurable,
    BuysClothing,
    BuysDigital,
    BuysService,
}

impl Tab {
    /// The list's name in the current language.
    fn title(self) -> &'static str {
        match self {
            Tab::Tree => t("Layout"),
            Tab::Pending => t("Pending"),
            Tab::Disposals => t("Leaving"),
            Tab::Lost => t("Lost"),
            Tab::Places => t("Errands"),
            Tab::Search => t("Search"),
            Tab::Plan => t("To do"),
            Tab::Settings => t("Settings"),
            Tab::Stats => t("Statistics"),
            Tab::Past => t("Past"),
            Tab::Buys => t("All"),
            Tab::BuysDurable => t("Durable"),
            Tab::BuysClothing => t("Clothing"),
            Tab::BuysDigital => t("Digital"),
            Tab::BuysService => t("Service"),
        }
    }

    /// A purchase list's bucket: `Some(None)` for all of them, `None` for a list of records.
    fn bucket(self) -> Option<Option<&'static str>> {
        match self {
            Tab::Buys => Some(None),
            Tab::BuysDurable => Some(Some("durable")),
            Tab::BuysClothing => Some(Some("clothing")),
            Tab::BuysDigital => Some(Some("digital")),
            Tab::BuysService => Some(Some("service")),
            _ => None,
        }
    }

    /// The digit that opens the list, in the sidebar's order.
    fn digit(self) -> Option<char> {
        match self {
            Tab::Tree => Some('1'),
            Tab::Plan => Some('2'),
            Tab::Pending => Some('3'),
            Tab::Disposals => Some('4'),
            Tab::Lost => Some('5'),
            Tab::Places => Some('6'),
            Tab::Past => Some('7'),
            Tab::Stats => Some('8'),
            Tab::Buys => Some('9'),
            Tab::Settings => Some('0'),
            Tab::Search
            | Tab::BuysDurable
            | Tab::BuysClothing
            | Tab::BuysDigital
            | Tab::BuysService => None,
        }
    }

    fn from_digit(c: char) -> Option<Self> {
        SIDEBAR.iter().find_map(|s| match s {
            Side::List(t) if t.digit() == Some(c) => Some(*t),
            _ => None,
        })
    }

    /// The sidebar heading the list is under, if any.
    fn section(self) -> Option<&'static str> {
        let mut heading = None;
        for s in SIDEBAR {
            match s {
                Side::Heading(h) => heading = Some(h),
                Side::Gap => heading = None,
                Side::List(list) if list == self => return heading.map(t),
                Side::List(_) => {}
            }
        }
        None
    }
}

/// A line of the sidebar (spec/ui-sidebar.md): a section heading, a list, or the gap before the
/// lists kept at the bottom.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Heading(&'static str),
    List(Tab),
    Gap,
}

/// The sidebar, top to bottom: two levels, headings and the lists under them.
const SIDEBAR: [Side; 20] = [
    Side::Heading("HOME"),
    Side::List(Tab::Tree),
    Side::List(Tab::Plan),
    Side::List(Tab::Pending),
    Side::List(Tab::Disposals),
    Side::List(Tab::Lost),
    Side::List(Tab::Places),
    Side::Heading("PURCHASES"),
    Side::List(Tab::Buys),
    Side::List(Tab::BuysDurable),
    Side::List(Tab::BuysClothing),
    Side::List(Tab::BuysDigital),
    Side::List(Tab::BuysService),
    Side::Heading("HISTORY"),
    Side::List(Tab::Past),
    Side::Heading("INSIGHT"),
    Side::List(Tab::Stats),
    Side::Gap,
    Side::List(Tab::Search),
    Side::List(Tab::Settings),
];

/// The lists in the sidebar's order.
fn sidebar_lists() -> impl Iterator<Item = Tab> {
    SIDEBAR.into_iter().filter_map(|s| match s {
        Side::List(t) => Some(t),
        _ => None,
    })
}

/// The pane the keys go to; its border stands out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pane {
    Sidebar,
    List,
    Details,
}

/// How the sidebar is shown at a width (spec/ui-sidebar.md).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SideMode {
    /// Names and counts, beside the list.
    Full,
    /// Digits and counts only.
    Rail,
    /// Names and counts, over the list.
    Drawer,
    Hidden,
}

/// The sidebar's width in columns, with its borders, and the rail's.
const SIDEBAR_WIDTH: u16 = 24;
const RAIL_WIDTH: u16 = 7;
/// Below these widths the sidebar becomes a rail, then is hidden, then one pane is shown at a
/// time.
const WIDE: u16 = 120;
const MEDIUM: u16 = 90;
const NARROW: u16 = 70;

/// The tabs of the details pane. Each shows the node's `#id` and path on top; only the tabs
/// the selected node has something for are shown.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DetailTab {
    Summary,
    Photos,
    Documents,
    Grid,
    Contents,
    Suggestions,
    History,
}

impl DetailTab {
    const ALL: [DetailTab; 7] = [
        DetailTab::Summary,
        DetailTab::Photos,
        DetailTab::Documents,
        DetailTab::Grid,
        DetailTab::Contents,
        DetailTab::Suggestions,
        DetailTab::History,
    ];

    fn title(self) -> &'static str {
        match self {
            DetailTab::Summary => t("Summary"),
            DetailTab::Photos => t("Photos"),
            DetailTab::Documents => t("Documents"),
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
            DetailTab::Documents => "documents",
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
/// tree), one of the selected node's photos, or one of its documents or links (opened outside,
/// in the program the system gives that file or address).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Target {
    Node(i64),
    Photo(usize),
    Document(usize),
    Link(usize),
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

/// What is open on the first screen: homes, rooms and vehicles, so it shows the furniture and a
/// car's compartments.
fn first_open(roots: &[Value]) -> HashSet<i64> {
    fn open(n: &Value, out: &mut HashSet<i64>) {
        if matches!(n["kind"].as_str(), Some("home" | "room" | "vehicle")) {
            out.insert(n["id"].as_i64().unwrap_or_default());
            for c in children(n) {
                open(c, out);
            }
        }
    }
    let mut out = HashSet::new();
    for r in roots {
        open(r, &mut out);
    }
    out
}

/// The node `id` in the tree, if it is there.
fn find_in(nodes: &[Value], id: i64) -> Option<&Value> {
    nodes.iter().find_map(|n| {
        if n["id"].as_i64() == Some(id) {
            Some(n)
        } else {
            find_in(children(n), id)
        }
    })
}

/// `n` and every node below it that holds something.
fn holders_in(n: &Value, out: &mut Vec<i64>) {
    if !children(n).is_empty() {
        out.push(n["id"].as_i64().unwrap_or_default());
        for c in children(n) {
            holders_in(c, out);
        }
    }
}

/// Whether `n` is settled, adding every settled node under it to `out`: nothing waits on it or
/// on anything in it, and it has been counted. A place counted on its own carries its count
/// down to the things in it (`counted`); above those places, a holder is settled once
/// everything in it is, and an empty one never is.
fn settle(n: &Value, counted: Option<bool>, open: &HashSet<i64>, out: &mut HashSet<i64>) -> bool {
    let id = n["id"].as_i64().unwrap_or_default();
    // A counted place that changed since is not settled: what changed has not been counted.
    let counted = n["count"]
        .as_str()
        .map(|c| c == "toured" && n["changed_since"] != true)
        .or(counted);
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

fn kind_name(k: &str) -> &'static str {
    match k {
        "home" => t("home"),
        "room" => t("room"),
        "furniture" => t("furniture"),
        "container" => t("container"),
        "item" => t("item"),
        "vehicle" => t("vehicle"),
        _ => "?",
    }
}

/// A name's colour tells how far it is, not what kind it is (its mark says that): green once
/// settled or counted (a place counted and unchanged since; what still waits in it is marked
/// beside it), faded while it waits to leave.
fn name_style(n: &Value, snap: &Snapshot) -> Style {
    let s = match n["kind"].as_str() {
        Some("home" | "room" | "vehicle") => Style::new().bold(),
        _ => Style::new(),
    };
    let counted = n["count"] == "toured" && n["changed_since"] != true;
    if n["state"] == "candidate" {
        s.fg(pal().muted)
    } else if counted || snap.settled.contains(&n["id"].as_i64().unwrap_or_default()) {
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
        Some("vehicle") => ("▭", Style::new().bold()),
        Some("room") => ("◫", Style::new().bold()),
        Some("furniture") => ("▥", Style::new().fg(pal().furniture)),
        Some("container") => ("□", Style::new().fg(pal().code)),
        _ => ("·", Style::new().fg(pal().muted)),
    };
    Span::styled(format!("{mark} "), style)
}

/// Kind mark, code, name and state markers of a node as coloured spans.
fn node_spans(n: &Value, snap: &Snapshot) -> Vec<Span<'static>> {
    // A settled row is green from its mark to its name, so a finished stretch of the tree
    // reads as one at a glance; counts and fill keep their own colours.
    let settled = snap.settled.contains(&n["id"].as_i64().unwrap_or_default());
    let mark = kind_mark(n);
    let mut out = vec![if settled { mark.fg(pal().done) } else { mark }];
    if let Some(c) = n["code"].as_str() {
        let color = if settled { pal().done } else { pal().code };
        out.push(Span::styled(c.to_string(), Style::new().fg(color)));
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
        out.push(Span::raw(format!("  ×{q}")));
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
    // How far a place gone through on its own has been counted. Counted is where every place
    // is headed, so it takes no word: its green name says it. Only the exceptions are written:
    // not counted, being counted, left as is, and counted but changed since.
    match n["count"].as_str() {
        Some("toured") if n["changed_since"] == true => out.push(Span::styled(
            format!("  [{}]", t("counted, changed since")),
            Style::new().fg(pal().mark),
        )),
        Some("toured") | None => {}
        Some(c) => {
            let color = if c == "counting" {
                pal().mark
            } else {
                pal().muted
            };
            out.push(Span::styled(
                format!("  [{}]", crate::render::count_label(c)),
                Style::new().fg(color),
            ));
        }
    }
    // A box known to be empty says so: nothing in it is a fact, not a gap.
    if n["empty"] == true {
        out.push(Span::styled(t("  [empty]"), Style::new().fg(pal().muted)));
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
    /// The pane the keys go to, whether `b` showed or hid the sidebar (`None`: as the width
    /// says), where it was drawn and which list each of its lines opens.
    pane: Pane,
    sidebar: Option<bool>,
    sidebar_area: Rect,
    sidebar_hits: Vec<(u16, Tab)>,
    /// The width the panes had when last drawn, which decides how the sidebar is shown.
    screen: u16,
    /// `:` while open, and the lists and records opened before, newest last, for `Esc`.
    palette: Option<palette::Palette>,
    back: Vec<(Tab, Option<i64>)>,
    /// Every purchase line as of the last change, the selected line's details, the purchase
    /// lists' filters (`f`, `/`) and their title with what they add up to.
    purchases: Option<Vec<Value>>,
    purchase: Option<Value>,
    buy_state: buys::BuyState,
    buy_words: String,
    purchase_title: String,
    /// While the lists are rebuilt for a change made elsewhere: only then is a record that left
    /// its list news.
    refreshing: bool,
    /// The list behind each figure of the Statistics list, by row; a purchase list's year and
    /// shop when it was opened from a figure (`drilled`: Esc goes back to it at once).
    stat_drills: HashMap<usize, crate::render::Drill>,
    buy_year: Option<String>,
    buy_shop: Option<String>,
    drilled: bool,
    /// The sidebar's counts, from the queries that fill the lists, as of the last change.
    counts: HashMap<Tab, usize>,
    last_click: Option<(usize, Instant)>,
    picker: Option<Picker>,
    photo_idx: usize,
    /// The document or link picked on the Documents tab (`[` `]`), opened with `O`.
    doc_idx: usize,
    /// `E`: the summary also lists the identity fields still empty, as “—”, to be filled.
    show_empty: bool,
    /// `+`: the list's width before the details were widened, to go back to.
    split_before: Option<u16>,
    /// Photos decoded (downscaled), the latest `DECODED_KEPT` of them; `decoded_order` is the
    /// order they came in, the oldest let go first.
    decoded: HashMap<String, Option<image::DynamicImage>>,
    decoded_order: std::collections::VecDeque<String>,
    /// Decodes photos off the loop in `run`, so a key never waits for a photo; without it
    /// (tests), a photo is decoded where it is drawn.
    decoder: Option<Decoder>,
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
    /// The last `ev focus` request shown, so each is shown once.
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
    /// While `run` sets up (settings, layout, resuming): details wait until the node it opens
    /// on is known, so no other row's are worked out on the way.
    starting: bool,
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
    /// The series of marked photos sent with `ev focus --file` (and by `photo mark`/`cut`), the
    /// one shown; full screen until hidden.
    overlay: Option<Series>,
    /// The series hidden last, for `m` to open again.
    last_overlay: Option<Series>,
    /// The series shown as a grid (`g`) rather than one picture at a time (spec/series-grid.md).
    series_grid: bool,
    /// The grid's picture width for now (`+` / `-`), in cells; the setting when none.
    tile: Option<u16>,
    /// The grid's first row on screen, and its columns when it was last drawn.
    grid_top: usize,
    grid_cols: usize,
    /// Where each picture of the grid was last drawn, so a click opens the one under it.
    grid_hits: Vec<(Rect, usize)>,
    /// Series pictures scaled down for the grid, and their terminal pictures by tile size: a
    /// grid of forty would otherwise decode and encode every photo on every frame.
    small: HashMap<String, Option<image::DynamicImage>>,
    thumbs: HashMap<(String, u16, u16), Option<Protocol>>,
    /// `f` then digits: the series picture to go to (`f12`), until Enter.
    jump: Option<String>,
    /// The map (`M`) over the whole screen while open.
    map_view: Option<MapView>,
}

/// The series of marked photos a focus request holds (spec/focus-stack.md): each picture with
/// the note it was sent with and how many numbered frames it carries, and the one shown.
#[derive(Clone)]
struct Series {
    files: Vec<String>,
    notes: Vec<Option<String>>,
    frames: Vec<usize>,
    /// For a picture about another place than the series: that place's label and the series'
    /// (spec/series-batches.md).
    crossed: Vec<Option<(String, String)>>,
    at: usize,
}

impl Series {
    /// The request's series, without the pictures no longer on disk, showing the first picture
    /// the request sent; `None` when nothing of it is left.
    fn of(req: &Value) -> Option<Self> {
        // A request written before series existed is no series (see `focus_marked`).
        if !req["frames"].is_array() {
            return None;
        }
        let show = req["show"].as_u64().unwrap_or(0) as usize;
        let (mut files, mut notes, mut frames, mut at) = (Vec::new(), Vec::new(), Vec::new(), 0);
        let mut crossed = Vec::new();
        let about = &req["about"];
        for (i, f) in req["files"].as_array().into_iter().flatten().enumerate() {
            let Some(f) = f.as_str().filter(|f| std::path::Path::new(f).is_file()) else {
                continue;
            };
            if i <= show {
                at = files.len();
            }
            files.push(f.to_string());
            let note = req["notes"].get(i).unwrap_or(&req["note"]);
            notes.push(note.as_str().map(str::to_string));
            frames.push(req["frames"][i].as_array().map_or(0, Vec::len));
            let own = &req["abouts"][i];
            crossed.push(
                (own.is_object() && about.is_object() && own["id"] != about["id"]).then(|| {
                    let label = |a: &Value| a["label"].as_str().unwrap_or_default().to_string();
                    (label(own), label(about))
                }),
            );
        }
        (!files.is_empty()).then_some(Self {
            files,
            notes,
            frames,
            crossed,
            at,
        })
    }
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
        let expanded = first_open(&snap.roots);
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
            pane: Pane::List,
            sidebar: None,
            sidebar_area: Rect::default(),
            sidebar_hits: Vec::new(),
            screen: 0,
            palette: None,
            back: Vec::new(),
            purchases: None,
            purchase: None,
            buy_state: buys::BuyState::All,
            buy_words: String::new(),
            purchase_title: String::new(),
            refreshing: false,
            stat_drills: HashMap::new(),
            buy_year: None,
            buy_shop: None,
            drilled: false,
            counts: HashMap::new(),
            last_click: None,
            picker: None,
            photo_idx: 0,
            doc_idx: 0,
            show_empty: false,
            split_before: None,
            decoded: HashMap::new(),
            decoded_order: std::collections::VecDeque::new(),
            decoder: None,
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
            starting: false,
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
            series_grid: false,
            tile: None,
            grid_top: 0,
            grid_cols: 1,
            grid_hits: Vec::new(),
            small: HashMap::new(),
            thumbs: HashMap::new(),
            jump: None,
            map_view: None,
        };
        app.apply_prefs();
        // A request made before this UI started is old news.
        let req = app.inv.focus_request()?;
        app.focus_seen = req["at"].is_string().then(|| req.to_string());
        // ...but its series of marked photos, while still on disk, is one `m` away.
        app.last_overlay = Series::of(&req).map(|mut s| {
            s.at = 0;
            s
        });
        // The details wait for the node `ev ui` resumes at (see `run`): the first row's would
        // be thrown away at once, and the home's cost a whole-house regroup.
        app.rebuild_rows()?;
        app.count_lists()?;
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

    /// `+`: the details take the most room the list allows, and back.
    fn toggle_wide(&mut self) {
        match self.split_before.take() {
            Some(s) => self.split = s,
            None => {
                self.split_before = Some(self.split);
                self.split = 20;
            }
        }
    }

    /// `y`: copies what is picked: a document's file or a link on the Documents tab, else the
    /// thing's `#id` and name, the way it is named to the agent.
    fn copy(&mut self) {
        let Some(v) = &self.details else { return };
        let text = match (
            self.shown_detail_tab(),
            self.document_targets().get(self.doc_idx),
        ) {
            (DetailTab::Documents, Some(Target::Document(i))) => {
                str_of(&v["documents"][*i], "file")
            }
            (DetailTab::Documents, Some(Target::Link(i))) => self
                .detail_links()
                .get(*i)
                .map(|l| l.1.clone())
                .unwrap_or_default(),
            _ => format!("#{} {}", v["node"]["id"], str_of(&v["node"], "name")),
        };
        let (cmd, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
            ("pbcopy", &[])
        } else {
            ("xclip", &["-selection", "clipboard"])
        };
        let copied = std::process::Command::new(cmd)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut c| {
                use std::io::Write;
                c.stdin
                    .take()
                    .map_or(Ok(()), |mut i| i.write_all(text.as_bytes()))?;
                c.wait().map(|_| ())
            });
        self.status = match copied {
            Ok(()) => tf("copied: {}", &[&text]),
            Err(e) => tf("could not copy: {}", &[&e]),
        };
    }

    fn layout_json(&self) -> Value {
        serde_json::json!({
            "split": self.split,
            "photo": self.photo_split,
            "details": self.detail_tab.key(),
            "tile": self.tile,
            "sidebar": self.sidebar,
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
        if let Some(w) = v["tile"].as_u64() {
            self.tile = Some((w as u16).max(settings::SERIES_TILE_MIN));
        }
        self.sidebar = v["sidebar"].as_bool();
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
        if self.starting {
            return self.rebuild_rows();
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
        self.purchases = None;
        self.count_lists()?;
        self.refreshing = true;
        let rebuilt = self.rebuild();
        self.refreshing = false;
        rebuilt?;
        self.apply_focus()
    }

    /// Shows what `ev focus` asked for: the node in the tree and, when a photo was named, that
    /// photo full screen; or a picture that is no record (`--file`, a marked photo) full screen.
    /// Each request is shown once; the UI never writes, so it remembers the request's time
    /// instead of clearing it.
    fn apply_focus(&mut self) -> Result<()> {
        let req = self.inv.focus_request()?;
        // The whole request, not only its time: two sent in the same millisecond are two.
        let seen = req["at"].is_string().then(|| req.to_string());
        // The request is gone after one was seen: the series was ended (`ev focus --clear`).
        if seen.is_none() {
            if self.focus_seen.is_some() {
                self.forget_series();
            }
            return Ok(());
        }
        if seen == self.focus_seen {
            return Ok(());
        }
        self.focus_seen = seen;
        let series = Series::of(&req);
        if req["id"].is_null() {
            if let Some(s) = series {
                let shown = s.notes[s.at]
                    .clone()
                    .unwrap_or_else(|| s.files[s.at].clone());
                self.status = tf("showing: {}", &[&shown]);
                self.overlay = Some(s);
                self.fullscreen = true;
            }
            return Ok(());
        }
        // A node asked for keeps the series one `m` away.
        self.close_fullscreen();
        if series.is_some() {
            self.last_overlay = series;
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
        self.counts.insert(Tab::Search, self.search_rows.len());
        self.status = tf(
            "\"{}\": {} results",
            &[&self.query, &self.search_rows.len()],
        );
        Ok(())
    }

    /// `p`: the next place of a thing kept in several places (spec/portions.md §5), by id and
    /// round again, opened in the tree as Enter on a search result is.
    fn next_portion(&mut self) -> Result<()> {
        let Some(v) = &self.details else {
            return Ok(());
        };
        let here = v["node"]["id"].as_i64().unwrap_or(0);
        let mut others: Vec<i64> = v["thing"]["elsewhere"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p["id"].as_i64())
            .collect();
        others.sort_unstable();
        let Some(next) = others
            .iter()
            .find(|id| **id > here)
            .or(others.first())
            .copied()
        else {
            self.status = t("kept in one place").to_string();
            return Ok(());
        };
        self.reveal(next)
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
        self.rebuild_rows()?;
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
        // A figure's year and shop narrow the purchase lists only while they are open.
        if tab.bucket().is_none() {
            self.buy_year = None;
            self.buy_shop = None;
            self.drilled = false;
        }
        self.tab = tab;
        self.state.select(None);
        self.rebuild()
    }

    /// `Enter` on a figure of the Statistics list: the list it counts, as `Esc` comes back.
    fn drill(&mut self, d: crate::render::Drill) -> Result<()> {
        use crate::render::Drill;
        match d {
            Drill::Purchases { year, shop } => {
                self.go(Tab::Buys)?;
                self.buy_year = year;
                self.buy_shop = shop;
                self.buy_state = buys::BuyState::All;
                self.buy_words.clear();
                self.drilled = true;
                self.rebuild()
            }
            Drill::Todo => self.go(Tab::Plan),
            Drill::Past => self.go(Tab::Past),
        }
    }

    /// Opens the list `delta` places down the sidebar (up when negative), stopping at its ends.
    fn step_list(&mut self, delta: isize) -> Result<()> {
        let lists: Vec<Tab> = sidebar_lists().collect();
        let at = lists.iter().position(|&l| l == self.tab).unwrap_or(0) as isize;
        let to = at.saturating_add(delta).clamp(0, lists.len() as isize - 1) as usize;
        if lists[to] == self.tab {
            return Ok(());
        }
        self.switch(lists[to])
    }

    /// The keys go to the list; a sidebar opened over it as a drawer closes.
    fn enter_list(&mut self) {
        self.pane = Pane::List;
        if self.side_mode(self.screen) == SideMode::Drawer {
            self.sidebar = None;
        }
    }

    /// `Tab` / `Shift+Tab`: the next pane, sidebar → list → details, past a hidden sidebar.
    fn next_pane(&mut self, delta: isize) {
        let panes = [Pane::Sidebar, Pane::List, Pane::Details];
        let hidden = self.side_mode(self.screen) == SideMode::Hidden && self.screen >= NARROW;
        let mut i = panes.iter().position(|&p| p == self.pane).unwrap_or(1) as isize;
        loop {
            i = (i + delta).rem_euclid(panes.len() as isize);
            if !(hidden && panes[i as usize] == Pane::Sidebar) {
                break;
            }
        }
        self.pane = panes[i as usize];
    }

    /// `b`: hides a sidebar that is shown, shows one that is not; kept until changed.
    fn toggle_sidebar(&mut self) {
        let shown = self.side_mode(self.screen) != SideMode::Hidden;
        self.sidebar = Some(!shown);
        if shown && self.pane == Pane::Sidebar {
            self.pane = Pane::List;
        } else if !shown {
            self.pane = Pane::Sidebar;
        }
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
        if id == 0 {
            return Ok(());
        }
        if self.tab.bucket().is_some() {
            return match self.purchase_thing() {
                Some(thing) => self.jump_to(thing),
                None => Ok(()),
            };
        }
        if self.tab != Tab::Tree {
            return self.jump_to(id);
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
        self.counts.remove(&Tab::Search);
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
        let last = self.picture_count().saturating_sub(1);
        let cur = self.picture_idx() as isize;
        self.photo_idx = (cur + delta).clamp(0, last as isize) as usize;
    }

    /// The picture on screen full screen: a marked photo of the series, else the selected
    /// node's current photo.
    fn shown_picture(&self) -> Option<String> {
        match &self.overlay {
            Some(s) => s.files.get(s.at).cloned(),
            None => self.current_photo(),
        }
    }

    fn close_fullscreen(&mut self) {
        self.fullscreen = false;
        if let Some(o) = self.overlay.take() {
            self.last_overlay = Some(o);
        }
    }

    /// `m`: the series of marked photos again, after it was hidden — in this session, or sent
    /// before this `ev ui` started (it treats that request as seen, but keeps the series).
    fn reopen_marked(&mut self) {
        match self.last_overlay.clone() {
            Some(o) => {
                self.overlay = Some(o);
                self.fullscreen = true;
            }
            None => self.status = t("no marked photo series to show").to_string(),
        }
    }

    /// `X`: the person is done with the series of marked photos. It is gone from the screen and
    /// from the request, so what the agent sends next starts a new series numbered from 1. The
    /// one write `ev ui` makes, and to no inventory data (spec/focus-stack.md).
    fn close_series(&mut self) -> Result<()> {
        if self.overlay.is_none() && self.last_overlay.is_none() {
            self.status = t("no marked photo series to show").to_string();
            return Ok(());
        }
        self.inv.focus(None, None)?;
        self.forget_series();
        self.status = t("marked photo series closed").to_string();
        Ok(())
    }

    fn forget_series(&mut self) {
        if self.overlay.take().is_some() {
            self.fullscreen = false;
        }
        self.last_overlay = None;
        self.focus_seen = None;
    }

    /// `e` / `c` on the tree: open the selected node and everything below it, or close them all,
    /// the selection staying where it is.
    fn open_below(&mut self, open: bool) -> Result<()> {
        let Some(id) = self.selected_id() else {
            return Ok(());
        };
        let mut below = Vec::new();
        if let Some(n) = find_in(&self.snap.roots, id) {
            holders_in(n, &mut below);
        }
        for x in below {
            if open {
                self.expanded.insert(x);
            } else {
                self.expanded.remove(&x);
            }
        }
        self.rebuild()?;
        self.reveal(id)
    }

    /// `d` on the tree: open the selected node and every node in it, two levels down, and nothing
    /// further: a Kallax shows its compartments and the drawers in each.
    fn open_two_levels(&mut self) -> Result<()> {
        let Some(id) = self.selected_id() else {
            return Ok(());
        };
        if let Some(n) = find_in(&self.snap.roots, id) {
            self.expanded.insert(id);
            for c in children(n).iter().filter(|c| !children(c).is_empty()) {
                self.expanded.insert(c["id"].as_i64().unwrap_or_default());
            }
        }
        self.rebuild()?;
        self.reveal(id)
    }

    /// `C` on the tree: everything closed but the homes, so the rooms show closed, the selection
    /// moved up to the room it was in when it is hidden now.
    fn close_tree(&mut self) -> Result<()> {
        let id = self.selected_id();
        self.expanded = self
            .snap
            .roots
            .iter()
            .filter_map(|r| r["id"].as_i64())
            .collect();
        self.rebuild()?;
        let mut at = id;
        while let Some(x) = at {
            if self.rows.iter().any(|r| r.id == x) {
                let i = self.rows.iter().position(|r| r.id == x).unwrap_or(0);
                return self.select(i);
            }
            at = self.snap.parent.get(&x).copied();
        }
        Ok(())
    }

    /// The keys of the series in both its views (spec/series-grid.md): `g` grid ↔ single,
    /// `Home` / `End`, `f` then digits then `Enter` to go to a picture, and in the grid the
    /// arrows, `Enter` and `+` / `-`. Whether the key was the series'.
    fn series_key(&mut self, k: KeyEvent) -> Result<bool> {
        let Some(count) = self.overlay.as_ref().map(|s| s.files.len()) else {
            return Ok(false);
        };
        if let Some(buf) = self.jump.as_mut() {
            match k.code {
                KeyCode::Char(c) if c.is_ascii_digit() => buf.push(c),
                KeyCode::Backspace => {
                    buf.pop();
                }
                KeyCode::Enter => {
                    let n: usize = buf.parse().unwrap_or(0);
                    self.jump = None;
                    if (1..=count).contains(&n) {
                        self.go_to_picture(n - 1);
                    } else {
                        self.status = tf("the series has no f{}", &[&n]);
                    }
                }
                _ => self.jump = None,
            }
            return Ok(true);
        }
        let grid = self.series_grid;
        let cols = self.grid_cols.max(1) as isize;
        match k.code {
            KeyCode::Char('f') => self.jump = Some(String::new()),
            KeyCode::Char('g') => self.series_grid = !self.series_grid,
            KeyCode::Home => self.go_to_picture(0),
            KeyCode::End => self.go_to_picture(count - 1),
            KeyCode::Enter if grid => self.series_grid = false,
            KeyCode::Right | KeyCode::Char('l') if grid => self.step_overlay(1),
            KeyCode::Left | KeyCode::Char('h') if grid => self.step_overlay(-1),
            KeyCode::Down | KeyCode::Char('j') if grid => self.step_overlay(cols),
            KeyCode::Up | KeyCode::Char('k') if grid => self.step_overlay(-cols),
            KeyCode::Char('+' | '=') if grid => self.zoom_grid(4),
            KeyCode::Char('-') if grid => self.zoom_grid(-4),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn go_to_picture(&mut self, i: usize) {
        if let Some(s) = &mut self.overlay {
            s.at = i.min(s.files.len().saturating_sub(1));
        }
    }

    /// `+` / `-` in the grid: wider or narrower pictures for now, kept with the screen's layout.
    fn zoom_grid(&mut self, by: i16) {
        let now = self.tile.unwrap_or(self.prefs.series_tile) as i16;
        self.tile = Some((now + by).clamp(settings::SERIES_TILE_MIN as i16, 400) as u16);
    }

    /// `[` `]` over the series: the previous or next picture, stopping at the ends.
    fn step_overlay(&mut self, delta: isize) {
        if let Some(s) = &mut self.overlay {
            let last = s.files.len().saturating_sub(1) as isize;
            s.at = (s.at as isize + delta).clamp(0, last) as usize;
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
        let tx_wake = tx.clone();
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
        self.decoder = Some(Decoder::start(tx_wake));
        while !self.quit {
            self.take_decoded();
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
                    // A focus request is a file beside the database, not a write to it.
                    self.apply_focus()?;
                    self.reload_settings()?;
                    self.poll_background();
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        Ok(())
    }
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
    app.starting = true;
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
    app.starting = false;
    if app.details.is_none() {
        app.load_details()?;
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
