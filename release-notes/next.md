Draft for the next release.

## New

- **The back-fill asks only about toured places** (`ev buy for --toured`). Every thing in a
  toured place that no purchase is linked to yet, with the one open line that could be it,
  best first and numbered so the person can answer by number: the thing with its path, then the
  line and the reasons. Only lines above the bar `ev add` offers at (15 points) are listed. A
  toured drawer covers its boxes unless a box has a review of its own. The back-fill and the end
  of each tour now ask the same question, instead of an agent looping over every record (spec
  purchases §12.2). Ranking a line against many things now reads each line once; on a real
  inventory of about 1,000 lines the whole list takes a couple of seconds.

- **A Documents tab in `ev ui`.** A thing's invoices, manuals and policies — its own and those
  of its purchases — and its links, with its purchases' order and product pages. `[` `]` pick
  one, `O` or a click opens it in the program the system gives it, `y` copies its path or
  address.

- **"Not this one" for a purchase candidate** (`ev buy decline <line> <ref> [--why …]`). When the
  person says a line is not the thing in hand, the line stays open for other things and is no
  longer offered to that one; `--clear` takes it back. `ev buy show` lists them under "not".

## Upgrade notes

- The database moves to schema 26 (declined purchase candidates) the first time this version
  opens it; an older `ev` then refuses it.

## Changed

- **The details Summary reads in sections, not as one long list.** After purchases, documents,
  coverage and values were added, a single item's summary had become a column of label–value
  lines where a shop's three-line English title outweighed the price, three tasks of the
  drawers above repeated on every thing, and wrapped lines lost their column. Now: its state as
  badges first (kind, count, set aside, lost, broken, on sale, label to print), its identity,
  then Money (each purchase in one line — date, shop, quantity, price, today's money — with the
  shop's title dimmed under it, and the current value), Coverage (status in colour), Documents
  and links (counted), To do (its own tasks; those of the places above only counted), Note.
  Empty fields are not drawn; `E` shows the make, model and serial still to fill. Long values
  wrap under their own column. Modelled on the detail views of gitui, Snipe-IT, Under My Roof
  and Homebox.
- **Only the tabs a thing has something for are shown**, instead of dimmed ones in the way.
- **Above every tab, the name, then the place it is in**, instead of one long path that wrapped
  on the Summary and was cut on the others.
- **Money reads the reader's way in Turkish:** `1.999,50 TL`, also in the terminal output.
- **`+` widens the details** as far as the list allows, and back.
## Fixed

- **Purchase candidates need more than words.** In the toured back-fill, 24 of 30 offers were
  wrong: lines that shared only what a thing is for or kept with ("USB girişli, korumalı" for a
  charging module, "banyodaki dolabı için" for screw-cover stickers), a code mentioned in a note
  (every module noted "for an ESP32" was offered an ESP32 kit), or only a brand (a Pro'sKit
  pliers line for a Pro'sKit wire stripper). Words alone now offer a line only when it carries
  most of what the thing is (the head of its name, weighted by rarity); codes are read from the
  name, make, model and serial, not the note; a brand counts in full only with a word of what the
  thing is; and amperes are compared (`2,5 A` against `3A`), without reading `Pi 3 A+` as one.
  On a copy of the real inventory the list went from 30 offers (6 right) to 10 (7 right, 1
  unsure, 2 wrong), every known right match still first, and one more right match found.
- **The History tab says every event in words.** Linking a purchase, adding or removing a
  document or a coverage, a "do not track" decision, lending, breaking and fixing, and a plan
  import showed as `event purchase_linked {"purchase":636,"qty":2}`; each now reads like the
  others ("linked to purchase  #636 ×2").
- **A statutory warranty is no longer proposed after it has ended, or for a purchase from
  abroad.** `ev show` proposed a two-year statutory warranty for a module bought on AliExpress
  three years earlier. A proposal now needs a line sold at home, paid in the home currency and
  not from a marketplace abroad (AliExpress, Temu, Banggood, Amazon.com, .co.uk, .de, .fr, .it,
  .es, each with its country: a home in Germany counts Amazon.de as home), and its two years,
  plus any time in repair, must not have passed.
