Draft for the next release.

## A drawer with labelled bins stays one place

`ev progress` lists the places a person opens one at a time, and a holder used to stop being one
as soon as anything in it carried a code. Labelling the bins in a drawer (a grid's
`B1_007`) therefore took the drawer off the list: each bin, and every unlabelled box beside
them, showed up as a place of its own, a toured drawer vanished from the list, and its bins
inflated the counts as toured places. In one household's inventory the list read 111 places and
56 counted where 73 and 18 were true.

A labelled child is now a place of its own only when its code goes on from its holder's
(`K2-01-A` in `K2-01`: a drawer of that unit), or when the holder has no code. A label of
another series is a box in the place and is counted with it. The colour and status `ev ui` shows
for a place follow the same rule.

## Counted needs no word in `ev ui`

Counted is where every place is headed, so the tree no longer writes `[sayıldı]` beside a counted
place: its green name says it, also while something in it still waits (a planned move is marked
beside it). Only the exceptions are written: not counted, being counted, left as is, and a new
one, **counted but changed since**, for a counted place whose contents changed after its tour. It
used to look counted and green while it held things nobody had counted. `ev tree` carries the
same `changed_since` on such a place.

A lost thing found somewhere else no longer counts as a change to the place it was last seen in.
Finding it records a move out of that place, and an emptied, counted drawer showed "counted but
changed since" for a thing it had not held since before it was counted. `ev progress` and the
photos `ev todo` asks for read changes the same way.

## An empty box says so

A box known to be empty looked the same in `ev ui` as one nobody had looked into: `ev find --empty`
knew it, the screen did not, and the person asked whether anyone knew a drawer was empty. Now a box
nothing is in, counted on its own or with its place, shows `[boş]` in the tree and a "boş" badge in
the details, with the day and the note when the person called it empty (`ev empty`). `ev show`
carries it as `empty` (`from: said` with `at` and `note`, or `from: tour`), and `ev tree` as
`empty: true`.

## Fewer photos asked for

`ev todo` asked for a photo of every holder with a grid, since the boxes in a drawer's grid are
cut from the drawer's photo. A Kallax compartment holding two drawers has a grid too, and each
drawer has a photo of its own, so every compartment was asked for as an empty frame nobody
photographs: in one household, 17 of 38 photo rows. A holder whose every part has a whole photo of
its own is no longer listed.

## Opening and closing the tree a stretch at a time

`→`/`←` opened and closed one level, so seeing everything in a drawer of boxes took a key per box
and tidying the tree up afterwards as many again. In the tree, `d` now opens the selected node
two levels down (a Kallax shows its compartments and the drawers in each), `e` opens it
with everything below it and `c` closes it all, the selection staying where it is; `C` closes
everything but the home, so its rooms show closed. The tree's bottom edge lists its
keys, as the details' edge lists `H`/`L`.

## Every photo the person sends is shown

A photo of a place's current state is attached and left unframed, and `ev photo add` sent nothing
to `ev ui`: the person sent a drawer's photos and their screen stayed empty. `ev photo add` now
puts the attached photo into the marked photo series, unframed and titled with its `--note` (else
the record), as `photo mark` and `photo cut` do; `--no-show` keeps it off the screen. Pictures sent
together with `ev focus --file` can each carry a note of their own: `--file a.jpg=<note>`.

## `f12` names a picture

`#` named three things in conversation: records, the rows of the agent's table and the photos of
a batch, and `#2` was also a real record. The person settled one scheme, and ev now backs it: a
bare number is a frame of the open series, `f12` is the series' twelfth picture, `#12` is a
record. Each picture in `ev ui` is titled `f12/20 · <note>`, and `ev focus --list` carries `f` (and
`source`, the photo it was drawn on). Commands that take a photo take `f12` too (`ev photo add`,
`ev photo cut`, `ev photo mark`, `ev focus --file`), meaning that photo unmarked, so the agent
carries no file paths; `ev focus f12` shows that picture again.

Attaching a batch that is already on screen took a shell loop, one `ev photo add` per photo with
its upload path. `ev photo add` now takes several photos at once (`ev photo add <ref> f2 f3 f4`)
and, with `--stdin`, one `{"ref", "photo", "note"}` a line for many records; every record and photo
is checked before anything is attached. A series picture keeps the note it was sent with and is
not sent to the screen again.

## The series at a glance

A series now reaches 30–40 pictures, and stepping through them one by one was slow. In `ev ui`,
`g` shows the whole series as a grid: each picture titled `f12 · <note>` with a count of its
numbered frames, as many a row as the screen holds at the picture width, re-flowed when the
terminal is resized. The width is one setting, `ev settings series_tile <cells>` (28 by default);
`+` / `-` change it for now. Arrows (or the wheel, a row at a time) move, `Enter` or a click
opens the picture, and in both views
`Home` / `End` go to the first and last and `f12` then `Enter` goes straight to a picture. Only the
rows on screen are drawn, and each picture is scaled down once (spec/series-grid.md).

## A new layout for a whole piece of furniture

Once every drawer of a piece of furniture is toured, the person wants to reorganize it, and the
themes written while touring describe what each drawer holds today: `ev regroup` and `ev suggest`
lean on them and look at one drawer at a time, so they defend the present. `ev layout <furniture>`
reads all the places at once by what they hold: kinds of thing kept in several places, places that
read alike, nearly empty places and the one they could join, full or mixed ones. A thing's kind is
the last word of its name before the first comma (`Kablo, USB-C` is a cable, a `şarj modülü` a
module), since a Turkish name ends in what the thing is; shared words alone grouped things by
adjectives. `--propose` drafts a layout from the contents alone: each kind, largest first, goes to
the place holding most of it, with the moves that takes and a theme per place. Nothing is moved:
the draft is something to change together, and each move is planned on the person's word
(spec/reorganize.md).

## Smaller fixes

- A kit bought as one purchase line showed that line as ×1 whatever its quantity: `ev kit show`
  and `ev kit purchase` now carry the line's `qty`.
- A bundle line that names a component in short ("… inkl. ZM 18-55 VR") is offered for that
  component too: two numbers and two other words of a long model in the line count as the model
  in part, and the line's other numbers (the camera's screen size) no longer rule the lens out.
- `ev add` given `make=…` (`ev edit`'s form) says which flag it takes: `--make …`.
- On a line in packs, `ev buy link` without `--qty` took every unit left for the first thing,
  and the next things found nothing to link: it now takes as many units as the thing stands for
  (its `qty`, else one).
- A mark labelled with a number counted across photos (a lone `2` on the second photo) left a
  gap in the series: the number skipped 2 and drew 3. A photo's labels now only order its
  frames, which take the series' next free numbers in that order.
- A photo marked again with a frame left out (`1=… 3=…` after `1=… 2=…`) gave the new frame the
  number 2 the person had already been told, since the photo's numbers went to its frames in
  order. Each frame now keeps the label it was given: marked again, a label keeps its number, a
  new one takes the next free number, and a number left out is not handed out again.
- `ev history` as text printed each event's raw JSON, oldest first, though the History tab of
  `ev ui` said every event in words. Both now read from one place: newest first under a heading
  per day, each event in the tab's words (a note of several lines on one line), places by `#id`.
- `ev photo add f2 f3` (and `--stdin`) attached f2 and then refused f3 as a photo already whole
  on another record, leaving half the call done and not saying which photo it meant. Every
  photo is now checked first, the same photo given whole to two records in one call too, and
  the refusal starts with the photo or `line n` it is about.
- `ev todo` listed what is inside a thing marked `temporary` (a label maker's tape and
  batteries) as parked too, though it travels with its holder; and a thing given only
  `waits_for` was in no list until the awaited one turned up. A thing parked itself is now
  listed alone, a waiting thing is listed under `parked`, and each row says `why` (`place`,
  `own`, `waits_for`).
- `ev buy unlink` left the line's product pictures and pages on the thing, though they show
  another product, and the line still said they were brought there. Unlinking now takes them
  back (`taken_back`); a value or a coverage the line brought stays, listed under `left` with how
  to remove it, since the person may have kept it.
- `ev task drop` turned a done task into a dropped one without a word; it now refuses, and a
  task that was not done after all is reopened first.
- `ev add --stdin` said "unknown kind ``" for a line with no `kind`; it now says "kind is
  required".
- "Not linked" was a usage error (exit 2) for `ev buy unlink` and `ev doc` but "not found"
  (exit 3) for `ev kit unlink`; it is exit 2 everywhere.
- With `EV_DB` set, every usage line listed `--db <DB>` as if it were required. `EV_DB` is now
  read by `ev` itself, and the usage line is the same either way.
- `ev buy bring <line> <ref>` answers with what it did (`node`, `brought`, `brought_types`,
  `skipped`), no longer the whole thing as `ev show` gives it.
- A photo taken minutes before the records caught up with it (what it shows recorded right after)
  made `ev review --as toured` refuse it as old. The refusal now gives `minutes_after` per photo
  and, when every change came within two hours, says the records may only have caught up and
  puts `ev photo current` first. Whether the photo shows the change stays the person's word.
