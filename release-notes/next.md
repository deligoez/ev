Draft for the next release.

## New

- **Facets keep kinds of things apart when placing.** `ev facet add modül --words "modül,
  kart"` defines a kind; tagging a holder with its name puts it (and what is inside it) in that
  facet. A thing's facet comes from its own tag, else from a facet word in its name in any form
  (`modülü`, `modülleri`), else from where it is. `ev suggest` never ranks a holder of another
  facet (they are listed apart as `other_facet`) and `ev regroup` never proposes one: a buzzer
  module is no longer sent to the bare-buzzer box. `ev facet list|remove`. Schema version 10.
- **`ev tree` reads as a layout**: each node carries its theme, fill, size and tags.
- **`ev photo cut --grid` cuts every box of a drawer from one photo.** Give the grid's four
  corners in the photo (back-left, back-right, front-right, front-left, as fractions); each placed
  box gets a crop through the photo's perspective. A crop named by hand still wins for its box.
- **A tour needs current photos.** `ev review <x> --status toured` is refused while the place or
  a placed box in its grid shows an older state than it has (`details.stale`); attach a photo or
  run `ev photo current`.
- **`#id` references.** `ev ui` shows each node's `#id` at the top of its details, and every
  command takes `#534` in place of a name or code.
- **`ev ui` draws a grid as its plate**, each box a frame over the cells it covers.
- **`ev ui` details have tabs** — Summary, Photos, Grid, Contents, Suggestions, History (`H`/`L` or a
  click), each title with its count; the History tab tells a place's story newest first, with
  what came in, went out and was added (`ev history <x> --contents` prints the same). A click
  on a line opens what it names: a photo, a thing inside, a thing in the history, or — on a
  drawer's own grid — the box clicked. The key hints below fit the width, dropping the least
  useful first, and show only the keys that do something on the screen at hand.
- **Resizable panes in `ev ui`**: drag the divider between the list and the details, or under
  the photo (`<` `>` `{` `}` from the keyboard; a double click resets). Sizes and the details
  tab are kept between sessions. A photo narrower than its pane is centred.
- **`ev ui` reopens where you left it**: on the node selected in the tree when it last closed,
  per database. A new setting, on by default (Settings tab, or `ev settings resume on|off`).

## Changed

- **`ev ui` opens on the newest photo**, not the oldest, with the photo's note in the panel title.

## Fixed

- **`ev ui` tree rows now show fill bars.** They read the fill from `ev tree`, which did not
  carry it, so the bars added in v0.11.0 never appeared.
