//! The words people read, in English or Turkish.
//!
//! The English text is the key: `t("Tree")` returns it unchanged in English and looks up the
//! translation otherwise, falling back to English when one is missing. `tf` fills `{}`
//! placeholders in order, after translating, so a language may move words around a value but
//! keeps the same number of placeholders (a test checks that). Another language is one more
//! table and one more `Lang` variant.

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    Tr,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Tr => "tr",
        }
    }

    /// The name of the language in itself, as a language menu shows it.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Tr => "Türkçe",
        }
    }

    pub fn from_code(s: &str) -> Option<Self> {
        match s {
            "en" => Some(Lang::En),
            "tr" => Some(Lang::Tr),
            _ => None,
        }
    }

    /// A BCP 47 tag such as `tr-TR` or `en-TR`: only the language part counts, so an English
    /// system set to the Turkish region stays English.
    pub fn from_tag(tag: &str) -> Lang {
        let lang = tag
            .split(['-', '_', '.'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        Lang::from_code(&lang).unwrap_or(Lang::En)
    }
}

/// The computer's preferred language, English when it is not one ev speaks. Read once.
///
/// On macOS that is the first of System Settings' preferred languages, read with `defaults`
/// rather than CoreFoundation so the release can still be cross-built without the macOS SDK;
/// a terminal's `LANG` is often `en_US.UTF-8` whatever the system says, so it only counts
/// elsewhere, or when `defaults` gives nothing.
pub fn system_lang() -> Lang {
    static CACHE: OnceLock<Lang> = OnceLock::new();
    *CACHE.get_or_init(|| {
        #[cfg(target_os = "macos")]
        if let Some(tag) = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleLanguages"])
            .output()
            .ok()
            .and_then(|o| first_apple_language(&String::from_utf8_lossy(&o.stdout)))
        {
            return Lang::from_tag(&tag);
        }
        for var in ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"] {
            if let Ok(v) = std::env::var(var) {
                let v = v.split(':').next().unwrap_or_default();
                if !v.is_empty() && v != "C" && v != "POSIX" {
                    return Lang::from_tag(v);
                }
            }
        }
        Lang::En
    })
}

