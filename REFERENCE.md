# ev reference

## Node fields

| Field | Set with | Notes |
|---|---|---|
| name | `add <name>`, `edit name=` | required |
| kind | `--kind`, `edit kind=` | home, room, furniture, container, item |
| parent | `--in`, `move` | not editable; rooms only in homes/rooms |
| code | `--code`, `edit code=` | unique among non-gone nodes, folded; not digits only; `code=` clears |
| address | `--address`, `edit address=` | homes only |
| qty | `--qty`, `edit qty=` | ≥ 1; empty clears |
| note, theme | `--note`, `--theme`, `edit note=` | empty clears |
| fill | `--fill`, `edit fill=` | 0–100 estimate |
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
`address`, `qty`, `note`, `theme`, `fill`, `tags`, `photos`) plus optional `key`.
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
| pending | `pending`: `[{node, to}]` |
| disposals | `disposals`: `{trash, give, sell}` |
| lost | `lost`: `[{node, last_seen}]` |
| history | `node`, `events`: `[{at, type, data}]` |

Errors print nothing on stdout; stderr carries
`{"error": {"code", "kind", "message", "candidates"?, "details"?}}` in JSON mode.

## Event types

create, edit, move, plan, done, cancel, dispose, restore, gone, lost, found.

## `ev ui`

A read-only terminal browser. It never writes; it polls SQLite's `data_version` every half
second and re-reads when another process has written, highlighting the nodes that changed and
expanding their parents so they are in view.

| Key | Action |
|---|---|
| ↑ ↓ / j k, PgUp PgDn, g G | move |
| → / l / Enter | expand in the tree; in a list, jump to the node in the tree |
| ← / h | collapse, or go to the parent |
| Tab, Shift-Tab, 1–7 | tabs: tree, pending moves, disposals, lost, take/return (Götür/İade), search, everything waiting (Yapılacak) |
| / | search (same folding as `ev find`), Enter to run; Esc clears the typed text, then closes the box; Ctrl-U clears |
| x / Esc on the search tab, or click its title | clear the search and its results |
| click / double click | select / expand, collapse or jump; wheel scrolls; click a tab title to switch |
| [ / ], wheel over the photo | previous / next photo of the selected node |
| o, click on the photo | the current photo full screen, titled with the node and the photo's note; `[` `]` ← → step, `r` / `R` rotate 90° clockwise / counter-clockwise (on screen only, kept per photo for the session; also on the photo panel), Esc / o / click close |
| O | open the current photo in the system viewer |
| q / Esc | quit (Esc clears a search first) |

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
| `ev suggest <text> [--tag t]` | `rules`, `similar` (holders of alike items, most matches first), `containers` (every holder, with `path_text`, `theme`, `fill`, `items`, `sample`), `complete.containers` |
| `ev rule add <text>` / `ev rule list` / `ev rule remove <id>` | placement rules in plain words |
| `ev audit` | `spread` (words shared by items in 2–8 holders, top 40; Turkish forms such as `vida`/`vidası`/`vidalar` are one row, `word` is the shortest form and `forms` lists them all), `no_theme` (holders with items and no theme), `loose` (items directly in a room, on furniture or in a home), `unknown` (holders never inventoried) |

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
| `ev todo` | `counts` and lists: `tasks`, `moves`, `errands`, `disposals` (sell entries carry `sale`), `labels`, `needs`, `repairs`, `expiring` (`expires`, `days_left`), `lost`, `unknown`, `stale` (organize only), `unclear` (names containing "belirsiz", "muhtemelen" or "?"), `photos` (units with contents and no whole-view photo, `photo_reason: none`, or whose contents changed after it, `changed` with `photo_at` and `changed_at`; a move out counts, crops do not) |
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
