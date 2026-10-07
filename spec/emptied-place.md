# A place that was just emptied

## Decision (2026-10-08)

When the last thing leaves a box or a drawer, its theme ("kablolar") and the observations on it
("the HDMI cables still to sort") describe what used to be there. The inventory agent clears
them by hand afterwards, since the person said not to ask about it (its report), and nothing
told it which place had just become empty.

- **Every command that takes things out says which place it left empty:** `ev move` (one or
  several), `ev done` and `ev gone` answer `emptied` when a place
  they took the last live thing out of still carries a theme or observations:
  `[{node, theme, observations: [{id, text}]}]`. A place left empty with neither is not listed:
  nothing about it went stale.
- **ev clears nothing on its own.** The answer names what went stale; the text output gives the
  two commands that clear it, and the agent runs them on the person's word (a standing word
  counts, as in this household):
  - `ev edit <place> theme=` (already there), and
  - **`ev unobserve --on <place>`**, new: every observation of a place in one step, each kept in
    the history as `ev unobserve <id>` keeps it. `ev unobserve` also takes several ids now.
- A place emptied this way is still not "known empty" for `ev find --empty` unless it was
  toured or the person says so (`ev empty`): something being moved out is not someone looking
  inside.

## Why

The agent proposes, the person confirms; but the agent can only propose what it sees, and an
emptied place is seen only by the command that emptied it. Friction is a missing verb: clearing a
place's observations one id at a time is the script around ev this replaces.

## Not now

- Clearing the theme and observations automatically, even with a standing word: a setting for
  it, when another household asks.
- A place emptied by a tour's corrections (`ev edit qty=0`): only moves and leavings take things
  out.
