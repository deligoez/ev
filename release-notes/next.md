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
