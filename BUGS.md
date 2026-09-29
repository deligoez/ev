# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **A terminal that does not answer the image query loses the first keypress in `ev ui`.**
  At start, ratatui-image asks the terminal which picture protocol it supports and reads the
  answer on a thread of its own; when no answer comes before its timeout, that thread stays
  blocked on stdin and takes the next input (a key, or a mode 2031 report). Seen 2026-09-29 while driving `ev ui` in a bare pty that answers nothing; Ghostty,
  kitty and every terminal that answers the status report (`CSI 5 n`) are not affected. Older
  than 0.7.0 — crossterm read stdin the same way. A fix would ask for the protocol through
  `ev ui`'s own input reader instead of `Picker::from_query_stdio`.
