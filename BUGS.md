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
- **A photo marked again hands a told number to another frame.** `ev photo mark f3 1=… 2=…`
  numbers its frames 1 and 2; marking the same photo again as `1=… 3=…` (frame 1 fixed, frame
  2 dropped, a new one added) numbers the new frame 2, and the picture with the old 2 leaves the
  series. The photo's own numbers go to its frames by position, not by label. Expected: a
  label keeps the number it had on that photo, a new label takes the next free number (3), and
  a dropped frame's number is not handed out again until the series is closed. (QA round.)
