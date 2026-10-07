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
- **A purchase entered by hand cannot take a remembered month, nor be corrected later.**
  `ev buy add --date` takes only `YYYY-MM-DD`, so "bought in 2018-03" leaves the line undated
  (the month went into the shop text), while `ev add --came` and `--at` take `2018-03`. And a
  line entered by hand has no edit: a date, a price or a name said wrong stays wrong. Expected:
  `--date` takes a partial date as `--came` does, and `ev buy edit <line> field=…` for the
  person's own lines (`source: manual`). (Reported by the inventory agent.)
