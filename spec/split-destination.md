# A split part that goes somewhere at once

## Decision (2026-10-08)

Touring a place, the agent meets a record the inventory guessed in bulk ("many cables") and
breaks it up: one record per piece, each going its own way, one into a drawer, one to the bin.
That was `ev split`, then `ev move` and `ev gone` per piece: three commands or more each, ids
chained by hand (the inventory agent's report). A part of `ev split` now carries where it goes.

- **Inline, a place:** `<name>[=<qty>]@<place>` moves the new part there in the same step:
  `ev split #412 "USB-C kablo=2@S5-01" "HDMI kablo"` makes two records, the first in S5-01.
- **Every destination, from standard input:** `ev split #412 --stdin` reads one JSON object a
  line: `{"name", "qty"?, "to"?}` or `{"name", "qty"?, "gone": "<disposition>", "why"?}`. `gone`
  takes what `ev gone --as` takes (`trash`, `give`, `sell`, …) and leaves at once, the reason in
  the note as `ev gone --why` writes it. A line with both `to` and `gone` is refused.
- **All or nothing.** Parts, moves and leavings are one transaction: a place that does not
  exist, a disposition ev does not know, or a move the placement rules refuse undoes the split.
- **The answer** is `ev split`'s, each part in `into` where it ended up (its `path_text`, or
  gone with its disposition). A part that left at once is not offered a purchase to link.
- Nothing else changes: the parts are made beside the original as before, then moved; the
  history keeps `split_from`, then the move or the leaving, on each part.

## Why

Friction is a missing verb: the agent scripted around ev with ids it had to carry from one
answer to the next. A split is the moment the agent knows where each piece goes, so the split
is where it says so.

## Not now

- A destination for the original's remaining part (`ev move` it, as before).
- Planned moves (`--plan`) per part: the pieces are in hand when they are told apart.
