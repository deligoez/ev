//! Terminal input for `ev ui`, parsed here instead of by crossterm.
//!
//! To follow the terminal's light/dark switch live, `ev ui` turns on DEC private mode 2031:
//! the terminal then reports every change as `CSI ? 997 ; 1 n` (dark) or `; 2 n` (light).
//! crossterm 0.29 does not know that report — it treats it as an unfinished sequence and keeps
//! swallowing the keys typed after it — and it reads an OSC 11 background answer as Alt-`]`
//! followed by text. So `ev ui` reads the raw bytes itself and turns them into the same
//! crossterm key and mouse events, plus the two appearance reports.

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::theme::{self, Mode};

#[derive(Debug, PartialEq, Eq)]
pub enum Input {
    Key(KeyEvent),
    Mouse(MouseEvent),
    /// The terminal said which appearance it shows: a mode 2031 report or an OSC 11 answer.
    Appearance {
        mode: Mode,
        notified: bool,
    },
}

/// Turns on the appearance reports and asks for the current state both ways (DSR 996 for
/// terminals with mode 2031, OSC 11 for the rest).
pub const START: &str = "\x1b[?2031h\x1b[?996n\x1b]11;?\x1b\\";
/// Asks only for the background colour; used to poll terminals without mode 2031.
pub const ASK_BACKGROUND: &str = "\x1b]11;?\x1b\\";
pub const STOP: &str = "\x1b[?2031l";

#[derive(Default)]
pub struct Parser {
    buf: Vec<u8>,
}

enum Step {
    /// Consumed this many bytes, maybe producing an input.
    Done(usize, Option<Input>),
    /// The bytes so far are the start of a longer sequence.
    More,
}

fn key(code: KeyCode, modifiers: KeyModifiers) -> Option<Input> {
    Some(Input::Key(KeyEvent::new(code, modifiers)))
}

impl Parser {
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Input> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        loop {
            if self.buf.is_empty() {
                break;
            }
            match parse(&self.buf) {
                Step::Done(n, input) => {
                    self.buf.drain(..n);
                    out.extend(input);
                }
                Step::More => break,
            }
        }
        out
    }

    /// True when the buffer holds the start of a sequence; a lone ESC is only a key once
    /// nothing follows it for a moment.
    pub fn pending(&self) -> bool {
        !self.buf.is_empty()
    }

    /// Nothing more arrived: a lone ESC is the Esc key, and anything else unfinished is dropped.
    pub fn flush(&mut self) -> Vec<Input> {
        let out = if self.buf == [0x1b] {
            key(KeyCode::Esc, KeyModifiers::NONE).into_iter().collect()
        } else {
            Vec::new()
        };
        self.buf.clear();
        out
    }
}

fn parse(b: &[u8]) -> Step {
    match b[0] {
        0x1b => parse_escape(b),
        b'\r' | b'\n' => Step::Done(1, key(KeyCode::Enter, KeyModifiers::NONE)),
        b'\t' => Step::Done(1, key(KeyCode::Tab, KeyModifiers::NONE)),
        0x7f | 0x08 => Step::Done(1, key(KeyCode::Backspace, KeyModifiers::NONE)),
        c @ 0x01..=0x1a => Step::Done(
            1,
            key(KeyCode::Char((c - 1 + b'a') as char), KeyModifiers::CONTROL),
        ),
        c if c < 0x20 => Step::Done(1, None),
        _ => parse_char(b),
    }
}

fn parse_char(b: &[u8]) -> Step {
    let len = match b[0] {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => return Step::Done(1, None),
    };
    if b.len() < len {
        return Step::More;
    }
    match std::str::from_utf8(&b[..len])
        .ok()
        .and_then(|s| s.chars().next())
    {
        Some(c) => {
            let m = if c.is_uppercase() {
                KeyModifiers::SHIFT
            } else {
                KeyModifiers::NONE
            };
            Step::Done(len, key(KeyCode::Char(c), m))
        }
        None => Step::Done(1, None),
    }
}

