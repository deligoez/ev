# ev reference

## Node fields

| Field | Set with | Notes |
|---|---|---|
| name | `add <name>`, `edit name=` | required |
| kind | `--kind`, `edit kind=` | home, room, furniture, container, item |
| parent | `--in`, `move` | not editable; rooms only in homes/rooms |
| code | `--code`, `edit code=`, `recode` | unique among non-gone nodes, folded; not digits only; `code=` clears; a code ending in `*` takes the next free number of its series (`GF1x1-*` after `GF1x1-007` is `GF1x1-008`, padded like the series, 3 digits for a new one, gone nodes' numbers never reused) in `add`, `edit` and `edit --stdin`; `ev recode A=X B=Y …` sets several at once, checking uniqueness against the codes they end up with (swap or rotate codes when boxes change places) |
| address | `--address`, `edit address=` | homes only |
| qty | `--qty`, `edit qty=` | ≥ 1; empty clears |
| note, theme | `--note`, `--theme`, `edit note=` | empty clears |
| fill | `--fill`, `edit fill=` | 0–100 estimate |
| size | `--size`, `edit size=` | `WxDxH` or `WxD` in grid units, e.g. `1x2x0.5` (`×` and a decimal comma accepted); empty clears. What `regroup` compares when it offers a bigger spare box |
| tags | `--tag` (repeatable), `edit tags=+x` / `tags=-x` | stored lowercase |
| photos | `--photo` (repeatable), `edit photos=+p` / `photos=-p` | stored as absolute paths |
| to | `--to`, `edit to=` | place the node should be taken to; empty clears |
| owner | `--owner`, `edit owner=` | place the node belongs to when it is not ours |
| with | `lend --to`, `back`, `edit with=` | place holding our lent node |
| state | `dispose`, `restore`, `gone` | active, candidate, gone; dispositions trash, give, sell, return |
| lost | `--lost`, `lost`, `found`, any move | |
| unknown | `--unknown`, `edit unknown=true/false` | contents never inventoried; `audit` lists them |
| temporary | `--temporary`, `edit temporary=true/false` | a parking place: what is put straight into it waits for its final place (`ev todo` lists it as `parked`, `ev suggest` never offers the place or anything inside it, listing them under `parking`); on an item, that one thing waits where it is. A move clears an item's own mark (the event says `was_temporary`); a place keeps its mark until set back |

## Batch lines (`ev add --batch file` / `--stdin`)

One JSON object per line with the fields above (`name`, `kind`, `in`, `lost`, `code`,
`address`, `qty`, `note`, `theme`, `fill`, `size`, `tags`, `photos`) plus optional `key`.
`"in": "@key"` points at an earlier line. Unknown fields are rejected. All or nothing.

## Edit lines (`ev edit --stdin`)

One JSON object per line: `{"ref": "#551", "set": {"size": "1x2x1.5", "tags": ["+modül", "-boş kap"], "note": null}}`.
`ref` is a name, code, id or `#id`; each `set` entry is one `field=value` of `ev edit` (an
array is one per item, `null` clears). Blank lines are skipped. All or nothing: a failing line
is named (`line 2: …`) and nothing is changed. Each record gets one `edit` event, a field set
several times showing its value before the first and after the last. Output: `edited`.

## Output

JSON when piped, readable text on a terminal. `--json` forces JSON on a terminal; `--text`
forces the text through a pipe — to read a result, not to parse it.

## Payload shapes

Every node reference (`NodeRef`) is:

```json
{"id": 5, "code": null, "name": "Flipper Zero", "kind": "item", "state": "active",
 "lost": false, "path": [{"id": 1, "code": null, "name": "Ev"}, "…"],
 "path_text": "Ev › Salon › K4x4 › K4x4-15-A › Flipper Zero"}
```

`disposition` and `qty` appear when set. A path segment prints a node's code when it has
one, its name otherwise.

| Command | Top-level keys |
|---|---|
| show, add, edit, move, done, cancel, dispose, restore, gone, lost `<ref>`, found | `node` (all fields + `path`, `path_text`), `children`, `pending`, `last_seen`; `show` also `cells`, `grid`, `parent_grid` (the grid a placed box stands in), `room` (with a fill: `room`, `fill`, `fill_at`, `stale`) and `kits` (the kit parts it is: `[{kit, n, text}]`) |
| split | `node` (the original, after), `into` (the records split off), `photos` (the original's, to crop each part from) |
| add --batch | `created` |
| find | `query`, `results`; the text may be left out with `--tag` or `--kind` to list every match of the filter (`ev find --tag "3d yazıcı"`) |
| tree | `tree` (nested, each with `children`, and `theme`, `fill`, `size`, `tags` when set), `unplaced` (without a reference) |
| recode | `recoded`: `[{id, name, before, after}]` |
| pending | `pending`: `[{node, to}]` |
| disposals | `disposals`: `{trash, give, sell}` |
| lost | `lost`: `[{node, last_seen}]` |
| history | `node`, `events`: `[{at, type, data}]`; with `--contents` also the events of things that came in, went out (`move`, `done`, `plan` to or from it) or were added there (`create`), each with `item` (NodeRef) and `relation`: `in` \| `out` \| `added` |

Errors print nothing on stdout; stderr carries
`{"error": {"code", "kind", "message", "candidates"?, "details"?}}` in JSON mode.

## Event types

create, edit, move, plan, done, cancel, dispose, restore, gone, lost, found, back, photo,
photo_remove (`path`, `crop`, `note`, `n`: what was detached), grid, cell, observe, unobserve,
review, split (`into`: the records split off) and split_from (`from`, `name`), kit_link and
kit_unlink (`kit`, `part`, `text`), sketch (`before`, `after`: `{x, y, w, d, on}` or null).

## `ev ui`

A read-only terminal browser. It never writes the database; it polls SQLite's `data_version`
every half second and re-reads when another process has written, highlighting the nodes that
changed and expanding their parents so they are in view. The files it writes are the display
settings file, from its Settings tab, and on exit the tree position it reopens on.

| Key | Action |
|---|---|
| ↑ ↓ / j k, PgUp PgDn, g G | move |
| → / l / Enter | expand in the tree; in a list, jump to the node in the tree |
| ← / h | collapse, or go to the parent |
| Tab, Shift-Tab, 1–8 | tabs: layout (the tree of places and things), pending moves, leaving, lost, errands (take / return), search, everything waiting (To do), settings |
| / | search (same folding as `ev find`), Enter to run; Esc clears the typed text, then closes the box; Ctrl-U clears |
| x / Esc on the search tab, or click its title | clear the search and its results |
| click / double click | select / expand, collapse or jump; wheel scrolls; click a tab title to switch |
| [ / ], wheel over the photo | previous / next photo of the selected node |
| H / L, click a details tab title | the previous / next details tab the node has something for: Summary (fields, labels aligned), Photos (every photo newest first with when it was added, crop or whole and its note; the one shown is marked), Grid (the drawer's plate, a placed box framed), Contents, Suggestions (`ev regroup`, and theme words for an untitled place), History (`ev history --contents`, newest first by day). A tab with nothing for the node is dimmed and shows the summary; the choice is kept |
| click a details line | Photos: show that photo; Contents and History: open that thing in the tree; a drawer's own Grid: open the box clicked (a box's view of its drawer does not react, so a stray click stays put). These tabs cut long lines with … instead of wrapping |
| J / K, wheel over the details | scroll the details |
| drag a divider; < >, { } | resize: the list against the right side (20–80%), the photo against the details (15–85%); a double click on a divider resets it. Kept in `ui-state.json` with the details tab |
| o, click on the photo | the current photo full screen, titled with the node and the photo's note; `[` `]` ← → step, `r` / `R` rotate 90° clockwise / counter-clockwise (on screen only, kept per photo for the session; also on the photo panel), Esc / o / click close |
| O | open the current photo in the system viewer |
| m | the marked photos sent last with `ev focus --file`, again |
| M | the map (see **Maps**) full screen: the place holding the selected node, that node chosen. ← ↑ ↓ → move to the nearest tile that way (Tab steps in reading order), Enter goes into the tile, Backspace / u goes up a level with the place left chosen, t closes the map on the chosen tile in the tree; a click chooses a tile and a second click goes in; Esc / q / M close. A tile shows its label, its theme (or name), how many things it holds, its fill, and what is in it as far as it has room; it follows the data as it changes |
| Settings tab: Enter / → / Space, ← | next / previous option of the selected setting; saved at once and applied to the whole screen |
| q / Esc | quit (Esc clears a search first) |

**Appearance.** With the appearance on Automatic, `ev ui` follows the terminal's light or dark
background while it runs. At start it turns on DEC private mode 2031 (`CSI ? 2031 h`) and asks
both `CSI ? 996 n` and OSC 11. A terminal with mode 2031 (Ghostty, kitty, Contour, and others)
then reports every switch as `CSI ? 997 ; 1 n` (dark) or `; 2 n` (light). A terminal that never
sends such a report is asked for its background (OSC 11) every three seconds instead, and one
that answers neither keeps the `COLORFGBG` hint or dark. The dark palette uses the terminal's
named colours; the light one uses fixed darker tones readable on white. `ev ui` reads the
terminal input itself (keys, SGR mouse reports and these answers), because crossterm treats a
`997` report as an unfinished sequence and swallows the keys typed after it.

## Settings

Display settings belong to the person at the computer, not to the inventory: they live in
`~/.ev/settings.json` (`EV_CONFIG` to move it), not in the database, and `ev settings` works
without one.

| Command | Does |
|---|---|
| `ev settings` | `language` (`setting`, `effective`, `system`), `theme` (`setting`), `resume`, `file` |
| `ev settings language en\|tr\|auto` | the language of `ev ui` and of the readable terminal output |
| `ev settings theme dark\|light\|auto` | the appearance of `ev ui`; `auto` follows the terminal |
| `ev settings resume on\|off` | `on` (the default): `ev ui` opens on the node that was selected in the tree when it last closed. The position is kept per database in `ui-state.json` beside the settings file, written on every exit; a node gone since opens at the top |

`auto` language is the computer's, as the system reports it (`sys-locale`): on macOS the first
of the preferred languages in System Settings, elsewhere the locale variables.
Only the language part of the tag counts (`en-TR` is English); a language ev does not speak
falls back to English. JSON output and error messages are always English. A running `ev ui`
picks a change up within a second.
## Grids

A holder can be laid out in cells, like a gridfinity drawer: columns A…Z from the left, rows
1… from the back. A box in it covers a rectangle of cells, named by two opposite corners
(`A3-B3`) or one cell (`C4`).

| Command | Does |
|---|---|
| `ev grid <ref>` | `node`, `grid`: `cols`, `rows`, `boxes` (NodeRef + `cells`), `free` (cell names), `unplaced` (children without cells), `map` (rows of box ids, null where free); `grid` is null without one |
| `ev grid <ref>… --cols N --rows M` | set the size (1–26 × 1–99); refused (exit 5) while a placed box would fall outside. Several references get the same grid, all or none, and return `grids` |
| `ev grid <ref> --clear` | remove the grid; refused while boxes are placed in it |
| `ev cell <ref>=<cells>… [--recode]` | place boxes in their holder's grid, several at once; `<ref>=` takes one out. Bounds and overlaps are checked against where every box ends up, so boxes can swap places in one step. `--recode` names each placed box `<holder code>-<back-left cell>`. `placed`, `grids` |

`ev show` carries `cells` for a placed box and `grid` for a holder that has one; `ev suggest`
entries carry `cells`, and `grid` with its `free` cells. Moving a box out of its holder frees
its cells.

## Maps

Any place drawn as the tiles of what is in it, laid out from what is recorded:

- **grid**: the place has a grid; each placed box is a tile on its cells.
- **sketch**: the place has a size and at least one thing in it has a place and a size, in
  centimetres seen from above (a room in the home, a piece of furniture in a room). The others
  are `unplaced`.
- **tiles**: neither; what is in it is laid out on its own, holders first, labelled ones first.
- **stack**: furniture standing on another (`--on`) is drawn with it, front on, top first: one
  band per member, as tall as its grid has rows, each laid out by its own grid. In the room it
  is left out of the tiles and listed under `stacked` of the one it stands on.

| Command | Does |
|---|---|
| `ev sketch <ref>` | `node`, `sketch`: `{x, y, w, d, on}` or null |
| `ev sketch <ref> --size w,d` | its width and depth; a place with a size can be sketched in |
| `ev sketch <ref> --at x,y` | its top-left corner in its holder, seen from above |
| `ev sketch <ref> --on <ref>` | it stands on another; refused (exit 5) on itself or on what stands on it |
| `ev sketch <ref> --clear` | remove its sketch |
| `ev map [<ref>]` | the home without a reference. `node`, `path_text`, `path`, `parent`, `sketch`, `layout` (`grid`, `sketch`, `tiles`, `stack`), `size` (`cols`, `rows` or `w`, `d`), `tiles`, `unplaced`; a stack adds `bands` (NodeRef + `rect`, `layout`, `size`) |

Numbers are centimetres, `120,40`, `120x40` or `120×40`, decimals with a point. Each tile is a
NodeRef with `rect` (`[x, y, w, h]` as fractions of the place, from its top-left: the back of a
drawer, the top of a stack), `items` (things inside, counted all the way down), `children`,
`contents` (up to 40: holders by code, then things by name, `×n` for a count), and when set
`theme`, `fill`, `cells`, `temporary`, `unknown`, `stacked` (what stands on it, bottom up) and
`band` (the stack member it belongs to). `ev ui` shows the map with `M`.

## Places

| Command | Does |
|---|---|
| `ev place add <name> [--alias a]…` | create a place with aliases |
| `ev place alias <place> <alias>` | add an alias; one already used elsewhere exits 5 |
| `ev place list` | every place with aliases and counts of take / return / collect |
| `ev place merge <from> <into>` | move every reference and alias of `from` into `into` |
| `ev for [<place>]` | `take`, `return`, `collect` lists for a place, or `errands` for every place |
| `ev lend <ref> --to <place>` / `ev back <ref>` | lend a node out / it came back |

Place names match folded with apostrophes ignored; an unknown name in `to`/`owner`/`with`
creates the place.

## Splitting and kits

| Command | Does |
|---|---|
| `ev split <ref> <name>=<qty>… [--rename <name>] [--qty n]` | one record becomes several kinds of thing: each `<name>=<qty>` (or `<name>` without a count) is a new record beside it, with its kind and tags and a note naming where it came from; the original keeps what is left, renamed and recounted with `--rename` / `--qty`. A set recorded as one thing becomes a record per part (`ev split 598 "LM393 kart=3" "Kablo=3" --rename "HW-080 prob"`); straight and angled headers in one record become two. Events: `split` on the original, `split_from` on each new one. Photos stay on the original and are listed, to crop each part from. A holder with things inside is refused (exit 5); all or nothing |
| `ev kit add <name> [--copies n] [--note t] [--part "<name>[=<per copy>]"]…` | a kit: a bought set, how many of it were bought, and its parts numbered from 1, each with how many come in one copy (1 by default). Names are compared folded and are unique |
| `ev kit part <kit> "<name>[=<per copy>]"…` | add parts to the end of the list |
| `ev kit link <kit> <n> <ref>…` / `ev kit unlink <kit> <n> <ref>` | these records are part n (or are not); each record's history gets `kit_link` / `kit_unlink`, and `ev show` lists its `kits` |
| `ev kit show <kit>` | `kit` (`id`, `name`, `copies`, `note`), `counts`, `parts`: `[{n, text, qty, expected, found, lost, open, nodes}]`. `expected` is `qty × copies`; `found` sums the counts of the linked records that are here (no count is 1), `lost` those marked lost, gone ones count for nothing; `open` is what is expected and not recorded at all. Computed from the records every time, so finding or moving a record updates the kit |
| `ev kit list` / `ev kit remove <kit>` | every kit with its `counts`, most still open first; removing a kit keeps the records |

## Placement

| Command | Does |
|---|---|
| `ev suggest <text> [--tag t] [--for <ref>]` | `words` (the query as searched), `synonyms_added`, `facet` (the facets the query names), `other_facet` (up to 5 holders kept out because they are of another facet, with their `facet` and `score`), `parking` (up to 5 holders kept out because they are, or stand in, a `temporary` place: `container` with `temporary_in`, and `score`), `new_group_likely`, `considered` (how scores are made, in words), `rules`, `similar` (up to 12 holders, best first: `container` with `room`, `review` (its own or its nearest reviewed ancestor's: `status`, `at`, `from`; null when never gone through — `(not toured)` in text), `score`, `coverage`, `specific`, `matched` `[{term, points, from, specific}]`, `count`, `matches`), `containers` (every holder, with `path_text`, `theme`, `fill`, `room`, `items`, `sample`, `cells`/`grid`), `complete.containers`. `--for` places an existing node by its own name, tags and note, never into itself or anything inside it |
| `ev regroup [<ref>]` | for the holders under `<ref>` (or everywhere): `checked` (`items`, `best_where_they_are`), `elsewhere` (`item`, `now`, `better` with score and `matched`), `alone` (the same shape, for things that share no word with anything else in their holder: its score there is 0, so the other holder is a guess), `strays` (things named for another holder's theme), `full` (fill ≥ 90, with `bigger_spares` and the cells each `fits_at`), `sparse` (fill ≤ 25, with a `merge_into` sibling that has room), `mixed` (half or more of three or more things fit better elsewhere), `unknown_fill` (fill unknown or `stale`) |
| `ev synonym add <a, b, …>` / `ev synonym list` / `ev synonym remove <id>` | groups of words that mean the same thing for placing (`fotosel, ldr, ışık sensörü`); a query that has one also searches the others at 0.8 weight. `synonyms`: `[{id, words}]` |
| `ev themes [<ref>]` | containers and furniture with things in them and no theme (kits recorded as items are left out): `themes`: `[{holder, things, words: [{word, things}], contents, like}]`, most things first. `words` are the stems the contents share, written as the inventory writes them, rarer ones first, colours and numbers left out; `like` is the themed holder those words read most like (NodeRef + `theme`, `score`), or null. `ev ui` shows the same under a place's details |
| `ev facet add <name> [--words "a, b"]` / `ev facet list` / `ev facet remove <name>` | facets: kinds of things kept apart (modules and bare parts, novels and technical books). A holder is in a facet by carrying its name as a tag (`ev edit X tags=+modül`), or by being inside one that does; a thing by its own tag, else by a facet word in its name (any form: `modülü`, `modülleri` name `modül`), else by where it is. `suggest` never ranks a holder of another facet (it goes to `other_facet`) and `regroup` never proposes one. `facets`: `[{name, words, holders}]` |
| `ev rule add <text>` / `ev rule list` / `ev rule remove <id>` | placement rules in plain words |
| `ev audit` | `spread` (words shared by items in 2–8 holders, top 40; Turkish forms such as `vida`/`vidası`/`vidalar` are one row, `word` is the shortest form and `forms` lists them all), `no_theme` (holders with items and no theme), `loose` (items directly in a room, on furniture or in a home), `unknown` (holders never inventoried), `size_drift` (boxes whose name carries a size — `Gridfinity 1x2x0.5 — …` — that their `size` field lacks or contradicts: NodeRef + `name_size`, `size`; the field is what crops and bigger-box offers read) |

### How `suggest` and `regroup` score

Each holder is one document made of its own theme (×3), name (×2.5) and note (×1) and of
the names (×2), tags (×1.5) and notes (×1) of the things directly inside it. A query's words
are scored against it with BM25F (k1 = 1.2, b = 0.5): a word rare across holders (IDF) counts
more, repeats count less and less. The query itself is weighted by where its words came from:
the name 1, tags 0.8, a note 0.4 (`--for`), a synonym 0.8.

- **Words.** Text is folded (case and Turkish letters), filler words are dropped, part codes
  stay whole (`KY-018` → `ky018`, `HC-SR04` → `hcsr04`), bare numbers are not words. Turkish
  endings are cut against the inventory's own vocabulary: a form goes to the shortest shorter
  form written somewhere on its own (`kutuda` → `kutu`, `kitabı` → `kitap`), a word written on
  its own is never cut further, and otherwise to the longest stem two different words share.
- **Two-word terms.** A bare noun followed by a noun whose only ending is the 3rd-person
  possessive is also matched as a pair (`hesap makinesi` → `hesap+makine`, `kablo bağı`), and
  so is a colour with the word after it (`yeşil LED`). A colour alone weighs 0.3 in a query.
- **`coverage`** is the share of the query's weight the holder matched. Below 0.5 for the best
  holder, `new_group_likely` is true: nothing here is what the thing is.
- **`specific`** marks a word that says what a thing is rather than its family: one in about 1
  in 20 holders or fewer (IDF ≥ 3), or in at most 2 holders. Two-word terms are never
  `specific`: they add to a score but do not by themselves make `regroup` flag a move.
- **`room`**: `yes` below fill 70, `little` from 70, `none` from 90, `unknown` without a fill.
  `stale` is true when the contents changed after the fill was given.
- Ties go to the lower id, so the same inventory and question always give the same answer.

`regroup` asks the same question of every thing, leaving the thing itself out (it must not vote
for where it already is). A holder with the same theme words as the thing's own is the same
group split over boxes and counts as home. A thing is flagged `elsewhere` only when another
holder scores at least 1.5× its own, at least 3, and matched on a `specific` word. A spare box
is anything tagged `boş kap` (or `spare box`) with a `size`.

`ev restore <ref> --correction "<why>"` undoes a `gone` recorded by mistake; plain `restore` returns a
candidate to active.

`ev gone <ref> --as mistake --why "<text>"` closes a record that should never have existed (a
misreading, a duplicate): it keeps its history, is not a disposal, and the reason is required.

`ev focus <ref> [--photo n]` makes a running `ev ui` jump to the node and show that photo full
screen (the last one by default); `ev focus --file <picture>… [--note text]` shows pictures that
are no record (marked photos; repeat `--file` for several, stepped with `[` `]`) full screen,
titled with the note, until Esc (a click does not close them; `m` in `ev ui` opens the last ones
again, also after a restart); `ev focus --clear`
withdraws the request. Each request is shown once, without restarting `ev ui`.

`ev photo mark <target> <label>=<where>… [--grid corners] [--out file] [--show note]` draws a
red frame and a label for each mark on a copy of a photo: `<target>` is a photo file or a place
(its newest whole photo), `<where>` is `x,y,w,h` in fractions of the upright photo or cells of
the place's grid (`A6`, `A6-B7`). Cells are found through the grid corners the photo kept when
it was cut with `--grid` (schema 11), or through `--grid`. The copy goes to `--out` or to
`<temp>/ev-marks/` (files there older than a day are removed on each call); it is not stored,
not attached and leaves no history. `--show` also sends it to a running `ev ui`. Output:
`marked`, `source`, `marks: [{label, at}]`, and `shown` with `--show`.

`ev gone <ref> [--as d] [--why "<text>"]` records the reason in the `gone` event and appends it to the
note. A gone node is out of reach by name, but its id still works for `ev show <id> --include-gone`,
`ev history <id>` and `ev edit <id> note=…` (the note is the only field a gone node lets change; any
other exits 5).

## Photos

| Command | Does |
|---|---|
| `ev photo add <ref> <file> [--crop x,y,w,h] [--note text]` | copy into `~/.ev/photos/` (hash-named) and attach; with `--crop` attach the cut-out, remembering the original |
| `ev photo list <ref>` | `photos`: `n`, `path`, `exists`, `source`, `crop`, `note`, `added_at` |
| `ev photo remove <ref> <n>` | detach the n-th photo (the stored file stays); the history keeps a `photo_remove` event with what it was |
| `ev photo add <ref> <file> --whole` | attach a whole photo that is already attached whole to another node; without `--whole` (and without `--crop`) that is refused with exit 5 and `details.attached_to` |
| `ev photo cut <file> <ref>=x,y,w,h… [--place <ref>] [--grid <corners>] [--note n]` | one photo cut up among several nodes in one step: a crop for each `<ref>=` (the same `<ref>` may come several times, one crop each: the three probes of three sets in one photo are one record), and the whole photo on `--place`; every reference is resolved and every crop cut first, then all are recorded in one transaction. `--grid blx,bly,brx,bry,frx,fry,flx,fly` (needs `--place`, a place with a grid) gives the grid's back-left, back-right, front-right and front-left corners as fractions of the upright photo and adds a crop for every placed box, mapped with the photo's perspective and widened by a margin that grows with the box's height from its `size` (`1x2x1.5`: a tall box's rim leans out of its cells), never below 0.15 of a cell; a crop named by hand wins for its box. `--preview [note]` cuts and attaches nothing: it draws every crop it would make (each grid box framed on its cells, labelled with its back-left cell) on a temporary copy, `{preview, framed, sheet}`, and with a note shows it in a running `ev ui`. `attached`: `[NodeRef + photo, crop, path]`, and `sheet`: a contact sheet of every crop — each small, six to a row, labelled with its box's cell (`B3`), else its code or `#id` — written to the scratch folder of `photo mark`, to check a whole cut at a glance |
| `ev photo current <ref>` | the newest photo still shows the place well enough; off the photo-needed list until the next change |
| `ev photo adopt` | copy photos still referenced outside the store into it |

`ev review <ref> --as toured` is refused (exit 5) while the place, or a placed box in its grid,
has no photo or only one older than its last change; `details.stale` lists them
(`node`, `reason`: `none`|`changed`, `photo_at`, `changed_at`).

In `ev ui`, the newest photo shows first, with its note in the panel title; `[` / `]` or the wheel
over the photo step through the selected node's photos, `o` or a click shows the current one full
screen (with its note), and `O` opens it in the system viewer. The details start with the node's
`#id`, which any command takes in place of a name or code (`ev show #534`). A place with a grid is
drawn as its plate, each box a frame over the cells it covers.

## Plan

| Command | Does |
|---|---|
| `ev goal [organize\|track]` | show or set what the household wants: a tidy-up plan, or records only |
| `ev observe <ref> "<text>" [--photo n]` | a dated note on a place, optionally tied to its n-th photo; `ev unobserve <id>` removes one, leaving an `unobserve` event with its text in the place's history |
| `ev review <ref> --as toured\|kept\|raw [--note t]` | how far a place has been gone through; covers everything below it |
| `ev progress` | every unit with `review` (`status`, `at`, `from`, `changed_since`), `children`, `unknown`, `observations`, `planned`; counts `units`, `toured`, `kept`, `raw`, `changed_since_tour` |
| `ev task add "<title>" --why "<why>" [--on ref]… [--at n]` | a task at position n (last by default) |
| `ev task list [--all]` | unfinished tasks in order (`position`), then closed ones with `--all` |
| `ev task start\|done\|drop\|reopen <id> [--note t]` | one task is in progress at a time; `done` only when the person says so |
| `ev task edit <id> [--title] [--why] [--on ref]… [--off ref]… [--at n]` | change a task |
| `ev next` | `goal`, `task` (with `places`: each as `show`, plus `arriving`), `open_tasks`, `progress`, `unplanned` (raw units no task covers; empty under `track`), `rules` |

A unit is the innermost labelled holder, or an unlabelled holder standing on its own in a room
or on furniture: a holder none of whose children carries a code. `ev show` carries the node's
`review`, `observations` and `tasks`: the unfinished tasks linked to the node or to a place that
holds it, each with `via`, the node the link is on. In `ev ui`, tab 7 (Yapılacak) lists the tasks with progress in its
title.

## Everything waiting

| Command | Does |
|---|---|
| `ev todo` | `counts` and lists: `tasks`, `moves`, `errands`, `disposals` (sell entries carry `sale`), `labels`, `needs`, `repairs`, `expiring` (`expires`, `days_left`), `lost`, `unknown` (every node marked unknown, not only innermost places), `parked` (things waiting for their final place: put straight into a `temporary` place, or marked `temporary` themselves, each with `in`), `stale` (organize only), `unclear` (names containing "belirsiz", "muhtemelen" or "?"), `shared_photos` (a whole photo attached to several live nodes, with `nodes`), `photos` (units with contents and no photo of their own, `photo_reason: none`, or whose contents changed after it, `changed` with `photo_at` and `changed_at`; a move out counts; a crop attached to the place itself counts as its photo, crops on the things inside do not; a holder with a grid is checked too, since its photo is what its boxes' crops are cut from, and carries `grid: true`) |
| `ev label` | codes whose label still has to be printed; `ev label <ref>…` marks them printed, `--needed` marks them needed again. Setting or changing a code marks it needed |
| `ev broken <ref> [--note t]` / `ev fixed <ref>` | broken, and what is wrong / repaired |
| `ev expires <ref> <YYYY-MM-DD\|YYYY-MM>` / `--clear` | use-by date; `todo` shows it within 60 days or past |
| `ev sale <ref> --listed\|--reserved [--price n] [--where t]` / `--clear` | where a sale stands; only for a sell candidate; price and place carry over when not repeated |
| `ev need add "<text>" [--qty n] [--make] [--for ref] [--note t]` | something to buy (or make, e.g. 3D print) |
| `ev need list [--all]` · `ev need got <id>` · `ev need drop <id>` | open needs; close one |

`ev show` carries `marks` (`label`, `broken`, `expires`, `sale`, each with `value`, `amount`,
`note`, `at`) and `needs` (open needs for the node). In `ev ui`, tab 7 (Yapılacak) shows one
collapsible section per kind: Enter or → on a header opens and closes it, ← on a line goes up to
its header; unclear records start collapsed.
