# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **`ev ui` shows half blocks in a kitty terminal when the status report answers first.**
  Measured behind a multiplexer, both locally and over SSH, the terminal answered `CSI 16t`
  and `CSI 5n` at once and the kitty and DA1 answers 1-20 ms later. `on_graphics` ends the
  probe at the status report, so it decides before kitty is known and keeps half blocks for
  the whole session. The window's pixel size is 0 there as well, so a `TIOCGWINSZ` fallback
  would not help. Nothing on screen says which protocol was chosen or why. Also: the parser
  drops an unfinished answer after `ESC_WAIT` (30 ms), and `TERM_PROGRAM` does not cross SSH.
  Expected: kitty in such a terminal, the choice and its reason visible (Settings, a probe
  command), and an override (`EV_CELL_SIZE`, a forced protocol).

- **A root the inventory never inflects is still cut to a shorter written word.** `kapı` →
  `kap`, `veri` → `ver`, `mini` → `min`, `yani` → `yan`, `boya` → `boy`, `powerline` →
  `power`: the word reads as that word plus a valid ending (`kap`+`ı`), and no other written
  form of it shows it is a root. The cases the inventory does show (`altın`, `ünite`, `pense`,
  `cıvata`) and those an ending cannot follow (`varta`, `sabun`, `güneş`, `türkiye`) were fixed
  in v0.18.0. Expected: a root stays whole. The rest needs a dictionary and waits for
  Çözgü's embeddable core. Measure with `tools/measure/stems.py` (1,736/1,919 after the fix).
