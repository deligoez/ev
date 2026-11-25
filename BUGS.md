# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **`ev ui` has no way to force a picture protocol or a cell size.** A terminal that reports
  no cell size (`CSI 16 t`) and a window of 0×0 pixels gets half blocks however well it draws
  pictures, and the Settings row can only say why. Also: the parser drops an unfinished answer
  after `ESC_WAIT` (30 ms), so an answer split across packets with a longer gap is lost (20 ms
  measured over SSH), and `TERM_PROGRAM` does not cross SSH, so iTerm2 is not recognised there.
  Expected: an override (`EV_CELL_SIZE`, a forced protocol) and a longer wait for a sequence
  already started.

- **A root the inventory never inflects is still cut to a shorter written word.** `kapı` →
  `kap`, `veri` → `ver`, `mini` → `min`, `yani` → `yan`, `boya` → `boy`, `powerline` →
  `power`: the word reads as that word plus a valid ending (`kap`+`ı`), and no other written
  form of it shows it is a root. The cases the inventory does show (`altın`, `ünite`, `pense`,
  `cıvata`) and those an ending cannot follow (`varta`, `sabun`, `güneş`, `türkiye`) were fixed
  in v0.18.0. Expected: a root stays whole. The rest needs a dictionary and waits for
  Çözgü's embeddable core. Measure with `tools/measure/stems.py` (1,736/1,919 after the fix).
