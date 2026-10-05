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
- **`ev empty` ignores what the records say against it.** `ev empty S-11 --note "opened, empty"`
  on a box whose note says "full of odds and ends, not empty" and which an open task (#29
  "count the contents of S-11") covers is accepted without a word; the task stays open and still
  asks to count an empty box. Also `ev tree --text` shows no `[boş]` for it (only `(sayıldı)`);
  the JSON has `empty: true`. Expected: mention the open task on the box (done or drop?) and show
  `[boş]` in the text tree. (QA round.)
- **A found container reads as counted empty.** `ev found <case> --in K2-15-U`, a carry case
  lost with contents never counted, then `ev show` says `boş (sayıldı)` because the drawer it
  turned up in was toured. Nobody looked inside the case. Expected: a box found in a toured
  place is not known empty until it is opened (`not_known`, like an untoured box). (QA round.)
- **JSON noise on the new reads.** `ev next --json`: every place under `task.places` carries
  about ten empty fields (`arriving: []`, `coverages: []`, `kits: []`, `links: []`, `marks: {}`,
  `needs: []`, `purchases: []`, `tracking`, `valuations`, `waited_for_by`, `while_there: {}`),
  plus `hints: []`, against "the JSON leaves out what has no value"; a nine-place task is 64 KB.
  `ev found --json` repeats the same list as `waited_for_by` and `waiting`. `ev focus --list`
  carries both `f: "f1"` and `n: 1`, and `file` equal to `source` on every unmarked picture.
  `ev focus --file a.jpg=… b.jpg=…` returns only `series: 3`, not the `f` numbers given.
  `ev photo add` with series photos returns `shown: null`. A frame's label is a number in
  `ev focus --list` (`"label": 1, "n": 2`) but a string holding the series number in
  `ev photo mark` (`marks[].label: "2"`), so the same key means two things in two types.
  Expected: empty fields left out, one of each duplicate, `focus --file` naming each new
  picture's `f`, and one name and type per meaning. (QA round.)
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
