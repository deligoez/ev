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
| state | `dispose`, `restore`, `gone` | active, candidate, gone |
| lost | `--lost`, `lost`, `found`, any move | |

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
