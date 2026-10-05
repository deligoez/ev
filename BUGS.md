# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **A mark's `--show` title goes nowhere when it marks a series picture in place.** `ev photo
  mark f1 1=… --show "two joysticks"` answers `shown.note: "two joysticks"`, but `ev focus
  --list` (text and JSON) keeps only the sent note for f1, with no trace of the title. The
  sent note staying is right; the answer claiming another title is not. Expected: either the
  answer gives the note f1 is actually shown under, or the title is kept beside the note. Minor.
  (QA round 3.)

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
