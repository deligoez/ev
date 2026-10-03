//! The Settings tab and the language and appearance it controls.

use super::*;

impl App {
    /// The appearance in use: the fixed one, or what the terminal reported (dark until it
    /// says otherwise).
    pub(super) fn effective_mode(&self) -> Mode {
        match self.prefs.theme {
            ThemePref::Fixed(m) => m,
            ThemePref::Auto => self.detected.unwrap_or(Mode::Dark),
        }
    }

    /// Makes the language and palette follow the settings; rows keep their colours and words,
    /// so callers rebuild afterwards.
    pub(super) fn apply_prefs(&mut self) {
        i18n::set_lang(self.prefs.language.effective());
        theme::set_mode(self.effective_mode());
    }

    pub(super) fn set_prefs(&mut self, prefs: Settings) -> Result<()> {
        self.prefs = prefs;
        self.apply_prefs();
        if self.starting {
            return self.rebuild_rows();
        }
        self.rebuild()
    }

    /// Reads the settings file again when it changed on disk (`ev settings` from another
    /// terminal, or an edit by hand).
    pub(super) fn reload_settings(&mut self) -> Result<()> {
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
    pub(super) fn on_appearance(&mut self, mode: Mode, notified: bool) -> Result<()> {
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

    pub(super) fn language_label(&self) -> String {
        match self.prefs.language {
            LangPref::Auto => tf(
                "Automatic ({})",
                &[&tf("system: {}", &[&i18n::system_lang().native_name()])],
            ),
            LangPref::Fixed(l) => l.native_name().to_string(),
        }
    }

    pub(super) fn theme_label(&self) -> String {
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

    pub(super) fn settings_rows(&self) -> Vec<Row> {
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
    pub(super) fn settings_text(&self) -> Text<'static> {
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
    pub(super) fn cycle_setting(&mut self, id: i64, forward: bool) -> Result<()> {
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
}
