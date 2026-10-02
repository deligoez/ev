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
- **A second lot of the same kind of thing starts empty.** A few more of a thing already
  recorded (same make, model, tags, photos and a linked purchase) turned up in another place;
  `ev add` opened a separate record, rightly, but with every field to copy by hand, and its
  purchase candidates matched only by words, weaker than make and model would. Expected: a verb
  for "N more of this, there": `ev add --like <ref>` (name, make, model, tags, kind; perhaps the
  photo references), the reverse of `ev split`. Propose first. (Reported by the inventory
  agent.)
- **A purchase's date does not say which date it is.** `ev buy show --text`, `ev buy for` and
  the `bought:` line of `ev show` print one bare date, the delivery (`delivered_at`); the order
  date (`ordered_at`) is only in the JSON. An agent told the person a thing was bought on the
  delivery day, and the person, looking at the shop's order page, saw another date. Expected:
  the text labels the date it shows, or shows both (`ordered … · delivered …`). (Reported by the
  inventory agent.)
- **A shop's product image is not imported.** A shop's raw order data carries the product
  image (a URL with a size placeholder), but the imported line has no attachment, so `ev buy
  link` cannot offer to bring it. Expected: an adapter emits the product image as an attachment
  and the bring offer includes it; check every adapter, not only the one it was seen in.
  (Reported by the inventory agent.)
