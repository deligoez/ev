//! How `ev ui` draws photos: the terminal's answers to the picture-protocol query.

use super::*;

/// How long the terminal has to answer before photos are drawn with what came by then.
pub(super) const PROBE_WAIT: Duration = Duration::from_secs(2);

/// How the answers were taken as complete.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Settled {
    /// Both the device attributes and the status report came.
    Answered,
    /// `PROBE_WAIT` passed first.
    TimedOut,
}

/// What the terminal said about pictures. The query asks four things; a multiplexer between
/// may answer some itself at once and hand the others on later, so no single answer marks the
/// end: the device attributes end the queries the terminal answers, the status report those
/// answered on the way. Answers after that still count.
#[derive(Debug)]
pub(super) struct ImageProbe {
    pub(super) kitty: bool,
    pub(super) attributes: bool,
    pub(super) sixel: bool,
    pub(super) cell: Option<(u16, u16)>,
    pub(super) status: bool,
    /// When the query was sent.
    pub(super) asked: Instant,
    /// How and how long after the query the answers were taken as complete.
    pub(super) settled: Option<(Settled, Duration)>,
    /// The choice the picker in use was made from.
    pub(super) applied: Option<Choice>,
}

impl Default for ImageProbe {
    fn default() -> Self {
        ImageProbe {
            kitty: false,
            attributes: false,
            sixel: false,
            cell: None,
            status: false,
            asked: Instant::now(),
            settled: None,
            applied: None,
        }
    }
}

/// The protocol photos are drawn with and the cell size they are fitted to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Choice {
    pub(super) protocol: ProtocolType,
    pub(super) cell: Option<(u16, u16)>,
}

impl ImageProbe {
    pub(super) fn record(&mut self, g: Graphics) {
        match g {
            Graphics::Kitty => self.kitty = true,
            Graphics::Attributes { sixel } => {
                self.attributes = true;
                self.sixel |= sixel;
            }
            Graphics::CellSize { width, height } => self.cell = Some((width, height)),
            Graphics::Done => self.status = true,
        }
    }

    /// Takes the answers as complete when both closing answers came, or when `PROBE_WAIT`
    /// has passed; true when that happened now.
    pub(super) fn settle(&mut self, now: Instant) -> bool {
        if self.settled.is_some() {
            return false;
        }
        let waited = now.saturating_duration_since(self.asked);
        let how = if self.attributes && self.status {
            Settled::Answered
        } else if waited >= PROBE_WAIT {
            Settled::TimedOut
        } else {
            return false;
        };
        self.settled = Some((how, waited));
        true
    }

    /// The choice the answers call for, with ratatui-image's own preferences: kitty, then
    /// iTerm2 where the terminal is known to speak it, then sixel; half blocks without a cell
    /// size, since pictures are fitted to cells.
    pub(super) fn choice(&self) -> Choice {
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
        let cell = self.cell.filter(|&(w, h)| w > 0 && h > 0);
        Choice {
            protocol: if cell.is_some() {
                protocol
            } else {
                ProtocolType::Halfblocks
            },
            cell,
        }
    }
}

impl Choice {
    pub(super) fn picker(self) -> Picker {
        match self.cell {
            Some((w, h)) if self.protocol != ProtocolType::Halfblocks => {
                // Deprecated only in favour of the stdio query, which is what this replaces.
                #[allow(deprecated)]
                let mut p = Picker::from_fontsize((w, h).into());
                p.set_protocol_type(self.protocol);
                p
            }
            _ => Picker::halfblocks(),
        }
    }
}

impl App {
    /// Records an answer to the picture-protocol query; once the answers are complete, and
    /// again for any that comes after, photos are drawn with what they call for.
    pub(super) fn on_graphics(&mut self, g: Graphics) {
        let Some(probe) = self.probe.as_mut() else {
            return;
        };
        probe.record(g);
        probe.settle(Instant::now());
        self.apply_probe();
    }

    /// Takes the answers as they are once the terminal has had `PROBE_WAIT` to give them.
    pub(super) fn probe_deadline(&mut self) {
        if self
            .probe
            .as_mut()
            .is_some_and(|p| p.settle(Instant::now()))
        {
            self.apply_probe();
        }
    }

    /// Switches the picker to the settled answers' choice when it differs from the one in use.
    fn apply_probe(&mut self) {
        let Some(probe) = self.probe.as_mut() else {
            return;
        };
        if probe.settled.is_none() {
            return;
        }
        let choice = probe.choice();
        if probe.applied != Some(choice) {
            probe.applied = Some(choice);
            self.picker = Some(choice.picker());
            self.shown = None;
        }
    }

    /// The Settings row's value: the protocol photos are drawn with, and its cell size.
    pub(super) fn pictures_label(&self) -> String {
        let Some(probe) = &self.probe else {
            // Inside tmux ratatui-image asked the terminal; only its picker is known.
            return self.picker.as_ref().map_or_else(String::new, |p| {
                let f = p.font_size();
                tf(
                    "{} · {}×{} px cell (asked through tmux)",
                    &[&protocol_name(p.protocol_type()), &f.width, &f.height],
                )
            });
        };
        if probe.settled.is_none() {
            return t("asking the terminal").to_string();
        }
        let c = probe.choice();
        match (c.cell, probe.why_half_blocks()) {
            (_, Some(why)) => tf("{} ({})", &[&protocol_name(c.protocol), &t(why)]),
            (Some((w, h)), None) => tf("{} · {}×{} px cell", &[&protocol_name(c.protocol), &w, &h]),
            (None, None) => protocol_name(c.protocol).to_string(),
        }
    }

    /// The right pane for the Pictures row: what was asked, what came, and what was chosen.
    pub(super) fn pictures_text(&self) -> Text<'static> {
        let muted = |s: String| Line::from(Span::styled(s, Style::new().fg(pal().muted)));
        let mut lines = vec![
            Line::from(t("Pictures")).bold(),
            Line::raw(""),
            Line::raw(t(
                "ev ui draws photos with the terminal's picture protocol when the terminal reports one and its cell size, and with half-block characters otherwise. It asks each time it starts; this is not a setting.",
            )),
            Line::raw(""),
            Line::raw(tf("In use: {}", &[&self.pictures_label()])),
        ];
        if let Some(probe) = &self.probe {
            let mark = |b: bool| if b { "✓" } else { "–" };
            lines.push(Line::raw(tf(
                "Answers: picture protocol {}, device attributes {}, cell size {}, status report {}",
                &[
                    &mark(probe.kitty),
                    &mark(probe.attributes),
                    &mark(probe.cell.is_some()),
                    &mark(probe.status),
                ],
            )));
            lines.push(Line::raw(match probe.settled {
                Some((Settled::Answered, d)) => {
                    tf("All answers came within {} ms.", &[&d.as_millis()])
                }
                Some((Settled::TimedOut, d)) => tf(
                    "Not every answer came within {} ms; photos are drawn with what did, and a later answer still counts.",
                    &[&d.as_millis()],
                ),
                None => t("Waiting for the terminal's answers.").to_string(),
            }));
        }
        lines.push(Line::raw(""));
        lines.push(muted(
            t("ev ui --probe asks the terminal again and prints its answers as they came.")
                .to_string(),
        ));
        Text::from(lines)
    }
}

impl ImageProbe {
    /// Why the answers leave photos to half blocks, when they do.
    pub(super) fn why_half_blocks(&self) -> Option<&'static str> {
        if self.choice().protocol != ProtocolType::Halfblocks {
            return None;
        }
        // The protocol the answers would call for with a cell size.
        let sized = ImageProbe {
            cell: Some((1, 1)),
            ..ImageProbe::default()
        };
        let protocol = ImageProbe {
            kitty: self.kitty,
            sixel: self.sixel,
            ..sized
        }
        .choice()
        .protocol;
        Some(if std::env::var_os("KONSOLE_VERSION").is_some() {
            HALF_BLOCK_REASONS[0]
        } else if protocol == ProtocolType::Halfblocks {
            HALF_BLOCK_REASONS[2]
        } else {
            HALF_BLOCK_REASONS[1]
        })
    }
}

/// Why photos are drawn with half blocks; the English is `ev ui --probe`'s JSON.
const HALF_BLOCK_REASONS: [&str; 3] = [
    "Konsole draws pictures through half blocks only",
    "the terminal did not report its cell size",
    "the terminal reported no picture protocol",
];

/// A reason from `ev ui --probe`'s JSON in the language shown.
pub fn half_blocks_reason(why: &str) -> String {
    HALF_BLOCK_REASONS
        .into_iter()
        .find(|r| *r == why)
        .map_or_else(|| why.to_string(), |r| t(r).to_string())
}

/// A protocol's name as the person reads it.
pub(super) fn protocol_name(p: ProtocolType) -> &'static str {
    match p {
        ProtocolType::Kitty => "kitty",
        ProtocolType::Sixel => "sixel",
        ProtocolType::Iterm2 => "iTerm2",
        ProtocolType::Halfblocks => t("half blocks"),
    }
}

/// `ev ui --probe`: sends `ev ui`'s picture query to the terminal and reports every answer as
/// it came, with what `ev ui` would choose from them. It talks to the terminal itself, not to
/// standard output, so its report can be piped.
pub fn probe_terminal() -> Result<Value> {
    use ratatui::crossterm::terminal;
    use std::io::Read;
    let io = |e: std::io::Error| Error::Usage(format!("`ev ui --probe` needs a terminal: {e}"));
    let mut tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(io)?;
    let mut reader = tty.try_clone().map_err(io)?;
    let env = serde_json::json!({
        "TERM": std::env::var("TERM").ok(),
        "TERM_PROGRAM": std::env::var("TERM_PROGRAM").ok(),
        "tmux": std::env::var_os("TMUX").is_some(),
    });
    terminal::enable_raw_mode().map_err(io)?;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    // Left blocked on the terminal when the probe ends; the process exits right after.
    std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        while let Ok(n @ 1..) = reader.read(&mut buf) {
            if tx.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut probe = ImageProbe::default();
    let sent = tty
        .write_all(input::IMAGE_QUERY.as_bytes())
        .and_then(|()| tty.flush());
    let mut parser = input::Parser::default();
    let mut received = Vec::new();
    while sent.is_ok() && !probe.settle(Instant::now()) {
        if let Ok(bytes) = rx.recv_timeout(Duration::from_millis(50)) {
            received.push(serde_json::json!({
                "ms": probe.asked.elapsed().as_secs_f64() * 1000.0,
                "bytes": bytes.escape_ascii().to_string(),
            }));
            for i in parser.feed(&bytes) {
                if let Input::Graphics(g) = i {
                    probe.record(g);
                }
            }
        }
    }
    let _ = terminal::disable_raw_mode();
    sent.map_err(io)?;
    let c = probe.choice();
    let (how, waited) = probe.settled.unwrap_or((Settled::TimedOut, PROBE_WAIT));
    Ok(serde_json::json!({ "pictures": {
        "protocol": protocol_name(c.protocol),
        "cell": c.cell.map(|(w, h)| serde_json::json!({"width": w, "height": h})),
        "half_blocks_because": probe.why_half_blocks(),
        "answers": {
            "picture_protocol": probe.kitty,
            "device_attributes": probe.attributes,
            "sixel": probe.sixel,
            "cell_size": probe.cell.is_some(),
            "status_report": probe.status,
        },
        "complete": how == Settled::Answered,
        "ms": waited.as_millis(),
        "received": received,
        "env": env,
    }}))
}
