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
- **`ev layout --propose` drafts from the last word alone, and it shows.** On a toured 32-drawer
  Kallax with 400 records it proposes 153 moves and a theme for every drawer. Taken one by one:
  kinds are raw head words in their possessive form and unstemmed (`ucu` and `uçları` are two
  kinds; `adaptörü`, `kartı`, `kapağı`, `gövdesi`, `cihazı`, `seti` become themes), so a camera
  body goes with a furniture lock nut and a cutter body (`gövdesi`), lens caps with pen caps
  (`kapağı`), sensor boards and an RFID card with dev boards and a handheld whose name ends
  in "info card" (`kartı`), memory-card adapters with power adapters, and a device is split
  from its own test leads and adapter board. Things inside labelled, themed gridfinity bins are
  moved out one by one, as if the bins were not there. A parking drawer (`temporary`) gets a
  theme and things moved in. In `ev layout`, a colour (`bordo`) is the only shared word behind
  a merge proposal. Expected: the kind is the noun phrase (stemmed, a compound like `lens
  kapağı` kept whole), a box with a theme moves as a unit, records of one kit or one set stay
  together, parking places are left out, and colour words do not count. As it stands the draft
  cannot go to the person: every third line would need explaining away. (QA round.)
- **Marking a series photo adds a second copy of it.** `ev focus --file a.jpg=…` (f1), then
  `ev photo mark f1 1=… 2=…` adds f4, the marked copy; f1 stays unmarked next to it, so the
  person steps through the same photo twice. (`photo cut … --preview` then the real cut
  replaces the preview picture, which is the expected feel.) Expected: marking a picture already
  in the series marks it in place, or says that a copy was added. Minor. (QA round.)
