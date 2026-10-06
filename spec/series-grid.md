# The marked photo series at a glance

## Decision

A series now reaches 30–40 pictures (`ev photo add` takes many at once), and stepping f1→f40 with
`[` `]` is slow. `ev ui` gets a **grid of the series**, and quicker ways to move through it in both
views.

- **`g` toggles grid ↔ single** while the series is open. The grid shows every picture as a tile,
  titled `f12 · <note>` (cut to the tile) with a badge of how many frames it carries (`·3`); the
  picture shown in single view is the one selected in the grid, and back.
- **Columns follow the width, not a count.** Columns = the pane's width ÷ the tile width, at
  least one; recomputed on every draw, so a resized terminal re-flows at once. Tile height keeps
  the pictures' shape (about 4:3, a terminal cell being twice as tall as wide) at least; the
  height left under the rows that fit is shared among them (a short series takes the whole
  height in the rows it needs), so no band stays empty at the bottom and a portrait photo, the
  usual one, is drawn taller (2026-10-06: a fifth of a tall screen stayed empty). A picture is fitted inside
  its tile, centred both ways.
- **One setting, one zoom.** `ev settings series_tile <cells>` sets the tile width (default 28
  cells, at least 12): the size that reads on the person's screen. `+` / `-` in the grid change
  it by a step for now and are remembered in `ui-state.json` with the other screen state; the
  setting is where a new `ev ui` starts.
- **Moving:** arrows move by tile, `Home` / `End` first / last, `Enter` opens the selected
  picture in single view, `f` then digits then `Enter` jumps to that picture (`f12`), in both
  views. `Esc` hides the series and `X` closes it, as today. In single view, `Home` / `End` go
  to the first and last picture too.
- **Fast with many pictures.** Only the visible rows are drawn. Thumbnails are decoded and
  scaled down off the event loop (the decoder thread the photo pane uses) and cached by file and
  tile size, so scrolling the grid does not decode a photo again.

## Why

The series is how the person and the agent talk about a batch (`f12`, frame `3`). With forty
pictures, finding f27 by stepping takes 26 key presses, and the person cannot see the batch as a
whole. A width-based grid answers the open questions without one more thing to set up: how many
fit a row follows from the terminal and one size the person picks once.

## Not now

- Choosing several pictures in the grid to act on (attach, close): the agent attaches by
  `f`-number.
- A grid of a record's own photos (the Photos tab): only the series grows this long.
