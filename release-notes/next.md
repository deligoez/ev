Draft for the next release.

Records that stay true while things are unpacked: one record splits into a record per part
with its history linked both ways, a bought kit becomes a checklist counted from the records,
a detached photo stays in the history, and a tall box gets a crop that reaches its rim.

## Upgrading

The database moves from schema 11 to **schema 12** (kits) the first time this version opens
it; v0.14.0 then refuses it with exit 6. Copy `~/.ev/ev.db` aside first if you might go back.

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

- **`ev photo remove` left no trace in history.** Detaching a photo now records a
  `photo_remove` event with its path, crop, note and position, shown in the History tab.
