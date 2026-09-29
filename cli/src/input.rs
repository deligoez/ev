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
    /// An answer to the picture-protocol query (`IMAGE_QUERY`).
    Graphics(Graphics),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Graphics {
    /// The kitty graphics protocol answered OK.
    Kitty,
    /// Device attributes; `sixel` when they include 4.
    Attributes { sixel: bool },
    /// Cell size in pixels, from `CSI 16 t`.
    CellSize { width: u16, height: u16 },
    /// The status report that ends the query: every terminal answers it.
    Done,
}

/// Which picture protocol the terminal speaks and its cell size, ended by a status report so
/// the answers are known to be complete. The same queries ratatui-image sends, but answered
/// through this reader, so a terminal that never answers leaves no thread waiting on stdin.
pub const IMAGE_QUERY: &str = "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c\x1b[16t\x1b[5n";

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

fn parse_escape(b: &[u8]) -> Step {
    let Some(&next) = b.get(1) else {
        return Step::More;
    };
    match next {
        b'[' => parse_csi(b),
        b'O' => match b.get(2) {
            None => Step::More,
            Some(&c) => Step::Done(3, ss3(c)),
        },
        // OSC, DCS, APC and PM run to BEL or ST; only OSC 11 matters, the rest (graphics
        // protocol replies among them) is dropped rather than read as keys.
        b']' | b'P' | b'_' | b'^' => parse_string(b),
        0x1b => Step::Done(1, key(KeyCode::Esc, KeyModifiers::NONE)),
        _ => match parse(&b[1..]) {
            Step::More => Step::More,
            Step::Done(n, Some(Input::Key(mut k))) => {
                k.modifiers |= KeyModifiers::ALT;
                Step::Done(n + 1, Some(Input::Key(k)))
            }
            Step::Done(n, other) => Step::Done(n + 1, other),
        },
    }
}

fn ss3(c: u8) -> Option<Input> {
    let code = match c {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'H' => KeyCode::Home,
        b'F' => KeyCode::End,
        b'P'..=b'S' => KeyCode::F(c - b'P' + 1),
        _ => return None,
    };
    key(code, KeyModifiers::NONE)
}

fn parse_string(b: &[u8]) -> Step {
    for i in 2..b.len() {
        let end = match b[i] {
            0x07 => Some(i + 1),
            0x1b if b.get(i + 1) == Some(&b'\\') => Some(i + 2),
            0x1b if i + 1 == b.len() => return Step::More,
            _ => None,
        };
        if let Some(end) = end {
            let body = std::str::from_utf8(&b[2..i]).unwrap_or("");
            let input = match b[1] {
                b']' => theme::parse_osc11(body).map(|mode| Input::Appearance {
                    mode,
                    notified: false,
                }),
                b'_' if body.starts_with("Gi=31;") && body.ends_with("OK") => {
                    Some(Input::Graphics(Graphics::Kitty))
                }
                _ => None,
            };
            return Step::Done(end, input);
        }
    }
    Step::More
}

fn parse_csi(b: &[u8]) -> Step {
    // Parameters and intermediates, then one final byte in 0x40..=0x7e.
    let Some(end) = b[2..].iter().position(|c| (0x40..=0x7e).contains(c)) else {
        return Step::More;
    };
    let end = end + 2;
    let body = std::str::from_utf8(&b[2..end]).unwrap_or("");
    let fin = b[end];
    let n = end + 1;
    if let Some(sgr) = body.strip_prefix('<') {
        return Step::Done(n, sgr_mouse(sgr, fin == b'M'));
    }
    if let Some(private) = body.strip_prefix('?') {
        // Mode 2031's report, also the answer to DSR 996.
        let input = match (fin, private) {
            (b'n', "997;1") => Some(Mode::Dark),
            (b'n', "997;2") => Some(Mode::Light),
            _ => None,
        }
        .map(|mode| Input::Appearance {
            mode,
            notified: true,
        });
        return Step::Done(n, input);
    }
    let params: Vec<u16> = body.split(';').map(|p| p.parse().unwrap_or(0)).collect();
    let modifiers = params
        .get(1)
        .map(|&m| modifiers_of(m))
        .unwrap_or(KeyModifiers::NONE);
    let code = match fin {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'H' => KeyCode::Home,
        b'F' => KeyCode::End,
        b'Z' => return Step::Done(n, key(KeyCode::BackTab, KeyModifiers::SHIFT)),
        b'~' => match params.first().copied().unwrap_or(0) {
            1 | 7 => KeyCode::Home,
            2 => KeyCode::Insert,
            3 => KeyCode::Delete,
            4 | 8 => KeyCode::End,
            5 => KeyCode::PageUp,
            6 => KeyCode::PageDown,
            _ => return Step::Done(n, None),
        },
        _ => return Step::Done(n, None),
    };
    Step::Done(n, key(code, modifiers))
}

fn modifiers_of(m: u16) -> KeyModifiers {
    let bits = m.saturating_sub(1);
    let mut out = KeyModifiers::NONE;
    if bits & 1 != 0 {
        out |= KeyModifiers::SHIFT;
    }
    if bits & 2 != 0 {
        out |= KeyModifiers::ALT;
    }
    if bits & 4 != 0 {
        out |= KeyModifiers::CONTROL;
    }
    out
}

/// SGR mouse report `<b;x;y` with `M` for press/drag and `m` for release; x and y are 1-based.
fn sgr_mouse(body: &str, press: bool) -> Option<Input> {
    let mut it = body.split(';').map(|p| p.parse::<u16>().ok());
    let (cb, x, y) = (it.next()??, it.next()??, it.next()??);
    let mut modifiers = KeyModifiers::NONE;
    if cb & 4 != 0 {
        modifiers |= KeyModifiers::SHIFT;
    }
    if cb & 8 != 0 {
        modifiers |= KeyModifiers::ALT;
    }
    if cb & 16 != 0 {
        modifiers |= KeyModifiers::CONTROL;
    }
    let button = match cb & 3 {
        0 => MouseButton::Left,
        1 => MouseButton::Middle,
        _ => MouseButton::Right,
    };
    let kind = if cb & 64 != 0 {
        match cb & 3 {
            0 => MouseEventKind::ScrollUp,
            1 => MouseEventKind::ScrollDown,
            2 => MouseEventKind::ScrollLeft,
            _ => MouseEventKind::ScrollRight,
        }
    } else if cb & 32 != 0 {
        if cb & 3 == 3 {
            MouseEventKind::Moved
        } else {
            MouseEventKind::Drag(button)
        }
    } else if press {
        MouseEventKind::Down(button)
    } else {
        MouseEventKind::Up(button)
    };
    Some(Input::Mouse(MouseEvent {
        kind,
        column: x.saturating_sub(1),
        row: y.saturating_sub(1),
        modifiers,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(p: &mut Parser, bytes: &[u8]) -> Vec<Input> {
        p.feed(bytes)
    }

    fn k(c: KeyCode) -> Input {
        Input::Key(KeyEvent::new(c, KeyModifiers::NONE))
    }

    #[test]
    fn an_appearance_report_does_not_swallow_the_keys_after_it() {
        let mut p = Parser::default();
        let got = keys(&mut p, b"\x1b[?997;2nj\x1b[?997;1nq");
        assert_eq!(
            got,
            vec![
                Input::Appearance {
                    mode: Mode::Light,
                    notified: true
                },
                k(KeyCode::Char('j')),
                Input::Appearance {
                    mode: Mode::Dark,
                    notified: true
                },
                k(KeyCode::Char('q')),
            ]
        );
        assert!(!p.pending());
    }

    #[test]
    fn a_background_answer_is_read_whole_in_pieces_and_either_terminator() {
        let mut p = Parser::default();
        assert!(keys(&mut p, b"\x1b]11;rgb:ffff/ff").is_empty());
        assert!(p.pending());
        let got = keys(&mut p, b"ff/ffff\x1b\\k");
        assert_eq!(
            got,
            vec![
                Input::Appearance {
                    mode: Mode::Light,
                    notified: false
                },
                k(KeyCode::Char('k'))
            ]
        );
        let got = keys(&mut p, b"\x1b]11;rgb:1a1a/1a1a/1f1f\x07");
        assert_eq!(
            got,
            vec![Input::Appearance {
                mode: Mode::Dark,
                notified: false
            }]
        );
        // A graphics protocol reply is dropped, not typed.
        assert!(keys(&mut p, b"\x1b_Gi=1;OK\x1b\\").is_empty());
    }

    #[test]
    fn keys_the_ui_uses_are_recognised() {
        let mut p = Parser::default();
        let got = keys(
            &mut p,
            b"\x1b[A\x1b[B\x1bOC\x1b[D\x1b[5~\x1b[6~\x1b[H\x1b[F\x1b[Z\t\r\x7f",
        );
        let codes: Vec<KeyCode> = got
            .into_iter()
            .map(|i| match i {
                Input::Key(k) => k.code,
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            codes,
            vec![
                KeyCode::Up,
                KeyCode::Down,
                KeyCode::Right,
                KeyCode::Left,
                KeyCode::PageUp,
                KeyCode::PageDown,
                KeyCode::Home,
                KeyCode::End,
                KeyCode::BackTab,
                KeyCode::Tab,
                KeyCode::Enter,
                KeyCode::Backspace,
            ]
        );
        let got = keys(&mut p, "\x03\x15ğR".as_bytes());
        assert_eq!(
            got,
            vec![
                Input::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
                Input::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
                k(KeyCode::Char('ğ')),
                Input::Key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::SHIFT)),
            ]
        );
    }

    #[test]
    fn a_lone_escape_is_the_esc_key_only_once_nothing_follows() {
        let mut p = Parser::default();
        assert!(keys(&mut p, b"\x1b").is_empty());
        assert!(p.pending());
        assert_eq!(p.flush(), vec![k(KeyCode::Esc)]);
        // A split arrow key is not an Esc.
        assert!(keys(&mut p, b"\x1b").is_empty());
        assert_eq!(keys(&mut p, b"[A"), vec![k(KeyCode::Up)]);
    }

    #[test]
    fn sgr_mouse_reports_become_mouse_events() {
        let mut p = Parser::default();
        let got = keys(&mut p, b"\x1b[<0;10;5M\x1b[<0;10;5m\x1b[<65;3;4M");
        let kinds: Vec<(MouseEventKind, u16, u16)> = got
            .into_iter()
            .map(|i| match i {
                Input::Mouse(m) => (m.kind, m.column, m.row),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                (MouseEventKind::Down(MouseButton::Left), 9, 4),
                (MouseEventKind::Up(MouseButton::Left), 9, 4),
                (MouseEventKind::ScrollDown, 2, 3),
            ]
        );
    }
}
