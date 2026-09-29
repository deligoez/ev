Draft for the next release.

## Fixed

- **`ev regroup` no longer proposes a thing as its own better place.** A kit recorded as an
  item with its parts inside (the LiPo box with its Seeed board, the DVD writer with its
  cables) is a holder too, and it used to come out best for itself. Its own subtree is now
  never a candidate, as with `ev suggest --for`.
- **Colours count with the noun they describe.** A colour alone now weighs 0.3 in a query, and
  a colour followed by a noun is also matched as a pair (`yeşil LED`), so a green heat gun no
  longer lands in the green LED box while green and yellow LEDs still find theirs.
- **Noun compounds match as compounds.** A bare noun followed by a noun whose only ending is
  the 3rd-person possessive (`hesap makinesi`, `kablo bağı`, `kontrol kalemi`) is also matched
  as a two-word term, so "hesap makinesi" finds the calculator rather than the machine-screw
  box. A two-word term adds to the score but never on its own makes `regroup` flag a move.
- **`ev ui`'s details pane scrolls.** `J`/`K` and the mouse wheel over the pane move through
  long contents (a drawer of 24 boxes showed 13 before); the title says when there is more,
  and another node starts at the top.
- **`ev ui` shows size and room.** A box's size, and its fill as a bar with the room it leaves
  ("room (50% full)", "full (95%)"), muted when the contents changed after the fill was given.
- **`ev ui` shows times as local and relative** ("2 h ago", or the local date and time),
  not as UTC timestamps.

## New

- **A box shows where it stands.** Selecting a placed box draws its drawer's map with the box
  highlighted; `ev show` carries the drawer's grid as `parent_grid`, and `room` for a node
  with a fill.
- **Suggestions in `ev ui`.** A drawer, or a box in it, lists what `ev regroup` says there:
  things that would fit better in another box, and whether the box is mixed.
- **Fill at a glance.** Rows of holders with a fill end in a four-step bar (`▮▮▯▯`).
- **Tidier rows and details.** A row cut at the pane's edge ends in `…`, the list takes 55% of
  the width, `×1` and the database id are no longer shown, and the key line names the detail
  scroll and photo keys.
