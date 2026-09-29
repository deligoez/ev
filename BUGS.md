# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **A word that is a root of its own is cut to another root.** `altın` → `alt` (meets
  `altında`), `ünite` → `uni`: the shortest shorter form written somewhere wins, and nothing in
  the inventory's own words tells a root from a suffixed form of another. Expected: a root stays
  whole. Left from the "one noun gets two keys" entry, whose ending cases (`bağı`/`bağları`,
  `bacaklarında`, `bloğu`) are fixed for the next release; this part needs a dictionary and
  waits for Çözgü's embeddable core. Measure with `tools/measure/stems.py`.
