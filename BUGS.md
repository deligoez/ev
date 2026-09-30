# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **`ev sketch --import` forgets how it was told to read the plan.** The author's plan needed
  five `--room` pairs (`Oda #3=Çalışma odası`, …) and one `--space Antre@1500,1000`; none of it
  is stored, so importing the edited plan again means typing them all again, and a run without
  them leaves those rooms as they were without saying why. Expected: each room keeps the plan
  name (or space point) it was sketched from, and the file it came from, so `ev sketch
  --import` alone repeats the last import; a flag given again replaces what was kept.

- **`--space` asks for a point in the plan's own centimetres.** Nobody drawing in Sweet Home 3D
  knows where x=1500, y=1000 is; the author's point was read off the plan's `Home.xml` by the
  agent. Expected: a way a person can give — a point next to a room they name (`--space
  "Antre@right of Kiler"`), or the dry run listing the closed spaces no plan room covers, each
  with its size and neighbours, to pick one by number.

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
