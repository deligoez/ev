Show, don't tell: the agent marks up a photo of what goes where and `ev ui` puts it full screen
the moment it is sent; one drawer photo cuts every box in it, and photos stay current by
construction; `ev ui` gets a tabbed, clickable details pane with a history of each place,
resizable panes, and reopens where it was left; facets keep kinds of things apart when placing.

## Upgrading

The database moves from schema 9 to **schema 11** the first time this version opens it (10
adds facets, 11 keeps a photo's grid corners). An older `ev` then refuses the database with
exit 6, so copy `~/.ev/ev.db` aside before the first run if you might go back.

## New

### Showing what goes where

- **`ev photo mark` draws what goes where.** Numbered red frames with labels on a copy of a
  photo — on the parts (`"1 → A6"=x,y,w,h`, fractions of the upright photo), or on a drawer's
  cells by name (`1=A6`, `2=A6-B7`), found through the grid corners its photo was cut with (or
  `--grid`). Labels step aside so they never cover each other, and Turkish letters are drawn in
  their plain form. The copy is scratch: written to a temporary folder (cleared after a day) or
  `--out`, never stored, never attached, no history.
- **Live in `ev ui`.** `--show "note"` puts the marked copy full screen in a running `ev ui` at
  once, titled with the note. `ev focus --file a --file b --note "…"` sends several pictures
  together, stepped with `[` `]` (`1/2` in the title). Esc closes them — a stray click does not —
  and `m` opens the last ones again, also after a restart; the key hints say so.
- **`ev photo cut --preview`** cuts and attaches nothing: it frames every box a cut would make on
  a copy of the photo, labelled, to check the grid corners by eye before cutting.

### One drawer photo, every box current

- **`ev photo cut --grid` cuts every box of a drawer from one photo.** Give the grid's four
  corners in the photo (back-left, back-right, front-right, front-left, as fractions); each placed
  box gets a crop through the photo's perspective. A crop named by hand still wins for its box.
  The photo keeps its corners (schema 11), which is how `ev photo mark` finds cells by name.
- **A tour needs current photos.** `ev review <x> --as toured` is refused while the place or
  a placed box in its grid shows an older state than it has (`details.stale`); attach a photo or
  run `ev photo current`.

### `ev ui`

- **Tabbed details** — Summary, Photos, Grid, Contents, Suggestions, History (`H`/`L` or a
  click), each title with its count; a tab with nothing for the node is dimmed.
- **History of a place.** The History tab tells a place's story newest first by day, with what
  came in, went out and was added; `ev history <x> --contents` prints the same.
- **Everything is clickable.** A line on the Photos, Contents or History tab opens what it
  names (a thing that has since left says so instead); a box on any grid — a drawer's, or a
  box's view of its drawer — opens that box, and the Grid tab stays open, so a drawer can be
  walked box by box.
- **A grid is drawn as its plate**, each box a frame over the cells it covers.
- **Resizable panes**: drag the divider between the list and the details, or under the photo
  (`<` `>` `{` `}` from the keyboard; a double click resets). Sizes and the details tab are
  kept between sessions. A photo narrower than its pane is centred.
- **Reopens where you left it**: on the node selected in the tree when it last closed, per
  database. A new setting, on by default (Settings tab, or `ev settings resume on|off`).
- **Key hints fit the width** and show only the keys that do something on the screen at hand,
  dropping the least useful first.
- **`#id` references.** Each node's details start with its `#id`, and every command takes
  `#534` in place of a name or code.

### Placing

- **Facets keep kinds of things apart when placing.** `ev facet add modül --words "modül,
  kart"` defines a kind; tagging a holder with its name puts it (and what is inside it) in that
  facet. A thing's facet comes from its own tag, else from a facet word in its name in any form
  (`modülü`, `modülleri`), else from where it is. `ev suggest` never ranks a holder of another
  facet (they are listed apart as `other_facet`) and `ev regroup` never proposes one: a buzzer
  module is no longer sent to the bare-buzzer box. `ev facet list|remove`.
- **`ev tree` reads as a layout**: each node carries its theme, fill, size and tags.

## Changed

- **`ev find --tag t` lists everything tagged t**; the search text is optional with `--tag` or
  `--kind`.
- **`ev unobserve` leaves a trace**: the removed note's text goes into the place's history as an
  `unobserve` event, so closing a note once the work is done loses nothing.
- **`ev ui` opens on the newest photo**, not the oldest, with the photo's note in the panel title.
- **The skill** names every part by where it is in the photo, shows placement proposals as
  marked photos, previews a grid cut before cutting, closes observations with the work they
  asked for, and uses `ev find --tag` to gather things kept apart.

## Fixed

- **A drawer's own photo goes out of date with its boxes.** `ev todo` checked only the smallest
  units (the boxes), so a box added after the drawer's photo never put the drawer back on the
  photo list, though the drawer photo is what every box's crop is cut from. A holder with a grid
  is now checked too and listed with `grid: true`.
- **`ev ui` tree rows now show fill bars.** They read the fill from `ev tree`, which did not
  carry it, so the bars added in v0.11.0 never appeared.
- **One noun no longer gets two keys by its ending.** A stem that some word carries a plural
  ending on is a noun, and wins: `kablo bağı` and `kablo bağları` now meet at `bağ` (it used to
  read `bağı` as `bak` + ı), `bacaklarında` reaches `bacak`, `bloğu` stays `blok`. The
  instrumental after a consonant (`-la`/`-le`: `lehimle`, `bağlarla`) is an ending too. Against
  the author's hand-judged stems, 1728 of 1919 words now get the stem a person would: 29 fixed,
  8 changed that were already fine (mostly to a better stem, `yeri` → `yer`). A root cut to
  another root (`altın` → `alt`) still needs a dictionary.

## Measuring and testing

- **Placement and stems can be measured.** `tools/measure/` scores `ev suggest` against a
  reviewed answer key of where things belong, and stems against a hand-judged list; the keys
  stay out of the repository, in `~/.ev/measure/`. On the author's inventory, 266 of 416 placed
  things score best where they belong.
- **The documents are checked against the binary.** A test reads every backticked `ev …` command
  in the README, the reference, the skill and these notes, and fails on a subcommand or flag the
  CLI does not have (it caught the skill telling the agent to mark a tour with a `--status`
  flag, which never existed: the flag is `--as`).
- **End-to-end tests** for this release's commands: a grid cut previewed then cut, marked
  pictures sent to `ev ui` with a note, ids, tags, place histories and observations from the
  command line, a box added after its drawer's photo; and `ev ui` tests for marked photos
  (stepping, a click that does not close, `m`, a restart), clicks on every tab, the resume
  setting, and a history line naming something gone.
