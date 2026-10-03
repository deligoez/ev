Draft for the next release.

## Bringing what came with a purchase says what it did

`ev buy bring` used to answer with the thing alone: when it brought nothing (everything already
brought, or an `--only` id the line does not carry) it said nothing either, and the agent had to
guess. It now answers with what it brought, by type (`brought_types`), and what it left and why
(`skipped`: already brought, and to which thing, or of a type not asked for); the text output
puts that above the thing. An `--only` id the line does not carry is refused instead of being
skipped.

Two additions make the product pictures easy to bring without scripting around ev:

- `--type image` (or `link`, `valuation`, `coverage`, comma-separated) brings one kind only, and
  `--only` takes a comma-separated list.
- `ev buy bring --all [--type image]` is the back-fill: every linked line's attachments not
  brought yet go to the thing it is linked to. A line linked to several things, or to a thing
  that is gone, is left and named, for the person to decide.

Pictures are never brought on their own: the agent brings them (on the person's word, or by a
rule the person set) and tells the person what it brought.

## Product images in ev ui's Photos tab

A thing's product images used to sit in the Documents tab next to the invoice, and opened only
in an outside viewer. They now show on the Photos tab, under their own heading after the
person's photos ("Photos (2)", "Product images (3)"), each with the purchase it came with, and
inside ev ui like a photo: the pane shows the selected one ("Product image 1/3"), `[` `]` step
through photos and product images alike, `o` shows it full screen and `O` still opens it
outside. The tab counts the two apart (`Photos 2+3`). Nothing about the person's own photos
changes: the current photo, the one that says how a place looks now, is still their newest, and
a product image never counts as one. The Documents tab and the summary leave product images out.

## Adapters live with the data

The shop adapters (`tools/purchases/`) left this repository. An adapter depends on how one
shop's pages look and how the agent saved them, so it changes with that shop, not with ev; it
now lives next to the raw export it reads (`~/.ev/purchases/<shop>/adapter.py`), kept by the
inventory agent, and emits the `image` lines for its pictures itself (matching saved file names
to lines afterwards failed for some shops). ev keeps the import contract (REFERENCE, "Import
lines") and gains `examples/purchases/`: a worked adapter over an invented shop export and a
README on writing one for a new shop. A test runs the example into `ev buy import`, so the
adapter an agent starts from always works. The fifteen adapters written so far stay in the
`v0.21.0` tag.

## Writes that changed nothing no longer change `ev.db`

The data repository commits `ev.db`, so a command that changes nothing in the inventory should
leave the file alone. Two did not, and each made a commit about nothing:

- `ev focus` kept its request for `ev ui` in the database's settings table. A request is a
  message to the UI, not a change to the inventory: it now goes to `ev.db-focus.json` beside the
  database (keep it out of version control, like `ev.db-wal`), and `ev ui` reads it on its
  half-second tick instead of waiting for a write.
- Importing the same purchase lines again raised the attachments table's AUTOINCREMENT counter
  by one per attachment, because an ignored `INSERT OR IGNORE` still advances it. An attachment
  is now looked up first; a re-import that changes nothing leaves the file byte for byte as it
  was.

## A used-up thing and its replacement are one thing

A consumable used up and replaced with the same one (a label maker's tape cassette, a battery)
was recorded as `ev gone <old> --as used`, and then `ev add --of <old>` refused, because the
used-up record was gone and often the thing's only record. The agent had to copy the fields by
hand into a new record, which then counted as a different thing. `ev add --of` now takes a gone
record as what the thing is, so the new units are a portion of it and the thing reads "here 1 ·
gone: used 1". `ev join` takes a gone record too, as long as one live record is among them, so
pairs already made apart can be joined.

## Empty boxes are worked out, not tagged

An empty box used to be one tagged `boş kap` by hand, and the tag went stale both ways: a box
recorded empty without it was missed, and a box that had something put in it kept the tag and
still looked empty (`ev regroup` even offered it as a spare). Empty is now worked out from the
records: a container no live record is in, and known to be empty — its place was toured, or
something was once recorded in it and left. A box with nothing recorded only because nobody
looked into it (an unopened carton, an uncounted drawer) is not empty: `ev find --empty` lists
it apart under `not_known`, and nothing offers it. A box that moves comes before a slot of
furniture. When the person opens a box and says it is empty, `ev empty <box>… --note "…"`
records that on their word, and the box counts as known to be empty from then on.

- `ev find --empty` lists them (also `empty` on the MCP `find` tool).
- When a thing has no group here (`new_group_likely`), `ev suggest` lists the empty boxes with
  no theme yet under `empty`, those in the thing's own room first (`--for`), so the agent does
  not have to remember a separate search to start a new group.
- `ev regroup` takes a spare box to be one with a size, no theme and nothing in it; the old tag
  still marks a spare that is not a container, but a box with something in it is never one.

The tag can be dropped from boxes that carry it; nothing reads it as "empty" any more.

## Sticking labels on a drawer's boxes

Labelling a gridded drawer is one question asked many times: which code goes on which box.

- `ev photo mark <drawer> --codes` draws each placed box's code on its own cells of the
  drawer's photo; before, the agent typed that `code=cell` list itself from the grid.
- A label on cells now sits in the cells' top-left corner, no taller than about a third of
  them and no wider, so the box under it stays visible. It used to be drawn at the photo's own
  scale in the middle of the cells, where even a three-character code covered the box.
- A label keeps its letters as given: the built-in font had capitals only and drew `G1x1-001`
  as `G1X1-001`, while the person copies the code from the picture by hand.
- `ev grid <drawer>` lists each box by its cells, its code and its name.

## Siblings in reading order

`ev tree`, `ev show` and `ev ui` listed what is in a place in the order it was recorded, so a
series of boxes was split whenever a piece of furniture was recorded between them
(`S5-01`…`S5-07`, the desk, then `S5-08`…). Siblings now come rooms, furniture, containers,
then things; within each, the coded ones by their code read naturally (`S5-2` before `S5-10`,
the `S5` series before `S45`), then the rest by name.

## An empty box off a box of several

Two battery cases recorded as one (`qty 2`) with the cells in them could not lose an emptied
case: `ev split` refused a holder with things in it, and `ev add --of` refused a container with
a message about serial numbers the case did not have. `ev split <cases> "<name>=1" --take` now
takes empty units off a box of several, the contents staying in the original, and a record
that cannot be kept in several places is refused for its real reason: it is not an item
(pointing to `split --take` for boxes), or it has a serial number.
