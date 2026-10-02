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
- **A photo known to be out of date cannot be said so.** A drawer emptied before it was
  recorded keeps its old photo as current: nothing changed in the records after the photo, so
  `photos` in `ev todo` never lists it. The only trace was prose (an observation "the photo is
  old", a photo note "the drawer has changed since"), which no list reads. `ev photo current`
  says a photo still holds; nothing says the opposite. Expected: `ev photo stale <ref> [--why]`
  (or a photo note flag) puts the place on the photo list until a newer photo is attached.
