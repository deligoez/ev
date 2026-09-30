# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **No way to take the next free code of a series.** Movable boxes get serial codes (`S5-01`,
  `GF-001`) that stay when they move; a new box needs the next free number of its series, which
  today means listing the codes and counting by hand (or with a script). Expected: `code=GF-*`
  in `ev add`, `ev edit` and `ev edit --stdin` takes the next free number, padded like the
  series' existing codes. Related: `ev cell --recode` names boxes after their cells, which the
  serial-code decision (task #25) retires for boxes.

- **A word that is a root of its own is cut to another root.** `altın` → `alt` (meets
  `altında`), `ünite` → `uni`: the shortest shorter form written somewhere wins, and nothing in
  the inventory's own words tells a root from a suffixed form of another. Expected: a root stays
  whole. Left from the "one noun gets two keys" entry, whose ending cases (`bağı`/`bağları`,
  `bacaklarında`, `bloğu`) are fixed for the next release; this part needs a dictionary and
  waits for Çözgü's embeddable core. Measure with `tools/measure/stems.py`.
