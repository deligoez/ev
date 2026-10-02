Draft for the next release.

## New

- **A cut shows what it recognised** (`ev photo cut … --show`). Every cut and `--preview` now
  also draws `marked`: the whole photo with a numbered red frame on each crop, numbered from 1
  in the order the crops were given and then the grid's boxes, with bare numbers as labels. Its
  `legend` (`[{n, ref, crop}]`) says which record each number is, and the text output lists it
  (`1  #484 …`). `--show` sends the numbered photo to a running `ev ui` as `ev focus --file`
  does, titled with `--note` or else with each number and its code or name. So an agent that
  cuts a photo among records shows the person what it read in the same command, instead of
  having to remember a separate `ev photo mark` and `ev focus`. Sending stays opt-in: a cut can
  come from a script, and a request replaces the picture the person may be looking at.
- **A pack or set bought as one line can be linked to every record it went into**
  (`ev buy pack <id> <n>`, `ev buy add --pack n`). A line now has a pack size, the units in
  each bought quantity: an 8-pack of cells, a charger set with four AA and four AAA cells. Its
  `units` (qty × pack) are what links take, so `ev buy link <id> <ref> --qty n` spreads one
  pack over the boxes and kinds its units were split into, and each thing's share of the price
  is by units. Before, a line of one pack was used up by its first link and the second record
  got the purchase only as prose in its note. Schema 27 adds the column; every existing line
  keeps a pack of 1.
- **Photographed, then thrown out** (`ev dispose <x> --as digitize`). A ticket, a letter or an
  old statement is often worth keeping only as a picture. It now has its own disposition. It
  waits in its pile while a drawer is sorted, gets its copy later in one sitting (a photo or a
  crop when the paper is the thing; a document linked to the paper and to the thing it is about,
  when it is an invoice or a warranty card), and only then leaves: `ev gone` refuses a digitized
  record that has no photo and no document, and also checks one that sits in a box leaving with
  it. When every copy is an image under 800 px on its short side, it still leaves but warns
  (`warnings`), since a crop from a whole-table photo may not read. A digitized record stays in
  `ev find` without `--include-gone`, marked `(gone, copy kept)`, because finding it later is the
  point of the copy. Documents get the `scan` kind for a copy that is none of the others.
- **Shredded rather than thrown out whole** (`--shred` on `ev dispose` and `ev gone`, for trash
  and digitize). An old ID card, a boarding pass or a statement carries a name, a number or a
  barcode. The mark shows in `ev disposals` and `ev todo` as `(shred)`, and `ev restore` takes it
  back. Neither feature changes the schema: the disposition is a value, the shred a mark.

## Fixed

- **`ev photo mark` drew a long label so large it covered the frames.** Labels like
  `"1 LR44 Mettzchrom x17 (#484)"` on a phone photo came out as full-width red bars over the
  parts and over each other. A label is now no wider than its frame (or an eighth of the photo,
  so a number on a small frame stays legible): a long one is drawn smaller, down to a third of
  the photo's size, and broken onto up to three lines at spaces. It goes above its frame, else
  below it, else inside it, at the first place that stays in the photo and off every other label
  and frame, and all frames are drawn before any label, so no frame line crosses a label.
- **`ev edit <x> note=+text` replaced the whole note with `+text`.** `tags=+x` and
  `photos=+p` add, so `note=+…` read as "append", but the note took the value literally and
  the old one survived only in the history. `note=+text` now appends on a new line, since the
  note is a log the person adds to; `note=text` still replaces it.
