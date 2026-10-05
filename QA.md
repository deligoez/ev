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
   uncommitted (`git -C ~/.ev status`). Note the guard: `sqlite3 -readonly ~/.ev/ev.db "SELECT
   count(*), max(id) FROM events"` (every write adds an event).
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

- **Places with labelled bins** — `ev progress`: a drawer whose bins carry another series'
  labels is one place; the counts look like the household's own idea of its places.
- **Counted without a word** — `ev ui` tree: a counted place is green with no word; "counted but
  changed since" on one whose contents changed after its tour; `ev tree` has `changed_since`.
  A lost thing found elsewhere does not mark the place it was last seen in as changed.
- **Empty box** — `ev empty <box> --note …`, then `ev show` (`empty`), `ev tree` (`empty: true`),
  `[boş]` in the tree and the badge in the details.
- **Fewer photos asked** — `ev todo`: no compartment whose drawers all have photos of their own.
- **Tree keys** — `d` opens two levels, `e`/`c` open or close everything below, `C` closes all
  but the home; the bottom edge lists the keys.
- **Every photo shown** — `ev photo add` puts the photo in the series with its note;
  `--no-show` keeps it off; `ev focus --file a.jpg=<note>`.
- **`f12`** — titles `f12/20 · <note>`; `ev focus --list` (`f`, `source`); `ev photo add <ref> f2
  f3`, `--stdin` lines; `ev photo cut f12 …`, `ev photo mark f12 …`, `ev focus f12`. A wrong
  `f` number or ref attaches nothing.
- **Series grid** — `g`, arrows, Enter, `+`/`-`, Home/End, `f12` Enter, a resize; `ev settings
  series_tile 40`; a series of 30–40 pictures stays quick.
- **Layout** — `ev layout <furniture>` and `--propose` on a toured piece of furniture: are the
  kinds sensible, the moves fewer than the things, the themes words a person would write?
  Nothing moved (`ev pending` unchanged).
- **Smaller fixes** — each line of "Smaller fixes" once.

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
