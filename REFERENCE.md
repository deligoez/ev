# ev reference

## Node fields

| Field | Set with | Notes |
|---|---|---|
| name | `add <name>`, `edit name=` | required |
| kind | `--kind`, `edit kind=` | home, room, furniture, container, item |
| parent | `--in`, `move` | not editable; rooms only in homes/rooms |
| code | `--code`, `edit code=`, `recode` | unique among non-gone nodes, folded; not digits only; `code=` clears; `ev recode A=X B=Y …` sets several at once, checking uniqueness against the codes they end up with (swap or rotate codes when boxes change places) |
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

## Batch lines (`ev add --batch file` / `--stdin`)

One JSON object per line with the fields above (`name`, `kind`, `in`, `lost`, `code`,
`address`, `qty`, `note`, `theme`, `fill`, `size`, `tags`, `photos`) plus optional `key`.
`"in": "@key"` points at an earlier line. Unknown fields are rejected. All or nothing.

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
| show, add, edit, move, done, cancel, dispose, restore, gone, lost `<ref>`, found | `node` (all fields + `path`, `path_text`), `children`, `pending`, `last_seen` |
| add --batch | `created` |
| find | `query`, `results` |
| tree | `tree` (nested, each with `children`), `unplaced` (without a reference) |
| recode | `recoded`: `[{id, name, before, after}]` |
| pending | `pending`: `[{node, to}]` |
| disposals | `disposals`: `{trash, give, sell}` |
| lost | `lost`: `[{node, last_seen}]` |
| history | `node`, `events`: `[{at, type, data}]` |

Errors print nothing on stdout; stderr carries
`{"error": {"code", "kind", "message", "candidates"?, "details"?}}` in JSON mode.

## Event types

create, edit, move, plan, done, cancel, dispose, restore, gone, lost, found, photo, grid, cell.

## `ev ui`

A read-only terminal browser. It never writes the database; it polls SQLite's `data_version`
every half second and re-reads when another process has written, highlighting the nodes that
changed and expanding their parents so they are in view. The one file it writes is the
display settings file, from its Settings tab.

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
| o, click on the photo | the current photo full screen, titled with the node and the photo's note; `[` `]` ← → step, `r` / `R` rotate 90° clockwise / counter-clockwise (on screen only, kept per photo for the session; also on the photo panel), Esc / o / click close |
| O | open the current photo in the system viewer |
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
| `ev settings` | `language` (`setting`, `effective`, `system`), `theme` (`setting`), `file` |
| `ev settings language en\|tr\|auto` | the language of `ev ui` and of the readable terminal output |
| `ev settings theme dark\|light\|auto` | the appearance of `ev ui`; `auto` follows the terminal |

`auto` language is the computer's: on macOS the first of the preferred languages
(`defaults read -g AppleLanguages`), elsewhere `LC_ALL`, `LC_MESSAGES`, `LANGUAGE` or `LANG`.
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
| `ev grid <ref> --cols N --rows M` | set the size (1–26 × 1–99); refused (exit 5) while a placed box would fall outside |
| `ev grid <ref> --clear` | remove the grid; refused while boxes are placed in it |
| `ev cell <ref>=<cells>… [--recode]` | place boxes in their holder's grid, several at once; `<ref>=` takes one out. Bounds and overlaps are checked against where every box ends up, so boxes can swap places in one step. `--recode` names each placed box `<holder code>-<back-left cell>`. `placed`, `grids` |

`ev show` carries `cells` for a placed box and `grid` for a holder that has one; `ev suggest`
entries carry `cells`, and `grid` with its `free` cells. Moving a box out of its holder frees
its cells.

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

## Placement

| Command | Does |
|---|---|
| `ev suggest <text> [--tag t] [--for <ref>]` | `words` (the query as searched), `synonyms_added`, `new_group_likely`, `considered` (how scores are made, in words), `rules`, `similar` (up to 12 holders, best first: `container` with `room`, `score`, `coverage`, `specific`, `matched` `[{term, points, from, specific}]`, `count`, `matches`), `containers` (every holder, with `path_text`, `theme`, `fill`, `room`, `items`, `sample`, `cells`/`grid`), `complete.containers`. `--for` places an existing node by its own name, tags and note, never into itself or anything inside it |
| `ev regroup [<ref>]` | for the holders under `<ref>` (or everywhere): `checked` (`items`, `best_where_they_are`), `elsewhere` (`item`, `now`, `better` with score and `matched`), `strays` (things named for another holder's theme), `full` (fill ≥ 90, with `bigger_spares` and the cells each `fits_at`), `sparse` (fill ≤ 25, with a `merge_into` sibling that has room), `mixed` (half or more of three or more things fit better elsewhere), `unknown_fill` (fill unknown or `stale`) |
| `ev synonym add <a, b, …>` / `ev synonym list` / `ev synonym remove <id>` | groups of words that mean the same thing for placing (`fotosel, ldr, ışık sensörü`); a query that has one also searches the others at 0.8 weight. `synonyms`: `[{id, words}]` |
| `ev rule add <text>` / `ev rule list` / `ev rule remove <id>` | placement rules in plain words |
| `ev audit` | `spread` (words shared by items in 2–8 holders, top 40; Turkish forms such as `vida`/`vidası`/`vidalar` are one row, `word` is the shortest form and `forms` lists them all), `no_theme` (holders with items and no theme), `loose` (items directly in a room, on furniture or in a home), `unknown` (holders never inventoried) |

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
- **`coverage`** is the share of the query's weight the holder matched. Below 0.5 for the best
  holder, `new_group_likely` is true: nothing here is what the thing is.
- **`specific`** marks a word that says what a thing is rather than its family: one in about 1
  in 20 holders or fewer (IDF ≥ 3), or in at most 2 holders.
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
screen (the last one by default); `ev focus --clear` withdraws the request. Each request is shown
once.

`ev gone <ref> [--as d] [--why "<text>"]` records the reason in the `gone` event and appends it to the
note. A gone node is out of reach by name, but its id still works for `ev show <id> --include-gone`,
`ev history <id>` and `ev edit <id> note=…` (the note is the only field a gone node lets change; any
other exits 5).

## Photos

| Command | Does |
|---|---|
| `ev photo add <ref> <file> [--crop x,y,w,h] [--note text]` | copy into `~/.ev/photos/` (hash-named) and attach; with `--crop` attach the cut-out, remembering the original |
| `ev photo list <ref>` | `photos`: `n`, `path`, `exists`, `source`, `crop`, `note`, `added_at` |
| `ev photo remove <ref> <n>` | detach the n-th photo (the stored file stays) |
| `ev photo add <ref> <file> --whole` | attach a whole photo that is already attached whole to another node; without `--whole` (and without `--crop`) that is refused with exit 5 and `details.attached_to` |
| `ev photo cut <file> <ref>=x,y,w,h… [--place <ref>] [--note n]` | one photo cut up among several nodes in one step: a crop for each `<ref>=`, and the whole photo on `--place`; every reference is resolved and every crop cut first, then all are recorded in one transaction. `attached`: `[NodeRef + photo, crop, path]` |
| `ev photo current <ref>` | the newest photo still shows the place well enough; off the photo-needed list until the next change |
| `ev photo adopt` | copy photos still referenced outside the store into it |

In `ev ui`, `[` / `]` or the wheel over the photo step through the selected node's photos, `o` or a
click shows the current one full screen (with its note), and `O` opens it in the system viewer.

## Plan

| Command | Does |
|---|---|
| `ev goal [organize\|track]` | show or set what the household wants: a tidy-up plan, or records only |
| `ev observe <ref> "<text>" [--photo n]` | a dated note on a place, optionally tied to its n-th photo; `ev unobserve <id>` removes one |
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
| `ev todo` | `counts` and lists: `tasks`, `moves`, `errands`, `disposals` (sell entries carry `sale`), `labels`, `needs`, `repairs`, `expiring` (`expires`, `days_left`), `lost`, `unknown`, `stale` (organize only), `unclear` (names containing "belirsiz", "muhtemelen" or "?"), `shared_photos` (a whole photo attached to several live nodes, with `nodes`), `photos` (units with contents and no photo of their own, `photo_reason: none`, or whose contents changed after it, `changed` with `photo_at` and `changed_at`; a move out counts; a crop attached to the place itself counts as its photo, crops on the things inside do not) |
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
