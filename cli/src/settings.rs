//! Display preferences: the language and the light/dark appearance of the terminal output.
//!
//! They belong to the person at this computer, not to the inventory, so they live in a small
//! JSON file (`~/.ev/settings.json`, or `EV_CONFIG`) instead of the database. `ev ui` may write
//! this file; it still never writes the database.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::i18n::{self, Lang};
use crate::theme::Mode;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LangPref {
    #[default]
    Auto,
    Fixed(Lang),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ThemePref {
    #[default]
    Auto,
    Fixed(Mode),
}

impl LangPref {
    pub const ALL: [LangPref; 3] = [
        LangPref::Auto,
        LangPref::Fixed(Lang::En),
        LangPref::Fixed(Lang::Tr),
    ];

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(LangPref::Auto),
            other => Lang::from_code(other).map(LangPref::Fixed),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            LangPref::Auto => "auto",
            LangPref::Fixed(l) => l.code(),
        }
    }

    /// The language to show: the fixed one, or the computer's.
    pub fn effective(self) -> Lang {
        match self {
            LangPref::Auto => i18n::system_lang(),
            LangPref::Fixed(l) => l,
        }
    }
}

impl ThemePref {
    pub const ALL: [ThemePref; 3] = [
        ThemePref::Auto,
        ThemePref::Fixed(Mode::Dark),
        ThemePref::Fixed(Mode::Light),
    ];

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(ThemePref::Auto),
            other => Mode::from_code(other).map(ThemePref::Fixed),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ThemePref::Auto => "auto",
            ThemePref::Fixed(m) => m.code(),
        }
    }
}

/// One step through a list of options, wrapping at both ends.
pub fn cycle<T: PartialEq + Copy>(all: &[T], cur: T, forward: bool) -> T {
    let i = all.iter().position(|x| *x == cur).unwrap_or(0);
    let n = all.len();
    all[if forward {
        (i + 1) % n
    } else {
        (i + n - 1) % n
    }]
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Settings {
    pub language: LangPref,
    pub theme: ThemePref,
}

impl Settings {
    /// `EV_CONFIG` when set, else `~/.ev/settings.json`.
    pub fn path() -> Option<PathBuf> {
        if let Some(p) = std::env::var_os("EV_CONFIG") {
            return Some(PathBuf::from(p));
        }
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".ev").join("settings.json"))
    }

    /// A missing or unreadable file, or an unknown value, falls back to the default.
    pub fn load_from(path: &Path) -> Settings {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Settings::default();
        };
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        Settings {
            language: v["language"]
                .as_str()
                .and_then(LangPref::parse)
                .unwrap_or_default(),
            theme: v["theme"]
                .as_str()
                .and_then(ThemePref::parse)
                .unwrap_or_default(),
        }
    }

    pub fn load() -> Settings {
        Settings::path()
            .map(|p| Settings::load_from(&p))
            .unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(&json!({
            "language": self.language.as_str(),
            "theme": self.theme.as_str(),
        }))
        .unwrap_or_default();
        std::fs::write(path, text + "\n")
    }

    pub fn to_json(self, path: Option<&Path>) -> Value {
        json!({
            "language": {
                "setting": self.language.as_str(),
                "effective": self.language.effective().code(),
                "system": i18n::system_lang().code(),
            },
            "theme": { "setting": self.theme.as_str() },
            "file": path.map(|p| p.display().to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_bad_values_fall_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("settings.json");
        assert_eq!(Settings::load_from(&path), Settings::default());
        let s = Settings {
            language: LangPref::Fixed(Lang::Tr),
            theme: ThemePref::Fixed(Mode::Light),
        };
        s.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path), s);
        std::fs::write(&path, r#"{"language":"klingon","theme":"dark"}"#).unwrap();
        let s = Settings::load_from(&path);
        assert_eq!(s.language, LangPref::Auto);
        assert_eq!(s.theme, ThemePref::Fixed(Mode::Dark));
    }

    #[test]
    fn options_cycle_both_ways() {
        let all = LangPref::ALL;
        assert_eq!(cycle(&all, LangPref::Auto, true), LangPref::Fixed(Lang::En));
        assert_eq!(
            cycle(&all, LangPref::Auto, false),
            LangPref::Fixed(Lang::Tr)
        );
        assert_eq!(cycle(&all, LangPref::Fixed(Lang::Tr), true), LangPref::Auto);
    }
}
