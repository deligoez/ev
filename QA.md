# QA rounds

A QA round runs every feature against a copy of a real inventory, the new ones first, the way
the person and the inventory agent use them. The gate's tests check what was thought of; a
round finds what a household's data and an agent's habits do to it. Run one before a release,
and a short one for a feature as it lands. A round can stop and pick up later: each area below
stands on its own.

## Data safety: never the real inventory

The real inventory is never written by a round, not even "only a read" through a command that
might write (`ev ui` keeps state, `ev settings` writes, `ev focus` writes a series file).

1. **Before:** the inventory agent has committed its last change and `~/.ev` has nothing
   uncommitted (`git -C ~/.ev status`). Note the guard: `sqlite3 ~/.ev/ev.db "SELECT count(*),
   max(id) FROM events"` (every write adds an event). A SELECT changes nothing; `-readonly`
   is no safer and fails on a database in write-ahead-log mode when no `ev` has it open.
2. **Sandbox:** `tools/qa/sandbox.sh` copies the database (SQLite backup), photos, documents and
   settings into a new temporary directory and prints `EV_DB=… EV_CONFIG=…`.
3. **Every command** of the round carries both, in front (`EV_DB=… EV_CONFIG=… ev …`), or
   `--db` (settings then still need `EV_CONFIG`). An agent's shell starts fresh each call, so an
   exported variable does not carry over. `ev ui` and `ev mcp` take the same variables.
4. **After:** the guard reads the same, and `~/.ev` is still clean. A difference stops the round:
   find what wrote before anything else.
5. Shop exports under `~/.ev/purchases/` are only read; an adapter's output goes into the
   sandbox, never into the real database.

## Who does what

- **The development agent** runs the CLI and MCP checks below on the sandbox, compares output
  with REFERENCE.md, and fixes what it finds (each fix with its test, as always).
- **The inventory agent** works scenarios on the sandbox with the skill, as on a normal day: it
  finds friction a checklist does not (a missing verb, a confusing answer, a rule the skill
  forgets to say). It writes findings into BUGS.md, as always.
- **The person** checks `ev ui` by eye (images, layout, keys) on the sandbox:
  `EV_DB=… EV_CONFIG=… ev ui`.

## Findings

Every finding goes into BUGS.md as usual: what was run, what happened, what was expected. This
repository is public: name things with invented data (`#12 Kablo`, `K2-01-A`), never the
household's names, codes or prices. What the sandbox printed stays in the sandbox, which is
deleted at the end of the round.

## The checklist

Each line: the command or key, then what to look for. **New** marks what changed since the
last release (see `release-notes/next.md`); start there.

### New since the last release

- **Past belongings** — `ev add "<name>" --gone <how> --at 2019-05 --came 2016 --where "<place>"`
  (and batch lines with `gone`/`at`/`came`/`where`); `--gone` with no `--at` is "when not known";
  `ev gone <x> --at … --where …`; `--as left|stolen|unknown`; `ev edit #id name=… qty=… came=…` on
  a gone record (tags refused); `ev doc add … --for #id` and `ev photo add #id …` on a gone
  record; `ev buy link` / `ev buy add --for` on one. Read back in the past tense everywhere.
- **Sales and swaps** — `ev sold <x> --price n [--currency] [--via]`, said again corrects it;
  a listed sale carries `via`, not the asking price; `--as trade --traded-for <y>`, `ev traded
  <x> --for <y>` (also from `give`), `traded_from` on what came.
- **`ev past`** — two lists, remembered first, each with years, "when not known" and money;
  `--name`, `--where`, `--year Y` (owned that year; `unknown` apart); `ev stats` past section;
  the Gidenler tab on `0` (both headings fold, years under them, details show came/left).
- **Counting follows the work** — `ev task start` marks nothing; an add, move, photo or edit
  inside a place of the task in progress marks it `counting`; nothing outside a task does;
  closing a task resets nothing. `ev review --as toured|kept` gives `left_here`; `ev next`
  `left_nearby`; `ev progress <furniture|room>` with each place's `tasks`; a task on a cabinet
  plans its drawers.
- **Final photo check** — `ev review --as toured` gives `photo_check` (`located`,
  `not_located`); nothing refused over it.
- **What is never a thing** — `ev buy add --bucket digital|service`; neither in `ev buy list
  --open`; `ev stats` `purchases.buckets`.
- **A warranty bought as its own line** — `ev cover add … --purchase <line>`, `ev cover purchase
  <c> <line>` / `--clear`: the line settles, the premium comes from it, `ev todo` counts it no
  more; a kit-settled line is no longer counted open either.
- **Schema 35** — an older copy migrates on open, and the migration reaches `ev.db` itself even
  with `ev ui` open (`ev.db-wal` empty after).

### Every area, briefly

| Area | Check |
|---|---|
| Records | `ev add --stdin` (a box and contents, `@key`), a bad line refuses all; `ev edit`, `ev edit --stdin`; `ev recode` a swap |
| Moves | `ev move --plan`, `ev pending`, `ev done`, `ev cancel`; several at once; `--qty` |
| Leaving | `ev dispose --as …`, `ev disposals`, `ev gone` (with `--as`, `--qty`, `--why`), `ev restore --correction`, `--as digitize` refused without a copy |
| Lost | `ev lost`, `ev found` (back, `--in`), `waits_for` listed on `found` |
| Several places | `ev move --qty`, `ev add --of`, `ev join` / `ev unjoin`, the account in `ev show` |
| Parts and kits | `ev split` (`--take`, `--rename`), `ev kit add/link/show/purchase/part/unlink` |
| Placing | `ev suggest` (`--for`, `new_group_likely`, `empty`, `parking`), `ev regroup` (`--decline`), `ev themes`, `ev audit`, `ev layout` |
| Grids and map | `ev grid`, `ev cell` (a swap), `ev map`, `ev sketch`; `M` in `ev ui` |
| Photos | `ev photo add/cut/mark/rotate/remove/current/stale`, `--grid` with `--preview`, the sheet and the marked copy; the series in `ev ui` (`[` `]`, `m`, `X`, Esc) |
| Plan | `ev next`, `ev todo`, `ev progress`, `ev task` (add, start, done, drop, reopen, `--due`), `ev observe` / `ev unobserve`, `ev review` (`toured` refused on an old photo) |
| Purchases | an adapter into the sandbox (`ev buy import --stdin`), `ev buy for --toured`, `link`, `unlink`, `decline`, `dismiss`, `pack`, `bring` |
| Papers and money | `ev doc add`, `ev cover add`, `ev value`, `ev link add`, `ev track`, `ev money status` |
| Errands | `ev for <place>`, `ev lend` / `ev back`, `ev place alias/merge` |
| Marks | `ev label`, `ev need`, `ev broken` / `ev fixed`, `ev expires`, `ev sale` |
| Numbers | `ev stats`; tab `9` in `ev ui` |
| Output | `--text` and `--json` of each command above; exit codes 2–5 on a bad ref, an ambiguous name, a refusal |
| MCP | `ev mcp` from a client: the `ev` tool, the read-only tools, a photo returned as an image, a refusal as `isError` |
| UI | live refresh while another shell writes, photos inline, the details tabs, mouse, resize, `ev settings language` / `theme`, reopening where it was left |

## Teardown

Close `ev ui` on the sandbox, delete the sandbox directory, and read the guard once more.
