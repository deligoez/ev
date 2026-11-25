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
}
