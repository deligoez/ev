# Where a marked-photo series ends

## Decision

A marked-photo series (spec/focus-stack.md) gathers the pictures the agent shows the person,
numbered `f1…`, with numbered frames drawn on them, until the person closes it. A batch of one
drawer's photos joined the series still open from the previous drawer: its frames were numbered
16–32 and its pictures f16–f34, so "frame 16" and "f16" named different things, and the person
saw it only afterwards. The agent may not close a series on its own, and a series closed by
mistake loses what was on screen; but nothing told the agent that a new batch had begun.
(Reported by the inventory agent, 2026-10-06.)

- **A series knows what it is about.** The first picture of a series about a place sets the
  series' `about`: the place `ev photo mark <place> …`, `ev photo cut … --place <ref>` or
  `ev photo add <ref> …` named, or `ev focus --file … --for <place>` names it outright. A
  series of pictures about no place (a receipt, a parts list) has none.
- **A picture about another place says so, and changes nothing.** It joins the series as today,
  and ev's answer carries `series_about: {about, now}` (each a brief record) with a hint: the
  series is about A, this picture about B; ask the person whether to close it before the next
  batch (`ev focus --clear`). ev closes nothing by itself. `ev ui` shows the same in the
  series' title (`f17 · about K4x4-06-A, the series K4x4-07-Ü`).
- **Numbers stay unique while a series lives.** A new batch in the same series keeps counting
  frames, so a number on screen still means one frame; numbering from 1 comes with the next
  series, the one opened after the person closes this. The hint above is what makes that
  happen at the right moment.
- **The skill:** when a batch for another place begins and a series is open, ask the person in
  one line whether to close it; never close it unasked.

## Data

The focus request (the file `ev ui` reads) gains `about`: `{id, label}` of the place, set by
the first picture that names one and kept until the series closes.

## Not now

- Several series side by side, one per place.
- Closing a series by a timer or by "done" said of a place.
