//! `ev ui`: a read-only terminal browser that follows the database as it changes.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use ev_core::{Error, Inventory, Result};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Tabs, Wrap};
use ratatui::{DefaultTerminal, Frame};
use serde_json::Value;

const POLL: Duration = Duration::from_millis(500);
const HIGHLIGHT_FOR: Duration = Duration::from_secs(6);
const TABS: [&str; 5] = ["Ağaç", "Bekleyen", "Çıkış", "Kayıp", "Ara"];

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
    text: String,
    expandable: bool,
    expanded: bool,
    muted: bool,
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

