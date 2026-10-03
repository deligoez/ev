# Turning a photo that was taken sideways

## Decision

A photo is turned for good, on the record, not only on the screen.

1. **`--rotate 90|180|270` on `ev photo add` and `ev photo cut`** (and `cut --preview`): the
   photo is turned clockwise before anything else, and is stored turned. Every coordinate given
   with it (`--crop`, `<ref>=x,y,w,h`, `--grid`) is a fraction of the turned photo — the photo as
   it should have been.
2. **`ev photo rotate <ref> <n> 90|180|270`** turns a photo already attached, and everything cut
   from it: the photo is stored again turned; every record that has it whole gets the turned one
   (with its grid corners turned); every crop cut from it — on this record or any other — gets
   its rectangle turned with it and is cut again from the turned photo, so it shows the same part
   upright. When `<n>` is a crop, its source photo is turned, with all its crops: a crop is
   sideways because the photo is. Files nothing uses any more are deleted.

The EXIF orientation of a phone photo was already applied on every read (`open_upright`); what
this adds is the photo with no orientation tag, or a wrong one, which only a look reveals.

## Why

A table photo sent from a phone came in on its side, with no orientation tag. The agent attached
it as it was, and every crop came out sideways. `r`/`R` in `ev ui` turn it on the screen for the
session only, nothing on the record. Fixing it took a turned copy made by hand, removing the
crops, turning each rectangle's coordinates by hand and cutting again — and the first try turned
the wrong way and cut every crop upside down. Turning is arithmetic ev can do once and right.

## Model

No schema change. `photos.path` and `photos.source` point at the turned file; `crop` and `grid`
hold the turned coordinates. A clockwise quarter turn maps a point `(x, y)` to `(1 - y, x)` and a
rectangle `(x, y, w, h)` to `(1 - y - h, x, h, w)`; 180 and 270 are two and three of those. The
grid's corners keep their order (back-left stays back-left: it names the furniture's corner, not
the picture's). Each record whose photo turned gets a `photo_rotate` event `{from, to, degrees}`.

## Not now

- Turning by a free angle (a slightly tilted photo): the crops are rectangles of the picture.
- Guessing that a photo is sideways: noticing it is the agent's look, not a formula.
