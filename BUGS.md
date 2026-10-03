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
- **`ev photo mark` draws a label in capitals.** `G1x1-001=…` comes out as `G1X1-001`; the person
  copies a code by hand from the picture, so the text drawn must be the text given. (Reported by
  the inventory agent.)
- **A label drawn on a cell covers the cell, and its size cannot be chosen.** `<label>=C2` (or
  `=A1-A2`) draws the text across the whole cell, so the box under it is hidden; a 3-character
  label comes out bigger than a 5-character one. The agent worked out the cell corners by hand
  to draw small frames in each box's corner, and the text still spilled above the frame.
  Expected: the label in the box's corner, scaled to the box (or a `--label-size small|auto`),
  in cell mode too. (Reported by the inventory agent.)
- **Cell frames drift on tall boxes.** On a drawer photo with its grid corners kept, the frames
  drawn for 1x2 boxes sat a little low on the top edge (perspective). Minor. (Reported by the
  inventory agent.)
- **Showing each box's code on the drawer photo takes a list typed by hand.** While labelling a
  gridded drawer, the agent draws every box's code in its cell (`ev photo mark <drawer>
  <code>=<cell>…`) so the person reads which label goes where; it builds that list itself.
  Expected: one command, e.g. `ev photo mark <drawer> --codes` (or `ev label --show <drawer>`),
  drawing the codes still to stick in their cells. (Reported by the inventory agent.)
