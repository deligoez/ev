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
| make, model, serial | `--make`, `--model`, `--serial`, `edit make=` | what the thing is beyond its name, as on its label (`make=Bosch model=GSB 13 RE`); searched by `ev find` as strongly as a code; empty clears |
| fill | `--fill`, `edit fill=` | 0–100 estimate |
| size | `--size`, `edit size=` | `WxDxH` or `WxD` in grid units, e.g. `1x2x0.5` (`×` and a decimal comma accepted); empty clears. What `regroup` compares when it offers a bigger spare box |
| tags | `--tag` (repeatable), `edit tags=+x` / `tags=-x` | stored lowercase |
| photos | `--photo` (repeatable), `edit photos=+p` / `photos=-p` | stored as absolute paths |
| to | `--to`, `edit to=` | place the node should be taken to; empty clears |
| owner | `--owner`, `edit owner=` | place the node belongs to when it is not ours |
| with | `lend --to`, `back`, `edit with=` | place holding our lent node |
| state | `dispose`, `restore`, `gone` | active, candidate, gone; dispositions trash, give, sell, return |
| lost | `--lost`, `lost`, `found [--in]`, any move | its place is not known: out of where it was last seen (kept as the parent), listed under "Unknown place" in `ev tree` and `ev ui`, not counted in that place's `items`; a thing added with `--lost` and no place was never seen |
| temporary | `--temporary`, `edit temporary=true/false` | a parking place: what is put straight into it waits for its final place (`ev todo` lists it as `parked`, `ev suggest` never offers the place or anything inside it, listing them under `parking`); on an item, that one thing waits where it is. A move clears an item's own mark (the event says `was_temporary`); a place keeps its mark until set back |

## Batch lines (`ev add --batch file` / `--stdin`)

One JSON object per line with the fields above (`name`, `kind`, `in`, `lost`, `code`,
`address`, `qty`, `note`, `theme`, `fill`, `size`, `tags`, `photos`, `to`, `owner`,
`temporary`, `make`, `model`, `serial`) plus optional `key`.
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
| show, add, edit, move, done, cancel, dispose, restore, gone, lost `<ref>`, found | `node` (all fields + `path`, `path_text`), `children`, `pending`, `last_seen`; `show` also `cells`, `grid`, `parent_grid` (the grid a placed box stands in), `room` (with a fill: `room`, `fill`, `fill_at`, `stale`), `kits` (the kit parts it is: `[{kit, n, text}]`), `documents` (see Documents), `purchases` (see Purchases), `coverages`, `coverage_proposal` and `tracking` (see Coverage) |
| split | `node` (the original, after), `into` (the records split off), `photos` (the original's, to crop each part from) |
| add --batch | `created` |
| find | `query`, `results`, best first; every word of the text must match name, code, note, theme or tags in any order, by its Turkish stem or a synonym group too, and a word that matches nothing is retried allowing a typo; the text may be left out with `--tag` or `--kind` to list every match of the filter (`ev find --tag "3d yazıcı"`) |
| tree | `tree` (nested, each with `children`, and `theme`, `fill`, `size`, `tags` when set; `count` on a place gone through on its own: `raw`, `counting`, `toured` or `kept`; lost things are not among the children or in `items`), `lost` (without a reference: every lost thing, with `last_seen`, null when never seen) |
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
kit_unlink (`kit`, `part`, `text`), sketch (`before`, `after`: `{x, y, w, d, on}` or null), grid_face (`before`, `after`), decline (`holder`, `why`) and decline_cleared, doc_linked and doc_unlinked (`document`, `kind`), purchase_linked (`purchase`, `qty`) and purchase_unlinked (`purchase`), coverage_added and coverage_removed (`coverage`, `kind`), track (`subject`, `decision`, `why`).

## `ev ui`

A read-only terminal browser. It never writes the database; it polls SQLite's `data_version`
every half second and re-reads when another process has written, highlighting the nodes that
changed and expanding their parents so they are in view. The files it writes are the display
settings file, from its Settings tab, and on exit the tree state it reopens with.

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
| H / L, click a details tab title | the previous / next details tab the node has something for; only those tabs are shown. Above every tab: the `#id` and name, then the place it is in. Summary: its state as badges (kind, count, set aside, lost, broken, on sale, label to print), its identity fields, then sections — Money (each purchase in one line: date, shop, quantity, price, today's money; the shop's own title dimmed under it; the current value), Coverage (status in colour, a proposal dimmed), Documents and links (counted, by kind), To do (its own tasks and needs; those of the places it is in only counted), Note — long values wrapped under their own column. Photos (every photo newest first with when it was added, crop or whole and its note; the one shown is marked), Documents (its documents, also through its purchases, and its links with its purchases' order and product pages), Grid (the drawer's plate, a placed box framed), Contents, Suggestions (`ev regroup`, and theme words for an untitled place), History (`ev history --contents`, newest first by day, every event in words). The choice is kept |
| Documents tab: [ / ], O / o, click | pick a document or link, open it in the program the system gives it (`open`, `xdg-open`) |
| E | the Summary also lists make, model and serial still empty, as “—” |
| y | copy what is picked on the Documents tab (a file's path, an address), else the thing's `#id` and name |
| + | widen the details as far as the list allows, and back |
| click a details line | Photos: show that photo; Documents: open it; Contents and History: open that thing in the tree; a drawer's own Grid: open the box clicked (a box's view of its drawer does not react, so a stray click stays put). These tabs cut long lines with … instead of wrapping |
| J / K, wheel over the details | scroll the details |
| drag a divider; < >, { } | resize: the list against the right side (20–80%), the photo against the details (15–85%); a double click on a divider resets it. Kept in `ui-state.json` with the details tab |
| o, click on the photo | the current photo full screen, titled with the node and the photo's note; `[` `]` ← → step, `r` / `R` rotate 90° clockwise / counter-clockwise (on screen only, kept per photo for the session; also on the photo panel), Esc / o / click close |
| O | open the current photo in the system viewer |
| m | the marked photos sent last with `ev focus --file`, again |
| M | the map (see **Maps**) full screen, from the home: its rooms first, the room on the way to the selected node chosen, and on every level Enter leads further down that way. ← ↑ ↓ → move to the nearest tile that way (Tab steps in reading order), Enter goes into the tile, Backspace / u goes up a level with the place left chosen, t closes the map on the chosen tile in the tree; a click chooses a tile and a second click goes in; Esc / q / M close. A tile shows its label, its theme (or name), how many things it holds, its fill, and what is in it as far as it has room; a place on a grid (a drawer, a Kallax) is drawn as its plate, column letters above and row numbers beside, a dot on every free cell; it follows the data as it changes |
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
| `ev settings resume on\|off` | `on` (the default): `ev ui` opens the tree as it was left — the nodes that were open, the list headings that were closed, and the node that was selected. The state is kept per database in `ui-state.json` beside the settings file, written on every exit; a node gone since is dropped, and a selected one gone opens at the top |

`auto` language is the computer's, as the system reports it (`sys-locale`): on macOS the first
of the preferred languages in System Settings, elsewhere the locale variables.
Only the language part of the tag counts (`en-TR` is English); a language ev does not speak
falls back to English. JSON output and error messages are always English. A running `ev ui`
picks a change up within a second.

The inventory's own settings live in its database (`spec/purchases.md` §3.8), read and set
by name: `ev settings inventory` lists them with their values (`default` when unset), `ev
settings <name> <value>` sets one.

| Name | Default | Is |
|---|---|---|
| `home_country` | `TR` | the country whose price index money over time follows |
| `home_currency` | `TRY` | the currency amounts are compared in |
| `price_index` | `eurostat:TR` | the index series |
| `valuable_threshold` | `1000` | from this much (home currency) a thing is valuable: its coverage is asked about |
| `coverage_warning_days` | `60` | how close an end makes a coverage `ending` |
## Grids

A holder can be laid out in cells, like a gridfinity drawer: columns A…Z from the left, rows
1… from the back. A box in it covers a rectangle of cells, named by two opposite corners
(`A3-B3`) or one cell (`C4`).

| Command | Does |
|---|---|
| `ev grid <ref>` | `node`, `grid`: `cols`, `rows`, `boxes` (NodeRef + `cells`), `free` (cell names), `unplaced` (children without cells), `map` (rows of box ids, null where free); `grid` is null without one |
| `ev grid <ref>… --cols N --rows M` | set the size (1–26 × 1–99); refused (exit 5) while a placed box would fall outside. Several references get the same grid, all or none, and return `grids` |
| `ev grid <ref>… --face above\|front` | how the grid is seen: `above` (the default; a drawer, row 1 at the back) or `front` (furniture and its compartments, row 1 at the top); with or without `--cols`/`--rows`, all or none; refused (exit 5) for a holder without a grid. `grid.face` |
| `ev grid <ref> --clear` | remove the grid; refused while boxes are placed in it |
| `ev cell <ref>=<cells>…` | place boxes in their holder's grid, several at once; `<ref>=` takes one out. Bounds and overlaps are checked against where every box ends up, so boxes can swap places in one step. A box keeps its code: it is the box's serial label, not its place. `placed`, `grids` |

`ev show` carries `cells` for a placed box and `grid` for a holder that has one; `ev suggest`
entries carry `cells`, and `grid` with its `free` cells. Moving a box out of its holder frees
its cells.

## Maps

Any place drawn as the tiles of what is in it, laid out from what is recorded:

- **grid**: the place has a grid; each placed box is a tile on its cells.
- **sketch**: something in it has a place and a size, in centimetres seen from above (a room
  in the home, a piece of furniture in a room), or the place has an outline of its own. The view covers the place's size and everything drawn. The others are `unplaced`.
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
| `ev sketch <ref> --points "x,y x,y …"` | an outline of three or more corners, for a room that is not a rectangle; its place and size become the rectangle around them. The place's own frame starts at that rectangle's corner |
| `ev sketch <ref> --size w,d --right-of\|--left-of\|--above\|--below <ref> [--offset n]` | beside another thing in the same holder, touching it on that side (above and below are up and down on the map, not standing on), slid `--offset` centimetres along that side from the other's top or left edge. ev works out the corner once; moving the other later does not move this one. Refused (exit 5) beside something with no place yet or in another holder; an outline moves with its room |
| `ev sketch --stdin` | NDJSON lines `{"ref": …, "at": [x, y], "size": [w, d], "points": [[x, y], …], "on": …, "right_of": …, "left_of": …, "above": …, "below": …, "offset": n, "clear": true}`, applied in order (a line may be placed beside one above it), all or none; a failing line is named. `sketched`. The way to put a plan from another program in: its rooms' corners, one line each |
| `ev sketch <ref> --clear` | remove its sketch |
| `ev map [<ref>]` | the home without a reference. `node`, `path_text`, `path`, `parent`, `sketch`, `layout` (`grid`, `sketch`, `tiles`, `stack`), `size` (`cols`, `rows` or `w`, `d`), `tiles`, `unplaced`; a stack adds `bands` (NodeRef + `rect`, `layout`, `size`) |

Numbers are centimetres, `120,40`, `120x40` or `120×40`, decimals with a point. Each tile is a
NodeRef with `rect` (`[x, y, w, h]` as fractions of the place, from its top-left: the back of a
drawer, the top of a stack), `items` (things inside, counted all the way down), `children`,
`contents` (up to 40: holders by code, then things by name, `×n` for a count), and when set
`theme`, `fill`, `cells`, `temporary`, `count` (how far a place is counted), `stacked` (what stands on it, bottom up) and
`band` (the stack member it belongs to). A tile with an outline carries `shapes`: its outline
and the outlines of rooms inside it, as corners in fractions of the view. A sketch's `size`
holds `w`, `d` (the view in centimetres) and `floor` (the place's own outline, or the
rectangle of a room's size). A room is drawn as a floor whether it was given corners or only a
size; furniture in it is a frame to scale (a 100×100 cupboard in a 200×200 room covers a
quarter of it). `ev ui` shows the map with `M`; a sketch with
outlines is drawn as a floor plan, each room a floor of its own shape and tone; the width of
a wall between two rooms goes to the nearer, so rooms meet, while the home's outer edge stays.

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
| `ev split <ref> <name>=<qty>… [--rename <name>] [--qty n]` | one record becomes several kinds of thing: each `<name>=<qty>` (or `<name>` without a count) is a new record beside it, with its kind and tags and a note naming where it came from; the original keeps what is left, renamed and recounted with `--rename` / `--qty`. A set recorded as one thing becomes a record per part (`ev split 598 "LM393 kart=3" "Kablo=3" --rename "HW-080 prob"`); straight and angled headers in one record become two. Events: `split` on the original, `split_from` on each new one. Photos stay on the original and are listed, to crop each part from. Each new part in `into` carries `purchase_candidates` like a fresh `ev add`. A holder with things inside is refused (exit 5); all or nothing |
| `ev kit add <name> [--copies n] [--note t] [--part "<name>[=<per copy>]"]…` | a kit: a bought set, how many of it were bought, and its parts numbered from 1, each with how many come in one copy (1 by default). Names are compared folded and are unique |
| `ev kit part <kit> "<name>[=<per copy>]"…` | add parts to the end of the list |
| `ev kit link <kit> <n> <ref>…` / `ev kit unlink <kit> <n> <ref>` | these records are part n (or are not); each record's history gets `kit_link` / `kit_unlink`, and `ev show` lists its `kits` |
| `ev kit show <kit>` | `kit` (`id`, `name`, `copies`, `note`), `counts`, `parts`: `[{n, text, qty, expected, found, lost, open, nodes}]`. `expected` is `qty × copies`; `found` sums the counts of the linked records that are here (no count is 1), `lost` those marked lost, gone ones count for nothing; `open` is what is expected and not recorded at all. Computed from the records every time, so finding or moving a record updates the kit |
| `ev kit list` / `ev kit remove <kit>` | every kit with its `counts`, most still open first; removing a kit keeps the records |

## Placement

| Command | Does |
|---|---|
| `ev suggest <text> [--tag t] [--for <ref>]` | `words` (the query as searched), `synonyms_added`, `facet` (the facets the query names), `other_facet` (up to 5 holders kept out because they are of another facet, with their `facet` and `score`), `parking` (up to 5 holders kept out because they are, or stand in, a `temporary` place: `container` with `temporary_in`, and `score`), `new_group_likely`, `considered` (how scores are made, in words), `rules`, `similar` (up to 12 holders, best first: `container` with `room`, `review` (its own or its nearest reviewed ancestor's: `status`, `at`, `from`; null when never gone through — `(not toured)` in text), `score`, `coverage`, `specific`, `matched` `[{term, points, from, specific}]`, `count`, `matches`), `containers` (every holder, with `path_text`, `theme`, `fill`, `room`, `items`, `sample`, `cells`/`grid`), `complete.containers`. `--for` places an existing node by its own name, tags and note, never into itself or anything inside it |
| `ev regroup [<ref>]` | for the holders under `<ref>` (or everywhere): `checked` (`items`, `best_where_they_are`), `elsewhere` (`item`, `now`, `better` with score and `matched`), `alone` (the same shape, for things that share no word with anything else in their holder: its score there is 0, so the other holder is a guess), `strays` (things named for another holder's theme), `full` (fill ≥ 90, with `bigger_spares` and the cells each `fits_at`), `sparse` (fill ≤ 25, with a `merge_into` sibling that has room), `mixed` (half or more of three or more things fit better elsewhere), `unknown_fill` (fill unknown or `stale`), `declined` (`item`, `holder`, `why`: moves the person said no to, left out of the lists above) |
| `ev regroup --decline <ref> [--why "…"]` | the person said no to moving it: it stays in the holder it is in and regroup no longer proposes moving it, until it is moved somewhere else. A `decline` event |
| `ev regroup --allow <ref>` | takes a decline back; a `decline_cleared` event |
| `ev synonym add <a, b, …>` / `ev synonym list` / `ev synonym remove <id>` | groups of words that mean the same thing for placing (`fotosel, ldr, ışık sensörü`); a query that has one also searches the others at 0.8 weight. `synonyms`: `[{id, words}]` |
| `ev themes [<ref>]` | containers and furniture with things in them and no theme (kits recorded as items are left out): `themes`: `[{holder, things, words: [{word, things}], contents, like}]`, most things first. `words` are the stems the contents share, written as the inventory writes them, rarer ones first, colours and numbers left out; `like` is the themed holder those words read most like (NodeRef + `theme`, `score`), or null. `ev ui` shows the same under a place's details |
| `ev facet add <name> [--words "a, b"]` / `ev facet list` / `ev facet remove <name>` | facets: kinds of things kept apart (modules and bare parts, novels and technical books). A holder is in a facet by carrying its name as a tag (`ev edit X tags=+modül`), or by being inside one that does; a thing by its own tag, else by a facet word in its name (any form: `modülü`, `modülleri` name `modül`), else by where it is. `suggest` never ranks a holder of another facet (it goes to `other_facet`) and `regroup` never proposes one. `facets`: `[{name, words, holders}]` |
| `ev rule add <text>` / `ev rule list` / `ev rule remove <id>` | placement rules in plain words |
| `ev audit` | `spread` (words shared by items in 2–8 holders, top 40; Turkish forms such as `vida`/`vidası`/`vidalar` are one row, `word` is the shortest form and `forms` lists them all), `no_theme` (holders with items and no theme), `loose` (items directly in a room, on furniture or in a home), `size_drift` (boxes whose name carries a size — `Gridfinity 1x2x0.5 — …` — that their `size` field lacks or contradicts: NodeRef + `name_size`, `size`; the field is what crops and bigger-box offers read) |

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

## Documents

Invoices, warranty certificates, manuals, service forms, appraisals and policies, copied into
`docs/` beside the database (named by content hash, like photos) and linked to the things they
belong to. The copy outlives the file it came from.

| Command | Payload |
|---|---|
| `ev doc add <file> --kind k [--for <ref>]… [--number n] [--ettn u] [--issued d] [--issuer i] [--note t]` | `document`, `existing`. `kind`: `invoice`, `warranty`, `manual`, `service`, `appraisal`, `policy`, `other`; `issued`: `YYYY-MM-DD`, `YYYY-MM` or `YYYY`. The same file again is the same document: `existing: true`, its fields are kept, only new links are added. Anything refused stores nothing |
| `ev doc list [<ref>]` | `documents`: every document (each with `nodes`), or one thing's, newest issue first |
| `ev doc show <id>` | `document`: `id`, `kind`, `file` (the stored copy), `original_name`, `number`, `ettn`, `issued_at`, `issuer`, `note`, `added_at`, `nodes` |
| `ev doc link <id> <ref>` / `ev doc unlink <id> <ref>` | `document`; unlinking keeps it in the store |

`ev show` lists a thing's documents under `documents` (each without `nodes`); the history has
`doc_linked` and `doc_unlinked` events (`document`, `kind`).

## Purchases

Lines of what was bought (`spec/purchases.md`). A line never creates a thing; it is linked to
one on the person's word, for part or all of its quantity. Amounts are kept in minor units and
shown as `1234.56`; input takes `1234.56`, `1.234,56` or `1234` (a lone separator before three
digits is refused, not guessed).

| Command | Payload |
|---|---|
| `ev buy import <file>` / `--stdin` | `imported`: `new`, `updated`, `unchanged`, `skipped` (cancelled and consumable lines), `document_links`, `documents_skipped` (a document naming no imported line is not stored), `attachments` (new ones stored), `attachments_skipped` (naming no imported line), `joined` (lines found to be another source's line, see below). All or nothing; the same lines again change nothing, and an update never touches links or a dismissal |
| `ev buy add <name> [--shop s] [--date d] [--paid n] [--currency c] [--qty n] [--order o] [--order-url u] [--url u] [--brand b] [--for <ref>]` | `purchase`: a line entered by hand (`source: manual`), linked to `--for` at once |
| `ev buy list [--open] [--bucket b] [--shop s] [--since d]` | `purchases`, newest first; `--open`: something left to link and not dismissed |
| `ev buy show <id>` | `purchase`: `id`, `source`, `source_key`, `shop`, `merchant`, `order_no`, `order_url`, `product_url`, `shop_sku`, `name`, `brand`, `category`, `ordered_at`, `delivered_at`, `qty`, `paid`, `currency`, `billed_to`, `status` (`delivered`, `returned`), `bucket` (`durable`, `clothing`, `digital`), `dismissed`, `why`, `raw`, `same_as`, `imported_at`, `linked` (`[{node, qty}]`), `open_qty` (0 for a joined line), `joined` (the ids of lines that are the same purchase), `attachments` (its own and its joined lines': `id`, `purchase`, `type`, the fields given, `brought_to`), `documents` (also those of its joined lines), `today` (see **Money over time**) |
| `ev buy for <ref>` | `node`, `candidates` (up to 12, best first): `purchase` (`id`, `name`, `shop`, `brand`, dates, `qty`, `open_qty`, `paid`, `currency`), `score`, `why` (`[{why, points}]`), `linked` when already linked to it. Open, undismissed lines scoring above zero, and any linked to it. Points: a product linked before to a thing of the same name 80; the thing's `model` or `serial` in the line 60; each shared model code (letters and digits, four or more, not a size like `64gb` or `3x3`; read from the thing's name, make, model and serial, never its note) 25, at most two; the line's brand as whole words in the thing's name or make (never the shop's own name) 12, or 5 when the line shares no word of what the thing is (the head of its name: before the first comma or dash, without brackets); shared words weighted by how rare they are among lines and records, at most 30, but at most 10 when nothing stronger says it and the line carries no more than half of that head (by the same weights), so words shared only with what a thing is for or kept with never offer a line; each unit whose numbers all differ (`125 kHz` against `13,56 MHz`, `2,5 A` against `3A`; units converted; a lone `A` counts as amperes only against its number or after a decimal, not in `Pi 3 A+`) −40 |
| `ev buy for --toured` | The back-fill (spec §12.2): `backfill`, best first, `[{node, candidate}]` with `candidate` shaped as above; `toured_things`, how many things were looked at. Every thing no purchase is linked to whose nearest reviewed place above it is `toured` (a toured drawer covers its boxes unless a box has a review of its own), each with its one best open line, only when that line scores above 15 (the bar `ev add` offers at). One line may be offered to several things |
| `ev buy link <id> <ref> [--qty n]` / `ev buy unlink <id> <ref>` | `purchase`; a link takes all that is left of the line by default, never more; it remembers the shop's product key for the next purchase of it |
| `ev buy dismiss <id> --as <reason> [--why t]` / `--clear` | `purchase`; reasons: `consumed`, `given`, `returned`, `elsewhere`, `not-mine`, `duplicate`. A dismissed line cannot be linked |
| `ev buy bring <id> <ref> [--only <attachment>]…` | the thing, as `ev show`, plus `brought` (attachment ids). The line's attachments not brought yet (all, or `--only` these) become the thing's own: a link, a value, a coverage. The line must be linked to the thing first |

**One purchase seen by two sources** (a shop's export and another app that recorded the same
thing) is joined on import: a line whose `order_url` contains another source's order number
(six characters or more) points at that line with `same_as`; when the order has several lines,
the product key (`sku`) in the line's addresses picks one; with no order address, a product key
named by exactly one other line is enough. Anything less certain is left unjoined. The line
pointed at is the one linked; the joined line counts as settled, and its attachments and
documents come with the line it joins.

`ev show` lists a thing's purchases under `purchases` (each with `linked_qty`, without `linked`,
`documents` or `raw`), and its `documents` include those of its purchases, marked
`via_purchase`. Events: `purchase_linked` (`purchase`, `qty`), `purchase_unlinked` (`purchase`).

### Import lines

One JSON object per line; `type` is `purchase` (the default) or `document`.

- `purchase`: `source` and `key` (required, the line's identity), `name` (required), `shop`,
  `merchant`, `order`, `order_url`, `product_url`, `sku`, `brand`, `category`, `ordered_at`,
  `delivered_at` (`YYYY-MM-DD`; a longer timestamp is cut to its date), `qty` (default 1),
  `paid` (the line total paid), `currency`, `billed_to`, `status` (`delivered`, `returned`,
  `cancelled`), `bucket` (`durable`, `clothing`, `digital`, `consumable`), `raw` (path of the
  raw record).
- `document`: `source`, `file`, `purchases` (keys of that source's lines), `kind` (default
  `invoice`), `number`, `ettn`, `issued`, `issuer`, `note`.
- Attachments, hung on lines by `purchase` (one key) or `purchases`, brought to a thing with
  `ev buy bring`: `link` (`url`, `kind`, `archive`, `note`), `valuation` (`amount`, `currency`,
  `at`, `approximate`, `from`: where the figure came from, `note`), `coverage` (`kind`, `term`,
  `from`, `ends`, `issuer`, `number`, `note`).

Adapters live in `tools/purchases/` and read a shop's raw export from `~/.ev/purchases/<shop>/`,
outside every repository: `tools/purchases/hepsiburada.py | ev buy import --stdin`. There is one
for each of AliExpress, Amazon.com.tr, Amazon.de, Decathlon, GittiGidiyor, Hepsiburada, idefix,
IKEA, Kitapyurdu, n11, Robo90, Robotistan, sahibinden, Trendyol and Vivense, and `umr.py` reads
Under My Roof's own store (a copy of it; the app's data is never written): each item becomes a
line with its value, warranties and info link as attachments, and its receipts and attachments
as documents.

## Coverage

Warranties and insurance as one kind of record (`spec/purchases.md` §3.6). A coverage covers
one or more things; its status is computed from its start and term, never stored.

| Command | Payload |
|---|---|
| `ev cover add <ref>… --kind k [--term t] [--from f] [--ends d] [--usage u] [--issuer i] [--number n] [--premium p] [--deductible p] [--currency c] [--scope s] [--note t]` | `coverage`. `kind`: `statutory`, `manufacturer`, `extended`, `store`, `insurance`; `term`: `2y`, `18m`, `6w`, `90d` or `lifetime`; `from`: `delivery` (the default: the earliest delivery of the purchases linked to its things), a date, or `after:<id>` (starts when that coverage ends). A term or an end is required; an insurance needs an end or a time term. Recording one clears the things' own "do not track" decision on coverage |
| `ev cover list [--ending]` | `coverages`; `--ending` only those within the warning window |
| `ev cover show <id>` | `coverage`: the fields given, plus `start`, `end`, `days_left`, `status` (`active`, `ending`, `ended`, `undetermined` when no start can be known), `repair_days`, `nodes`, `documents` |
| `ev cover remove <id>` | `removed`; for a coverage recorded by mistake. Its documents stay in the store |
| `ev doc add <file> … --coverage <id>` | links the document to a coverage too (a warranty certificate, a policy) |
| `ev track <ref> value\|coverage no\|later\|yes [--why t]` | the node, as `ev show`. `no` (do not track) and `later` (not now) close the question: `ev` never raises it again on its own. A decision on a holder covers everything in it, also what is put there later. `yes` clears it; recording the data clears it too |

The end of a `statutory` or `manufacturer` coverage moves by the days its things spent broken
(`ev broken` to `ev fixed`, or to today while still broken) after it started.

`ev show` carries `coverages` (each without `nodes`), `coverage_proposal` (a durable linked
purchase and no statutory coverage nor decision: `kind`, `term`, `start`, `end`, `why`; a
proposal, never a record) and `tracking` (`value`, `coverage`: `decision`, `why`, `on`, the node
the decision was made on). Events: `coverage_added`, `coverage_removed` (`coverage`, `kind`),
`track` (`subject`, `decision`, `why`).

The statutory proposal counts only lines sold at home: paid in the `home_currency` (or with no
currency) and not from a marketplace abroad. The marketplaces abroad are a fixed list by shop
name, each with the country it sells from: AliExpress, Temu, Banggood (CN), Amazon.com (US),
Amazon.co.uk (GB), Amazon.de, Amazon.fr, Amazon.it, Amazon.es; one whose country is the
`home_country` is at home. The start is the earliest delivery of such a line, and there is no
proposal once its two years (plus any time in repair) have passed.

## Values and links

What a thing is worth is a dated observation (`spec/purchases.md` §3.3): a second-hand listing,
a shop's price, an appraisal. The latest is the current value; the purchase price is never one.
A link (§3.4) keeps an archive for when its page dies.

| Command | Payload |
|---|---|
| `ev value <ref> [<amount>] [--currency c] [--at d] [--source s] [--note t] [--approximate]` | `node`, `added` (the new observation's id, or null), `valuations`: newest first, each `id`, `amount`, `currency` (the home one by default), `at` (today by default), `approximate` (the date is a guess), `source`, `note`, `today` (as for a purchase). Recording one clears the thing's own "do not track" decision on value |
| `ev value <ref> --remove <id>` | the same; for an observation recorded by mistake |
| `ev link add <ref> <url> [--kind k] [--archive a] [--note t]` | `node`, `links`. `kind`: `info` (the default), `manual`, `support`, `driver`, `other`. `archive`: a saved file, copied into the document store, or a web address (a Wayback Machine copy). The same address again on the same thing updates its kind, archive and note |
| `ev link list <ref>` | `node`, `links`: each `id`, `kind`, `url`, `archive`, `note`, `added_at` |
| `ev link remove <id>` | `node`, `links` |

`ev show` and the details in `ev ui` carry `valuations` (the first is the current value) and
`links`.

## Money over time

A purchase price in today's money (`spec/purchases.md` §3.9). One country per inventory: an
amount in another currency is turned into the home currency at the purchase day's rate (or the
closest earlier one within a week), then grown by the home price index from the purchase month
to the latest cached one; a month the series lacks takes its year's value. `ev` stays offline:
`tools/money/fetch.py` fetches the index (Eurostat HICP for `eurostat:<geo>`, World Bank annual
CPI for `worldbank:<iso2>`) and the rates (the Central Bank of Türkiye when home is TRY, the ECB
otherwise), with no key.

    ev money needs | tools/money/fetch.py | ev money import --stdin

| Command | Payload |
|---|---|
| `ev money needs` | `money_needs`: `home_currency`, `home_country`, `index` (the series), `from_month` (the earliest purchase month), `rates` (`[{currency, day}]` not cached) |
| `ev money import <file>\|--stdin` | `money_imported`: `index`, `rates` counted. Lines `{"type":"index","series","period":"YYYY-MM\|YYYY","value","source"}` and `{"type":"rate","currency","home","day","rate","source"}` (home per unit of `currency`); all or nothing, a value fetched again replaces the cached one |
| `ev money status` | `money`: `home_currency`, `index`, `periods`, `latest`, `stale` (the latest period began more than 75 days ago), `rates`, `missing_rates` |

A purchase in `ev buy show`, and each in `ev show`'s `purchases` (for the linked quantity), carries
`today` when it can be computed: `amount`, `currency` (home), `index`, `index_month`; for a foreign
currency also `rate`, `rate_day`, `in_home_then`. The valuable count in `ev todo` uses it, falling
back to the price paid in the home currency.

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
| `ev review <ref> --as counting\|toured\|kept\|raw [--note t]` | how far a place has been counted: `raw` not counted (the default), `counting` its tour has begun (set when a task on it starts, cleared back to `raw` when the task closes unfinished), `toured` counted, `kept` left as it is on purpose; covers everything below it |
| `ev progress` | every place gone through on its own (a unit: a holder with no labelled child, or a room with no furniture, box or room in it) with `review` (`status`, `at`, `from`, `changed_since`), `children`, `observations`, `planned`; counts `units`, `toured`, `kept`, `counting`, `raw`, `changed_since_tour` |
| `ev task add "<title>" --why "<why>" [--on ref]… [--at n]` | a task at position n (last by default) |
| `ev task list [--all]` | unfinished tasks in order (`position`), then closed ones with `--all` |
| `ev task start\|done\|drop\|reopen <id> [--note t]` | one task is in progress at a time; `done` only when the person says so |
| `ev task edit <id> [--title] [--why] [--on ref]… [--off ref]… [--at n]` | change a task |
| `ev next` | `goal`, `task` (with `places`: each as `show`, plus `arriving`), `open_tasks`, `progress` (the counts of `ev progress`: `units`, `toured`, `kept`, `counting`, `raw`, `changed_since_tour`), `unplanned` (raw units no task covers; empty under `track`), `rules` |

A unit is the innermost labelled holder, or an unlabelled holder standing on its own in a room
or on furniture: a holder none of whose children carries a code. `ev show` carries the node's
`review`, `observations` and `tasks`: the unfinished tasks linked to the node or to a place that
holds it, each with `via`, the node the link is on. In `ev ui`, tab 7 (Yapılacak) lists the tasks with progress in its
title.

## Everything waiting

| Command | Does |
|---|---|
| `ev todo` | `goal`, `progress` (as in `ev next`), `counts` and lists: `tasks`, `moves`, `errands`, `disposals` (sell entries carry `sale`), `labels`, `needs`, `repairs`, `expiring` (`expires`, `days_left`), `lost`, `uncounted` (places not counted yet or being counted, from `ev progress`), `parked` (things waiting for their final place: put straight into a `temporary` place, or marked `temporary` themselves, each with `in`), `stale` (organize only), `unclear` (names containing "belirsiz", "muhtemelen" or "?"), `shared_photos` (a whole photo attached to several live nodes, with `nodes`), `photos` (units with contents and no photo of their own, `photo_reason: none`, or whose contents changed after it, `changed` with `photo_at` and `changed_at`; a move out counts; a crop attached to the place itself counts as its photo, crops on the things inside do not; a holder with a grid is checked too, since its photo is what its boxes' crops are cut from, and carries `grid: true`), `coverage_ending` (coverages within the warning window, each with `nodes`), `coverage` (`count` of valuable things — a durable linked purchase in the home currency from `valuable_threshold` — with no coverage and no decision, `threshold`, `currency`, `top`: the five dearest, each with `worth`, in today's money when the index is cached), `values` (`count` of things with a durable linked purchase and no value nor decision, `currency`, `top`: the five dearest, as for `coverage`); `counts.purchases` is the number of open durable purchase lines (nothing linked yet, not dismissed, not joined to another line) |
| `ev label` | codes whose label still has to be printed; `ev label <ref>…` marks them printed, `--needed` marks them needed again. Setting or changing a code marks it needed |
| `ev broken <ref> [--note t]` / `ev fixed <ref>` | broken, and what is wrong / repaired |
| `ev expires <ref> <YYYY-MM-DD\|YYYY-MM>` / `--clear` | use-by date; `todo` shows it within 60 days or past |
| `ev sale <ref> --listed\|--reserved [--price n] [--where t] [--condition c]` / `--clear` | where a sale stands; only for a sell candidate; price and place carry over when not repeated. `--condition` (`new`, `like-new`, `used`) is what the buyer is told, kept as the `condition` mark; the only place a condition is recorded. `--clear` drops both |
| `ev need add "<text>" [--qty n] [--make] [--for ref] [--note t]` | something to buy (or make, e.g. 3D print) |
| `ev need list [--all]` · `ev need got <id>` · `ev need drop <id>` | open needs; close one |

`ev show` carries `marks` (`label`, `broken`, `expires`, `sale`, each with `value`, `amount`,
`note`, `at`) and `needs` (open needs for the node). In `ev ui`, tab 7 (Yapılacak) shows one
collapsible section per kind: Enter or → on a header opens and closes it, ← on a line goes up to
its header; unclear records start collapsed.
