# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **`ev regroup` cannot be told "no".** It proposed moving the ULN2003 drivers into the motor
  box (a stepper beside its driver); the person declined because the motor box is already full.
  Nothing records that answer, so every later regroup of the drawer proposes the same move
  again, and the only trace is prose in the item's note, which regroup does not read. Expected:
  a declined proposal is kept (`ev regroup --decline <item> [--why]`, or a `keep` mark on the
  item) and left out of later runs until something about it changes.

- **A word that is a root of its own is cut to another root.** `altın` → `alt` (meets
  `altında`), `ünite` → `uni`: the shortest shorter form written somewhere wins, and nothing in
  the inventory's own words tells a root from a suffixed form of another. Expected: a root stays
  whole. Left from the "one noun gets two keys" entry, whose ending cases (`bağı`/`bağları`,
  `bacaklarında`, `bloğu`) are fixed for the next release; this part needs a dictionary and
  waits for Çözgü's embeddable core. Measure with `tools/measure/stems.py`.
