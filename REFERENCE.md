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
| Tab, Shift-Tab, 1–6 | tabs: tree, pending moves, disposals, lost, take/return (Götür/İade), search |
| / | search (same folding as `ev find`), Enter to run, Esc to cancel |
| click / double click | select / expand, collapse or jump; wheel scrolls; click a tab title to switch |
| q / Esc | quit |

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
| `ev audit` | `spread` (words shared by items in 2–8 holders, top 40), `no_theme` (holders with items and no theme), `loose` (items directly in a room, on furniture or in a home) |
