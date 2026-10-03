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
- **"Changed since its tour" fires on bookkeeping, so it fires almost everywhere.** In one
  household, 47 of 49 toured places read as changed since their tour. What came after the
  tours was mostly not a change to what is in the place: re-coding a box (most of it), linking a
  document or a purchase, editing a note, a name or a theme, a photo. Only what moves things in
  or out (add, move, gone, split, join, done) changes what a tour counted. Expected: the signal
  follows contents only, so a place that reads as changed is worth going back to.
