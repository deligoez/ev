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
