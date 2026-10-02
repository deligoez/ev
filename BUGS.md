# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **A root the inventory never inflects is still cut to a shorter written word.** `kapı` →
  `kap`, `veri` → `ver`, `mini` → `min`, `yani` → `yan`, `boya` → `boy`, `powerline` →
  `power`: the word reads as that word plus a valid ending (`kap`+`ı`), and no other written
  form of it shows it is a root. The cases the inventory does show (`altın`, `ünite`, `pense`,
  `cıvata`) and those an ending cannot follow (`varta`, `sabun`, `güneş`, `türkiye`) are fixed
  for the next release. Expected: a root stays whole. The rest needs a dictionary and waits for
  Çözgü's embeddable core. Measure with `tools/measure/stems.py` (1,736/1,919 after the fix).
- **A pack or set bought as one line can be linked to only one record.** A line of quantity 1
  that is an 8-pack of AA cells, or a charger set with four AA and four AAA cells, holds
  several units that end up in several records (cells kept in two boxes, the AA and the AAA
  cells as separate kinds). `ev buy link <line> <second record>` fails with `purchase N has
  nothing left to link` after the first link, so the second record gets the purchase only as
  prose in its note. Expected: a line can say how many units it carries (a pack size), or a
  link can split a line across records, so every record the pack went into reaches the line.
