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
- **Some shops' saved pictures match no line.** `tools/purchases/images.py` matched none of the
  files in three shops' `raw/images` (decathlon 93 files, robo90 14, sahibinden 4): their names
  carry another id than the line's `sku` or `key`. Three shops have no `raw/images` at all
  (amazon-de, robotistan, idefix). One shop had 629 files and 489 attached; the rest may be
  cancelled or consumable lines, unchecked. Expected: each adapter names its pictures by an id
  its lines carry (or emits the `image` lines itself), and the tool says which files it matched
  to nothing. (Reported by the inventory agent.)
- **Bringing a line's product pictures takes a loop.** The person's rule is that after every
  `ev buy link` the product pictures come along without asking; today that is `ev buy bring
  <line> <ref> --only <id>…` per line, scripted around ev, and `--only` is a repeated flag a
  shell loop splits badly. Expected: a way to bring the pictures with the link (a flag on `ev
  buy link`, or a setting), a back-fill (`ev buy bring --all --type image`), and `--only`
  taking a comma-separated list. Propose first. (Reported by the inventory agent.)
- **`ev buy bring` to a thing that is gone brings nothing and says nothing.** On a thing gone
  `--as used`, it brought 0 documents with no error. Not bringing may be right; silence is not.
  Expected: a warning, or a refusal that says why. (Reported by the inventory agent.)
