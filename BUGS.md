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
- **Recording a thing's make and model, or finding it, does not offer its purchases.** `ev add`
  shows "Could be one of these purchases"; a lost thing found again (`ev found`) and then given
  `make=` / `model=` with `ev edit` showed none, though `ev buy for` on it ranked two strong
  candidates first (a model match). Expected: when a record gains make or model, or comes back
  from lost, the best purchase candidates are offered as `ev add` offers them. (Reported by the
  inventory agent.)
- **A second lot of the same kind of thing starts empty.** A few more of a thing already
  recorded (same make, model, tags, photos and a linked purchase) turned up in another place;
  `ev add` opened a separate record, rightly, but with every field to copy by hand, and its
  purchase candidates matched only by words, weaker than make and model would. Expected: a verb
  for "N more of this, there": `ev add --like <ref>` (name, make, model, tags, kind; perhaps the
  photo references), the reverse of `ev split`. Propose first. (Reported by the inventory
  agent.)
