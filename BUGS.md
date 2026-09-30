# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **A grid cut's crops cannot be checked at a glance.** The skill says to look at every crop,
  but a drawer cut makes 36; checking them meant listing each record's photos, resizing each
  file outside `ev` and opening them one by one, so 2–3 were looked at. Expected: `ev photo cut`
  (and `--preview`) write one contact sheet, every crop small and labelled with its cell, and
  return its path.
- **Readable output cannot be asked for through a pipe.** `ev` prints its compact text only on
  a terminal; an agent (always piped) gets full JSON with path arrays and trims it with a script
  just to read it, or fakes a terminal (`script -q /dev/null`). Expected: `--text`, the
  counterpart of `--json`.
- **No bulk edit.** Setting `size` on 21 boxes took 21 `ev edit` calls from a script: not all or
  nothing, and the decision (read from each name) lived outside `ev`. Expected: `ev edit
  --stdin` taking NDJSON lines `{"ref": …, "set": {…}}`, all or nothing like `ev add --stdin`.
- **A box's name and its `size` drift apart.** 21 boxes in 07-A had `1x1x1` in their names and no
  `size`, so the taller-box crop margin and `regroup`'s bigger-spare offers never saw them.
  Expected: `ev audit` lists holders whose name carries a size that the field lacks or
  contradicts.

- **A word that is a root of its own is cut to another root.** `altın` → `alt` (meets
  `altında`), `ünite` → `uni`: the shortest shorter form written somewhere wins, and nothing in
  the inventory's own words tells a root from a suffixed form of another. Expected: a root stays
  whole. Left from the "one noun gets two keys" entry, whose ending cases (`bağı`/`bağları`,
  `bacaklarında`, `bloğu`) are fixed for the next release; this part needs a dictionary and
  waits for Çözgü's embeddable core. Measure with `tools/measure/stems.py`.
