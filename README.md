# ev

Agent-first home inventory. An AI agent records what a person reports at the shelves and
answers "where is it?". Homes, rooms, furniture, boxes and items live in one tree, with planned
moves, a give/sell/trash pipeline, lost items, errands for other households, placement
suggestions, photos with crops, and a full history. A read-only terminal UI follows every
change live, and shows the agent's marked-up photos of what goes where the moment it sends
them.

## Install

```bash
brew install deligoez/tap/ev     # the `ev` binary (or: cargo install --path cli)
npx skills add -g deligoez/ev   # the agent skill (skills/ev); update with `npx skills update -g`
```

The database lives at `~/.ev/ev.db` (`--db` or `EV_DB` to change it), photos next to it in
`~/.ev/photos`. Output is JSON when piped and readable text on a terminal (`--json` / `--text`
to choose either way); errors go to stderr with a distinct exit code (see `REFERENCE.md`).

## What it does

**One tree.** `home` › `room` (rooms nest) › `furniture` › `container` › `item`, and any node can
hold others. Codes are the physical labels (`K4x4-07-Ü`, `S5-01`) and compare case- and
diacritic-insensitively.

```bash
ev add Ev --kind home
ev add Salon --kind room --in Ev
ev add --stdin < box.ndjson      # a box and its contents in one all-or-nothing batch
ev find flipper                  # folded search over name, code, note, theme, tags
ev find --tag "3d yazıcı"        # everything carrying a tag, no text needed
ev show K4x4-07-Ü                # one node with its path, children and photos
ev show #534                     # any command takes the #id ev ui shows
ev tree Salon --depth 2          # the picture, with item totals
ev edit 391 qty=11 note="…"      # change fields
ev edit --stdin < edits.ndjson   # many records at once, all or nothing: {"ref":…,"set":{…}}
ev recode A3=A4 A4=A3            # swap or rotate codes when boxes trade places
ev grid 07-A --cols 6 --rows 7   # a gridfinity drawer: row 1 at the back, columns A…
ev cell 07-A-A3=A3-B3 --recode   # a box covers cells; ev grid 07-A draws the map
ev history 391                   # everything that happened to it
ev history 07-A --contents       # and to a place: what came in, went out, was added
```

**Moves are planned, then confirmed.** `ev move X --to Y --plan` records the intention;
`ev pending` is the checklist; `ev done X` / `ev cancel X` once it happened or did not.

**Leaving the home.** `ev dispose X --as trash|give|sell` sets a thing aside; `ev disposals` shows
each pile; `ev gone X` (or `ev gone X --as trash --why "…"` in one step) records that it left. A
mistaken gone comes back with `ev restore X --correction "…"`. A record that should never have
existed (misread from a photo, entered twice) closes with `ev gone X --as mistake --why "…"`,
keeping its history without counting as thrown away.

**Lost and found.** `ev lost X` keeps where it was last seen; `ev lost` lists them; `ev found X`.

**Parked for now.** A place where things only wait until their places are decided is marked
`ev edit X temporary=true` (or one thing waiting among things that belong there). Placement
never offers a parking place, `ev todo` lists what waits there, and a move takes a thing's
mark with it — "for now" stays in the records instead of in someone's memory.

**One record per kind of thing, and what a kit still misses.** A set of parts recorded as one
thing becomes a record per part with `ev split X "LM393 kart=3" "Kablo=3" --rename "Prob"`;
the history links the pieces both ways, and the place's photo stays current (the same things
lie there, only recorded apart). A bought kit is a checklist: `ev kit add "Proje seti" --copies 2
--part "RC522 okuyucu" --part "Kablo=3"`, `ev kit link "Proje seti" 1 <record>…` as its parts
turn up, and `ev kit show "Proje seti"` counts each part found, lost and still missing, from
the records themselves: find or move one and the kit follows.

**Other households.** Places have aliases (`ev place add|alias|list|merge`). A node can be
meant for a place (`to=`), belong to one (`owner=`) or be lent out (`ev lend X --to P`,
`ev back X`). `ev for Mahmutlar` answers "what do I take, return and collect when I go there?".

**Where should this go?** `ev suggest "<what it is>"` ranks the holders by how well their
theme, name, note and contents match, and shows why: the words that matched, from where, and
how many points each. It says how much of the description the best holder covers and flags a
thing nothing here is like (`new_group_likely`), shows each holder's room from its fill, and
still lists every holder, the placement rules (`ev rule add|list|remove`) and synonyms
(`ev synonym add "fotosel, ldr"`). Turkish word forms meet (`kutuda` → `kutu`), part codes stay
whole (`KY-018`), and the same question always gets the same answer. `ev suggest --for <thing>`
places something already recorded by its own words.

**What could regroup?** `ev regroup <drawer>` asks the same question of every thing inside:
what would fit better in another box (and, apart, the guesses: things that share no word with
anything in their box, where the other box is only a hint), boxes that are mixed, full boxes
with a bigger spare box (tagged `boş kap`, with a `size`) and the cells it would fit in, nearly
empty boxes that could merge, and boxes whose fill is unknown or out of date. Noun compounds
(`hesap makinesi`, `kablo bağı`) and colour-noun pairs (`yeşil LED`) are matched as such.
Facets keep kinds of things apart when placing (`ev facet add modül --words "modül, kart"`, then
tag the holders): a buzzer module is never proposed for the bare-buzzer box, a novel never for
the technical shelf.
`ev themes` lists the places with things in them and no theme, with what a theme could be read
from: the words their contents share and the themed place they read most like (a theme is the
summary every placement answer leans on, so the agent writes one from this with the person).
`ev audit` finds alike things split across places, holders without a theme, loose items,
holders whose contents were never inventoried (`unknown=true`), and boxes whose name says a
size their `size` field does not.

**Gridfinity drawers.** A drawer can be a grid (`ev grid <drawer> --cols 6 --rows 7`, row 1 at
the back) and each box covers cells in it (`ev cell <box>=A3-B3`). `ev grid <drawer>` draws the
map and lists the free cells, `ev ui` shows it in the drawer's details, and `ev suggest` names
the free cells. Boxes trade places in one `ev cell` call, and `--recode` renames them after
their cells.

**A plan for tidying up.** The order of work is data, not the agent's memory. `ev progress`
counts the places a person opens one at a time (the innermost labelled holders) as raw,
toured or kept as is, and flags toured ones that changed since. `ev observe` keeps what was
noticed about a place (`ev unobserve` closes a note once it is dealt with, and the place's
history keeps what it said); `ev task` is an ordered work list where every entry says why it
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
refused (`--whole` when that is really meant), and `ev todo` lists older slips. One cut can
give the same record several crops (`ev photo cut sets.jpg 598=… 598=…`), one per set in the
photo. `ev photo list|remove|adopt`; a removed photo stays in the history.

**One drawer photo, every box cut from it.** For a drawer with a grid, `ev photo cut drawer.jpg
--place 07-A --grid 0.07,0.09,0.95,0.09,0.93,0.83,0.07,0.83` takes the grid's four corners in the
photo (back-left, back-right, front-right, front-left, as fractions) and cuts every placed box
through the photo's perspective, so no box keeps an older photo than its drawer; a taller box
(its `size`, `1x2x1.5`) gets a wider crop, as its rim leans out of its cells. `--preview`
cuts nothing: it frames every box it would cut on a copy of the photo, to check the corners by
eye first (and with a note, shows it in a running `ev ui`). Both the preview and the cut
return a contact sheet: every crop small, labelled with its cell, so a whole drawer's cut is
checked at a glance. The photo keeps its corners, so its
cells can be found by name later. Photos stay current by construction: `ev review <drawer> --as
toured` is refused while the drawer or any box in it shows an older state than it has, and
`ev todo` lists a drawer again when a box is added after its photo.

**Showing what goes where.** When the agent proposes where the parts on the table go,
`ev photo mark parts.jpg "1 → A6"=0.10,0.20,0.15,0.12 "2 → C1"=… --show "batch 3"` draws numbered
red frames on the parts in a copy of the photo, and `ev photo mark 07-A 1=A6 2=C1 --show "where"`
frames the target cells on the drawer's own photo, by cell name. `--show` puts them full screen
in a running `ev ui` at once; `ev focus --file a.jpg --file b.jpg --note "…"` sends several
together, stepped with `[` `]`. Esc closes them (a stray click does not), and `m` opens the last
ones again, even after a restart. The marked copies are scratch: never stored, never attached,
no history, cleared after a day. `ev focus X [--photo n]` does the same for a recorded node,
so "which one do you mean?" is answered on screen too.

**Watching.** `ev ui` is a read-only browser with tabs for the layout (the tree), pending moves,
things leaving, lost items, errands, search, everything waiting (one collapsible section per
kind) and settings. It refreshes the moment another process writes, flashes what changed, shows
photos inline (Ghostty's graphics protocol, half-blocks elsewhere) newest first with their note,
full screen with `o` (`r` / `R` rotate it on screen), and in the system viewer with `O`. Mouse
works for tabs, rows, the wheel and photos. Every node shows its `#id`, and every command takes
`#534` in place of a name or code. The key hints at the bottom show only the keys that do
something on the screen at hand, and fit the width, dropping the least useful first.

**The details pane.** Beside the tree, the selected node's details are split into tabs (`H`/`L`
or a click; a tab with nothing for the node is dimmed):

- **Summary** — the fields, labels aligned, with the fill as a bar (`▮▮▯▯`) and the room left.
- **Photos** — every photo, newest first, with when it was added, crop or whole, and its note.
- **Grid** — a drawer drawn as its plate, each box a frame over its cells; a box shows its place
  on its drawer's plate.
- **Contents**, and **Suggestions** — what `ev regroup` proposes there, guesses marked as such.
- **History** — newest first by day; a place's includes what came in, went out and was added
  (`ev history X --contents` prints the same).

A click on a line opens what it names — a photo, a thing inside, a thing in the history — and a
click on a box in any grid opens that box with the Grid tab still showing, so a drawer can be
walked box by box. The pane scrolls (`J`/`K`, the wheel). Drag the divider between the list and
the details, or under the photo, to resize (`<` `>` `{` `}` from the keyboard, a double click
resets); the sizes and the tab are kept.

**English and Turkish, light and dark.** `ev ui` and the readable terminal output speak English
or Turkish: the computer's language by default, or the one picked on the Settings tab or with
`ev settings language en|tr|auto`. The appearance follows the terminal's light or dark
background live — switch the system or terminal theme and `ev ui` changes palette without a
restart — or is fixed with `ev settings theme dark|light`. `ev ui` reopens on the node that was
selected in the tree when it last closed (`ev settings resume off` starts at the top). Settings
live in `~/.ev/settings.json`, outside the database; JSON output stays English.

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
The tests include one that reads every backticked `ev …` command in this README,
`REFERENCE.md`, the skill and `release-notes/next.md`, and fails on a subcommand or flag the CLI
does not have, so the documents cannot drift from the binary. `tools/measure/` scores stems and
placement against answer keys kept outside the repository.

### Releasing

Releases are batched: bugs collect in `BUGS.md` and the draft notes in `release-notes/next.md`,
and a version is cut only when asked. Before tagging:

1. **Check this README against the release.** Every user-visible change in `next.md` must be
   reflected here and in `REFERENCE.md` and `skills/ev/SKILL.md`; a feature missing from the
   README is not shipped.
2. Rename `release-notes/next.md` to `release-notes/vX.Y.Z.md` and drop its draft line.
3. Bump `version` in `core/Cargo.toml` and `cli/Cargo.toml`, run the quality gate, commit.
4. Tag `vX.Y.Z` and push. The release workflow is [dist](https://opensource.axo.dev/cargo-dist/)'s
   (`dist-workspace.toml`; regenerate `.github/workflows/release.yml` with `dist generate` after
   changing it): every target builds natively on its own runner in parallel (macOS on macOS,
   Linux on x86 and ARM Linux), then the GitHub release gets the archives, `release-notes.yml`
   puts `release-notes/vX.Y.Z.md` on it, and the formula in `deligoez/homebrew-tap` is updated.
   A tag with a pre-release suffix (`v0.12.0-rc.1`) makes a pre-release and leaves the tap
   alone, which is how a change to the pipeline is tried.
