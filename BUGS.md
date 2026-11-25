# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **`ev show --text` prints both photo states after `ev photo stale`.** A place once confirmed
  with `ev photo current`, then marked with `ev photo stale --why …`, shows both "photo still
  current" and "photo stale: …". `ev todo --only photos` gets it right. `ev show`'s JSON has no
  top-level key for the photo's state, so an agent cannot read it without guessing where it
  lives. Expected: the later mark replaces the earlier one, and the state is readable in JSON.
  (Reported by the inventory agent on 0.32.0.)

- **Nothing lists what went to a place by the place.** Things owned by another household,
  recorded as returned with `ev gone <ref> --as return --at <date>`, leave `ev for <place>` with
  only its header. The question "what went back to them?" is answered only by name
  (`ev find <name> --include-gone`, then `ev history`); `ev past` lists them, `ev past --year`
  for that year reportedly does not. Expected: what was returned, given or lent to a place,
  with dates, from the place (for example `ev for <place> --history` or `ev past --to <place>`).
  (Reported by the inventory agent on 0.32.0.)

- **A root the inventory never inflects is still cut to a shorter written word.** `kapı` →
  `kap`, `veri` → `ver`, `mini` → `min`, `yani` → `yan`, `boya` → `boy`, `powerline` →
  `power`: the word reads as that word plus a valid ending (`kap`+`ı`), and no other written
  form of it shows it is a root. The cases the inventory does show (`altın`, `ünite`, `pense`,
  `cıvata`) and those an ending cannot follow (`varta`, `sabun`, `güneş`, `türkiye`) were fixed
  in v0.18.0. Expected: a root stays whole. The rest needs a dictionary and waits for
  Çözgü's embeddable core. Measure with `tools/measure/stems.py` (1,736/1,919 after the fix).
