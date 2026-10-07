# ev

Agent-first home inventory. An AI agent records what a person reports at the shelves and
answers "where is it?". Homes, rooms, furniture, boxes and items live in one tree, beside the
household's cars (a car holds its glovebox and boot, and is bought, insured and sold like a thing), with planned
moves, a give/sell/trash pipeline, lost items, errands for other households, placement
suggestions, photos with crops, and a full history. A read-only terminal UI follows every
change live, and shows the agent's marked-up photos of what goes where the moment it sends
them.

## Install

```bash
brew install deligoez/tap/ev     # the `ev` binary (or: cargo install --locked --path cli)
npx skills add -g deligoez/ev   # the agent skill (skills/ev); update with `npx skills update -g`
```

The database lives at `~/.ev/ev.db` (`--db` or `EV_DB` to change it), photos next to it in
`~/.ev/photos`. Output is JSON when piped (one line, without the fields that have no value) and
readable text on a terminal (`--json` / `--text` to choose either way); errors go to stderr with
a distinct exit code (see `REFERENCE.md`).
Several agents, `ev ui` and the CLI can use one inventory at the same time; after every write
`ev.db` alone holds everything, so it can be committed or copied as one file (back it up with
`sqlite3 ev.db ".backup …"`, see `REFERENCE.md`).

### Connect your agent

An agent with a shell uses `ev` and the skill directly. Any MCP client (Claude Code, Codex,
OpenCode, Claude Desktop, Cursor) can also use it through `ev mcp`, a server over stdio, with
the person's own subscription and no key on ev's side:

```bash
claude mcp add ev -- ev mcp                 # Claude Code (--scope user: every project)
codex mcp add ev -- ev mcp                  # Codex
```

OpenCode: `"mcp": { "ev": { "type": "local", "command": ["ev", "mcp"] } }` in `opencode.json`.
The server has one general tool, `ev`, that runs any command from its arguments, read-only tools
for the common reads (`next`, `todo`, `find`, `show`, `suggest`, `history`, `tree`), `photo` to
see a photo, and a numbered photo a command makes comes back as an image. For clients with no
skills, the skill is the prompt `ev` and the resource `ev://skill`.

## What it does

**One tree.** `home` › `room` (rooms nest) › `furniture` › `container` › `item`, and any node can
hold others. Codes are the physical labels (`K4x4-07-Ü`, `S5-01`) and compare case- and
diacritic-insensitively, with `-` and `_` as one and a number's leading zeros ignored (`S03_12` is
`S3-12`): a label maker that cannot print a hyphen is no reason to relabel.

```bash
ev add Ev --kind home
ev add Salon --kind room --in Ev
ev add --stdin < box.ndjson      # a box and its contents in one all-or-nothing batch
ev find flipper                  # word search over name, code, note, theme, tags
ev find --tag "3d yazıcı"        # everything carrying a tag, no text needed
ev find --empty                  # the boxes known to be empty, worked out, no tag to keep
ev find --any esp32 "raspberry pi"  # several kinds of thing in one list, counted or guessed
ev empty S3-12 --note "came empty" # a box the person opened and found empty
ev show K4x4-07-Ü                # one node with its path, children and photos
ev show #534                     # any command takes the #id ev ui shows
ev tree Salon --depth 2          # the picture, with item totals; a series of boxes in code order
ev edit 391 qty=11 note="…"      # change fields
ev edit --stdin < edits.ndjson   # many records at once, all or nothing: {"ref":…,"set":{…}}
ev recode A3=A4 A4=A3            # swap or rotate codes when boxes trade places
ev grid 07-A --cols 6 --rows 7   # a gridfinity drawer: row 1 at the back, columns A…
ev cell GF1x2-003=A3-B3          # a box covers cells; ev grid 07-A draws the map
ev sketch K4x2 --on K4x4         # furniture standing on another, drawn with it
ev map K4x4                      # a place as tiles: its grid, its sketch or a stack
ev history 391                   # everything that happened to it
ev history 07-A --contents       # and to a place: what came in, went out, was added
```

**Moves are planned, then confirmed.** `ev move X --to Y --plan` records the intention;
`ev pending` is the checklist; `ev done X` / `ev cancel X` once it happened or did not (`ev done X
Y Z` for a box unpacked at once).

**Leaving the home.** `ev dispose X --as trash|give|sell` sets a thing aside; `ev disposals` shows
each pile; `ev gone X` (or `ev gone X --as trash --why "…"` in one step) records that it left. A
mistaken gone comes back with `ev restore X --correction "…"`. A record that should never have
existed (misread from a photo, entered twice) closes with `ev gone X --as mistake --why "…"`,
keeping its history without counting as thrown away. A paper kept only as a picture (a ticket,
a letter) leaves `--as digitize`: only once its photo or scan is on the record, and it stays in
`ev find`. `--shred` marks what goes in the bin shredded (an old ID card, a boarding pass).
What was used up (a tape run out, a dead cell) leaves `--as used`, and `--qty 2` lets two of a
counted thing go while the rest stay.

**What was ours.** Purchase mails reach back years, to things long gone: an old phone, a console
sold, a screen left behind at a move. Such a thing can be recorded as it is remembered, in one
step and in no place: `ev add "Oyun konsolu" --gone sell --at 2019-05 --came 2016 --where "Eski
ev"` (a year, a month or a day, never a day nobody said). How it left may be `left` (left
behind), `stolen`, or `unknown` (sold or thrown out, not sure). `ev gone X --at 2016 --where …`
does the same for a record already here, `ev edit X came=2014-03` says when any thing came, and
`ev sold X --price 1500 [--currency EUR] [--via "…"]` what a sale brought (a sale still listed
carries where it was listed; its asking price is not taken for what it brought). A purchase
line of a past thing is settled with `ev buy link`, as for any record. A swap leaves `--as
trade`, linked to what came in exchange (`--traded-for`, or later `ev traded X --for Y`). `ev
past` lists them in two lists, what was remembered first and then what left the inventory on a
tour, each by year with the money paid for them and got for them; `ev past --year 2018` is what
was ours that year, present things included, and what nothing dates is counted apart rather
than guessed in. `ev stats` has a past section of its own, and `ev ui` a Past list (`7`); the
tree, the to-do list and every count of today never see them. Nothing asks about the past on
its own: the person decides what is worth a record.

**Lost and found.** A lost thing's place is unknown: `ev lost X` lists it under "Unknown
place" with where it was last seen; `ev lost` lists them; `ev found X` puts it back there, `ev
found X --in Y` where it turned up.

**One thing, several places.** Twenty rechargeable cells, two in a flashlight, two in a toy and
sixteen spare in a drawer, are one thing kept in three places, not three records typed three
times. The verbs take a count: `ev move "Eneloop AA" --qty 2 --to "El feneri"` moves two and
leaves eighteen; `--qty` works on `ev lend`, `ev dispose`, `ev gone` and `ev lost` too. Each
place's portion is an ordinary record with its own count, note and photos, and what the thing
is (name, make, model, tags) stays the same on all of them: fix the model on one and it is fixed
everywhere. Moving the flashlight's two back to the drawer joins the sixteen there, so nothing
piles up. `ev show` and `ev ui` show the whole thing (`×20 in 3 places · in use 4 · spare 16`),
every other place, its purchases and documents whichever place they were linked on, and an
account: `bought 20 · here 18 · gone: used up 2`, or how many are unaccounted for. `ev find`
lists a thing's places together, and `p` in `ev ui` walks them. More of the same turns up:
`ev add --of "Eneloop AA" --qty 4 --in <box>`; two records made apart are one thing:
`ev join <a> <b>`.

**Parked for now.** A place where things only wait until their places are decided is marked
`ev edit X temporary=true` (or one thing waiting among things that belong there). Placement
never offers a parking place, `ev todo` lists what waits there, and a move takes a thing's
mark with it — "for now" stays in the records instead of in someone's memory. A thing whose
place waits for another (glue sticks until the lost glue gun turns up) says so with
`ev edit X waits_for=<other>`: `ev found <other>` then lists it, and its move ends the wait.

**One record per kind of thing, and what a kit still misses.** A set of parts recorded as one
thing becomes a record per part with `ev split X "LM393 kart=3" "Kablo=3" --rename "Prob"`;
the history links the pieces both ways, and the place's photo stays current (the same things
lie there, only recorded apart). When the parts are some of its units instead (two of four
cells are another make), `--take` takes their count off the original. A part that goes elsewhere
says so in the same step: `ev split X "USB-C kablo=2@S5-01"` moves it to S5-01, and `--stdin`
takes a destination or a way out per part (`{"name":"Kablo","gone":"trash","why":"broken"}`),
all in one transaction. A bought kit is a
checklist: `ev kit add "Proje seti" --copies 2
--part "RC522 okuyucu" --part "Kablo=3"`, `ev kit link "Proje seti" 1 <record>…` as its parts
turn up, and `ev kit show "Proje seti"` counts each part found, lost and still missing, from
the records themselves: find or move one and the kit follows. A set bought as one purchase line
is linked to it once, on the kit (`ev kit purchase "Proje seti" 412`): the line is settled and
every part shows that purchase, instead of each part being asked which line it was.

**Other households.** Places have aliases (`ev place add|alias|list|merge`). A node can be
meant for a place (`to=`), belong to one (`owner=`) or be lent out (`ev lend X --to P`,
`ev back X`). `ev for Annemler` answers "what do I take, return and collect when I go there?".

**Where should this go?** `ev suggest "<what it is>"` ranks the holders by how well their
theme, name, note and contents match, and shows why: the words that matched, from where, and
how many points each. It says how much of the description the best holder covers and flags a
thing nothing here is like (`new_group_likely`) with the empty boxes to start its group in
(those in its own room first), shows each holder's room from its fill, and
still lists every holder, the placement rules (`ev rule add|list|remove`) and synonyms
(`ev synonym add "fotosel, ldr"`). Turkish word forms meet (`kutuda` → `kutu`), part codes stay
whole (`KY-018`), and the same question always gets the same answer. `ev suggest --for <thing>`
places something already recorded by its own words.

**What could regroup?** `ev regroup <drawer>` asks the same question of every thing inside:
what would fit better in another box (and, apart, the guesses: things that share no word with
anything in their box, where the other box is only a hint), boxes that are mixed, full boxes
with a bigger spare box (an empty one with a `size`) and the cells it would fit in, nearly
empty boxes that could merge, and boxes whose fill is unknown or out of date. A move the
person says no to is kept (`ev regroup --decline <thing> --why "…"`) and not proposed again
until the thing is moved. Noun compounds
(`hesap makinesi`, `kablo bağı`) and colour-noun pairs (`yeşil LED`) are matched as such.
Facets keep kinds of things apart when placing (`ev facet add modül --words "modül, kart"`, then
tag the holders): a buzzer module is never proposed for the bare-buzzer box, a novel never for
the technical shelf.

**What would a new layout be?** Once every drawer of a piece of furniture is toured, `ev layout
<furniture>` reads all its places at once by what they hold, never by their themes: kinds of
thing kept in several places (a thing's kind is the end of its name before the first comma:
`Kablo, USB-C` is a cable, a `lens kapağı` a lens cap and no pen cap), places that read alike,
nearly empty places and the one they could join, full or mixed places. `--propose` drafts a
layout from the contents alone: each kind of three or more things, largest first, goes to the
place holding most of it, with the moves that takes and a theme per place; what sits in a bin of
its own, inside a device or in a kit stays, and a parking place gets no theme.
Nothing is moved; the person changes the draft and the moves are planned on their word.
`ev themes` lists the places with things in them and no theme, with what a theme could be read
from: the words their contents share and the themed place they read most like (a theme is the
summary every placement answer leans on, so the agent writes one from this with the person).
`ev audit` finds alike things split across places (and, for the same name in two places,
suggests `ev join`), holders without a theme, loose items, and boxes whose name says a size
their `size` field does not.

**Gridfinity drawers.** A drawer can be a grid (`ev grid <drawer> --cols 6 --rows 7`, row 1 at
the back) and each box covers cells in it (`ev cell <box>=A3-B3`). `ev grid <drawer>` draws the
map, lists the free cells and each box by its cells, code and name, `ev ui` shows it in the
drawer's details, and `ev suggest` names the free cells. Boxes trade places in one `ev cell`
call and keep their codes. Furniture is a grid too, seen from the front (`ev grid K4x4 --cols 4
--rows 4 --face front`, row 1 at the top); several holders take the same grid in one call. Movable
boxes get serial codes that stay on their labels wherever they go: `code=GF1x1-*` takes the
next free number of the series.

**A map to walk.** `ev map <place>` lays out what is in a place: on its grid, on a sketch in
centimetres, or on their own. A sketch takes what a person can say: a room's size and the room
it is beside (`ev sketch Mutfak --size 300,500 --right-of Salon`), a desk's place in a room
(`ev sketch <desk> --at 0,0 --size 120,60`), or a room's corners (`--points`, or many rooms
at once with `ev sketch --stdin`, the way a plan drawn elsewhere comes in); furniture standing
on another (`ev sketch K4x2 --on K4x4`) is drawn with it, front on, top first. In `ev ui`, `M`
opens it full screen on the home, the rooms first, with the way to the selected node already
chosen on every level: the arrows walk the tiles, Enter goes in — home, room, Kallax, drawer,
gridfinity box — Backspace comes back up, and `t` shows the chosen tile in the tree.

**A plan for tidying up.** The order of work is data, not the agent's memory. `ev progress`
counts the places a person opens one at a time (the innermost labelled holders, a drawer
with its labelled bins counted as one, and rooms
with nothing in them that holds things) as not counted, being counted, counted or left as
is (`ev review <place> --as counting|toured|kept`; a place is being counted once work in it
starts during a task on it, never because the task started), and flags counted ones that
changed since. `ev progress K4x4` reads one cabinet or room, each place with its tasks, and a
place settled names what is still not counted in the same cabinet and room. `ev observe` keeps what was
noticed about a place (`ev unobserve` closes a note once it is dealt with, and the place's
history keeps what it said); `ev task` is an ordered work list where every entry says why it
matters, and may carry the day it is due (`--due`); `ev next` hands over the current task (one
due within a day goes first) with its places, what is planned to arrive there, everything
else waiting in each place to do while it is open (a photo, labels, unclear names, things
leaving, coverage and value questions), notes on the order it never applies itself, and the
places no task covers yet. `ev goal organize|track` says whether the household
wants a tidy-up at all — under `track` ev only keeps the records.

**Numbers on one page.** `ev stats` says how much there is (records, units, rooms, boxes),
what it cost by the linked purchases (and in today's money), per room, how far the counting
has come, the purchases by year and shop, the last 30 days, the boxes known to be empty, the
coverages, the tags most used and the things bought longest ago; `ev ui` shows it as its
Statistics list (`8`), where a figure marked `›` opens the list behind it with `Enter` (a year's
or a shop's purchase lines, the counting's To do list, the past) and `Esc` comes back.

**Everything waiting, in one list.** `ev todo` gathers tasks, planned moves, errands, things
leaving, labels to print, things to buy or make, broken things, use-by dates, lost things,
uninventoried places, and places whose photo of the current state is missing or older than their
last change (listed where the place is counted; elsewhere the photo is taken on its tour, so
only their number is shown); a long list is read a part at a time (`ev todo --only tasks,needs`).
What is already state on a record is read where it lives and leaves the
list by its own verb, so nothing is kept twice. The kinds that had no state get small marks:
`ev label` (a new or changed code needs its label printed), `ev need add|list|got|drop`,
`ev broken` / `ev fixed`, `ev expires <x> 2026-07`, `ev sale <x> --listed --price n`
for a thing being sold, `ev photo current <x>` when the old photo still shows a place well
enough after a small change, and `ev photo stale <x> --why "…"` when it no longer does for a
change the records never saw (a drawer emptied before it was recorded).

**Photos.** `ev photo add X photo.jpg` copies a photo into the store and attaches it;
`--crop x,y,w,h` attaches a cut-out of a drawer photo to each box in it, remembering the
original. A crop on a box is that box's current photo. `ev photo cut drawer.jpg --place 07-A
A3=0.1,0.3,0.3,0.1 B4=…` does a whole drawer in one step: the whole view on the drawer, a crop
on each box. A group photo goes whole on one place only: attaching it whole to a second node is
refused (`--whole` when that is really meant), and `ev todo` lists older slips. One cut can
give the same record several crops (`ev photo cut sets.jpg 598=… 598=…`), one per set in the
photo. `ev photo list|remove|adopt`; a removed photo stays in the history, and a removed crop
no record uses any more leaves the store with it (a whole photo stays: it may be the only copy).
A photo taken on its side is turned for good: `--rotate 90|180|270` on `photo add` or
`photo cut` stores it turned, and `ev photo rotate X 1 90` turns one already attached, every crop
cut from it turned and cut again with it.

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
`ev todo` lists a drawer again when a box is added after its photo. Closing a tour also checks
the newest photo against the records: which things in the place have a crop cut from it and
which do not yet, so the agent says which record is which thing before the person calls the
place done.

**Showing what goes where.** When the agent proposes where the parts on the table go,
`ev photo mark parts.jpg "1 → A6"=0.10,0.20,0.15,0.12 "2 → C1"=… --show "batch 3"` draws numbered
red frames on the parts in a copy of the photo, and
`ev photo mark 07-A 1=A6 2=C1 --show "where" --keep-numbers` frames the target cells on the
drawer's own photo, by cell name. Each goes full screen in a running `ev ui` at once, as every
cut and preview does (`--no-show` to keep it off the
screen), into the **marked photo series**: everything shown piles up there, stepped with `[` `]`,
until the person closes it with `X`. ev numbers the frames across a series, so "3 at, 7 ver"
means one frame each until it is closed; a photo marked again keeps its numbers, and a cut of a
marked photo draws the same ones. Esc only hides the series (a stray click does not), and `m`
brings it back, even after a restart; `g` shows the whole series as a grid, a click opening a picture, as many a row as
the screen holds (`ev settings series_tile` sets the picture width), and `f12` then Enter goes
straight to a picture; `ev focus --list` reads it. A photo the agent only shows joins it too
(`ev focus --file photo.jpg="what this is"`), and a picture of the series is attached by its
number (`ev photo add X f12`), with the note it was sent with. The marked copies are scratch:
never stored, never attached, no history, cleared after a day; their red frames are edged in dark,
so they read on a red box too. `ev focus X [--photo n]` shows a recorded node,
so "which one do you mean?" is answered on screen too. While labels go on a gridded drawer's
boxes, `ev photo mark <drawer> --codes` writes each box's code in the corner of its own cells,
so the person reads which label goes on which box; a label keeps the letters as given.
For a batch, `ev photo sheet f16..f31` puts those pictures on one image for the agent to look at
together, and `ev photo mark --whole f2 f3 f4` frames each of them whole, numbered on, in one
call: a batch of photos of one thing each becomes frames the person can name.

**Watching.** `ev ui` is a read-only browser in three panes: a sidebar of lists, the list chosen,
and the selected record's details. The sidebar groups the lists under headings, each with the
digit that opens it and how many it holds: HOME has the layout (the tree, `1`), everything
waiting (one collapsible section per kind, `2`), pending moves (`3`), things leaving (`4`), lost
items (`5`) and errands (`6`); PURCHASES every purchase line (`9`) and one list per bucket
(durable, clothing, digital, service); HISTORY the past (`7`); INSIGHT the statistics (`8`); then
search (`/`) and settings (`0`). A purchase list keeps an order's lines under one heading with its
total, adds up what it shows per currency, and is narrowed with `f` (open, linked, dismissed) and
`/` (words, as they are typed); its details are the line's, and `Enter` opens the thing it is
linked to. `Tab` moves between the panes (the one the keys go to has a coloured
border); in the sidebar the arrows open each list as they pass it and `Enter` goes into it. `:` finds any list
or record by code, name or `#id`, typed with Turkish letters or without, and `Esc` steps back
through what was opened. The same `:` runs the screen's commands by name (the map, the sidebar,
opening the tree, rotating a photo, …), each shown with its key so the palette teaches it; a
query starting with `>` finds commands only, and only those that apply where you are. The
sidebar narrows to a rail of digits and counts under 120 columns, hides under 90 (`b` opens it
over the list), and under 70, as on a phone, one pane shows at a time; `b` hides or shows it at
any width and is remembered. It refreshes the moment another process writes, flashes what
changed, shows photos inline (Ghostty's graphics protocol, half-blocks elsewhere) newest first
with their note, full screen with `o` (`r` / `R` rotate it on screen), and in the system viewer
with `O`. Mouse works for the sidebar, rows, tabs, the wheel and photos. A mark before each row says its kind (⌂ home, ◫ room,
▥ furniture, □ box, · thing), and a row turns green once the node is settled: counted, with
nothing in `ev todo` hanging on it or on anything in it. A counted place's name is green with no
word beside it, since counted is where every place is headed; only the exceptions are written
(not counted, being counted, left as is, counted but changed since). Every node shows its `#id`, and every
command takes `#534` in place of a name or code. In the tree, `→`/`←` open and close one level,
`d` opens two (a Kallax with its compartments and their drawers), `e`/`c` open or close the
selected node with everything below it, and `C` closes the whole tree down to its closed rooms.
The key hints at the bottom show only the keys that do something on the screen at hand, and fit
the width, dropping the least useful first.

**The details pane.** Beside the tree, the selected node's details are split into tabs (`H`/`L`
or a click; only the tabs the node has something for are shown):

- **Summary** — its state as badges, then what it is, then sections in the order a person asks:
  Money (each purchase in one line with today's money), Coverage (its status in colour),
  Documents and links, To do, Note. A thing kept in several places shows its total and every
  other place (`p` goes there). Empty fields are not drawn (`E` shows the identity still to
  fill), long values wrap under their own column, and money reads the reader's way
  (`1.999,50 TL`).
- **Photos** — every photo, newest first, with when it was added, crop or whole, and its note.
- **Documents** — its invoices, manuals and policies, also through its purchases, and its links
  with the purchases' order and product pages; `[` `]` pick one, `O` or a click opens it in the
  program the system gives it, `y` copies its path or address.
- **Grid** — a drawer drawn as its plate, each box a frame over its cells; a box shows its place
  on its drawer's plate.
- **Contents**, and **Suggestions** — what `ev regroup` proposes there, guesses marked as such.
- **History** — newest first by day; a place's includes what came in, went out and was added
  (`ev history X --contents` prints the same).

A click on a line opens what it names — a photo, a thing inside, a thing in the history — and a
click on a box in any grid opens that box with the Grid tab still showing, so a drawer can be
walked box by box. The pane scrolls (`J`/`K`, the wheel). Drag the divider between the list and
the details, or under the photo, to resize (`<` `>` `{` `}` from the keyboard, a double click
resets), or widen the details with `+`; the sizes and the tab are kept.

**What it cost, what proves it, what still covers it.** A thing carries its make, model and
serial (`ev edit X make=Bosch model="GSB 13 RE"`), searched like a code. Invoices, manuals and
policies are copied into the store (`ev doc add invoice.pdf --kind invoice --for X`).
Purchases are a list of their own, never things in the tree: an adapter the agent writes for
each shop turns its saved export into lines (`… | ev buy import --stdin`; a worked example and
how to write one are in `examples/purchases/`), and a line is linked to a thing only on the
person's word (`ev buy link`);
`ev buy for --toured` asks it for the things of toured places at once, and `ev buy decline`
records a "not this one" so the line is not offered to that thing again. What was paid for but
is never a thing in the home goes in by its bucket, never waiting to be linked: `digital` (an
app, a download) and `service` (a repair, an installation), beside `durable` and `clothing`
(`ev buy add "Kurulum" --paid 500 --bucket service`, or later `ev buy bucket 412 digital`); the
account a line was billed to is listed and filters the list (`ev buy list --billed-to ayse`), and
so do the source and the key a line came with: `ev buy list --source ak --key 412` finds what one
of ak's payments (the household's money, spec/ak.md) bought, the payment's own line or its items.
`ev add`, `ev split`, `ev found` and an edit that sets the make or model offer the purchases a
record could be, with the reasons, while the thing is in hand; a purchase names its dates
(`ordered … · delivered …`); one purchase seen by two sources is joined (ak's lines by their order number, and
`ev buy join` for one ev cannot tell); what came with a line
(a link, a value, a warranty, the shop's product pictures) comes along with `ev buy bring`
(`--type image` for the pictures, `--all` for every linked line at once), the pictures as
documents shown with the photos in `ev ui`, never as the thing's own photos, and ev says what
it brought, what it left and why. Warranties and insurance have a computed status
(`ev cover add X --kind manufacturer --term 2y`; repair time extends it; an extended warranty
bought as a purchase line of its own settles that line and takes its price as the premium,
`ev cover purchase 3 412`), values are dated
observations (`ev value X 2500 --source "listing"`), links keep an archive copy
(`ev link add X <url> --archive page.html`), and "don't track this" or "not now" is never asked
again (`ev track X coverage no`). A price shows in today's money: `ev money needs`, piped
through `tools/money/fetch.py` into `ev money import --stdin`, caches the official price index
and exchange rates, so `ev` itself stays offline.

**English and Turkish, light and dark.** `ev ui` and the readable terminal output speak English
or Turkish: the computer's language by default, or the one picked on the Settings tab or with
`ev settings language en|tr|auto`. The appearance follows the terminal's light or dark
background live — switch the system or terminal theme and `ev ui` changes palette without a
restart — or is fixed with `ev settings theme dark|light`. `ev ui` reopens the tree as it was
left — the same nodes open, the same headings closed, the same node selected — and
`ev settings resume off` starts it fresh at the top. Settings
live in `~/.ev/settings.json`, outside the database; JSON output stays English.

## Documents

- `REFERENCE.md` — every command, field, key and payload shape
- `skills/ev/SKILL.md` — how an agent should use it in conversation
- `spec/` — the decision behind each feature: `0.1.0.md` the model, `purchases.md` what a thing
  cost, what proves it and what still covers it, `mcp.md` the MCP server, and one per feature since
- `release-notes/` — what changed in each version; `next.md` is the draft of the coming one
- `BUGS.md` — rough edges found in use, fixed in batches
- `QA.md` — a QA round on a copy of a real inventory (`tools/qa/sandbox.sh` makes the copy)

## Development

Workspace: `core` (model, rules, SQLite, photos) and `cli` (the `ev` binary and its UI). A Tauri
app is expected to reuse `core` later.

Quality gate: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo shear &&
cargo nextest run --workspace` ([cargo-shear](https://github.com/Boshen/cargo-shear) catches
unused dependencies and `.rs` files no `mod` declares; [nextest](https://nexte.st) runs the same
tests as `cargo test`, about 2.5 times faster here, and the workspace has no doc tests it would skip).
Clippy also fails on a function longer than `too-many-lines-threshold` in `clippy.toml`; the
threshold only goes down as functions are split, and the two flat dispatchers above it (the
command `match` in `main.rs`, the output shapes in `render.rs`) carry an `#[expect]` that fails
once they shrink below it.
The tests include one that reads every backticked `ev …` command in this README,
`REFERENCE.md`, the skill and `release-notes/next.md`, and fails on a subcommand or flag the CLI
does not have, so the documents cannot drift from the binary. `tools/measure/` scores stems and
placement against answer keys kept outside the repository. `tools/deadcode.sh` lists code no
one calls, or only tests call, across both crates (rustc's own lint cannot see a library's
`pub` items); it is a scan to read, not part of the gate.

### Releasing

Releases are batched: bugs collect in `BUGS.md` and the draft notes in `release-notes/next.md`,
and a version is cut only when asked. Before tagging:

1. **Check this README against the release.** Every user-visible change in `next.md` must be
   reflected here and in `REFERENCE.md` and `skills/ev/SKILL.md`; a feature missing from the
   README is not shipped.
2. Rename `release-notes/next.md` to `release-notes/vX.Y.Z.md`, drop its draft line and give it
   an opening paragraph; start a new `next.md` that reads only `Draft for the next release.`.
3. Bump `version` in `core/Cargo.toml` and `cli/Cargo.toml`, run the quality gate, commit.
4. Tag `vX.Y.Z` and push. The release workflow is [dist](https://opensource.axo.dev/cargo-dist/)'s
   (`dist-workspace.toml`; regenerate `.github/workflows/release.yml` with `dist generate` after
   changing it): every target builds natively on its own runner in parallel (macOS on macOS,
   Linux on x86 and ARM Linux), then the GitHub release gets the archives, `release-notes.yml`
   puts `release-notes/vX.Y.Z.md` on it, and the formula in `deligoez/homebrew-tap` is updated.
   A tag with a pre-release suffix (`v0.12.0-rc.1`) makes a pre-release and leaves the tap
   alone, which is how a change to the pipeline is tried.
