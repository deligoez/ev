//! The screen's own commands, named once with their key and when they apply: what `:` offers
//! besides going somewhere (spec/ui-sidebar.md, "The palette's commands").

use super::*;

/// One command: its name as the palette shows it, the key that does it, and whether it applies
/// to what is on the screen now. Running it presses the key, so the two cannot drift apart.
pub(super) struct Command {
    pub(super) name: fn() -> &'static str,
    pub(super) key: KeyCode,
    pub(super) applies: fn(&App) -> bool,
}

fn always(_: &App) -> bool {
    true
}

fn details(app: &App) -> bool {
    app.details.is_some()
}

fn photo(app: &App) -> bool {
    app.current_photo().is_some() && app.shown_detail_tab() != DetailTab::Documents
}

fn tree(app: &App) -> bool {
    app.tab == Tab::Tree
}

fn series(app: &App) -> bool {
    app.last_overlay.is_some()
}

pub(super) const COMMANDS: &[Command] = &[
    Command {
        name: || t("Search everything"),
        key: KeyCode::Char('/'),
        applies: always,
    },
    Command {
        name: || t("Map of the home"),
        key: KeyCode::Char('M'),
        applies: always,
    },
    Command {
        name: || t("Show or hide the sidebar"),
        key: KeyCode::Char('b'),
        applies: always,
    },
    Command {
        name: || t("Narrower list"),
        key: KeyCode::Char('<'),
        applies: always,
    },
    Command {
        name: || t("Wider list"),
        key: KeyCode::Char('>'),
        applies: always,
    },
    Command {
        name: || t("Widen the details, or put them back"),
        key: KeyCode::Char('+'),
        applies: details,
    },
    Command {
        name: || t("Copy: the #id and name, or the document"),
        key: KeyCode::Char('y'),
        applies: details,
    },
    Command {
        name: || t("Show or hide the empty fields"),
        key: KeyCode::Char('E'),
        applies: details,
    },
    Command {
        name: || t("Next details tab"),
        key: KeyCode::Char('L'),
        applies: details,
    },
    Command {
        name: || t("Previous details tab"),
        key: KeyCode::Char('H'),
        applies: details,
    },
    Command {
        name: || t("Next place of this thing"),
        key: KeyCode::Char('p'),
        applies: App::elsewhere_shown,
    },
    Command {
        name: || t("Filter: open, linked, dismissed"),
        key: KeyCode::Char('f'),
        applies: |app| app.tab.bucket().is_some(),
    },
    Command {
        name: || t("Open everything below"),
        key: KeyCode::Char('e'),
        applies: tree,
    },
    Command {
        name: || t("Close everything below"),
        key: KeyCode::Char('c'),
        applies: tree,
    },
    Command {
        name: || t("Open two levels"),
        key: KeyCode::Char('d'),
        applies: tree,
    },
    Command {
        name: || t("Close the whole tree"),
        key: KeyCode::Char('C'),
        applies: tree,
    },
    Command {
        name: || t("Photo full screen"),
        key: KeyCode::Char('o'),
        applies: photo,
    },
    Command {
        name: || t("Open the photo outside"),
        key: KeyCode::Char('O'),
        applies: photo,
    },
    Command {
        name: || t("Rotate the photo right"),
        key: KeyCode::Char('r'),
        applies: photo,
    },
    Command {
        name: || t("Rotate the photo left"),
        key: KeyCode::Char('R'),
        applies: photo,
    },
    Command {
        name: || t("Smaller photo"),
        key: KeyCode::Char('{'),
        applies: photo,
    },
    Command {
        name: || t("Larger photo"),
        key: KeyCode::Char('}'),
        applies: photo,
    },
    Command {
        name: || t("Reopen the marked photo series"),
        key: KeyCode::Char('m'),
        applies: series,
    },
    Command {
        name: || t("Close the marked photo series"),
        key: KeyCode::Char('X'),
        applies: series,
    },
    Command {
        name: || t("Quit"),
        key: KeyCode::Char('q'),
        applies: always,
    },
];

/// A key as the palette and the help line write it.
pub(super) fn key_label(key: KeyCode) -> String {
    match key {
        KeyCode::Char(c) => c.to_string(),
        other => format!("{other}"),
    }
}

impl App {
    /// The selected thing is also in other places, which `p` steps through.
    pub(super) fn elsewhere_shown(&self) -> bool {
        self.details.as_ref().is_some_and(|d| {
            d["thing"]["elsewhere"]
                .as_array()
                .is_some_and(|e| !e.is_empty())
        })
    }

    /// Runs a command of the table by pressing its key.
    pub(super) fn run_command(&mut self, i: usize) -> Result<()> {
        match COMMANDS.get(i) {
            Some(c) => self.key(KeyEvent::new(c.key, KeyModifiers::NONE)),
            None => Ok(()),
        }
    }
}
