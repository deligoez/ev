# ev 0.1.0 — agent-first home inventory

## 1. Decision

`ev` is a command-line home inventory whose primary user is an AI agent working in
conversation with a person standing at the shelves. The person reports what is where; the
agent records it through `ev`; either of them asks `ev` where something is.

0.1.0 ships a Rust workspace with two crates — a `core` library that owns the data model,
the rules and the storage, and a `cli` binary named `ev` — plus a Claude Code skill that
teaches an agent the workflow. A graphical app is deferred (§10) and will reuse `core`.

One database holds any number of homes. A car, an office or a storage unit is recorded as a
home of its own.

## 2. Why

The inventory started as hand-written Markdown files maintained by an agent. That record
drifts: the same container carried two codes in two files, a planned move and a completed
move looked alike, and "where is the Flipper Zero?" could only be answered by grepping
prose. General-purpose home inventory apps are built for a person tapping a screen, not for
an agent issuing commands and reading structured results. `ev` makes the agent's writes
consistent by construction and its reads machine-parseable.

Rust over Go: the later app layer is expected to be Tauri, and a shared Rust `core` lets
the CLI and the app run the same rules against the same database.

Several homes are supported from the first release because the root of the tree is the one
thing that cannot be changed later without rewriting every record's path.

## 3. Data model

### 3.1 Everything is a node

Homes, rooms, furniture, compartments, drawers, boxes, bags and items are all **nodes** in
one tree. Any node may hold child nodes: a bag becomes a container the moment something is
put in it, and an item can hold other items. `kind` is a label that describes a node; apart
from the placement rules of §3.2 it grants or withholds nothing.

Each node has:

| Field | Required | Meaning |
|---|---|---|
| id | yes | Integer assigned by `ev`, never reused |
| name | yes | Free text, may repeat across nodes |
| kind | yes | One of `home`, `room`, `furniture`, `container`, `item` |
| parent | see §3.2 | The node that holds it |
| code | no | Human label printed on the physical object, e.g. `K4x4-07-Ü`, `S5-01` |
| address | no | Free text, homes only |
| qty | no | Positive integer count for a record that stands for several identical things; absent means one |
| note | no | Free text |
| tags | no | Set of short lowercase strings |
| theme | no | Free text describing what a node is used to hold, e.g. "ses ve kablo" |
| fill | no | Estimated fullness, integer 0–100 |
| photos | no | List of absolute file paths; `ev` stores the path, not the file |
| state | yes | `active`, `candidate`, or `gone` (§3.3) |
| lost | yes | Whether the node's whereabouts are unknown (§3.6); false by default |

### 3.2 Tree invariants

1. A `home` has no parent.
2. A `room`'s parent is a `home` or another `room`.
3. Every other node's parent is any node that is not `gone`. Such a node may have no parent
   only while it is lost (§3.6).
4. A node cannot be moved into itself or into any of its descendants.
5. A `code` is unique among nodes whose state is not `gone`, across every home, and is not
   made only of digits.
6. Moving a node moves its whole subtree.
7. A node whose state is `gone` cannot be a parent or a move target.
8. `ev edit <ref> kind=<k>` may change a node's kind; rules 1–3 are checked against the new
   kind.

A command that would break an invariant changes nothing and exits with code 5 (§6).

```
Ev (home)
└─ Yatak odası (room)
   └─ Giyinme odası (room)      ← a room inside a room: allowed
      └─ Gardırop (furniture)
```

`ev move "Giyinme odası" --to Gardırop` exits 5 (rule 2), and so does
`ev move "Yatak odası" --to "Giyinme odası"` (rule 4).

### 3.3 Lifecycle: candidate, then gone

A node that leaves the home carries a disposition — `trash`, `give` or `sell`. It is usually
set aside first and only later actually leaves, so leaving is two steps; when something is
thrown away on the spot, one command does both.

| From | Command | To | Effect |
|---|---|---|---|
| `active` | `ev dispose <ref> --as <d>` | `candidate` | Stays in the tree where it physically is; carries disposition `d` |
| `candidate` | `ev restore <ref>` | `active` | Disposition cleared |
| `candidate` | `ev gone <ref> [--as <d>]` | `gone` | Leaves the tree; `--as` replaces the disposition |
| `active` | `ev gone <ref> --as <d>` | `gone` | Both steps at once; history records dispose then gone |
| `active` | `ev gone <ref>` | — | Refused, exit 5: how it left is unknown |

A `gone` node is no longer a candidate: `ev disposals` does not list it. It keeps its last
disposition, so the record answers "did we sell that or give it away?".

`dispose` and `gone` refuse, with exit 5, a node that still has non-`gone` children, and
list those children in the error payload.

### 3.4 Pending moves

A move can be **planned** and later **confirmed**, because the agent proposes a destination
and the person carries the item.

- `ev move <ref> --to <ref> --plan` records a pending move. The node stays where it is and
  shows the pending destination.
- `ev pending` lists every pending move.
- `ev done <ref>` applies the node's pending move; the invariants of §3.2 are checked at
  that moment, not at planning time.
- `ev cancel <ref>` drops the node's pending move.
- A node has at most one pending move; planning a second one exits 5.
- `ev move <ref> --to <ref>` without `--plan` moves immediately and drops any pending move
  the node had.

### 3.5 History

Every change is appended to an event log that is never edited or pruned: create, edit,
move, plan, done, cancel, dispose, restore, gone, lost. `ev history <ref>` returns a node's
events in order, each with its timestamp and the before/after values it changed.

### 3.6 Lost

A node whose whereabouts are unknown is **lost**.

- `ev lost <ref>` marks a node lost. It keeps its parent as its **last seen** place, so the
  search can start there.
- `ev add <name> --kind <k> --lost` without `--in` records something known to exist whose
  place was never known; it has no parent.
- `ev lost` with no reference lists every lost node with its last seen path, or none.
- Any move of a lost node clears the flag; a planned move does not, `ev done` does.
- A lost node's descendants are not flagged themselves; their path runs through a lost node.

## 4. References

Every command argument that names a node is a **reference**, resolved in this order:

1. A reference made only of digits is an id.
2. Otherwise it is compared, after folding (§5), against codes; an exact match wins.
3. Otherwise it is compared, after folding, against names; an exact match wins.

No match exits 3. More than one match at the deciding step exits 4, and the payload lists
every candidate with its id, code, name and path, so the agent can retry with an id. A
partial name never resolves; the agent searches with `ev find` and retries with the id.
`gone` nodes are excluded unless the command is `history` or the flag `--include-gone` is
given.

| id | code | name | place |
|---|---|---|---|
| 42 | `S5-01` | Ses ve kablo | Ev › Kiler |
| 57 | `K4x4-07-Ü` | Çekmece | Ev › Salon › K4x4 |
| 88 | — | Flipper Zero | Ev › Salon › K4x4 › K4x4-15-A |
| 91 | — | Anten | Ev › Salon › K4x4 › K4x4-02-Ü |
| 92 | — | Anten | Ev › Salon › Masa |

With that database:

| Reference | Result |
|---|---|
| `42` | node 42 |
| `s5-01` | node 42 |
| `k4x4-07-u` | node 57 |
| `flipper zero` | node 88 |
| `flipper` | exit 3 |
| `anten` | exit 4, candidates 91 and 92 |

## 5. Search and folding

`ev find <text>` returns every non-`gone` node whose name, code, note, theme or tags contain
the text after folding, each with its full path from the home down
(`Ev › Salon › K4x4 › K4x4-15-A › Flipper Zero`) and its lost flag. `--tag`, `--kind` and
`--include-gone` narrow or widen the set.

Folding makes comparison case-insensitive with Turkish casing rules and removes diacritics:

| Input | Folded |
|---|---|
| `Çekmece` | `cekmece` |
| `IŞIK` | `isik` |
| `İğne` | `igne` |
| `Kör` | `kor` |
| `Şarj` | `sarj` |
| `Üst` | `ust` |
| `Hâlâ` | `hala` |

So `ev find cekmece` finds a node named "Çekmece", and `ev find ISIK` finds "ışık".

## 6. Command-line contract

Output goes to stdout as JSON when stdout is not a terminal and as readable text when it is;
`--json` forces JSON. Errors, warnings and hints go to stderr only. Exit codes are fixed:

| Exit | Meaning |
|---|---|
| 0 | Success |
| 1 | Internal error |
| 2 | Usage error |
| 3 | Reference not found |
| 4 | Reference ambiguous |
| 5 | Invariant or lifecycle rule refused the change |
| 6 | Database was written by a newer `ev` |

Commands in 0.1.0:

| Command | Purpose |
|---|---|
| `ev add <name> --kind <k> [--in <ref>] [--lost] [field flags]` | Create a node |
| `ev add --batch <file.ndjson>` / `--stdin` | Create many nodes in one call; all or none |
| `ev show <ref>` | One node with its path, direct children, pending move, disposition and lost flag |
| `ev tree [<ref>] [--depth <n>]` | The subtree under a node, or every home |
| `ev find <text>` | Folded search (§5) |
| `ev edit <ref> <field>=<value>…` | Change fields; tags and photos take `+value` / `-value` |
| `ev move <ref> --to <ref> [--plan]` | Move now, or plan a move (§3.4) |
| `ev pending` / `ev done <ref>` / `ev cancel <ref>` | Pending moves (§3.4) |
| `ev dispose <ref> --as <d>` / `ev restore <ref>` / `ev gone <ref> [--as <d>]` | Lifecycle (§3.3) |
| `ev disposals [--as <d>]` | Every candidate, grouped by disposition |
| `ev lost [<ref>]` | Mark a node lost, or list lost nodes (§3.6) |
| `ev history <ref>` | A node's events (§3.5) |

Batch creation lets a line refer to a node created by an earlier line of the same batch, so
a whole box and its contents can be entered in one call.

## 7. Storage

The database is one SQLite file at `~/.ev/ev.db`, created on first use; `--db <path>` or the
`EV_DB` environment variable selects another file. The file records the schema version that
wrote it; `ev` migrates an older file forward and refuses a newer one with exit 6.

The inventory is personal data and never lives in the `ev` repository.

## 8. The skill

`skills/ev/SKILL.md` tells an agent:

- the conversation workflow — the person reports, the agent records, a move is planned,
  the person carries it, the agent confirms with `ev done`;
- that codes are what is printed on the physical labels and must be used as given;
- that uncertainty is recorded as a note, never guessed into a field;
- how to read exit codes 3 and 4 and retry with an id.

## 9. Acceptance

| # | Given | When | Then |
|---|---|---|---|
| A1 | home "Ev" ⊃ room "Salon" ⊃ furniture "K4x4" ⊃ drawer `K4x4-15-A` ⊃ item "Flipper Zero" | `ev find flipper` | one hit whose path is Ev › Salon › K4x4 › K4x4-15-A › Flipper Zero |
| A2 | box `S5-01` inside drawer D | `ev move D --to S5-01` | exit 5, nothing changed |
| A3 | item X with a pending move to `S5-03` | `ev show X` | parent is unchanged, pending destination is `S5-03` |
| A4 | the same X | `ev done X` | parent is `S5-03`, no pending move, history holds plan then done |
| A5 | active item X | `ev gone X` | exit 5 |
| A6 | active item X | `ev gone X --as trash` | state `gone`, disposition `trash`, history holds dispose then gone |
| A7 | candidate box B holding an active item | `ev gone B` | exit 5, payload lists the item |
| A8 | a gone node with code `S5-02` | `ev add Kutu --kind container --code S5-02 --in Kiler` | succeeds |
| A9 | an active node with code `S5-02` | the same command | exit 5 |
| A10 | any database | `ev add Kutu --kind container --code 12 --in Kiler` | exit 5 |
| A11 | room "Yatak odası" | `ev add "Giyinme odası" --kind room --in "Yatak odası"` | succeeds |
| A12 | furniture "Gardırop" | `ev add "Oda" --kind room --in Gardırop` | exit 5 |
| A13 | item X in drawer D | `ev lost X`, then `ev lost` | X is listed with last seen path ending in D |
| A14 | lost item X | `ev move X --to S5-01` | X is no longer lost |
| A15 | a batch whose third line breaks an invariant | `ev add --batch` | exit 5, no line of the batch was written |
| A16 | a database written by a newer schema | any command | exit 6, file unchanged |

## 10. Non-goals for 0.1.0

- A graphical or mobile app (Tauri is the expected route, later).
- Homebrew distribution.
- Copying or storing image files; only paths are kept.
- Sync between machines or multiple users.
- Usage-frequency tracking.
- Importing the existing Markdown records; the agent enters them through the CLI.
