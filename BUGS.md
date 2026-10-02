# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **`ev photo mark` draws a long label so large it covers the frames.** Labels like
  `"1 LR44 Mettzchrom x17 (#484)"` on a phone photo came out as full-width red bars over the
  parts and over each other (2026-10-02); only bare numbers were readable. Expected: a label
  sized to its frame (or to a fraction of the photo), kept inside the photo, and moved off
  another label it would cover — or a legend beside the photo for anything longer than a number.
- **A root the inventory never inflects is still cut to a shorter written word.** `kapı` →
  `kap`, `veri` → `ver`, `mini` → `min`, `yani` → `yan`, `boya` → `boy`, `powerline` →
  `power`: the word reads as that word plus a valid ending (`kap`+`ı`), and no other written
  form of it shows it is a root. The cases the inventory does show (`altın`, `ünite`, `pense`,
  `cıvata`) and those an ending cannot follow (`varta`, `sabun`, `güneş`, `türkiye`) are fixed
  for the next release. Expected: a root stays whole. The rest needs a dictionary and waits for
  Çözgü's embeddable core. Measure with `tools/measure/stems.py` (1,736/1,919 after the fix).
