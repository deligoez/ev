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
