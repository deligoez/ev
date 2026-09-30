Draft for the next release.

Records that stay true while things are unpacked: one record splits into a record per part
with its history linked both ways, a bought kit becomes a checklist counted from the records,
a detached photo stays in the history, and a tall box gets a crop that reaches its rim.

## Upgrading

The database moves from schema 11 to **schema 13** (12: kits, 13: parking places) the first
time this version opens it; v0.14.0 then refuses it with exit 6. Copy `~/.ev/ev.db` aside first if you might go back.

## New

- **`ev split <ref> <name>=<qty>… [--rename] [--qty]`.** One record becomes a record per kind
  of thing: a soil-moisture set recorded as one item becomes its probes, boards and cables;
  one record of straight and angled headers becomes two. Each new record lies beside the
  original with its kind and tags; the original keeps what is left, renamed and recounted in
  the same step. The history links both ways (`split`, `split_from`), shown in the History
  tab. The place's photo stays current: the same things lie there, only recorded apart. The
  result lists the original's photos, to crop each part from.
- **Kits: `ev kit add|part|link|unlink|show|list|remove`.** A bought set as a checklist: its
  name, how many copies were bought, and its parts with how many come in one copy. Records
  are linked to a part as they turn up, and `ev kit show` counts each part found, lost and
  still missing from the records themselves — find a lost card or move a board and the kit
  follows. A link is in the record's history and in its `ev show` (`kits`).

- **A contact sheet of every crop.** `ev photo cut` and its `--preview` return `sheet`: one
  image with every crop small, six to a row, labelled with its box's cell. A drawer cut makes
  three dozen crops; checking them used to mean opening each one, so few were checked.
- **`ev edit --stdin`: many records at once, all or nothing.** NDJSON lines
  `{"ref": "#551", "set": {"size": "1x2x1.5", "tags": ["+modül"], "note": null}}`; a failing
  line is named and nothing changes. A field set twice in one line is one change in the
  history (before the first, after the last).
- **`--text`**, the counterpart of `--json`: the readable output through a pipe, for an agent
  reading a result rather than parsing it.
- **`ev audit` finds boxes whose name says a size their `size` field does not**
  (`size_drift`). The field is what crops and bigger-box offers read; 21 boxes of the author's
  drawer had their size only in their names.

- **Serial codes for movable boxes: `code=GF1x1-*`.** A code ending in `*` takes the next free
  number of its series in `ev add`, `ev edit` and `ev edit --stdin`, padded like the series and
  never reusing a gone box's number. Movable boxes get serial codes that stay on their labels
  wherever they go (`GF1x1-012`, `S5-01`); furniture slots keep positional ones (`K4x4-07-A`).

- **Parking places: `temporary`.** Mark a place `temporary` (`ev add --temporary`, `ev edit X
  temporary=true`) when things are put there only until their places are decided, or mark one
  thing that waits among things that do belong where it is. `ev suggest` never offers a parking
  place or anything inside it (they are listed apart under `parking`), `ev todo` lists what waits
  (`parked`), and a move clears a thing's own mark. Schema 13.
- **`ev suggest` says whether a place has been gone through.** Each offered place carries its
  review, its own or the nearest reviewed ancestor's; the text marks an untoured one
  `(not toured)`, a guess to check before it is proposed.

## Changed

- **A taller box gets a wider crop.** A grid cut widens each box's crop with its height from
  its `size` (`1x2x1.5` gets 1.5 times the margin), never below the plain margin: a tall box's
  rim leans out of its cells in a photo taken from above, and its crop was cut short.
- **The skill** records parts in the bag they came out of and plans their move (the bag's
  history then lists what came out of it, and an unclear "I put them there" stays a planned
  move until confirmed), uses `ev split` and `ev kit`, and gives a record several crops in one
  `ev photo cut` (which already accepted the same reference several times, now documented and
  tested).

## Fixed

- **`ev regroup` offered a full drawer of boxes a spare box, even one standing inside it.** A
  holder without a size took every spare as bigger. A holder laid out in a grid is no longer
  offered one, and no holder is offered a spare that stands inside it.
- **A tall box's crop now reaches out where its rim leans**, up at the back and down at the
  front of a photo taken from above, instead of growing evenly on every side.

- **An uncounted place dropped off `ev todo` once a box was put in it.** The list of places
  whose contents were never counted came from the innermost places only, so a desk marked
  `unknown` left the list the moment a labelled box stood on it, though its own contents were
  still uncounted. Every node marked `unknown` is now listed until it is set back.

- **`ev photo remove` left no trace in history.** Detaching a photo now records a
  `photo_remove` event with its path, crop, note and position, shown in the History tab.
