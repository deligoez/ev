# ev reference

## Node fields

| Field | Set with | Notes |
|---|---|---|
| name | `add <name>`, `edit name=` | required |
| kind | `--kind`, `edit kind=` | home, room, furniture, container, item, vehicle (a car: at the top beside the homes, a holder of its compartments with a thing's life: make, model with its year, serial = VIN, code = plate, purchases, coverage, values, `ev gone`/`ev sold`/`ev past`; never lost, a stolen one is gone `--as stolen`; its plate needs no label; placement keeps it apart from the home, and `ev stats` counts it apart; spec/vehicles-homes.md) |
| parent | `--in`, `move` | not editable; rooms only in homes/rooms; homes and vehicles only at the top |
| code | `--code`, `edit code=`, `recode` | unique among non-gone nodes, folded (case, diacritics, `_` as `-`, a number's leading zeros dropped: `S03_12` is `S3-12`, spec/codes.md), and found the same way; kept as printed; not digits only; `code=` clears; a code ending in `*` takes the next free number of its series (`GF1x1-*` after `GF1x1-007` is `GF1x1-008`, padded like the series' widest printed number, 3 digits for a new one; `S3_*` and `S5-*` are one series, gone nodes' numbers never reused) in `add`, `edit` and `edit --stdin`; `ev recode A=X B=Y …` sets several at once, checking uniqueness against the codes they end up with (swap or rotate codes when boxes change places) |
| address | `--address`, `edit address=` | homes only |
| qty | `--qty`, `edit qty=` | ≥ 1; empty clears. `--qty n` on `move`, `lend`, `dispose`, `gone` and `lost` acts on n of an item's units (see **One thing in several places**) |
| note, theme | `--note`, `--theme`, `edit note=` | empty clears; `note=+text` appends on a new line |
| make, model, serial | `--make`, `--model`, `--serial`, `edit make=` | what the thing is beyond its name, as on its label (`make=Bosch model=GSB 13 RE`); searched by `ev find` as strongly as a code; empty clears |
| fill | `--fill`, `edit fill=` | 0–100 estimate |
| size | `--size`, `edit size=` | `WxDxH` or `WxD` in grid units, e.g. `1x2x0.5` (`×` and a decimal comma accepted); empty clears. What `regroup` compares when it offers a bigger spare box |
| tags | `--tag` (repeatable), `edit tags=+x` / `tags=-x` | stored lowercase |
| photos | `--photo` (repeatable), `edit photos=+p` / `photos=-p` | a file in the store (`photos/`) is kept relative to the database and output as a full path; one outside it stays absolute until `ev photo adopt` |
| to | `--to`, `edit to=` | place the node should be taken to; empty clears |
| owner | `--owner`, `edit owner=` | place the node belongs to when it is not ours |
| with | `lend --to`, `back`, `edit with=` | place holding our lent node |
| state | `dispose`, `restore`, `gone` | active, candidate, gone; dispositions trash, digitize, give, sell, return, mistake, used (used up), left (left behind), stolen and unknown (sold or thrown out, not sure) (the last four `gone` only) and merged (ev's own, for a portion that joined another); `--shred` (trash, digitize) marks it `shred` |
| came | `--came`, `edit came=` | when it came, as remembered: `2014`, `2014-03` or `2014-03-08`; empty clears. When not given, a linked purchase's order date stands for it (see **Past belongings**) |
| thing | `move --qty`, `add --of`, `join`, `unjoin` | the thing a record is a portion of, when it is kept in several places: the id of the thing's first record; not set by hand (see **One thing in several places**) |
| lost | `--lost`, `lost`, `found [--in]`, any move | its place is not known: out of where it was last seen (kept as the parent), listed under "Unknown place" in `ev tree` and `ev ui`, not counted in that place's `items`; a thing added with `--lost` and no place was never seen |
| temporary | `--temporary`, `edit temporary=true/false` | a parking place: what is put straight into it waits for its final place (`ev todo` lists it as `parked`, `ev suggest` never offers the place or anything inside it, listing them under `parking`); on an item, that one thing waits where it is. A move clears an item's own mark (the event says `was_temporary`); a place keeps its mark until set back |
| waits_for | `edit waits_for=<ref>` (`waits_for=` clears) | the record this one's place waits for (spec/waits-for.md): glue sticks parked until the lost glue gun turns up. Not itself or anything inside it. `ev show` gives `waits_for` (NodeRef) and, on the awaited one, `waited_for_by`; `ev found` of the awaited one adds `waiting` (what waited for it, to settle now; nothing moves on its own); `ev todo` lists it under `parked` (`why: waits_for` when it is not parked otherwise), with `waits_for`. A move of the waiting thing ends the wait (the event says `waited_for`) |

## Batch lines (`ev add --batch file` / `--stdin`)

One JSON object per line with the fields above (`name`, `kind`, `in`, `lost`, `code`,
`address`, `qty`, `note`, `theme`, `fill`, `size`, `tags`, `photos`, `to`, `owner`,
`temporary`, `make`, `model`, `serial`, `came`, and for a past thing `gone`, `at`, `where`,
`traded_for`; `gone` with `of` exits 2)
plus optional `key`, and `of`: more of a thing already
recorded (`{"of": "#647", "qty": 4, "in": "K4x4-13-Ü"}`; name and kind come from it).
`"in": "@key"` points at an earlier line. `name` and `kind` are required (a past thing's `kind`
is `item` unless given). Unknown fields are rejected. All or nothing.

## Edit lines (`ev edit --stdin`)

One JSON object per line: `{"ref": "#551", "set": {"size": "1x2x1.5", "tags": ["+modül", "-boş kap"], "note": null}}`.
`ref` is a name, code, id or `#id`; each `set` entry is one `field=value` of `ev edit` (an
array is one per item, `null` clears; in `tags` and `photos` an item without `+` or `-` is
added: `["modül"]` is `["+modül"]`). Blank lines are skipped. All or nothing: a failing line
is named (`line 2: …`) and nothing is changed. Each record gets one `edit` event, a field set
several times showing its value before the first and after the last. Output: `edited`, each a
NodeRef with its `changed` as `ev edit` gives it; a line
that set `make` or `model` carries `purchase_candidates`, as a single `ev edit` does.

## Output

JSON when piped, readable text on a terminal. `--json` forces JSON on a terminal; `--text`
forces the text through a pipe — to read a result, not to parse it. JSON is one line, here and
over MCP (pipe it through `jq` to read it). Below a payload's top-level keys, a field with no
value is left out: where this reference says a field is null, it is missing, which reads as null
(`jq '.x'`, `d.get("x")`). A reader that stops early (`| head`) is no error.

A record that is not a node (a purchase line, a task, a document, an observation, a value) is
named by its id, as ev prints it or bare: `ev task done '#21'` and `ev task done 21` are the
same. In a shell, quote it: an unquoted `#` starts a comment.

## Storage

One SQLite file, `ev.db`, with `photos/` and `docs/` beside it. The database keeps the files in
those two as paths relative to its own directory (`photos/<hash>.jpg`), so the directory can
move or be copied whole; a photo still outside the store (not adopted) keeps its absolute path.
Output always gives a file's full path. It runs in write-ahead-log mode:
readers (`ev ui`, an agent's reads) and the writer do not wait for each other, and writers queue
for up to 30 seconds instead of failing. While a process has the file open, `ev.db-wal` and
`ev.db-shm` sit beside it; keep them out of version control and never delete them by hand, and
keep `ev.db-focus.json` (the last `ev focus` request) out too. After
every command that writes, ev folds the log back into `ev.db`, so the file alone holds every
change: it is what a data repository commits. Back it up with
`sqlite3 ev.db ".backup '<target>'"`, which copies a consistent whole even while another
command is writing. A git `textconv` that dumps `ev.db` for diffs must open the file as
immutable (`sh -c 'sqlite3 "file:$0?immutable=1" .dump'`): git hands it a temporary copy with
no log beside it, which `sqlite3 -readonly` cannot open in this mode.

## MCP (`ev mcp`)

An MCP server over stdio on the same inventory (`--db` or `EV_DB` fixes it for the whole
server). Each call parses its arguments with the CLI's own parser and runs the CLI's own
dispatcher, opening and closing the inventory like a CLI call, so other ev processes can use the
file at the same time. Only MCP messages reach stdout. The text output follows `ev settings
language`. See `spec/mcp.md` for the reasons.

| Tool | Arguments | Same as | Annotations |
|---|---|---|---|
| `ev` | `args: [string]`, `input?: string`, `format?` | `ev <args…>`, `input` as its stdin | destructive |
| `next` | `format?` | `ev next` | read-only |
| `todo` | `only?` (sections, comma-separated), `format?` | `ev todo [--only …]` | read-only |
| `find` | `text?`, `tag?`, `kind?`, `include_gone?`, `empty?`, `format?` | `ev find` | read-only |
| `show` | `ref`, `include_gone?`, `format?` | `ev show` | read-only |
| `suggest` | `text?` or `for?`, `format?` | `ev suggest` | read-only |
| `history` | `ref`, `contents?`, `format?` | `ev history` (with `names`: the records, places and purchase lines its events name by id, `{"#12": "K4x4-07 Kutu", "p42": "…"}`) | read-only |
| `tree` | `ref?`, `depth?`, `format?` | `ev tree` | read-only |
| `photo` | `ref`, `n?` | a node's n-th photo (the newest by default) as an image | read-only |

- `format`: `text` (default) is the readable output with `#id`s; `json` is the JSON as text
  and as `structuredContent`. A text result over 50,000 characters is cut there, with a note on
  how to narrow the call; a JSON result that long is refused with the same advice, since cut
  JSON does not parse.
- A failed command is a result with `isError: true` and the CLI's error message (in JSON with
  `format: json`). Refused through `ev`: `ui`, `mcp`, and any `--db`. A command reading
  `--stdin` with no `input` is refused (the server's own stdin is the protocol).
- `["<command>", "--help"]` returns the help text as the result.
- Images: `photo` returns a text line and the photo as JPEG, its long side at most 1,568 px. A
  result carrying `marked` (`photo cut`, `photo mark`) or `sheet` gets those pictures after its
  text, the same size.
- Instructions: at most 2,048 characters (Claude Code keeps no more). Prompt `ev`: the skill as
  one user message. Resources `ev://skill` and `ev://reference` (`text/markdown`): the skill and
  this reference, compiled into the binary.
## Payload shapes

Every node reference (`NodeRef`), a row in any list, is:

```json
{"id": 5, "code": "FZ-1", "name": "Flipper Zero", "kind": "item", "state": "active",
 "path_text": "Ev › Salon › K4x4 › K4x4-15-A › FZ-1"}
```

`lost` (true), `disposition` and `qty` appear when set. `path_text` names each place from the
home down by its code when it has one, its name otherwise. The node a payload is about (`node`
in `show` and the commands that answer with it) also carries `path`: `[{id, code, name}]`
from the home down, the ancestors' ids to step up with.

| Command | Top-level keys |
|---|---|
| edit | `node` (NodeRef, as it is now) and `changed`: `{<field>: {before, after}}`, each field the edit changed with its value before the first and after the last assignment (a missing `before` or `after` was empty); `{}` when nothing changed. `ev show` has the rest. A make or model just set adds `purchase_candidates` |
| show, add, move, done, cancel, dispose, restore, gone, lost `<ref>`, found | `node` (all fields + `path`, `path_text`; `lost` and `temporary` only when true), `children`, `pending`, `last_seen`; `show` also `cells`, `grid`, `parent_grid` (the grid a placed box stands in), `room` (with a fill: `room`, `fill`, `fill_at`, `stale`), `kits` (the kit parts it is: `[{kit, n, text}]`), `documents` (see Documents), `purchases` (see Purchases), `coverages`, `coverage_proposal` and `tracking` (see Coverage), `thing` (a portion of a thing kept in several places, see **One thing in several places**). In JSON a section with nothing in it is left out: `node` is always there, `children` only when it has some |
| split | `node` (the original, after), `into` (the records split off), `photos` (the original's, to crop each part from) |
| add --batch | `created` |
| find | `query`, `results`, best first; every word of the text must match name, code, note, theme or tags in any order, by its Turkish stem or a synonym group too, and a word that matches nothing is retried allowing a typo (a typo of a whole word ranks above one that only matches the start of another); the text may be left out with `--tag`, `--kind` or `--empty` to list every match of the filter (`ev find --tag "3d yazıcı"`). `--empty` keeps only the containers no live record is in and known to be empty: their place was toured, something was once recorded in them and left, or the person said so (`ev empty`); each carries `slot` (a slot of furniture, not a box that moves), and those with nothing recorded only because they were never counted come apart under `not_known`. Worked out from the records, so no "empty" tag has to be kept (`ev find --empty`). A portion of a thing kept in several places carries `thing`: `{id, total, places, in_use, spare}`; the text shows a thing's portions together under one line |
| tree | `tree` (nested, each with `children` (rooms, furniture, containers, then things; within each the coded ones by their code read naturally, `S5-2` before `S5-10`, then the rest by name; `ev show`'s `children` the same), and `theme`, `fill`, `size`, `tags`, `lost` (true) when set; `count` on a place gone through on its own: `raw`, `counting`, `toured` or `kept`, and `changed_since` (true) on a toured one whose contents changed after its tour; `empty` (true) on a box nothing is in, counted on its own or with its place (`ev empty` counts it too); lost things are not among the children or in `items`), `lost` (without a reference: every lost thing, with `last_seen`, null when never seen) |
| recode | `recoded`: `[{id, name, before, after}]` |
| pending | `pending`: `[{node, to}]` |
| disposals | `disposals`: `{trash, digitize, give, sell, trade, return}` |
| lost | `lost`: `[{node, last_seen}]` |
| history | `node`, `events`: `[{at, type, data}]`; with `--contents` also the events of things that came in, went out (`move`, `done`, `plan` to or from it) or were added there (`create`), each with `item` (NodeRef) and `relation`: `in` \| `out` \| `added` |

Errors print nothing on stdout; stderr carries
`{"error": {"code", "kind", "message", "id"?, "values"?, "at"?, "candidates"?, "details"?}}` in
JSON mode, a mistyped argument too (`kind: usage`, exit 2, the message with the command's usage
line). `--help` and `--version` stay text, on stdout. An error with an `id` (spec/error-ids.md)
names it in stable snake_case, its `values` by name, and `at`, where it happened, outermost
first (`[{"line": 2}]` for a batch line; `{"of": "f3"}` for what else it is about); `message`
stays the English sentence. Every error but an internal one (exit 1) carries an id, apart from
three whose message is another's (a JSON line that does not parse in `ev sketch --stdin`, a
picture or a file that cannot be read) and the composite "not toured yet" of `ev review`.
The ids and their sentences are `ERRORS` in `core/src/errors.rs`; the ones met most:
`no_record_matches` (`ref`), `no_record_with_id` (`id`), `ref_matches_several` (`ref`,
`count`, with `candidates`), `record_gone` (`id`), `record_joined` (`id`, `into`). The text
output words an error with an id in the reader's language.

## Event types

create, edit, move, plan, done, cancel, dispose, restore, gone, lost, found (`at` where it was
last seen, or `from` and `to` when it turned up elsewhere), back, photo,
photo_remove (`path`, `crop`, `note`, `n`: what was detached), photo_rotate (`from`, `to`, `degrees`), grid, cell, observe, unobserve,
review, sold (`price`, `currency`, `at`, `via`), split (`into`: the records split off) and split_from (`from`, `name`), kit_link and
kit_unlink (`kit`, `part`, `text`), sketch (`before`, `after`: `{x, y, w, d, on}` or null), grid_face (`before`, `after`), decline (`holder`, `why`) and decline_cleared, doc_linked and doc_unlinked (`document`, `kind`), purchase_linked (`purchase`, `qty`) and purchase_unlinked (`purchase`), coverage_added and coverage_removed (`coverage`, `kind`), track (`subject`, `decision`, `why`); for a thing kept in several places portion_out (`qty`, `to`) and portion_in (`qty`, `from`), merged (`into`, `qty`) and joined (`from`, `qty`), more_of (`of`), join (`thing`) and unjoin (`thing`), and an `edit` reaching a portion through another (`via`, `fields`).

## `ev ui`

A read-only terminal browser. It never writes the database; it polls SQLite's `data_version`
every half second and re-reads when another process has written, highlighting the nodes that
changed and expanding their parents so they are in view. The files it writes are the display
settings file, from its Settings list, and on exit the tree state it reopens with.

Three panes (spec/ui-sidebar.md): the sidebar of lists, the list, the details. The sidebar is
shown in full from 120 columns, as a rail of digits and counts from 90, hidden below that, and
below 70 only the focused pane is shown. Its lists, under their headings, with their digits:
HOME — layout `1` (the tree of places and things), To do `2` (everything waiting), pending moves
`3`, leaving `4`, lost `5`, errands `6` (take / return); PURCHASES — all `9`, then durable,
clothing, digital and service (`ev buy list`, newest first; the lines of one order under a heading
with its total, which opens and closes; the title counts the lines shown and adds them up per
currency; `f` steps through all, open, linked and dismissed, `/` narrows to the lines with every
word typed in their name, shop, make, order number or account, as they are typed, and Esc clears
both; the details are `ev buy show` of the line; Enter opens the thing it is linked to, still
here, in the tree); HISTORY — the past `7` (`ev past`: a
collapsible heading per year with how many left and the money paid and got, each thing under it;
its details say when it came and how it left); INSIGHT — statistics `8` (`ev stats`, a
collapsible section per heading; a line that names a record opens it; a figure marked `›` opens
the list it counts: the purchase lines, all of them or a year's or a shop's, the To do list for
the counting, the past for what left; Esc comes straight back); then search `/` and
settings `0`. A list's count is its own length: pending, leaving, lost, errands, past, the
purchase lists (every line of the bucket, whatever the filter) and a search's results show one,
the others none. The top line names the section and list. A
record that leaves the list it is selected in (made, found, gone, from another process) hands
the selection to its neighbour, and the status line names it (`#12 left this list`).

| Key | Action |
|---|---|
| ↑ ↓ / j k, PgUp PgDn, g G | move |
| → / l / Enter | expand in the tree; in a list, jump to the node in the tree |
| ← / h | collapse, or go to the parent |
| d | in the tree: open the selected node two levels down, the nodes in it opened and nothing further (a Kallax shows its compartments and the drawers in each); the selection stays |
| e / c | in the tree: open the selected node and everything below it, or close them all; the selection stays |
| C | in the tree: close everything but the home, so its rooms show closed; the selection moves up to what still shows |
| 1–9, 0 | open that list (see above) |
| Tab, Shift-Tab | the next / previous pane: sidebar, list, details; a hidden sidebar is passed by. The focused pane's border is coloured |
| in the sidebar: ↑ ↓ / j k, g G, Enter / → / l, Esc | open the previous / next list (the first / last) as the selection moves; go into the list |
| in the details: j k, h l, Esc | scroll; the previous / next details tab; back to the list |
| b | hide the sidebar, or show it (over the list below 90 columns, closing again when a list is chosen); kept in `ui-state.json` |
| / | search (same folding as `ev find`), Enter to run; Esc clears the typed text, then closes the box; Ctrl-U clears |
| x / Esc on the search list, or click its title | clear the search and its results |
| click / double click | select / expand, collapse or jump; wheel scrolls; click a list in the sidebar to open it |
| [ / ], wheel over the photo | previous / next picture of the selected node: its photos, then its product images (`o` full screen, `O` outside) |
| H / L, click a details tab title | the previous / next details tab the node has something for; only those tabs are shown. Above every tab: the `#id` and name, then the place it is in. Summary: its state as badges (kind, count, set aside, lost, broken, on sale, label to print), its identity fields, then sections — Money (each purchase in one line: date, shop, quantity, price, today's money; the shop's own title dimmed under it; the current value), Coverage (status in colour, a proposal dimmed), Documents and links (counted, by kind), To do (its own tasks and needs; those of the places it is in only counted), Note — long values wrapped under their own column. Photos (every photo newest first with when it was added, crop or whole and its note; then, under their own heading, the product images that came with its purchases, never counted as a photo; the tab counts the two apart, `2+3`; the one shown is marked), Documents (its documents but the product images, also through its purchases, and its links with its purchases' order and product pages), Grid (the drawer's plate, a placed box framed), Contents, Suggestions (`ev regroup`, and theme words for an untitled place), History (`ev history --contents`, newest first by day, every event in words). The choice is kept |
| Documents tab: [ / ], O / o, click | pick a document or link, open it in the program the system gives it (`open`, `xdg-open`) |
| E | the Summary also lists make, model and serial still empty, as “—” |
| y | copy what is picked on the Documents tab (a file's path, an address), else the thing's `#id` and name |
| p | a thing kept in several places: open its next place in the tree (the Summary lists them all, under its total) |
| + | widen the details as far as the list allows, and back |
| click a details line | Photos: show that photo; Documents: open it; Contents and History: open that thing in the tree; a drawer's own Grid: open the box clicked (a box's view of its drawer does not react, so a stray click stays put). These tabs cut long lines with … instead of wrapping |
| J / K, wheel over the details | scroll the details |
| drag a divider; < >, { } | resize: the list against the right side (20–80%), the photo against the details (15–85%); a double click on a divider resets it. Kept in `ui-state.json` with the details tab |
| o, click on the photo | the current photo full screen, titled with the node and the photo's note; `[` `]` ← → step, `r` / `R` rotate 90° clockwise / counter-clockwise (on screen only, kept per photo for the session; also on the photo panel), Esc / o / click close |
| O | open the current photo in the system viewer |
| m | the marked photo series again, after Esc hid it |
| g | in the marked photo series: a grid of all its pictures ↔ one at a time. The grid fits as many a row as the width holds at the picture width (`ev settings series_tile`), re-flowed on resize; each tile reads `f12 · <note>` and `▣n` for its numbered frames; arrows move, `Enter` or a click opens the one selected (or clicked), the wheel moves a row, `+` / `-` widen or narrow the pictures for now (kept in `ui-state.json` until the setting changes) |
| f then digits, Enter | in the marked photo series: go to that picture (`f12`); `Home` / `End` go to the first and the last |
| X | close the marked photo series: it leaves the screen, and what the agent sends next starts a new one numbered from 1 |
| M | the map (see **Maps**) full screen, from the home: its rooms first, the room on the way to the selected node chosen, and on every level Enter leads further down that way. ← ↑ ↓ → move to the nearest tile that way (Tab steps in reading order), Enter goes into the tile, Backspace / u goes up a level with the place left chosen, t closes the map on the chosen tile in the tree; a click chooses a tile and a second click goes in; Esc / q / M close. A tile shows its label, its theme (or name), how many things it holds, its fill, and what is in it as far as it has room; a place on a grid (a drawer, a Kallax) is drawn as its plate, column letters above and row numbers beside, a dot on every free cell; it follows the data as it changes |
| Settings tab: Enter / → / Space, ← | next / previous option of the selected setting; saved at once and applied to the whole screen |
| : | go to: a palette that finds a list, or a record by code, name or `#id` (Turkish letters or not: `kayip` finds Kayıp); ↑ ↓ choose, Enter opens (a record in the tree), Esc closes |
| Esc | back to the list and record before the last jump (a digit, a click in the sidebar, `:`, Enter on a list's line, a click on a details line or the grid, `t` on the map); the top line names it. With nothing behind, Esc quits (it clears a search first) |
| q | quit |

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
| `ev settings series_tile <cells>` | the width of a picture in the grid of the marked photo series (`g` in `ev ui`), in terminal cells, 12 to 400, 28 by default: the number a row follows from the width |
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
| `ev grid <ref>` | `node`, `grid`: `cols`, `rows`, `boxes` (NodeRef + `cells`; the text lists each by its cells, code and name), `free` (cell names), `unplaced` (children without cells), `map` (rows of box ids, null where free); `grid` is null without one |
| `ev grid <ref>… --cols N --rows M` | set the size (1–26 × 1–99); refused (exit 5) while a placed box would fall outside. Several references get the same grid, all or none, and return `grids` |
| `ev grid <ref>… --face above\|front` | how the grid is seen: `above` (the default; a drawer, row 1 at the back) or `front` (furniture and its compartments, row 1 at the top); with or without `--cols`/`--rows`, all or none; refused (exit 5) for a holder without a grid. `grid.face` |
| `ev grid <ref> --clear` | remove the grid; refused while boxes are placed in it |
| `ev cell <ref>=<cells>…` | place boxes, and things, in their holder's grid, several at once; `<ref>=` takes one out. Bounds and overlaps are checked against where everything ends up, so boxes can swap places in one step. A box (anything but an item) keeps its cells to itself; things (items) share a cell with other things, as several kinds lie in one compartment of a case. A box keeps its code: it is the box's serial label, not its place. `placed`, `grids` |

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
| `ev sketch <ref> --at x,y` | its top-left corner in its holder, seen from above; a piece of furniture or a box that would lie outside a holder of known size (a centimetre of slack) exits 5, a room may reach out of the one it hangs on (a balcony). Measures are kept to the millimetre |
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
| `ev place home <place> [--came <date>] [--left <date>] [--address a]` | a place that was a home of ours becomes a home that was left (`moved`), with the place's name: the things that named it as where they were move into it (`moved_in`), and the place goes with its aliases; an event `was_place` keeps its name. A place with errands (something to take there, return there or collect from there) is another household: exit 5, `place_is_a_household` (spec/vehicles-homes.md) |
| `ev for [<place>]` | `take`, `return`, `collect` lists for a place, or `errands` for every place |
| `ev lend <ref> --to <place>` / `ev back <ref>` | lend a node out / it came back. Lent again to whom it is with exits 5; to someone else, it passed on. `ev lost` of a thing already lost exits 5 |

Place names match folded with apostrophes ignored; an unknown name in `to`/`owner`/`with`
creates the place.

## Splitting and kits

| Command | Does |
|---|---|
| `ev split <ref> <name>=<qty>… [--rename <name>] [--qty n \| --take]` | one record becomes several kinds of thing: each `<name>=<qty>` (or `<name>` without a count) is a new record beside it, with its kind and tags (where it came from is its history, `split_from`); the original is renamed and recounted with `--rename` / `--qty`. By default the parts are what each unit is made of and the original keeps its count (3 sets: `kart=3`, `kablo=3`); with `--take` they are some of its units and their counts come off its count (2 of 4 cells are another make: `ev split 650 "AA beyaz=2" --take` leaves 2), refused when they would take all of it. For the same thing in another place, move part of it instead (`ev move <ref> --qty n`). A set recorded as one thing becomes a record per part (`ev split 598 "LM393 kart=3" "Kablo=3" --rename "HW-080 prob"`); straight and angled headers in one record become two. Events: `split` on the original, `split_from` on each new one. Photos stay on the original and are listed, to crop each part from. Each new part in `into` carries `purchase_candidates` like a fresh `ev add`. A holder with things inside is refused (exit 5) unless `--take`: then the units taken leave empty and what is inside stays in the original (two battery cases recorded as one, one emptied: `ev split <cases> "Pil kutusu=1" --take`); all or nothing |
| `ev kit add <name> [--copies n] [--note t] [--part "<name>[=<per copy>]"]… [--purchase <line>]` | a kit: a bought set, how many of it were bought, and its parts numbered from 1, each with how many come in one copy (1 by default). Names are compared folded and are unique. `--purchase`: the line it was bought as, as `ev kit purchase` |
| `ev kit part <kit> "<name>[=<per copy>]"…` | add parts to the end of the list; answers with `added` (the new parts with their numbers) and the kit's `counts` |
| `ev kit drop <kit> <n>` | take part `n` off the list (entered by mistake): `dropped` `{n, text}` and `counts`; refused (exit 5, `details.linked`) while records are linked to it. The other parts keep their numbers |
| `ev kit rename <kit> <n> "<name>[=<per copy>]"` | name part `n` anew and say how many come in one copy; its records stay linked. Answers as `ev kit link` does |
| `ev kit link <kit> <n> <ref>…` / `ev kit unlink <kit> <n> <ref>` | these records are part n (or are not); each record's history gets `kit_link` / `kit_unlink`, and `ev show` lists its `kits`. Answers with what changed: `kit` (`id`, `name`), `part` (as in `ev kit show`) and the kit's `counts`; the whole list is `ev kit show`'s, whose text names each part's records by the holder they are in |
| `ev kit show <kit>` | `kit` (`id`, `name`, `copies`, `note`, `purchase`: the line it was bought as, `{id, name, shop, ordered_at, paid, currency}`), `counts`, `parts`: `[{n, text, qty, expected, found, lost, open, nodes}]`. `expected` is `qty × copies`; `found` sums the counts of the linked records that are here (no count is 1), `lost` those marked lost, gone ones count for nothing; `open` is what is expected and not recorded at all. Computed from the records every time, so finding or moving a record updates the kit |
| `ev kit purchase <kit> <line>` / `--clear` | the purchase line the whole kit was bought as (spec/kit-purchase.md): the line is settled (`open_qty` 0, `kits` on it in `ev buy show` and `ev buy list`); every record linked to the kit lists it under `purchases` in `ev show` with `kit` (no quantity of its own, no price of its own), and is offered no purchase candidates by `ev add`, `ev found` or an edit of make or model, nor by the back-fill. Each linked record gets a `kit_purchase` event `{kit, purchase}` (null when cleared) |
| `ev kit list` / `ev kit remove <kit>` | every kit with its `counts`, most still open first; removing a kit keeps the records |

## One thing in several places

A thing kept in several places (20 cells: 2 in a flashlight, 2 in a toy, 16 in a drawer) is one
thing whose **portions** are ordinary records, each with its own place and count, tied by
`thing` (spec/portions.md). Only items without a serial are spread, and a holder with things
inside moves whole.

- **Shared:** name, kind, make, model, size and tags are equal on every live portion; setting
  one on any portion sets it on all (each gets an `edit` with `via`). `edit kind=` or `serial=`
  on a portion is refused (exit 5): `ev unjoin` it first.
- **Per portion:** place, count, note, photos, code, cells, lost, pending move, lending and
  owner, state and disposition, marks.
- **Across the thing:** purchases, documents, coverage, values, links and declined purchase
  candidates are written on the portion named and read over every portion (gone ones too), each
  entry marked `on` with the portion it is on when that is not this one.
- **Joining:** a portion that arrives (`move`, `done`, `back`, `found`, `add --of`) where a live
  portion of the same thing already is, in the same condition (active, not lost, not lent, no
  pending move, same disposition and owner), joins it: the counts add up, the arriving record
  ends as gone with disposition `merged`, keeping its photos and history, and its tasks and kit
  parts move to the one that holds the units.

| Command | Does |
|---|---|
| `ev move <ref> --qty n --to <place> [--plan]` | n of the record's units go (or, with `--plan`, are set apart beside the rest with the planned move, which `ev done` takes); the rest stay. All of them is a plain move; more is refused (exit 5) |
| `ev move <ref> <ref>… --to <place> [--plan]` | several records to one place in one call (a box emptied before it goes), all or none: one refused leaves every one where it was. `moved` (or `planned`): `[NodeRef]`, and `to`. `--qty` takes one record only |
| `ev move <ref> --to <where it is>` | refused (exit 5), with or without `--plan`: a move to where it already is says nothing, and a planned one would wait forever. A compartment inside a holder is a grid cell (`ev grid`, `ev cell`). A lost thing moved to where it was last seen is found there |
| `ev done <ref>…` / `ev cancel <ref>…` | makes (or drops) the planned move of one record, answered as `ev show`, or of several at once, all or none: one without a plan refuses them all (exit 5). Several answer with `done` (`[NodeRef]`, where each is now) or `cancelled` |
| `ev lend <ref> --to <place> --qty n` · `ev dispose <ref> --as … --qty n` · `ev gone <ref> --as … --qty n` · `ev lost <ref> --qty n` | the verb acts on n of the units, split off as a portion in the same transaction: a refused verb leaves nothing split |
| `ev add --of <ref> [--qty n] --in <place> [--note t]` | more of a thing already recorded: name, kind, make, model, size and tags come from it; a portion of the same thing, joining one already in the place. `<ref>` may be gone (a cassette used up and replaced with the same one): the thing then reads "here 1 · gone: used 1". Also `of` in `ev add --stdin` |
| `ev join <ref> <ref>…` | records made separately are one thing: name from the first, a make, model or size only another has filled in, the tags of all; a make or model that differs is refused (exit 5) with the values in `details`. A record already a portion brings its thing's portions along; records in one place join. A gone record may be among them as long as one lives; it keeps what it was, and the record returned is a live one |
| `ev unjoin <ref>` | a portion is a thing of its own after all; it keeps what it is and what is linked to it |

`ev show` of a portion carries `thing`: `id`, `total` (units here, lost ones left out),
`places` (the portions in a place: a lost one is in none), `in_use` (units inside an item: a device, a toy), `spare`, `lost`, `elsewhere` (each
other live portion: NodeRef with `qty` and `in_use`), `bought` (the units its purchase links
name, null without one), `gone` (units gone by disposition: `{"trash": 2, "used": 1}`; merged
ones are not gone) and `unaccounted` (bought − here − lost − gone: above zero some are missing,
below zero more are here than were bought). A last portion with units gone beside it keeps
`thing` for that account. The text reads `thing: ×20 in 3 places · in use 4 · spare 16`, each
other place, and `accounted: bought 20 · here 18 · gone: used up 2`.

Purchase candidates treat a line linked to any portion as linked to the thing, and a "not this
one" said of one portion as said of all. `ev add --of`, a split and a found portion ask about a
purchase only while the thing has more units here than its purchases account for. `ev audit`
counts a thing in one place only and, for a name recorded in more than one place, hints
`ev join` (`same_name`).

## Placement

| Command | Does |
|---|---|
| `ev suggest <text> [--tag t] [--for <ref>]` | `words` (the query as searched), `synonyms_added`, `facet` (the facets the query names), `other_facet` (up to 5 holders kept out because they are of another facet, with their `facet` and `score`), `parking` (up to 5 holders kept out because they are, or stand in, a `temporary` place: `container` with `temporary_in`, and `score`), `new_group_likely`, `empty` (when `new_group_likely`: up to 10 containers with no theme, known to be empty as with `ev find --empty`, where a new group can start; those in the `--for` thing's own room first, then boxes before furniture slots; each with `same_room` and `slot`), `considered` (how scores are made, in words), `rules`, `similar` (up to 12 holders, best first: `container` with `room`, `review` (its own or its nearest reviewed ancestor's: `status`, `at`, `from`; null when never gone through — `(not toured)` in text), `score`, `coverage`, `specific`, `matched` `[{term, points, from, specific}]`, `count`, `matches`), `containers` (every holder, with `path_text`, `theme`, `fill`, `room`, `items`, `sample`, `cells`/`grid`), `complete.containers`. `--for` places an existing node by its own name, tags and note, never into itself or anything inside it |
| `ev regroup [<ref>]` | for the holders under `<ref>` (or everywhere): `checked` (`items`, `best_where_they_are`), `elsewhere` (`item`, `now`, `better` with score and `matched`), `alone` (the same shape, for things that share no word with anything else in their holder: its score there is 0, so the other holder is a guess), `strays` (things named for another holder's theme), `full` (fill ≥ 90, with `bigger_spares` and the cells each `fits_at`), `sparse` (fill ≤ 25, with a `merge_into` sibling that has room), `mixed` (half or more of three or more things fit better elsewhere), `unknown_fill` (fill unknown or `stale`), `declined` (`item`, `holder`, `why`: moves the person said no to, left out of the lists above) |
| `ev regroup --decline <ref> [--why "…"]` | the person said no to moving it: it stays in the holder it is in and regroup no longer proposes moving it, until it is moved somewhere else. A `decline` event |
| `ev regroup --allow <ref>` | takes a decline back; a `decline_cleared` event |
| `ev layout <furniture> [--propose]` | every place of a piece of furniture (the places gone through one at a time below it, two or more) compared by what it holds, never by its theme. A thing's kind is the end of its name before the first comma (a dash does not cut it): its last word, or its last two when they are a noun compound (`lens kapağı`, a plural one too: `şarj adaptörleri`); after a head that is one abbreviation in capitals (`USB-A, USB-C, Lightning kablolar`) it is read from the last part. Colour words never count. `furniture`, `places`, `things`; `spread` (up to 12 kinds in two or more places and three or more things: `word`, `things`, `places` `[{place, things, records: [{id, name}]}]`: the records that make the kind there), `overlap` (up to 12 pairs of places whose weighted words read alike, cosine ≥ 0.3: `places`, `alike`, `shared` words), `merge` (places with three things or fewer, or a low fill: `place`, `things`, `fill`, `into` the most alike place with room or null, `shared`), `split` (`why`: `full` at fill ≥ 90, or `mixed` when eight or more things and no kind is 40% of them: `place`, `things`, `fill`, `groups` the four largest kinds). A word in more than 70% of the places tells none apart and is left out. `--propose` adds `proposal`: `themes` (`place`, `theme`, `things`: each kind of three or more things, largest first, given the place holding most of it and not yet given one, else the emptiest left; a parking place is given none), `moves` (`thing`, `from`, `to`; never a thing in a box of its own inside the place, inside another thing, or linked to a kit) and `kept` (`word`, `things`: kinds left where they are, no place being left), and `not_counted`: the places not toured or kept yet, left out of the draft (nothing moves from them and none gets a theme: their records may still be wrong). Nothing is moved |
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
is a box with a `size`, no theme and nothing in it: a container known to be empty (as with
`ev find --empty`), or anything tagged `boş kap` (or `spare box`); a box with something in it
is never one, whatever its tag says.

`ev restore <ref> --correction "<why>"` undoes a `gone` recorded by mistake; plain `restore` returns a
candidate to active.

`ev gone <ref> --as mistake --why "<text>"` closes a record that should never have existed (a
misreading, a duplicate): it keeps its history, is not a disposal, and the reason is required.

`ev focus <ref> [--photo n]` makes a running `ev ui` jump to the node and show that photo full
screen (the last one by default). `ev focus --file <picture>… [--note text]` (`--file a.jpg=<note>` titles that one on its own) adds pictures that
are no record (marked photos) to the **marked photo series** (spec/focus-stack.md): `photo mark`,
`photo cut` and its preview add theirs on their own. A series holds every picture sent until the
person closes it, each titled with its note and `n/total`, stepped with `[` `]`, the newest one
sent shown; a new copy of a photo in it (marked again, or cut after it was marked, also by its
`f`) takes that photo's place. Esc hides it (a click does not), `m` brings it back, also after a restart, and `X`
closes it: what is sent next starts a new series, numbered from 1. Within a series ev numbers the
frames, so a number on screen means one frame until it is closed. A picture of the series is
named `f12` (its title in `ev ui` reads `f12/20 · <note>`), a frame by its bare number, a record
by `#12`. `ev focus f12` shows that picture again, and `ev photo add`, `ev photo cut`, `ev photo
mark` and `ev focus --file` take `f12` for a photo: the photo it was drawn on, unmarked (a file of
that name wins). A series knows what it is about (spec/series-batches.md): the place of its first
picture that names one, the place `ev photo mark <place>`, `ev photo cut --place` or `ev photo add`
named (a thing stands for the place it is in), or `ev focus --file … --for <place>`. A picture
about another place joins the series all the same, and the answer's `shown` (`focus` for
`--file`) carries `series_about: {about, now}` (each `{id, label}`): ask the person whether to
close the series before that batch goes on; ev closes nothing by itself. Its title in `ev ui`
says so too (`f17/20 · <note> · about S5-02, the series S5-01`). `ev focus --list` reads
the series:
`{series: {since, next, about, pictures: [{f, file, source, note, about, frames: [{n, at, given} | {n, ref, crop}]}]}}`
(`f` names the picture; `source`, the photo it was drawn on, only when that is not `file`;
`about` only when set; `given`: the label number the agent gave a mark's frame on that photo, `n`
the number drawn) (`series: null` when there is none); `ev focus --clear` closes it from outside,
for an agent the person asked. The answer to `--file` is
`{focus: {files, f, note, series, next, at, series_about?}}`: this request's pictures, their `f`
names, and the series' size. Each request is shown once, without restarting `ev ui`.
A request is a message to the UI, not a change to the inventory: it is kept beside the database
in `ev.db-focus.json`, and `ev.db` stays as it was; `X` removing it is the one write `ev ui`
makes.

`ev photo mark <target> <label>=<where>… [--codes] [--grid corners] [--out file] [--show note | --no-show] [--keep-numbers]` draws a
red frame (edged in dark, so it reads on a red thing too) and a label for each mark on a copy of a photo: `<target>` is a photo file or a place
(its newest whole photo), `<where>` is `x,y,w,h` in fractions of the upright photo or cells of
the place's grid (`A6`, `A6-B7`). `--codes` adds a mark for every box placed in the place's grid
that has a code, its code on its own cells (which label goes on which box). Cells are found through the grid corners the photo kept when
it was cut with `--grid` (schema 11), or through `--grid`. Cells that are exactly one placed box's
are framed out to where its rim leans, as `photo cut --grid` crops it: a box higher than 1 (its
`size`) stands above the floor the corners are read at. The copy goes to `--out` or to
`<temp>/ev-marks/` (files there older than a day are removed on each call); it is not stored,
not attached and leaves no history. It is also sent to a running `ev ui`, into the marked photo
series (titled with `--show <note>`, else with the labels), unless `--no-show` is given. There a
numbered label (`1`, `2 → A6`: a number, alone or followed by a space) counts this photo's frames
(the labels' numbers only order them: a lone `2` is the photo's first frame) and is drawn with the
frame's number in the series: the series' next free ones, in the labels' order — so `1=… 2=…` on
the second photo of a series draws `3` and `4`. A photo marked again keeps each label's number
from its last mark, and a new label takes the next free one: `1=… 3=…` after `1=… 2=…` draws `1`
and `3`, and 2, left out, is not handed out again until the series closes. `--keep-numbers` draws the numbers as given, for marks that point at
frames numbered already (a destination: `4=A6`, frame 4 goes to A6). Output:
`marked`, `source`, `marks: [{label, at}]` (the labels as drawn), and `shown` when sent. A label keeps the
letters as given (lowercase too; Turkish letters are drawn plain). A label on a frame is no wider
than its frame (or an eighth of the photo, so a number on a small frame stays legible): a long
one is drawn smaller, down to a third of the photo's size, and broken onto up to three lines at
spaces. It goes above its frame, else below it, else inside it, at the first of those that
stays in the photo and off every other label and frame; with no such place it may cover a
frame, and moves down its column before it covers another label. A frame inside another frame
takes its label inside itself first, since outside it the label would read as the outer one's.
A label on cells sits inside
them, in the top-left corner, no taller than about a third of the cells and no wider than them,
so the box under it stays visible. Photos cut among records
are numbered by `ev photo cut` itself (`marked`, `legend`, `--show`).

`ev gone <ref> [--as d] [--why "<text>"]` records the reason in the `gone` event and appends it to the
note; `ev dispose <ref> --as d --why "<text>"` appends it to the note too (an `edit` event) when the
thing is set aside. A gone node is out of reach by name, but its id still works for `ev show <id> --include-gone`,
`ev history <id>`, `ev edit <id> note=…` (a gone node lets change only what it was: `name`,
`note`, `came`, `qty`, `make`, `model`, `serial`, `tags`, its photos, and when and where it left: `left=<date>`,
`left_in=<place>`, empty clears; any other field exits 5. A date still to come, a leaving
before it came (by `came`, or by the purchase that brought it), or a place given as an id
exits 2), and for adding its
history: `ev doc add <file> --for <id>`, `ev doc link <doc> <id>` and `ev photo add <id> <photo>`
(a sale mail, an old photo of a thing long gone).

**Photographed, then thrown out: `--as digitize`.** For a paper whose content is worth keeping
but whose paper is not: a ticket, a letter, an old statement, a manual. `ev dispose <ref> --as
digitize` puts it in the pile to photograph (`ev disposals`, "Leaving" in `ev todo`). Its copy
goes on its own record: `ev photo add` (or a crop with `ev photo cut`) when the paper is the
thing, `ev doc add <file> --kind invoice|warranty|manual|… --for <ref> --for <thing>` when the
paper is about another thing, so that thing keeps reaching it (`scan` is the kind for a copy
that is none of these). `ev gone` refuses a digitized record with no photo
and no document (exit 5); when every copy is an image under 800 px on its short side it still
leaves, with `warnings: ["…"]` in the result, since only the person can tell whether it reads.
A candidate inside a box that leaves is checked the same way. A digitized record stays in
`ev find` without `--include-gone`, marked `(gone, copy kept)`. The copy is the only one left
once the paper is gone; `~/.ev` is backed up by whatever backs up the home folder, not by ev.

**Shredded: `--shred`.** `ev dispose <ref> --as trash|digitize --shred` (or `ev gone … --shred`)
says the thing is shredded rather than thrown out whole: an old ID card, a boarding pass, a
statement. It is a `shred` mark (`marks.shred`), `shred: true` in `ev disposals`, `(shred)` in
the pile; `--shred` with give or sell exits 2, and `ev restore` takes the mark back.

## Past belongings

A thing that left long ago, recorded as it is remembered (spec/past-belongings.md). It is an
ordinary record in state `gone`: never in the tree, `ev todo`, `ev suggest`, placement or any
count of today, and it holds purchases, documents, photos and a note like any other. Dates are
partial: `2016`, `2016-06` or `2016-06-14`; anything else exits 2.

| Command | Does |
|---|---|
| `ev add "<name>" --gone <how> [--at <date>] [--came <date>] [--where <place>]` | records a past thing in one step, in no holder (kind `item` unless `--kind`); `--in`, `--lost` and `--of` exit 2 with it, and `--at`/`--where` need it. `how` is any way of leaving but mistake, digitize and merged. Batch lines take `gone`, `at`, `came`, `where`. A former home of ours is added so, `--kind home --gone moved\|sell` with `--came`, `--at` and `--address` (a home leaves no other way; a room is never added on its own). `--where` (here and on `ev gone`) names a former home first, by its name, code or `#id`, then a place: a thing left in a former home is recorded inside it, so `ev show <home> --include-gone` lists what was left there (`children`, as for any record that left) and `ev past --where <home>` finds it |
| `ev gone <ref> [--as d] [--at <date>] [--where <place>]` | a record here that left long ago: when it left and where it was then. `--where` names a place (`ev place`) by its name, made when new, so a former home is one place for all that was left there; an id (`42`, `#42`) exits 2. `--at` before the thing came (by `came` or the purchase that brought it) exits 2 |
| `ev gone <ref> --as left\|stolen\|unknown` | left behind, stolen, or "sold or thrown out, not sure"; `ev dispose` refuses them (exit 2), nothing is set aside to be stolen. Read back in the past tense: sold, thrown out, given away, left behind, how not known |
| `ev gone <home> --as moved\|sell [--at <date>]` | a home left: `moved` (moved out of; a home's only, and `ev dispose` refuses it) or `sell` (an owned one; `mistake` closes a wrong record). Any other way exits 2, and `--as moved` on anything but a home exits 2 (what stayed behind at a move is `--as left`). A home's rooms, and a vehicle's compartments when it goes, are its structure and leave with it, the way it left; anything else still active in it exits 5 (`leaving_still_holds`), its `details.remaining` listing what is left by room or compartment (`in`, `nodes`, the outermost only), with `move` (`ev move --to <where> #…`) and `left` (an `ev gone #… --as left` per record), the two ways to empty it |
| `ev edit <ref> came=<date>` | when it came, on any record (`came=` clears) |
| `ev sold <ref> --price <n> [--currency C] [--at <date>] [--via "…"] [--note "…"]` | what a sale brought, on a record gone or set aside as `sell` (else exit 5); the currency is the home one when not given; said again, the price is replaced and what is not said again stays. `--at` dates the leaving. A thing that then leaves another way (given, thrown out, swapped) keeps no price. A `sold` event |
| `ev buy link <line> <ref>`, `ev buy add … --for <ref>`, `ev buy decline <line> <ref>` | work on a gone record: the line is settled and leaves the open lists. A line bought after the thing left exits 5 and is never offered by `ev buy for` |
| `ev past [--name <word>] [--where <place>]` | first `homes_and_vehicles`: every home and vehicle of ours, here now or left (current first, then the last left first), each a NodeRef with `came`, `left` and `how` once left, `address`, `paid` by currency and `paid_today` in today's money when every line converts, and `got` (a sale); empty with `--where`, narrowed by `--name` (the address history and the cars, in one list). Then the past things in two lists, `remembered` first (left before they were recorded: added already gone, or gone with `--at`; a date said later, with `ev sold --at` or `left=`, dates the leaving but keeps it in the second list), then `left_inventory` (recorded here and seen leaving); each lists its things last gone first (with `qty`, and `traded_for`, a NodeRef, for a swap), and per year how many left and the money paid for them (their linked purchases) and got for them (`ev sold`), by currency. Mistakes, joined portions and digitized papers are left out, and so are homes and vehicles (listed first) and what left with one as its structure (its rooms, a car's compartments); `ev stats` `past` still counts the homes and vehicles that left |
| `ev gone <ref> --as trade [--traded-for <ref>]`, `ev add … --gone trade [--traded-for <ref>]` | swapped for something else; `--traded-for` links what came in exchange (any other `--as` with it exits 2) |
| `ev traded <ref> [--for <ref>]` | a gone record left as a trade, found by id or by name: corrects one first recorded as given or sold, and links what came in exchange once it is recorded. A record that has not left, one that left another way (thrown out, used up, …), or a trade said again with nothing new exits 5. What came is a thing, not a place, still ours when the swap was (not gone before it) and not ours before it (`came` not earlier, nor years later): else exit 2. A `traded` event `{was, for}`. `ev dispose --as trade` sets a thing aside to swap, like a sale |
| `ev past --year <y>` | what was ours in that year: every home, vehicle, item and piece of furniture, past or present and not kept for someone else, that came by the end of the year and had not left before it began, the homes first (where we lived), then the vehicles, then the things, oldest coming first; a home nothing dates is neither listed nor counted; `unknown` counts those whose coming, or (once the year they came is over) whose leaving, nothing says, never guessed in; a thing here today counts as ours this year, and one that left in a year as ours that year, whenever it came |
| `ev edit <id> name=… note=… came=… left=… left_in=… qty=… make=… model=… serial=…` | a gone record takes what it was, so a past thing is completed as remembered; any other field exits 5 |

When a record came is `came`, else the earliest order date of a purchase linked to it. When it
left is `--at`, else the day it was seen leaving (`ev gone`); a past thing added with `ev add
--gone` and no `--at` left when nothing says, and is listed apart from the years, never in the
year it was recorded. The `gone` event keeps the moment it was written either way. A thing gone
`--as sell` while a sale was listed (`ev sale --listed --where …`) takes where it was listed as
what it went through (`via`); the asking price is not carried, since it is not what the sale
brought.

`ev show` gives `came`, `departure`: `{at, where, price, currency, via, note, traded_for}` (left
out unless gone, except a sale said with `ev sold` before the thing leaves: `{price, currency,
via, pending: true}`; `at` null when nothing says; `traded_for` a NodeRef), and `traded_from` (what was
traded away for this record). `ev past`: `{remembered: List, left_inventory: List}`, each List
`{past: [{id, name, came, left, how, where, paid: {CUR: amount}, got: {price, currency, via} |
null}], years: [{year, left, paid, got}], undated: {left, paid, got} | null}`. `ev past --year`: `{year, owned: [NodeRef + came, left], unknown}`.
`ev stats` adds `past: {records, how: {d: n}, paid, got}`.
## Documents

Invoices, warranty certificates, manuals, service forms, appraisals and policies, copied into
`docs/` beside the database (named by content hash, like photos) and linked to the things they
belong to. The copy outlives the file it came from.

| Command | Payload |
|---|---|
| `ev doc add <file> --kind k [--for <ref>]… [--number n] [--ettn u] [--issued d] [--issuer i] [--note t]` | `document`, `existing`. `kind`: `invoice`, `warranty`, `manual`, `service`, `appraisal`, `policy`, `scan` (the copy of a paper thrown out, see `--as digitize`), `image` (a shop's picture of the product, brought from a purchase; never the thing's own photo), `other`; `issued`: `YYYY-MM-DD`, `YYYY-MM` or `YYYY`. The same file again is the same document: `existing: true`, its fields are kept, only new links are added. Anything refused stores nothing |
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
| `ev buy import <file>` / `--stdin` | `imported`: `new`, `updated`, `unchanged`, `skipped` (cancelled and consumable lines), `document_links`, `documents_skipped` (a document naming no imported line is not stored), `attachments` (new ones stored), `attachments_skipped` (naming no imported line), `joined` (lines found to be another source's line, see below), `unjoined` (only when some are: `[{id, key, order, candidates}]`, ak lines whose order ev has several lines of and none could be told, for the person to join with `ev buy join` or leave), `unknown_fields` (`{field: lines}`, only when a purchase line carried fields ev does not read: a misspelt field loses its value, and says so). A `currency` is a code in use (ISO 4217); another exits 2, naming the line. All or nothing; the same lines again change nothing, and an update never touches links or a dismissal |
| `ev buy add <name> [--shop s] [--date d] [--paid n] [--currency c] [--qty n] [--pack n] [--order o] [--order-url u] [--url u] [--brand b] [--for <ref>]` | `purchase`: a line entered by hand (`source: manual`), linked to `--for` at once (all its units); `--date` is a day, or a month or a year as remembered (`2018-03`, `2018`; today's money reads it as its middle), and one still to come exits 2 |
| `ev buy edit <id> field=value…` | `purchase`: corrects a line entered by hand: `name=`, `date=` (a day, a month or a year), `paid=`, `currency=`, `shop=`, `brand=`, `order=`, `qty=` (never below the units linked, exit 5); an empty value clears what may be empty. A line from a source exits 5 (`purchase_edit_not_manual`): it is corrected at the source and imported again; another field exits 2 |
| `ev buy list [--open] [--bucket b] [--shop s] [--since d] [--query text] [--billed-to account] [--source s] [--key k] [--dismissed [reason]]` | `purchases`, newest first; `--open`: something left to link, not dismissed, and not `digital` or `service` (never a thing); `--query`: every word in the line's name, shop, brand, product code, order number or the account it was billed to, compared folded (is there a purchase of X, before or without a record); `--billed-to`: only lines billed to an account holding it (an Apple ID of a family member), compared folded; `--source`: only lines from that source, exactly (`ak`); `--key`: the line keyed so and the lines keyed as its parts, so `--key 412` finds an ak payment sent whole (`412`) or its items (`412.1`, `412.2`) and `--key 412.2` that item alone (spec/ak.md: which thing a payment bought); `--dismissed`: only dismissed lines, of any reason or of the one given (`--dismissed elsewhere`: the lines kept elsewhere, such as the bills ak holds), with `dismissed` and `why` on each; another reason exits 2. Each row is the line without `merchant`, `order_url`, `product_url`, `raw` and `imported_at`, and with counts for what came with it: `attachments` `{<type>: n}` and `documents` (a number); `ev buy show` has all of it |
| `ev buy show <id>` | `purchase`: `id`, `source`, `source_key`, `shop`, `merchant`, `order_no`, `order_url`, `product_url`, `shop_sku`, `name`, `brand`, `category`, `ordered_at`, `delivered_at`, `qty`, `pack` (units in each bought quantity, 1 unless set), `units` (`qty` × `pack`), `paid`, `currency`, `billed_to`, `status` (`delivered`, `returned`), `bucket` (`durable`, `clothing`, `digital`, `service`), `dismissed`, `why`, `raw`, `same_as`, `imported_at`, `linked` (`[{node, qty}]`, in units), `open_qty` (units left to link; 0 for a joined line), `joined` (the ids of lines that are the same purchase), `joined_to` (on a joined line only: `{id, source, source_key, linked}`, the line it joins and what that is linked to; on `ev buy list` rows too), `attachments` (its own and its joined lines': `id`, `purchase`, `type`, the fields given, `brought_to`), `documents` (also those of its joined lines), `today` (see **Money over time**) |
| `ev buy for <ref>` | `node`, `candidates` (up to 12, best first): `purchase` (`id`, `name`, `shop`, `brand`, dates, `qty`, `open_qty`, `paid`, `currency`), `score`, `why` (`[{why, kind, value, points}]`: `kind` one of `bought_before`, `model`, `serial`, `model_part`, `code`, `brand`, `words`, `words_aside`, `differs`, with `value` what it names, so the text reads it in the reader's language), `linked` when already linked to it. Open, undismissed lines scoring above zero, and any linked to it. Points: a product linked before to a thing of the same name 80; the thing's `model` or `serial` in the line 60; each shared model code (letters and digits, four or more, not a size like `64gb` or `3x3`; read from the thing's name, make, model and serial, never its note) 25, at most two; the line's brand as whole words in the thing's name or make (never the shop's own name) 12, or 5 when the line shares no word of what the thing is (the head of its name: before the first comma or dash, without brackets); shared words weighted by how rare they are among lines and records, at most 30, but at most 10 when nothing stronger says it and the line carries no more than half of that head (by the same weights), so words shared only with what a thing is for or kept with never offer a line; each unit whose numbers all differ (`125 kHz` against `13,56 MHz`, `2,5 A` against `3A`; units converted; a lone `A` counts as amperes only against its number or after a decimal, not in `Pi 3 A+`) −40. The best three above 15 points also come as `purchase_candidates` on `ev add`, `ev split` (each new part), `ev found` and an `ev edit` that sets `make` or `model`: the moments the thing is in hand |
| `ev buy for --toured` | The back-fill (spec §12.2): `backfill`, best first, `[{node, candidate}]` with `candidate` shaped as above; `toured_things`, how many things were looked at. Every thing no purchase is linked to whose nearest reviewed place above it is `toured` (a toured drawer covers its boxes unless a box has a review of its own), each with its one best open line, only when that line scores above 15 (the bar `ev add` offers at). One line may be offered to several things |
| `ev buy link <id> <ref> [--qty n]` / `ev buy unlink <id> <ref>` | `purchase`; a link takes all that is left of the line by default, never more; on a line in packs (`ev buy pack`, `units` above 1) it takes as many units as the thing stands for (its `qty`, else one), so the next things still find theirs; it remembers the shop's product key for the next purchase of it. A purchase is a thing's: a link to a home or room, or `--qty` below 1, exits 2; to a record gone as a mistake, merged or digitized, exits 5; a `digital` or `service` line has nothing to link (exit 2). `unlink` takes back what `ev buy bring` brought from the line to that thing: its product pictures and links leave it (`taken_back`: `[{id, type}]`); a value or a coverage stays, listed under `left` with `how`, the command that removes it, ready to run; every attachment can be brought again |
| `ev buy bucket <id> <durable\|clothing\|digital\|service>` | `purchase`: what kind of purchase a line is, for a line entered by hand or from a source that cannot tell. The same bucket again exits 5, and so does `digital` or `service` for a line linked to a thing (unlink it first). An import that gives no `bucket` for a line keeps the one it has |
| `ev buy pack <id> <n>` | `purchase`: each bought quantity of the line holds `n` units (an 8-pack of cells, a charger set with its cells), so its units can be linked to several things, each with `--qty`. A thing's share of the price is by units. Refused below 1 or below the units already linked |
| `ev buy dismiss <id> --as <reason> [--why t]` / `--clear` | `purchase`; reasons: `consumed`, `given`, `returned`, `elsewhere`, `not-mine`, `duplicate`. A dismissed line cannot be linked |
| `ev buy join <id> <other>` / `ev buy join <id> --clear` | `purchase`: on the person's word, line `id` is the same purchase as `other` (an `unjoined` ak line), and counts as settled through it; `--clear` takes that back. Refused (exit 5): a line to itself, to a line that joins another (join to that one), a line linked to a thing (unlink it first) or one other lines join. A join an import made and `--clear` took back is made again by the next import |
| `ev buy decline <id> <ref> [--why t]` / `--clear` | `purchase`, with `declined` (`[{node, why}]`): the person said the line is not this thing. It stays open for other things and is no longer offered to this one by `ev buy for`, `--toured`, `ev add` or `ev split`; `--clear` takes it back, and so does `ev buy link` of the line to that thing on the person's word. A line linked to the thing exits 5 (unlink it first). Events: `purchase_declined`, `purchase_decline_cleared` |
| `ev buy bring <id> <ref> [--only <attachment>,…] [--type <type>,…]` | `node` (NodeRef: `ev show` has the rest), `from_purchase`, `brought` (attachment ids), `brought_types` (`{"image": 2, "link": 1}`) and `skipped` (`{id, type, why}`: `why` is `brought`, with `to` the thing it went to, or `type`, of a type not asked for). The line's attachments not brought yet (all, or `--only` these, or `--type` link, valuation, coverage, image) become the thing's own: a link, a value, a coverage, a product image (a document of kind `image`). The line must be linked to the thing first; an `--only` id the line does not carry is a usage error. The text says what was brought, what was left and why |
| `ev buy bring --all [--type <type>,…]` | `brought_from` (`{purchase, node, name, brought_types, brought}` per line), `brought_types` (the totals), `checked` (`{lines, carrying, already}`: the linked lines looked at, those carrying the type, and how many of it were brought before, so an empty run says why) and `left` (`{purchase, node, name, why, waiting}`). Every linked line's attachments not brought yet go to the thing it is linked to: the back-fill. A line linked to several things (`why: several`) or to a thing that is gone (`why: gone`) is left for `ev buy bring <id> <ref>` |

**One purchase seen by two sources** (a shop's export and another app that recorded the same
thing) is joined on import: a line whose `order_url` contains another source's order number
(six characters or more) points at that line with `same_as`; when the order has several lines,
the product key (`sku`) in the line's addresses picks one; with no order address, a product key
named by exactly one other line is enough. A line from ak (the household's money tool, which
relays purchases ev may have read from a shop already; spec/ak.md) is joined by its `order`
exactly equal to another source's order number (six characters or more), each of the other
source's lines taking one ak line at most: first every line paid the same as exactly one free
line of its order (or as several alike in name and price: one each), then a whole payment of a
one-line order, or the line whose name clearly shares most words and whose price is within a
fifth. An ak line is never the line kept; one linked to a thing or dismissed is left as it is,
and one that cannot be told is listed as `unjoined`. Anything less certain is left unjoined. The line
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
  `cancelled`), `bucket` (`durable`, `clothing`, `digital`, `service`, `consumable`), `raw` (path of the
  raw record). `digital` (a licence, a game key, a membership) and `service` (a diet
  programme, a repair) are paid for and never a thing: they never wait to be linked, and
  `ev stats` counts them apart. `ev buy add --bucket` takes the same.
- `document`: `source`, `file`, `purchases` (keys of that source's lines), `kind` (default
  `invoice`), `number`, `ettn`, `issued`, `issuer`, `note`.
- Attachments, hung on lines by `purchase` (one key) or `purchases` of the same `source` (which
  every attachment line carries too), brought to a thing with
  `ev buy bring`: `link` (`url`, `kind`, `archive`, `note`), `valuation` (`amount`, `currency`,
  `at`, `approximate`, `from`: where the figure came from, `note`), `coverage` (`kind`, `term`,
  `from`, `ends`, `issuer`, `number`, `note`), `image` (`file`: a saved product picture,
  `note`; brought as a document of kind `image`, never as a photo).

ev reads no shop. An **adapter**, a script the inventory agent writes and keeps next to the
raw export it reads (`~/.ev/purchases/<shop>/`, outside every repository), turns a shop's saved
order history into these lines: `~/.ev/purchases/<shop>/adapter.py | ev buy import --stdin`.
The adapter knows which product picture belongs to which line and emits the `image` lines
itself. `examples/purchases/` holds a worked adapter over an invented export, and its README
walks through writing one for a new shop (a test runs it, so it stays true).

## Coverage

Warranties and insurance as one kind of record (`spec/purchases.md` §3.6). A coverage covers
one or more things; its status is computed from its start and term, never stored.

| Command | Payload |
|---|---|
| `ev cover add <ref>… --kind k [--term t] [--from f] [--ends d] [--usage u] [--issuer i] [--number n] [--premium p] [--deductible p] [--currency c] [--scope s] [--note t]` | `coverage`. `kind`: `statutory`, `manufacturer`, `extended`, `store`, `insurance`; `term`: `2y`, `18m`, `6w`, `90d` or `lifetime`; `from`: `delivery` (the default: the earliest delivery of the purchases linked to its things), a date, or `after:<id>` (starts when that coverage ends). A term or an end is required; an insurance needs an end or a time term. Recording one clears the things' own "do not track" decision on coverage |
| `ev cover purchase <coverage> <line>` / `--clear`, `ev cover add … --purchase <line>` | the purchase line a coverage was bought as (an extended warranty sold as a line of its own): it settles the line (`open_qty` 0, `coverages` on `ev buy show`, out of `ev todo`'s purchase count), the coverage shows it as `purchase` (`id`, `name`, `shop`, `ordered_at`, `paid`, `currency`), and its price is the premium when none was given. A `coverage_purchase` event on each thing it covers; `--add` with a wrong line id writes nothing; `--clear` on a coverage with no line exits 5 |
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
| `ev value <ref> [<amount>] [--currency c] [--at d] [--source s] [--note t] [--approximate]` | `node`, `added` (the new observation's id, or null), `valuations`: newest first, each `id`, `amount`, `currency` (the home one by default), `at` (today by default), `approximate` (the date is a guess), `source`, `note`, `today` (as for a purchase). Recording one clears the thing's own "do not track" decision on value; an `--at` still to come exits 2 |
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
| `ev photo add <ref> <file>… [--crop x,y,w,h [--pad f]] [--note text] [--rotate 90\|180\|270] [--no-show]`, `ev photo add --stdin` | copy into `~/.ev/photos/` (hash-named) and attach; several photos at once (a file or `f12`, a picture of the marked photo series), or with `--stdin` one `{"ref", "photo", "note"}` a line for many records, every record and photo checked before anything is attached (the whole-photo rule below too, also between the photos of the call: one refusal attaches none, and the message starts with the photo or `line n` it is about), answered with `added` (`[{node, photo, from}]`); one photo is answered with `node` and `photos` holding only the photo added, with its number `n` (`ev photo list` has them all); a series picture takes the note it was sent with and is not sent again; `--crop` and `--rotate` take one photo; the attached photo joins the marked photo series in a running `ev ui`, unframed and titled with `--note` (else the record's code or name), and the answer carries `shown` (`--no-show` sends nothing: every photo the person sends is shown, framed or not); with `--crop` attach the cut-out, remembering the original. `--pad 0.1` grows the crop on every side by a tenth of its own size, inside the photo. To check a crop first, `ev photo cut <file> <ref>=x,y,w,h --preview` makes the same crop with a sheet and the framed photo. `--rotate` turns the photo clockwise first and stores it turned; `--crop` is then a fraction of the turned photo. A photo's EXIF orientation is always applied. A file ev cannot open as an image (a JPEG or PNG photo) exits 2 |
| `ev photo list <ref>` | `photos`: `n`, `path`, `exists`, `source`, `crop`, `note`, `added_at` |
| `ev photo remove <ref> <n>` | detach the n-th photo; the history keeps a `photo_remove` event with what it was. A whole photo's file stays in the store; a crop's file is deleted when no photo uses it any more as its picture or its source (`deleted_file`), since it can be cut again |
| `ev photo rotate <ref> <n> <degrees>` | turn the n-th photo 90, 180 or 270 degrees clockwise for good (spec/rotate.md): it is stored turned; every record holding it whole gets the turned one, its grid corners turned (each keeps its name); every crop cut from it, on any record, has its rectangle turned and is cut again, so it shows the same part upright. When the n-th photo is a crop, its source photo turns, with all its crops. Each record gets a `photo_rotate` event. Output: the node's `photos`, and `rotated`: `{degrees, records: [NodeRef], deleted_files}` (the old photo and crops nothing uses any more) |
| `ev photo add <ref> <file> --whole` | attach a whole photo that is already attached whole to another node; without `--whole` (and without `--crop`) that is refused with exit 5 and `details.attached_to` |
| `ev photo cut <file> <ref>=x,y,w,h… [--place <ref>] [--grid <corners>] [--note n] [--no-show] [--rotate 90\|180\|270] [--pad f]` | one photo cut up among several nodes in one step (`--place` is optional: one crop of one thing is a cut too, with its sheet and preview; `--rotate` turns the photo clockwise first and stores it turned, every crop and `--grid` then fractions of the turned photo; `--pad 0.1` grows every crop named by hand on each side by a tenth of its own size, inside the photo, so an edge the estimate cut off stays in): a crop for each `<ref>=` (the same `<ref>` may come several times, one crop each: the three probes of three sets in one photo are one record), and the whole photo on `--place`; every reference is resolved and every crop cut first, then all are recorded in one transaction. `--grid blx,bly,brx,bry,frx,fry,flx,fly` (needs `--place`, a place with a grid) gives the grid's back-left, back-right, front-right and front-left corners as fractions of the upright photo and adds a crop for every placed box, mapped with the photo's perspective and widened by a margin that grows with the box's height from its `size` (`1x2x1.5`: a tall box's rim leans out of its cells), never below 0.15 of a cell; a crop named by hand wins for its box. `--preview [note]` cuts and attaches nothing: it draws every crop it would make (each grid box framed on its cells, labelled with its back-left cell) on a temporary copy, `{preview, framed, sheet, marked, legend}`; the note titles it in `ev ui`. `attached`: `[NodeRef + photo, crop, path]`, and `sheet`: a contact sheet of every crop — each small, six to a row, labelled with its box's cell (`B3`), else its code or `#id` — written to the scratch folder of `photo mark`, to check a whole cut at a glance. Every cut and preview also draws `marked`: the whole photo with a numbered red frame on each crop, numbered in the order the crops were given and then the grid's boxes — from 1, or, when it is shown, with the frames' numbers in the marked photo series (the numbers this photo was marked with there, then the next free ones; see `ev focus`) — bare numbers as labels (null when nothing is cropped); `legend`: `[{n, ref: NodeRef, crop}]` says which record each number is, and the text output lists it as `1  #484 …`. Every cut and preview sends `marked` (and the preview, when it framed a grid's boxes on their cells; otherwise it shows the same crops under record ids) to a running `ev ui` as `ev focus --file <marked> --note <note>` does, titled with the preview's note or `--note`, else with each number and its code or name (`1 Düğme pil · 2 D-A1`), and adds `shown`. `--no-show` sends nothing (a script, or a picture the person should keep looking at); showing was opt-in once, and the person never saw what an agent that checked the frames itself had cut. `--show` is still accepted and changes nothing |
| `ev photo current <ref>` | the newest photo still shows the place well enough; off the photo-needed list until the next change, and a `photo_stale` mark it carried goes. A place with no photo exits 5 (`ev photo add` it) |
| `ev photo stale <ref> [--why t]` | the newest photo no longer shows the place, for a change the records never saw: a `photo_stale` mark (`note` = why); listed under `photos` with `photo_reason: "marked"` until a newer photo or `ev photo current`. An emptied place whose photo is older than its last change is listed too (`changed`); an empty place never photographed is not |
| `ev photo adopt` | copy photos still referenced outside the store into it |

`ev review <ref> --as toured` is refused (exit 5) while the place, or a placed box in its grid,
has no photo or only one older than its last change; `details.stale` lists them
(`node`, `reason`: `none`|`changed`|`marked`, `photo_at`, `changed_at`, `minutes_after`: how long
after the photo the records changed). The message names each place on a line of its own (`G-01
has no photo`, `K2-01-A's photo is 12 min older than its last change`), then what to do: a photo
to attach where there is none (the grid cut when the place has a grid), and `ev photo current
<ref>` only where a photo exists — first when every change came within two hours of its photo,
since the records may only have caught up with what it shows; ev does not decide that on its own. An empty place with no
photo at all needs none; an emptied one whose photo still shows what left needs a photo of it
empty, so its picture does not mislead.

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
| `ev review <ref> --as counting\|toured\|kept\|raw [--note t]` | how far a place has been counted: `raw` not counted (the default), `counting` its tour has begun (set by ev at the first work in it while a task on it, or on a holder above it, is in progress: a photo of it or of anything inside it, a thing recorded, moved, edited, gone or found inside it; a review event `{as: counting, by: <event>}`; starting or closing a task marks nothing, spec/counting.md), `toured` counted, `kept` left as it is on purpose; covers everything below it. The status it already has exits 5, except `toured` again once what it holds changed since. A thing is counted with its place: `--as` other than `raw` on an item exits 2. `toured` and `kept` answer with `left_here`: `{furniture, room}`, each `{node, places: [{node, status, tasks}]}` or null, the places still not counted in the same piece of furniture and, apart from those, in the same room, each with its open tasks (empty: in no task). `toured` also answers with `photo_check`: `{photo, located, not_located}`, the records in the place shown on its newest whole photo (a crop of theirs was cut from it) and those not yet; nothing is refused on it |
| `ev stats` | numbers about the home on one page (spec/stats.md), computed when asked: `overview` (`records`, `units`, `rooms`, `furniture`, `containers`, `in_several_places`, `with_photo`, `documents`), `value` (`cost` per currency of the things still here by their linked purchases, vehicles apart, each line's share by the units linked; `today` in today's money with how many `lines` of `of` could be converted; `things_with_cost` of `things`; `dearest`, the five that cost most today, each with `cost` (per currency, never added across currencies) and `today`; `valued`: the latest valuations, `things` and `latest` per currency), `vehicles` (each vehicle still here, a NodeRef with `cost` per currency and `today`), `rooms` (each with `records`, `units`, `holders`, `cost`), `tour` (`places`, `toured`, `counting`, `kept`, `raw`, `changed_since`, `things_in_counted_places` of `things`), `purchases` (`lines`, `linked`, `dismissed`, `open_durable`, `years` with `lines` and `paid`, the five `shops` with most lines, `buckets`: each bucket's `lines` and `paid`, delivered lines only; years, shops and buckets count the household's own spending: a line dismissed as `not-mine`, `duplicate`, `returned` or `elsewhere` is left out of them, one `consumed` or `given` stays), `activity` (the last 30 days: `added`, `moved`, `gone` by how, `photos`, `busiest_day`; a past thing recorded already gone is neither added nor gone in them, and a leaving taken back with `--correction`, a mistake or a joined portion is no thing gone), `holders` (`containers`, `empty`, `empty_not_known`, `with_fill`, `average_fill`, `full`, `most_records`), `coverage` (`coverages`, `active`, `ending`), `tags` (top 10 with `records`), `oldest` (the five bought longest ago, with `bought`). Read-only |
| `ev progress [<place>]` | every place gone through on its own (a unit: a holder with no labelled child, or a room with no furniture, box or room in it) with `review` (`status`, `at`, `from`, `changed_since`), `children`, `observations`, `planned` (a task is on it or on a holder above it), `tasks` (those open tasks, `{id, title}`); with a place (a piece of furniture, a room), only the places inside it, and `scope` (a box inside a place reads that place; a thing exits 2); counts `units`, `toured`, `kept`, `counting`, `raw`, `changed_since_tour` |
| `ev task add "<title>" --why "<why>" [--on ref]… [--at n] [--due YYYY-MM-DD]` | a task at position n (last by default); a malformed `--due` adds nothing. Every task carries `due` and `days_left` (negative when past) |
| `ev task list [--all]` | unfinished tasks in order (`position`), then closed ones with `--all` |
| `ev task start\|done\|drop\|reopen <id> [--note t]` | one task is in progress at a time: a start puts the one in progress back to open and says so (`stopped`: that task); `done` only when the person says so; `drop` refuses a done task and `start` a done or dropped one (exit 5): `reopen` it first; `done` or `drop` of a task already so exits 5 |
| `ev task edit <id> [--title] [--why] [--on ref]… [--off ref]… [--at n] [--due YYYY-MM-DD\|none]` | change a task; `--due none` clears the date |
| `ev next` | `goal`, `task` (the one in progress, else one due within a day or overdue, else the first; `picked`: `doing`, `due` or `order`; with `places`: each as `show`, plus `arriving` and `while_there` — what else `ev todo` lists in that place, by kind: `photos`, `labels`, `unclear`, `parked`, `leaving` (planned moves out), `disposals` (with `as`), `lost` (last seen there), `coverage` and `values` (things to ask about, dearest first); empty kinds are left out), `hints` (notes on the order, never applied: `due` with `due` and `days_left`, `settles_moves` with `moves` arriving at its places), `open_tasks`, `progress` (the counts of `ev progress`: `units`, `toured`, `kept`, `counting`, `raw`, `changed_since_tour`), `left_nearby` (while a task is in progress: the places not counted in the furniture its places are in that it does not cover, each `{node, status, tasks}`), `unplanned` (raw units no task covers, on them or on a holder above them; empty under `track`), `rules` |

A unit is the innermost labelled holder, or an unlabelled holder standing on its own in a room
or on furniture: a holder none of whose children is labelled as a place of its own. A labelled
child of an unlabelled holder is one; in a labelled holder a child is one when its code goes on
from the holder's (`K2-01-A` in `K2-01`, a drawer of that unit). A label of another series
(`B1_007` in `K2-01-A`, a labelled bin) is a box in the place, gone through with it: the
drawer stays the unit. `ev show` carries the node's
`review`, `observations` and `tasks`: the unfinished tasks linked to the node or to a place that
holds it, each with `via`, the node the link is on. In `ev ui`, list 2 (Yapılacak) lists the tasks with progress in its
title.

## Everything waiting

| Command | Does |
|---|---|
| `ev todo [--only <section>,…]` | `goal`, `progress` (as in `ev next`), `counts` and lists (`--only`: only those lists, with the counts of all; a section ev does not have exits 2): `tasks`, `moves`, `errands`, `disposals` (sell entries carry `sale`), `labels`, `needs`, `repairs`, `expiring` (`expires`, `days_left`), `lost`, `uncounted` (places not counted yet or being counted, from `ev progress`), `parked` (things waiting for their final place, each with `why`: `place` when put straight into a `temporary` place, `own` when an item is marked `temporary` itself (what is inside it is not listed: it travels with it), `waits_for` when its place waits for another record and neither applies; with `in` and, when set, `waits_for`), `stale` (organize only), `unclear` (names containing "belirsiz", "muhtemelen" or "?"), `shared_photos` (a whole photo attached to several live nodes, with `nodes`), `photos` (units with contents and no photo of their own, `photo_reason: none`, or whose contents changed after it, `changed` with `photo_at` and `changed_at`; a move out counts; a crop attached to the place itself counts as its photo, crops on the things inside do not; a holder with a grid is checked too, since its photo is what its boxes' crops are cut from, and carries `grid: true`, unless every part in it has a whole photo of its own (a compartment holding two photographed drawers is never photographed as an empty frame); each says `when`: `now` where its place is toured, kept or being counted, `on_tour` elsewhere, since a tour changes the place and a photo taken before it would go stale; `counts.photos_now` counts the first, and the text output lists only those), `coverage_ending` (coverages within the warning window, each with `nodes`), `coverage` (`count` of valuable things — a durable linked purchase in the home currency from `valuable_threshold` — with no coverage and no decision, `threshold`, `currency`, `top`: the five dearest, each with `worth`, its linked durable lines together as `ev stats` counts its cost, in today's money when the index is cached), `values` (`count` of things with a durable linked purchase and no value nor decision, `currency`, `top`: the five dearest, as for `coverage`); `counts.purchases` is the number of open durable purchase lines (delivered, something left to link, not dismissed, not joined to another line, not a kit's or a coverage's line); `ev stats` `open_durable` is the same number |
| `ev empty <ref>… [--note text]` | `empty` (the boxes, each with its own `note` when it has one, which may say otherwise: read it back) and `open_tasks` (`task`, `title`, `on`: tasks still open on a box, named, never closed: done or dropped is the person's word). The text of `ev tree` marks such a box `[empty]`. A box counted only through the tour of the place it is in is known empty only if it was there before that tour: one moved, found or recorded there afterwards was never opened on it. `ev show` of a box known to be empty carries `empty`: `{from: "said", at, note}` after `ev empty`, else `{from: "tour"}`; `ev ui` shows it as `[boş]` in the tree and a badge (with the day and note) in the details. On the person's word these boxes are empty (opened, nothing inside) though their place was never toured: an `empty` event with the note, and `ev find --empty`, `ev suggest`'s `empty` and `ev regroup`'s spares count them as known to be empty, and each is counted too (review `toured`, the same note): nothing is left to count, and something put in it later makes it changed since its tour. Only a container nothing is in; one with records in it is refused (exit 5, `inside`). All or none. `ev review <ref> --as raw` takes it back: what made a box known to be empty before then (its tour, an `empty`, something once in it) no longer counts |
| `ev label` | codes whose label still has to be printed; `ev label <ref>…` marks them printed, `--needed` marks them needed again. Setting or changing a code marks it needed |
| `ev broken <ref> [--note t]` / `ev fixed <ref>` | broken, and what is wrong / repaired (`fixed` on a thing not marked broken exits 5) |
| `ev expires <ref> <YYYY-MM-DD\|YYYY-MM>` / `--clear` | use-by date; `todo` shows it within 60 days or past |
| `ev sale <ref> --listed\|--reserved [--price n] [--where t] [--condition c]` / `--clear` | where a sale stands; only for a sell candidate; price and place carry over when not repeated; the price is in the home currency (`marks.sale.currency`). `--condition` (`new`, `like-new`, `used`) is what the buyer is told, kept as the `condition` mark; the only place a condition is recorded. `--clear` drops both |
| `ev need add "<text>" [--qty n] [--make] [--for ref] [--note t]` | something to buy (or make, e.g. 3D print); `--qty` below 1 exits 2 |
| `ev need list [--all]` · `ev need got <id>` · `ev need drop <id>` | open needs; close one |

`ev show` carries `marks` (`label`, `broken`, `expires`, `sale`, `condition`, `shred`, `photo_ok`,
`photo_stale`, each with `value`, `amount`, `note`, `at`; a sale's mark ends when the thing
leaves) and `needs` (open needs for the node). In `ev ui`, list 2 (Yapılacak) shows one
collapsible section per kind: Enter or → on a header opens and closes it, ← on a line goes up to
its header; unclear records start collapsed.
