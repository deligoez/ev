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

## A short model is not found inside another code

A thing's model or serial was looked for anywhere in a purchase line's name, so a Dremel bit
recorded as model `561` was offered a hard disk whose product code is `6002561`, at the top
with 60 points. A model or serial now counts only as whole words of the line (`GSB 13 RE` still
reads as `GSB13RE`); one of six characters or more may still start a longer code, where a
suffix names a colour or a region.

## A kit bought as one purchase line

A set of 128 bits bought as one line was recorded as a kit, its parts linked to the records,
but the records had no purchase: linking each part to the line meant nothing, and every part was
offered unrelated lines while the set's own line stayed open. Now (spec/kit-purchase.md)
`ev kit purchase <kit> <line>` (or `--purchase` on `ev kit add`) links the kit to the line: the
line is settled and named by the kit in `ev buy list` and `ev buy show`, every linked part shows
it under its purchases in `ev show`, and none of them is asked for a purchase again. `--clear`
takes it back. **Schema 31**: this version adds `kits.purchase_id` when it first opens an
inventory of 30.

## A crop with a margin

Crops cut by an estimate cut off the edge of the part again and again: the rim of a disc, the
shank of a bit. `--pad 0.1` on `ev photo cut` (and `ev photo add --crop`) grows every crop named
by hand on each side by a tenth of its own size, inside the photo; the crops of a grid already
have a margin. `ev photo add`'s help now points to `ev photo cut … --preview` for a crop to be
checked first.

## A move to where it already is is refused

`ev move <x> --to <its own box> --plan` made a pending move to the same box, which waited forever
(it was meant as a move between two compartments of the box). A move to where a thing already
is, planned or not, is now refused, and the message points to a grid: a box with fixed
compartments takes `ev grid <box> --cols 2 --rows 2` and `ev cell "<thing>"=A1`, which already
worked for any holder. A lost thing moved to where it was last seen is still found there.

## Things share a compartment

A case of four compartments is a 2×2 grid, but `ev cell` refused two things in one
compartment ("A1 and A1 would share cells"): a cell was a gridfinity box's, one record each.
Two kinds of cutting disc lie together in one compartment of a Dremel case. Now things (items)
share a cell with other things; a box (anything but an item) still keeps its cells to itself,
and nothing shares a cell with a box. `ev grid <case>` lists what is in each compartment.
