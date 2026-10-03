Draft for the next release.

## A photo taken on its side is turned for good

A table photo sent from a phone came in on its side with no orientation tag, and every crop cut
from it came out sideways. `ev ui` could turn it on the screen (`r`/`R`) for the session only;
fixing the record meant turning a copy by hand, removing the crops and turning each rectangle's
coordinates by hand — and getting the direction backwards cut them upside down. Now
(spec/rotate.md):

- `--rotate 90|180|270` (clockwise) on `ev photo add` and `ev photo cut` (with `--preview`) turns
  the photo first and stores it turned; every coordinate given with it is a fraction of the turned
  photo, the photo as it should have been.
- `ev photo rotate <ref> <n> <degrees>` turns a photo already attached, for good: every record
  holding it whole gets the turned one (a grid's corners turn with it and keep their names), and
  every crop cut from it, on any record, is turned and cut again, so it shows the same part
  upright. Turning a crop turns the photo it was cut from. Old files nothing uses are deleted, and
  each record's history gets a `photo_rotate` event.

A phone photo's EXIF orientation was already applied on every read; this is for the photo that
has none, or a wrong one.

## An edit says what it changed

`ev edit` answered with the whole record, so adding a line to a long note sent the note back
whole, and everything else the agent had just written. It now answers with the record as a row
and `changed`: each field it changed, before and after (`note: old → new` in the text);
`ev edit --stdin` gives each line's `changed` the same way, and `ev show` still has the rest.
A node's `lost` and `temporary` now appear only when true, as they already did on rows.

## A lost thing does not fill the box it was last seen in

A thing recorded as lost keeps the place it was last seen in, and `ev tree` already left it out
of that place; but `ev empty` refused a box whose only record was such a thing ("has 1
record(s) in it"), `ev find --empty` did not list it, and `ev review --as toured` asked for a
photo of what was in it. A lost thing now counts as in no place for all of them, as in the tree.

## An emptied place is photographed empty

`ev todo` already asked for a new photo of a place emptied after its photo was taken, but
`ev review --as toured` let any place with nothing in it through without one, so the record
kept a picture of things that had left. Decided with the person: an emptied place is
photographed empty. Touring now asks for that photo too; an empty place never photographed
still needs none.
