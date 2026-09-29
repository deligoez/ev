# ev

Agent-first home inventory. An AI agent records what a person reports at the shelves and
answers "where is it?". Homes, rooms, furniture, boxes and items live in one tree, with planned
moves, a give/sell/trash pipeline, lost items, errands for other households, placement
suggestions, photos with crops, and a full history. A read-only terminal UI follows every
change live.

## Install

```bash
brew install deligoez/tap/ev     # the `ev` binary (or: cargo install --path cli)
npx skills add -g deligoez/ev   # the agent skill (skills/ev); update with `npx skills update -g`
```

The database lives at `~/.ev/ev.db` (`--db` or `EV_DB` to change it), photos next to it in
`~/.ev/photos`. Output is JSON when piped and readable text on a terminal; errors go to stderr
with a distinct exit code (see `REFERENCE.md`).

## What it does

**One tree.** `home` › `room` (rooms nest) › `furniture` › `container` › `item`, and any node can
hold others. Codes are the physical labels (`K4x4-07-Ü`, `S5-01`) and compare case- and
diacritic-insensitively.

```bash
ev add Ev --kind home
ev add Salon --kind room --in Ev
ev add --stdin < box.ndjson      # a box and its contents in one all-or-nothing batch
ev find flipper                  # folded search over name, code, note, theme, tags
ev show K4x4-07-Ü                # one node with its path, children and photos
ev tree Salon --depth 2          # the picture, with item totals
ev edit 391 qty=11 note="…"      # change fields
ev recode A3=A4 A4=A3            # swap or rotate codes when boxes trade places
ev history 391                   # everything that happened to it
```

**Moves are planned, then confirmed.** `ev move X --to Y --plan` records the intention;
`ev pending` is the checklist; `ev done X` / `ev cancel X` once it happened or did not.

**Leaving the home.** `ev dispose X --as trash|give|sell` sets a thing aside; `ev disposals` shows
each pile; `ev gone X` (or `ev gone X --as trash --why "…"` in one step) records that it left. A
mistaken gone comes back with `ev restore X --correction "…"`. A record that should never have
existed (misread from a photo, entered twice) closes with `ev gone X --as mistake --why "…"`,
keeping its history without counting as thrown away.

**Lost and found.** `ev lost X` keeps where it was last seen; `ev lost` lists them; `ev found X`.

**Other households.** Places have aliases (`ev place add|alias|list|merge`). A node can be
meant for a place (`to=`), belong to one (`owner=`) or be lent out (`ev lend X --to P`,
`ev back X`). `ev for Mahmutlar` answers "what do I take, return and collect when I go there?".

**Where should this go?** `ev suggest "<what it is>"` lists every holder with its theme, fill and
contents, the placement rules (`ev rule add|list|remove`) and where alike things already are.
It enumerates everything instead of ranking a few, so nothing is missed. `ev audit` finds alike
things split across places (Turkish word forms grouped), holders without a theme, loose items
and holders whose contents were never inventoried (`unknown=true`).

**A plan for tidying up.** The order of work is data, not the agent's memory. `ev progress`
counts the places a person opens one at a time (the innermost labelled holders) as raw,
toured or kept as is, and flags toured ones that changed since. `ev observe` keeps what was
noticed about a place; `ev task` is an ordered work list where every entry says why it
matters; `ev next` hands over the current task with its places, what is planned to arrive
there and the places no task covers yet. `ev goal organize|track` says whether the household
wants a tidy-up at all — under `track` ev only keeps the records.

**Everything waiting, in one list.** `ev todo` gathers tasks, planned moves, errands, things
leaving, labels to print, things to buy or make, broken things, use-by dates, lost things,
uninventoried places, and places whose photo of the current state is missing or older than their
last change. What is already state on a record is read where it lives and leaves the
list by its own verb, so nothing is kept twice. The kinds that had no state get small marks:
`ev label` (a new or changed code needs its label printed), `ev need add|list|got|drop`,
`ev broken` / `ev fixed`, `ev expires <x> 2026-07`, `ev sale <x> --listed --price n`
for a thing being sold, and `ev photo current <x>` when the old photo still shows a place well
enough after a small change.

**Photos.** `ev photo add X photo.jpg` copies a photo into the store and attaches it;
`--crop x,y,w,h` attaches a cut-out of a drawer photo to each box in it, remembering the
original. A crop on a box is that box's current photo. `ev photo cut drawer.jpg --place 07-A
A3=0.1,0.3,0.3,0.1 B4=…` does a whole drawer in one step: the whole view on the drawer, a crop
on each box. A group photo goes whole on one place only: attaching it whole to a second node is
refused (`--whole` when that is really meant), and `ev todo` lists older slips.
`ev photo list|remove|adopt`.

**Watching.** `ev ui` is a read-only browser with tabs for the tree, pending moves, things
leaving, lost items, errands, search, everything waiting (one collapsible section per kind) and
settings. It refreshes the moment another process writes,
flashes what changed, shows photos inline (Ghostty's graphics protocol, half-blocks elsewhere),
full screen with `o` (`r` / `R` rotate it on screen), and in the system viewer with `O`. Mouse
works for tabs, rows, the wheel and photos. `ev focus X [--photo n]` points a running `ev ui`
at a node and shows its photo, so "which one do you mean?" is answered on screen.

**English and Turkish, light and dark.** `ev ui` and the readable terminal output speak English
or Turkish: the computer's language by default, or the one picked on the Settings tab or with
`ev settings language en|tr|auto`. The appearance follows the terminal's light or dark
background live — switch the system or terminal theme and `ev ui` changes palette without a
restart — or is fixed with `ev settings theme dark|light`. Settings live in
`~/.ev/settings.json`, outside the database; JSON output stays English.

## Documents

- `REFERENCE.md` — every command, field, key and payload shape
- `skills/ev/SKILL.md` — how an agent should use it in conversation
- `spec/0.1.0.md` — the decisions behind the model
- `release-notes/` — what changed in each version; `next.md` is the draft of the coming one
- `BUGS.md` — rough edges found in use, fixed in batches

## Development

Workspace: `core` (model, rules, SQLite, photos) and `cli` (the `ev` binary and its UI). A Tauri
app is expected to reuse `core` later.

Quality gate: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.

### Releasing

Releases are batched: bugs collect in `BUGS.md` and the draft notes in `release-notes/next.md`,
and a version is cut only when asked. Before tagging:

1. **Check this README against the release.** Every user-visible change in `next.md` must be
   reflected here and in `REFERENCE.md` and `skills/ev/SKILL.md`; a feature missing from the
   README is not shipped.
2. Rename `release-notes/next.md` to `release-notes/vX.Y.Z.md` and drop its draft line.
3. Bump `version` in `core/Cargo.toml` and `cli/Cargo.toml`, run the quality gate, commit.
4. Tag `vX.Y.Z` and push; the release workflow builds with GoReleaser, publishes the notes file
   and updates the Homebrew formula in `deligoez/homebrew-tap`.
