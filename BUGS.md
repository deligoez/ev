# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **`ev ui` over SSH falls back to half blocks without saying so.** A terminal that speaks
  the kitty protocol locally shows pictures as half blocks when `ev ui` runs on the far end of
  `ssh -t`. `ImageProbe::picker()` uses half blocks whenever no `CSI 16t` cell size arrived,
  whatever protocol was detected, and unlike ratatui-image's own query it never falls back to
  the pixel size in `TIOCGWINSZ`. The parser also drops an unfinished answer after `ESC_WAIT`
  (30 ms), so an answer split across packets is lost, and if that was the `CSI 5n` report the
  probe never ends. `TERM_PROGRAM` does not cross SSH, so the iTerm2 branch never runs there.
  Nothing shows which protocol, cell size or answers were used. Expected: the choice is
  visible (Settings or a doctor command), a missing cell size uses the window's pixel size,
  then an override (`EV_CELL_SIZE`, a forced protocol), before half blocks.

- **A root the inventory never inflects is still cut to a shorter written word.** `kapı` →
  `kap`, `veri` → `ver`, `mini` → `min`, `yani` → `yan`, `boya` → `boy`, `powerline` →
  `power`: the word reads as that word plus a valid ending (`kap`+`ı`), and no other written
  form of it shows it is a root. The cases the inventory does show (`altın`, `ünite`, `pense`,
  `cıvata`) and those an ending cannot follow (`varta`, `sabun`, `güneş`, `türkiye`) were fixed
  in v0.18.0. Expected: a root stays whole. The rest needs a dictionary and waits for
  Çözgü's embeddable core. Measure with `tools/measure/stems.py` (1,736/1,919 after the fix).
