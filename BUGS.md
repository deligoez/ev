# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **A root the inventory never inflects is still cut to a shorter written word.** `kapı` →
  `kap`, `veri` → `ver`, `mini` → `min`, `yani` → `yan`, `boya` → `boy`, `powerline` →
  `power`: the word reads as that word plus a valid ending (`kap`+`ı`), and no other written
  form of it shows it is a root. The cases the inventory does show (`altın`, `ünite`, `pense`,
  `cıvata`) and those an ending cannot follow (`varta`, `sabun`, `güneş`, `türkiye`) were fixed
  in v0.18.0. Expected: a root stays whole. The rest needs a dictionary and waits for
  Çözgü's embeddable core. Measure with `tools/measure/stems.py` (1,736/1,919 after the fix).
- **Cell frames drift on tall boxes.** On a drawer photo with its grid corners kept, the frames
  drawn for 1x2 boxes sat a little low on the top edge (perspective). Minor. (Reported by the
  inventory agent.)
- **`ev history` as text prints each event's raw JSON.** `ev history #12 --text` lists
  `create   {"code":null,"kind":"item",…}`, `photo {"crop":null,"path":…}`, oldest first, while
  the History tab of `ev ui` says every event in words, newest first by day; README and the skill
  say the two print the same. Expected: the text output in the tab's words. (QA round.)
- **A usage error lists `--db <DB>` as if it were required when `EV_DB` is set.** `EV_DB=… ev
  review x` answers `Usage: ev review --as <STATUS> --db <DB> <REFERENCE>`; without `EV_DB` the
  line has no `--db`. An agent reading it may think `--db` is needed. Expected: the same usage
  line either way. Minor. (QA round.)
