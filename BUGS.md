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
- **`ev edit <x> note=+text` silently replaces the note with `+text`.** `tags=+x` and
  `photos=+p` add, so `note=+…` reads as "append" to an agent, but `note` takes the value
  literally: the old note is gone (only the history keeps it) and the new one starts with a
  stray `+`. Five records lost their notes this way in one session before it was noticed.
  Expected: `note=+text` appends on a new line (the note is a log the person adds to), or a
  leading `+` is refused with a hint.
