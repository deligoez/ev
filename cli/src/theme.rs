//! Light and dark palettes, and reading which one the terminal is showing.
//!
//! The dark palette uses the terminal's own named colours, as `ev ui` always has. On a light
//! background several of those (yellow, cyan, dark grey) are hard to read, so the light palette
//! uses fixed, darker tones chosen for a white background.

use std::cell::Cell;

use ratatui::style::{Color, Style};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Dark,
    Light,
}

impl Mode {
    pub fn code(self) -> &'static str {
        match self {
            Mode::Dark => "dark",
            Mode::Light => "light",
        }
    }

    pub fn from_code(s: &str) -> Option<Self> {
        match s {
            "dark" => Some(Mode::Dark),
            "light" => Some(Mode::Light),
            _ => None,
        }
    }
}

pub struct Palette {
    pub code: Color,
    pub furniture: Color,
    pub qty: Color,
    pub mark: Color,
    pub lost: Color,
    pub muted: Color,
    pub blue: Color,
    pub brand: Style,
    pub flash: Style,
}

static DARK: Palette = Palette {
    code: Color::Cyan,
    furniture: Color::Yellow,
    qty: Color::Green,
    mark: Color::Magenta,
    lost: Color::Red,
    muted: Color::DarkGray,
    blue: Color::Blue,
    brand: Style::new().fg(Color::Black).bg(Color::Cyan),
    flash: Style::new().fg(Color::Black).bg(Color::Yellow),
};

static LIGHT: Palette = Palette {
    code: Color::Rgb(0, 110, 140),
    furniture: Color::Rgb(150, 95, 0),
    qty: Color::Rgb(30, 120, 45),
    mark: Color::Rgb(140, 40, 160),
    lost: Color::Rgb(190, 30, 30),
    muted: Color::Rgb(110, 110, 110),
    blue: Color::Rgb(30, 80, 190),
    brand: Style::new().fg(Color::White).bg(Color::Rgb(0, 110, 140)),
    flash: Style::new().fg(Color::Black).bg(Color::Rgb(255, 225, 110)),
};

thread_local! {
    static MODE: Cell<Mode> = const { Cell::new(Mode::Dark) };
}

pub fn set_mode(m: Mode) {
    MODE.with(|c| c.set(m));
}

pub fn mode() -> Mode {
    MODE.with(Cell::get)
}

/// The palette of the current mode.
pub fn pal() -> &'static Palette {
    match mode() {
        Mode::Dark => &DARK,
        Mode::Light => &LIGHT,
    }
}

/// Relative luminance of an sRGB colour given as fractions; above one half reads as light,
/// the same cut Claude Code and most terminal tools use.
pub fn mode_of(r: f64, g: f64, b: f64) -> Mode {
    if 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.5 {
        Mode::Light
    } else {
        Mode::Dark
    }
}

/// The answer to an OSC 11 query, without its introducer and terminator:
/// `11;rgb:RRRR/GGGG/BBBB` (one to four hex digits per channel).
pub fn parse_osc11(payload: &str) -> Option<Mode> {
    let rgb = payload.strip_prefix("11;")?.strip_prefix("rgb:")?;
    let mut ch = rgb.split('/').map(|h| {
        let h = h.trim();
        let max = (1u32 << (4 * h.len().clamp(1, 4))) - 1;
        u32::from_str_radix(h, 16)
            .ok()
            .map(|v| f64::from(v) / f64::from(max))
    });
    let (r, g, b) = (ch.next()??, ch.next()??, ch.next()??);
    Some(mode_of(r, g, b))
}

