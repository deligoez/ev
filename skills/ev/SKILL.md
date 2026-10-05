---
name: ev
description: Agent-first home inventory. Use when the user talks about where things are at home (shelves, drawers, boxes, storage, pantry, car), wants to record, find, move, give away, sell or throw out belongings, or mentions `ev`, labels like `K4x4-07-Ü` or `S5-01`, or a lost item.
---

# ev — home inventory for an agent in conversation

The person stands at the shelves and reports; you record through `ev`; either of you asks
`ev` where something is. `ev` prints JSON when piped — parse it when you need ids or fields
to act on, and add `--text` when you only need to read the result — and its stderr is the
error channel. The JSON is one line and leaves out what has no value: a missing field is null,
a row names its place by `path_text`, and `show` carries only the sections it has.

## Two ways in: the CLI and MCP

This skill writes every command as `ev …`. With a shell, run it. Through MCP (`ev mcp`), the
same command is the `ev` tool with the same arguments: `ev move #12 --to S5-01 --plan` is
`{"args": ["move", "#12", "--to", "S5-01", "--plan"]}`, and the lines you would pipe into a
`--stdin` go in `input`. The tool returns the readable text by default (`format: "json"` for
the JSON). The commonest reads have tools of their own, read-only so the person's client can
let them run unasked: `next`, `todo`, `find`, `show`, `suggest`, `history`, `tree`; `photo`
returns a node's photo as an image. A numbered photo a command makes (`photo cut`,
`photo mark`) comes back as an image in the tool result: the person sees it in the
conversation, so show it there too when no `ev ui` is open. A refused call returns its reason
(`isError`); read it and change the call. Everything else in this skill holds either way.

## Model in one paragraph

Everything is a node in one tree: `home` › `room` (rooms may nest) › `furniture` ›
`container` › `item`. `kind` is only a label — any node can hold other nodes. Codes are
what is printed on the physical labels, and there are two kinds. **A slot of furniture has a
positional code** (`K4x4-07-Ü`: Kallax compartment 07, upper drawer): it says where, so it is
found by walking to it, and a slot that moves gets a new label. **A movable box has a serial
code** (`S5-01`, a Samla box; `GF1x1-001`, a gridfinity box): it says which box, and it stays
when the box moves to another drawer or room — where it is lives in the record (its parent,
its cell), never in its code. Gridfinity boxes are `GF<footprint>-NNN` here (`GF1x1-012`,
`GF1x2-003`), one series per footprint; the prefix is the household's choice (another may
use `G1x1-012`), and a series is whatever comes before the number. Use codes exactly as the
person gives them; for a new box, `code=GF1x1-*` takes the next free number of its series —
say it, since it goes on the label. When labels themselves change (two slots trade labels, a
series is renumbered), give all the new codes in one `ev recode A=B B=A …`, never one `edit` at
a time. A node leaves in two steps (`dispose --as` then `gone`) or one (`gone --as`). A node
whose place is unknown is `lost`. **One thing can be kept in several places**: each place holds
a portion, an ordinary record with its own count, and the portions share what the thing is
(name, make, model, size, tags) — item 8.

## The conversation loop

1. **Record what the person reports.** Enter a box and its contents in one call with
   `ev add --stdin` (NDJSON; a line with `"key":"b"` can be referenced by later lines as
   `"in":"@b"`). Batches are all or nothing.
2. **Propose, don't assume a move happened.** Plan it with `ev move <x> --to <y> --plan`,
   tell the person where it goes, and only after they say they did it run `ev done <x>`.
   `ev pending` is the checklist; a move the person turns down is dropped with `ev cancel <x>`.
   A planned move is also how you park a thing whose right
   place is not settled yet: plan it towards the likeliest holder, and when you later go
   through that holder, bring up everything planned to arrive there so the person decides
   whether it really belongs.
3. **Nothing is finished until the person says so.** A photo of "the current state" is not
   "done". Never mark a bag, drawer or task complete — and never `gone` a record — on your own
   reading; ask. A mistaken `gone` is corrected with `ev restore X --correction "<why>"`.
4. **Leaving the home.** Set aside: `ev dispose <x> --as trash|give|sell|digitize`. Actually gone:
   `ev gone <x>` (or `ev gone <x> --as trash` when it is thrown out on the spot). A box that goes
   while its contents stay is emptied first in one call, `ev move <a> <b> <c> --to <place>`. Record
   what the person said about it with `--why "<text>"` ("probably thrown out" is a reason,
   not certainty). A gone node is still reachable by id: `ev show <id> --include-gone`,
   `ev history <id>`, and `ev edit <id> note=…` to annotate it later.
   `ev disposals` lists what is waiting in each pile. A record that was never real (misread
   from a photo, entered twice) is closed with `ev gone <x> --as mistake --why "<what>"`, not
   as trash. What was **used up** (a tape run out, a dead cell, a spent consumable) leaves
   `--as used`, not as trash. When only some of a counted thing leave, say how many:
   `ev gone <x> --as used --qty 2` (also on `dispose`, `lend` and `lost`); the rest stay.
   **Paper worth keeping only as a picture** (a ticket, a letter, an old statement, a manual)
   is `ev dispose <x> --as digitize`: it waits in its pile while the person sorts, and is
   photographed later in one sitting. Its copy goes on its own record before it leaves (`ev
   gone` refuses without one): a photo or a crop when the paper is the thing; when the paper is
   about another thing (an invoice, a warranty card, a manual), a document of that kind linked
   to both (`ev doc add <file> --kind warranty --for <paper> --for <thing>`). **Read every copy
   before the paper goes:** write what it says into the record (date, event, place, issuer) —
   that text is what `ev find` finds later — and if you cannot read it, ask for a closer photo
   instead of letting the paper go; a `warnings` line on `gone` means the copy is small. A
   memento is tagged `hatıra` whether it is kept or digitized. One record per paper worth
   finding on its own; a stack of alike ones (boarding passes) can be one record with a photo
   of each and each listed in its note. Never digitize what may be needed as the original
   (deeds, diplomas, contracts, anything signed or stamped) or what the person keeps for the
   paper itself; ask. Anything with a name, a number or a barcode on it is `--shred`.
5. **What the person tells you about a thing goes into its `note`** — what it is for, where it
   came from, why they keep it ("a wrench that came with the kitchen tap, kept because it is
   often needed"). That knowledge is what makes a later decision possible; record it the moment
   they say it, in their words, with the date.
   **Read the label while the person holds the thing.** A make, a model code and a serial
   number (`make=Bosch model="GSB 13 RE"`, `model=PKM17EPPH4001`) go into those fields, not
   the note: they are what finds the thing again and ties it to its purchase and its invoice.
   Read them off the label or the photo; ask when a photo does not show them. The name stays
   the person's — what the thing is, then make and model (`Darbeli matkap, Bosch GSB 13 RE`).
6. **Uncertainty goes into `note`**, never guessed into a field. If you are not sure what
   something is, ask the person, then record the answer.
   **A set of separate parts is one record per part kind, not one for the set.** A soil
   moisture set (probe, comparator board, cable), a stepper with its driver, straight and
   angled headers: record each kind as its own item in the same holder, with its count, and
   say in each note what it belongs with. One record per bag of the set is too much (the sets
   are interchangeable); one record for the whole set hides the parts — a cable cannot be
   moved to the jumper wires, nor a board missed. A record that already holds several kinds is
   split with `ev split <x> "<part>=<n>"… --rename "<what the original keeps>"`, never by
   editing it and adding new records by hand: the split links them in the history. When the
   parts are some of the record's own units instead (two of four cells turn out another make),
   add `--take`: their counts come off the original's; without it the original keeps its count,
   since the parts are what each unit is made of. `--take` also splits empty boxes off a box
   record of several (two cases as one record, one emptied): what is inside stays in the
   original. A boxed kit that stays in its case is one item.
   **A bought kit is a checklist in `ev kit`.** When the person says what set things came from
   and its contents are known (a shop's list, an observation), record it once — `ev kit add
   "<name>" --copies <n> --part "<part>[=<per copy>]"…` — and `ev kit link <kit> <n> <record>`
   each part as it turns up. `ev kit show <kit>` then answers "what is still missing from the
   set" from the records; a part missing from its bag is a record marked `ev lost` and linked,
   so it is counted as lost, not forgotten. A part the list lacks is added with `ev kit part
   <kit> "<part>"`; a record linked as the wrong part leaves with `ev kit unlink <kit> <n>
   <record>`. **A set bought as one purchase line** is linked to
   that line once, on the kit (`ev kit purchase <kit> <line>`, or `--purchase` on `ev kit add`),
   never part by part and never by `ev buy pack`: the line is settled, and every linked part
   shows it as its purchase and is offered no other.
7. **Lost = place unknown.** `ev lost <x>` takes it out of where it was (that place is kept
   as "last seen") and lists it under "Unknown place" in the tree; `ev lost` lists them. `ev
   found <x>` puts it back where it was last seen, `ev found <x> --in <place>` where it turned
   up; any move clears it too. A thing whose place was never known is added `--lost`; given an
   `in`, that place is only where it was last seen and does not count as holding it (a box with
   only lost things recorded in it can still be called empty).
8. **One thing in several places: never copy a record.** When some of a counted thing are
   elsewhere (two of the twenty cells are in the flashlight), the person reports a fact:
   `ev move <x> --qty 2 --to <flashlight>` records it now (a move you only propose is still
   `--plan`). More of a thing already recorded turns up somewhere (four more cells inside the
   label maker): `ev add --of <x> --qty 4 --in <place>`, never a new `ev add` with the fields
   copied by hand. The same goes for a consumable replaced with the same one (a tape cassette
   used up, a new one put in): `ev gone <old> --as used`, then `ev add --of <old> --in <place>`;
   `--of` takes the gone record. Two records you see are one thing (the same name in two places, which
   `ev audit` points out with `ev join`): propose it, and on the person's word
   `ev join <a> <b>`; a different make or model is refused — ask which is right first; one
   that turns out a thing of its own after all leaves with `ev unjoin <x>`. Portions
   that meet in one place join on their own, so moving units back and forth is safe. Fix what
   the thing is on any portion and it is fixed on all; a note stays the portion's own ("the
   flashlight's, charged in September"). `ev show` tells the whole thing (`×20 in 3 places · in
   use 4 · spare 16`) and its account (`bought 20 · here 18 · gone: used up 2`). Report an
   account that does not add up as a fact and ask ("2 of the 20 bought are not recorded
   anywhere: used up, or somewhere we have not toured?"); never invent where they went. Units
   that are a different variant (another colour, another make) are another thing: `ev split
   --take`, not a portion. In `ev ui`, `p` walks the thing's places. **In use or spare is read
   from the holder's kind:** inside an item (a flashlight, a toy) is in use, inside a container
   is spare. So a case or box that only stores things (a battery case, a pill box) is a
   `container`, not an item; when `ev show` calls stored units "in use", ask whether their
   holder is a case and fix its kind on the person's word.

## The plan: start every session with `ev next`

`ev` keeps the work list, so no session starts from memory. Run `ev next` first and tell the
person, in one short paragraph, what today's task is and why, what is planned to arrive at its
places, and how far the whole home is (`progress`). Then:

- **Ask the goal once.** If `goal` is null, ask whether they want to put things in order
  (`ev goal organize`) or only keep the records right (`ev goal track`). Under `track`, never
  propose tidy-up tasks, moves or disposals of your own; record what they report, answer where
  things are, and keep only the tasks they ask for. Under `organize`, a place they want left
  alone is `ev review <place> --as kept` — respect it and drop it from proposals.
- **Build the plan from what you see.** When looking at a place's photo, write what you notice
  as `ev observe <place> "<text>" --photo n` (a bag of mixed screws, a box that does not match
  the drawer's theme, things that belong elsewhere). **The records are the current state; a
  photo is the moment it was taken.** Before writing an observation from a photo, compare it
  with `ev show <place>` — things may have been moved out, thrown away or sorted since — and
  say which one you describe. Turn it into tasks with
  `ev task add "<title>" --why "<reason>" --on <place> --at <n>`. The reason is what makes the
  order defensible: mess, uncertainty (records guessed from a photo), blocking (a planned move
  waits on it), payoff (space freed, things to give or sell). Show proposed orderings as a
  numbered table and let the person reorder; `unplanned` in `ev next` lists raw places no task
  covers yet.
- **A day the person names is `--due`, never prose.** "I want to do this in a day or two",
  "before Saturday": ask for the day if it is vague and record it with `ev task add … --due
  YYYY-MM-DD` or `ev task edit <id> --due …`. `ev next` then puts the task first a day before
  (`picked: due`) and lists it under `hints` until it is done; a date written only in the
  reason is read by nothing. The order is still the person's: never rank tasks by a score of
  your own, and bring `hints` up as questions ("#21 is due tomorrow", "#9 settles 4 planned
  moves").
- **Work one task.** `ev task start <id>`, go through its places box by box, bring up
  everything under `arriving`, and add new tasks when you find work elsewhere. **Work its
  `while_there` list in the same visit:** the photo, the labels, the unclear names, the
  things leaving or waiting for their place, the coverage and value questions that sit in that
  place. They are not ranked on their own; the drawer is open anyway.
- **Close only on the person's word.** Before you ask, give the place its theme (see **Give
  every place a theme**). When they say the place is done: `ev review <place>
  --as toured` and `ev task done <id>`; then run `ev next` again and say what comes next.
  A toured place that changed later shows in `ev progress` (every place and how far it is) as
  `changed_since`; mention it.

**How many, how much, how far: `ev stats`.** When the person asks how big the inventory is,
what it cost, how far the counting has come or where the purchases stand, read `ev stats` and
answer from it (numbers, never a score); `ev ui` shows the same on its Statistics tab (`9`).
Say what a sum covers: the cost is only that of things with a linked purchase ("known for 12
of 300 records").

## Everything waiting: `ev todo`

When the person asks what is left, or a session starts with no clear task, run `ev todo` and
summarise by kind with counts ("19 tasks, 8 moves, 2 things to return to Annemler, 8 things
leaving, 1 thing to buy"); for photos say `photos_now`, the ones needed today — the rest are
taken on their places' tours, and asking for them earlier only makes photos that go stale. Most
of it is state on the records — never copy it into tasks; it
leaves the list when you record the thing itself (`done`, `gone`, `back`, `found`, …).
Record the kinds that have their own verbs as you meet them:

- **A code you set** needs a label: after adding or re-coding a holder, say it goes on the
  label list (`ev label` prints it for the label maker); `ev label <ref>` only when the person
  says the label is on. While the person sticks labels on a gridded drawer's boxes, show them
  which goes where: `ev photo mark <drawer> --codes --show "labels"` draws each box's code on its
  own cells of the drawer's photo, and `ev grid <drawer>` lists cell, code and name.
- **Something to buy or make** (a box ran out, a battery is low, a gridfinity bin to print):
  `ev need add "<what>" [--qty n] [--make] [--for <place>]`; `ev need got <id>` when it
  arrived, then record it in the tree.
- **Broken:** `ev broken <x> --note "<what is wrong>"`; `ev fixed <x>`; if it will not be fixed,
  propose `ev dispose`.
- **A use-by date** seen on a package or photo: `ev expires <x> 2026-07`.
- **Selling:** after `ev dispose <x> --as sell`, `ev sale <x> --listed --price n --where …
  --condition new|like-new|used` (ask the condition here, and only here),
  then `--reserved`; when it is sold, `ev gone <x>`.
- **Photo of the current state:** every place should have a photo of how it is now — its own
  photo, or a crop cut for it from a wider one (a box out of a drawer photo).
  `photos` in `ev todo` lists the places without one or changed since; after a move or a
  tour, ask for a fresh photo of each place it touched and attach it (see **Photos**) with
  `--note "son hali, <date>"`. Older photos stay as history.
- **Gridfinity drawers:** give the drawer a grid once (`ev grid <drawer> --cols 6 --rows 7`,
  row 1 at the back) and place each box with `ev cell <box>=A3-B3`. Then `ev grid <drawer>`
  draws the map and lists free cells — propose places for new boxes from it, and name them by
  cell. When boxes trade places, give all of them their new cells in one
  `ev cell A=A3-B3 B=A4 C=B4` once the person has moved them: overlaps are checked against
  the final layout. Each box keeps its code: it is the box's serial label, not its place.
- **A box with fixed compartments** (a tool case of four, an organiser) is a grid too: give it
  its layout once (`ev grid <box> --cols 2 --rows 2`) and put each thing in its compartment
  with `ev cell "<thing>"=A1 "<other thing>"=A1 …`: things share a compartment (two kinds of
  disc in one), a box keeps its cells to itself. Agree with the person which corner is A1 (row
  1 is the far side seen from above). `ev grid <box>` then lists what is in each compartment, to
  check against a photo. A compartment is a record of its own only when it is a box that comes
  out. A move within one box is a new cell, never `ev move` to the same box (refused).
- **The map:** `ev map <place>`, and `M` in `ev ui`, show a place as tiles: its grid, its
  sketch, or furniture stacked front on. Record how furniture stands as the person says it
  (`ev sketch <top> --on <bottom>` for a Kallax on another), give a piece of furniture a grid of
  its compartments seen from the front, row 1 at the top (`ev grid K4x4 --cols 4 --rows 4 --face
  front`, then `ev cell` per compartment), and a compartment with two drawers a one-column grid
  (`ev grid <c1> <c2>… --cols 1 --rows 2 --face front`, several at once). A drawer's grid is
  seen from above, the default. Sizes and positions come only from what the person measured,
  said or drew — never estimate one. Ask for as much as they can give, no more: nothing (the
  map lays rooms out as tiles), each room's width and depth and which room it is beside
  (`ev sketch Mutfak --size 300,500 --right-of Salon`, `--offset` to slide it along that side),
  or a plan. ev reads no plan file: read it yourself (a Sweet Home 3D `.sh3d` is a zip whose
  `Home.xml` has each room's corners in cm), ask which plan room is which record, and write one
  `ev sketch --stdin` line per room with its `points` (a room inside a room relative to its
  room's top-left). Only the rooms go in, never cabinets, doors or windows unless asked; a room
  the plan lacks is found from its walls, never guessed. When the person asks where something
  is, `ev focus` it and suggest `M` in their `ev ui`.
- **Unclear records** are names still guessed ("belirsiz", "muhtemelen"): ask about them when
  the person is at that place, then rename.

## Proposals the person can answer by number

Every table or list of proposed actions starts each row with a number (`#` column), so the
person can answer "did 1 and 3". Act only on the numbers they name; ask about the rest.

## Where should this go?

Never answer from memory. The decision is `ev suggest`'s ranking plus your judgement over it,
in this order:

1. **Describe it the way the inventory is written.** Name + part code + what kind of thing it
   is, in Turkish and English when both are used here: `ev suggest "KY-018 LDR ışık sensörü
   modülü"`, not just `KY-018`. The category words are what find the family box. For a thing
   already recorded use `ev suggest --for <ref>`: its own name, tags and note are the query.
2. **Read the ranking with its reasons.** `similar` is best first; each entry says which words
   matched, from which field or thing, and how many points. A match on a `specific` word
   (marked `*` in text output) says what the thing is; a match only on common words ("sensör",
   "modül") says only its family.
3. **If `new_group_likely` is true, nothing here is this kind of thing.** Do not squeeze it into
   the least bad box: propose a new group — an empty box first (`ev suggest` lists them under
   `empty`, the thing's own room first; `ev find --empty` lists them all), a free cell (the
   drawer's `grid.free`), or a mixed box if it is one of a kind. The box takes the group's
   `theme` once the thing is in it.
4. **Check the rules and the room.** Every rule in `rules` applies. `room` comes from `fill`:
   `none` means the best box is full — say so and offer the next one or a bigger box; `unknown`
   or `stale` means estimate the fill from the photo (0/25/50/75/100) and record it with
   `ev edit <box> fill=N` before you rely on it.
5. **Read the place before you name it.** Run `ev show <place>` on the one you are about to
   propose: its observations and tasks may say it is only a stop on the way, and a place never
   gone through (`review` null or `raw` — `(not counted)` in `ev suggest`) is a guess to say as
   one. A parking place (`temporary`) is never the answer: `ev suggest` lists it apart under
   `parking`; if it is still where the thing should go for now, say it is a stop, not its place.
6. **Say what you weighed**: "best: F2 (ışık*, ldr*; 69% of the description), room yes; rule 6
   applies; next was C5". The person can then disagree with a reason, not a guess.
7. **Turn corrections into data.** When the person picks another place, record why: a `theme`
   on the box, a rule (`ev rule add`), or a synonym (`ev synonym add "fotosel, ldr"`) when the
   miss was two words for one thing. Give boxes a `size` (`1x2x0.5`), so regrouping can offer
   the empty ones (a box with a size, no theme and nothing in it, known to be empty); a size written only in a box's name is not read —
   `ev audit` lists those under `size_drift`.

At the end of a drawer's tour, run `ev regroup <drawer>` and bring its findings as numbered
proposals: things better off elsewhere, mixed boxes, full boxes and the bigger spare box with
the cells it fits, nearly empty boxes to merge, unknown fills. `alone` lists things that share
no word with anything in their box: the other box there is only a guess (often wrong for
keepsakes and one-off tools, sometimes right, like a 9V battery among wires) — judge each one
yourself before proposing it. It only reports; nothing moves until the person says so. When
they say no to a move, record it: `ev regroup --decline <thing> --why "<their reason>"` —
later runs leave it out (listed under `declined`) until it is moved; `--allow` takes it back. The same
suggestions show in `ev ui` when the person selects the drawer or a box. Run `ev audit` now and
then for alike things split across the house.

**Keep kinds apart with facets, not with prose rules.** When a drawer keeps two kinds of
things apart that share words (modules and bare parts, novels and technical books), a rule in
plain words is not read by `suggest` or `regroup`. Make it a facet: `ev facet add modül --words
"modül, kart"`, `ev facet add çıplak`, and tag each holder with its facet (`ev edit <box>
tags=+modül`). Things inherit their holder's facet and new things take it from their words;
holders of another facet are listed apart under `other_facet`, never proposed.

**Give every place a theme.** A theme is the summary every placement answer leans on: places
without one are where suggestions go wrong. While touring, run `ev themes <drawer>` (the
details in `ev ui` show the same): for each place it lists the words its contents share and the
themed place they read most like. Write a short theme in the person's words from that and from
what you see ("Antenler ve anten kabloları"), say it, and record it with
`ev edit <place> theme="…"` once they agree. Do not theme a place that has not been toured.

**A theme describes the present, not the plan.** When the person reorganizes (merging drawers,
splitting one, moving parts between them), never defend a move against today's themes: propose
the layout that makes sense, then re-theme the places after the moves. When the last drawer of a
piece of furniture is toured, run `ev layout <furniture>` and bring what it finds (kinds spread
over several drawers, drawers that read alike, nearly empty and full or mixed ones) with marked
photos of the drawers named; when the person wants a new layout, `ev layout <furniture> --propose`
drafts one from the contents alone. It is a draft to change together, not a plan: plan each move
(`ev move … --plan`) and write each new theme only on their word.

**"For now" is data: `temporary`.** When the person puts something somewhere only until its
place is decided ("şimdilik buraya", "nihai yeri burası değil"), mark it the moment they say
it: `ev edit <place> temporary=true` when the whole place is a parking place, or `ev edit <thing>
temporary=true` for one thing waiting among things that do belong there. `ev todo` then lists
what waits (`parked`) and `ev suggest` stops offering the parking place. A thing's own mark
goes when it moves (like `lost`); a place's stays until you set it back — when the person says
the place is now final. **When its place waits for another thing** (glue sticks parked until the
lost glue gun turns up), say so with `ev edit <thing> waits_for=<other>`, never only in a note:
`ev found <other>` then lists it under `waiting` — ask the person where it goes now. Its move
ends the wait. What is not recorded is lost at the end of the conversation.

**Work you cannot do yet goes into the plan, not the records.** A move worked out before the
place is toured, a tag to add, a theme to decide: write it as `ev observe <place> "<text>"` so
`ev next` brings it up when that place's turn comes. Do not edit the records of a place that
has not been toured on the strength of the conversation alone; ask first. **Close the plan with
the work:** when you record what an observation asked for (the parts it said were waiting are
placed, the theme is set), remove it in the same step with `ev unobserve <id>` — the history
keeps both. Before you call a place done, read its `observations` in `ev show <place>`; one
left behind keeps coming back in `ev next` as work still to do.

## Documents

**An invoice, a warranty certificate or a manual the person shows goes into `ev doc add`,**
linked to the thing: `ev doc add <file> --kind invoice --for <ref> --issued <date> --issuer
<shop> [--number n]`. A photo of a paper invoice is a document, not a photo of the thing. The
file is copied into `ev`'s store, so the original may be deleted. When a thing breaks or is
sold, `ev show` lists its documents: read them out before the person goes looking.

## Purchases

**Import purchases into the person's inventory only when they ask.** The raw exports under
`~/.ev/purchases/` stay outside `ev` until the person says to bring a shop in; running an
adapter into their database on your own is not part of any other task.

**What was bought is evidence, not a record.** Purchase lines come from a shop's export
through an adapter you keep next to it, which also emits each line's product pictures as
`image` lines (`~/.ev/purchases/<shop>/adapter.py | ev buy import --stdin`), or by hand
(`ev buy add`); none of them is a thing in the tree. A new shop gets a new adapter: start from
`examples/purchases/` in the ev repository (its README walks through it) and keep the adapter
with the shop's raw export and its `RECIPE.md`.

**Say the right date:** a line shows `ordered …` (the day it was
bought, what the shop's order page says) and `delivered …` when it arrived; "bought on" is the
order date. When the person holds a thing that matches a line ("this is the drill I
bought from Amazon in 2024"), link it on their word: `ev buy link <line> <ref>`. The thing then
reaches the line's invoice and order page. A line that will never be a thing (eaten, given,
returned, someone else's) is settled with `ev buy dismiss <line> --as <reason>`. Never link on
your own reading of a name; ask.

**A pack or set whose units went into several records is one line, sized once.** When the
person's 8-pack is kept in two boxes, or a charger set's AA and AAA cells are separate records,
set the line's units per bought quantity with `ev buy pack <line> <n>` and link each record with
`ev buy link <line> <ref> --qty <its units>`; never leave the purchase as prose in a note.

**When `ev add`, `ev found` or an `ev edit` that sets make or model prints "Could be one of
these purchases", ask about the first one while the thing is in hand** ("is this the one bought
from <shop> on <date>?"); on a yes, `ev buy link`,
and offer the line's make and model when the record has none. `ev buy for <ref>` ranks lines
for a record added earlier; ask about a record only once its place has been toured (in a
place not toured yet, the record is still a guess: bring its candidates up during that tour).
"Is there a purchase of X?" before a record exists: `ev buy list --query "<words>"` (name, shop,
brand, product code; every word must match) — no line found means it was not imported.
For the back-fill and at the end of each tour, `ev buy for --toured` lists every unlinked thing
in a toured place with its one best line, numbered and best first: ask down that list by number
instead of looping over records yourself. The reasons say why: a shared model code is strong,
shared words are weak. Under My Roof is never imported; use it only to confirm a match.

**When the person says a candidate is not the thing, record it:** `ev buy decline <line> <ref>
--why "…"`. The line stays open for other things and is not offered to this one again; do not
`dismiss` it, which settles it for every thing.

**After a link, offer what came with the line in one question.** `ev buy link` shows the line's
`attachments` (a product page, a value, a warranty from the source, the shop's product
pictures): "bring along the invoice, 1 link, 3 product pictures, a value of 2,500 TRY
(approximate date), a 2-year warranty?" On a yes, `ev buy bring <line> <ref>`; `--only <id>,<id>`
for the ones they want, `--type image` for the pictures alone. Where the person has said the
pictures always come along (a household rule), bring them with the link without asking:
`ev buy bring <line> <ref> --type image`, or `ev buy bring --all --type image` after a run of
links. **Never bring silently:** each bring answers with what it brought and what it left and
why (`brought_types`, `skipped`, `left`); tell the person in one line ("brought 3 product
pictures to #12 Matkap; the value is still waiting"). A line linked to several things, or to
a thing that is gone, is left: ask which thing it belongs to. A product picture becomes a
document of kind `image`: it shows the product as sold, never the thing as it is now, so it
never replaces or counts as the thing's photo. On a thing kept in several places, a purchase
linked on one portion is the whole thing's: do not link it again on the others.

## Warranty, insurance and what is not tracked

**When the person mentions a warranty or an insurance, record it:** `ev cover add <ref> --kind
manufacturer|extended|store|statutory|insurance --term 2y [--from <date>|after:<id>] [--issuer
<brand>]`; an insurance takes `--ends`, `--number`, `--premium`, `--scope`. A certificate or a
policy goes in with `ev doc add <file> --kind warranty --coverage <id>`. `ev show` gives the
status; when it shows `coverage_proposal` (a two-year statutory warranty from a linked
purchase's delivery), offer it once; it is recorded only on a yes.

**Ask about coverage and value once, and respect the answer.** `ev todo` counts valuable things
with no coverage; ask about them while touring, dearest first. When the person says "don't
track this" or "not now", record it with `ev track <ref> coverage|value no|later --why "…"`,
on a whole box or drawer when they say so, and never bring it up again unless they ask.

**Write a value with its source and date:** `ev value <ref> 2500 --source "sahibinden ilanı"
--at <date>` when the person reports a price they saw; the purchase price is never a value.
`ev todo` counts bought things with no value; ask while touring, dearest first, and respect
"no" and "not now" as for coverage. A product page, manual or driver page goes in with
`ev link add <ref> <url> --kind manual`; add `--archive` (a saved copy or a Wayback address)
when the page may die.

**Prices in today's money.** A bought thing shows `≈ <amount> in <month> money` when the index
is cached; before quoting what something is worth, run `ev money status` and, if it is `stale`
or rates are missing, pipe `ev money needs` through `tools/money/fetch.py` into
`ev money import --stdin` (from the ev repository). Say which month's money a figure is in; it is the money's value, not the
thing's resale price.

## Photos

**A batch can span several messages.** A chat app may cap the photos per message (five in the
Claude app), and the person's notes belong next to the photos they describe. When a message
says more is coming ("devamı var"), answer only with a count ("aldım (5/…)") — no frames, no
table, no questions — and wait. When the person says the batch is done ("bitti"), work the whole
batch at once: number the photos in the order they arrived across the messages (1–5 the first,
6–10 the second, …), keep each note with the photos it came with, and answer in one table.

**Tell first, record after:** your first answer to a finished batch is short — what you see,
what the person should get ready, with the marked photo on screen; recording and cutting come
after it.

**Show what you read from every photo before you talk about it.** Frame each group you
recognise (what the person handles as one: a stack of LR44 cards of two makes is one group,
though records may be per make) with a numbered red frame, put that photo on their screen, and
talk in those numbers in a table. A photo cut among records: `ev photo cut <file>
<ref>=x,y,w,h… --note "<what this is>"` numbers the crops in the order given (then the grid's
boxes), puts it on their `ev ui` and returns `legend` (`n` → record). A photo you do not attach:
`ev photo mark <file> 1=x,y,w,h 2=… --show "<what this is>"` (bare numbers; the meaning goes in
your text). Cuts, previews and marks go to the screen on their own; never `--no-show` one the
person should see. **Open every marked file and check each
frame sits on its part before sending** — the coordinates are your estimate.

**Name every part by where it is in the photo, every time** — "left, the two on paper tape",
"top right, the big black one", "2nd from the left in the middle row" — in tables, proposals
and questions alike, even when you said it once further up: the person matches your words to
the picture.

**A photo of a place's current state ("son hali") is attached, not marked.** That rule is for
things to identify. When the person sends photos only to show how places look now, attach each to
its place, check it against the records, and frame nothing; mark only a thing you must ask about.
A thing missing from the photo is asked about in words, not by framing what is there. **Frame
nothing still means show it:** every photo the person sends goes into the series on their screen,
framed or not. `ev photo add` puts it there with its `--note`; a photo you do not attach goes
with `ev focus --file <photo>=<note>`.

**The marked photo series (işaretli foto serisi) is the person's, its numbers are ev's.**
Everything you put on their screen joins one series in `ev ui` until the person closes it with
`X`; Esc only hides it and `m` brings it back. Never decide where a series ends (`ev focus
--clear` only when they ask). ev numbers the frames across the series: write each photo's labels
from 1 and **quote the numbers ev returns** (`marks[].label`, the cut's `legend`) in your table,
never your own count. A number means one frame until the series is closed: a photo marked again
keeps its numbers and a frame added to it takes the next free one; a cut of a photo you marked
draws the numbers it was marked with, so give its crops in the order of the marks.
`ev focus --list` reads the series (each picture, its frames, the next number).

**Three kinds of reference, never mixed:** a bare number is a frame in the open series (`3`), `f`
and a number is a picture of the series (`f12`, as its title in `ev ui` reads), `#` and a number
is a record (`#12`). Number your table's rows with the frames' numbers. Commands take `f12` for a
photo (`ev photo cut f12 …`, `ev photo add <ref> f12`, `ev focus f12` to show it again), so you
carry no file paths. Attaching a batch already in the series: `ev photo add <ref> f2 f3 f4` for
one record, `ev photo add --stdin` with `{"ref": "…", "photo": "f5"}` a line for many; each
keeps the note it was sent with, and nothing is sent to the screen again.

**A question about one thing names it and shows it.** Before asking about a single thing ("where
did this go?"), mark it on the photo it is in, or `ev focus` it, so the question is on their
screen with the thing framed.

**Show a placement proposal, don't only write it.** Mark both ends with the same numbers: the
parts (`ev photo mark <file> "1 → A6"=x,y,w,h …`), then the destination's cells with the numbers
that mark returned, as drawn (`ev photo mark <drawer> 3=A6 4=B6 --keep-numbers`; `--grid
<corners>` for a photo cut before corners were kept). Both join the series; the person steps
with `[` `]`. Marked copies are temporary: nothing to undo after the move. The text still names
each part and its destination by position.

**Unpacking a bag: record in the bag, then plan.** When parts come out of a bag or box, record
each one in that bag first (`"in": "<bag>"`), then plan its move to the place you propose
(`ev move <x> --to <box> --plan`), and `ev done` it when the person says it is there. The bag's
`ev history --contents` then lists everything that came out of it, and `ev pending` holds what
was proposed and not yet put away — an unclear "I put them next to the others" stays a planned
move until it is confirmed, instead of a guess recorded as a fact. A new box for them is added
empty on its cell first, and the parts are planned into it like any other. Ask `ev suggest
--for <part>` (a text query ranks the bag itself first, since the part is in it). Do not write
"came out of bag #131" into notes: the bag's history already says it, and note words take part
in matching — bookkeeping there sends later suggestions to unrelated boxes.

**Don't ask for a drawer photo after every batch.** When parts go into a drawer batch by batch,
record each move as the person confirms it and ask for one photo of the drawer when the batches
are done; `ev todo` keeps the drawer on its photo list until then (a box added or changed after
the drawer's photo puts it back there, with `grid: true`).

**"It won't fit" is a guess until the person tries it.** Do not propose a bigger box because a
part looks too long for the one it belongs in: ask them to try it on edge or diagonally first
(the part is in their hand; a 1x1 gridfinity box is about 5 cm across its diagonal), and name
the bigger box only as the fallback. And a cell `ev grid` lists as `free` is only free in the
records — look at the drawer's photo before proposing a box there: a spare empty box may stand
in it. Record such a spare as its own empty box on its cell (`ev cell`; nothing in it makes it
empty, no tag needed),
not as a count on an unplaced record, so the grid shows it.

**Every photo of a place is attached the moment it arrives** — including one the person sends
only to confirm a state ("son hali bu mu?"). A photo that confirmed something and was not
attached leaves the place's current photo older than the place, which is the exact slip this
rule exists for. The newest photo is the current one; a place's older photos stay as history,
never replaced or removed. (A thing's group photo is different: see below — it is cropped per
record, and a record split in two loses the whole photo for its own crops.) **An emptied place
is photographed empty**: its older photo still shows what left, and would mislead as its current
one; `ev review --as toured` and `ev todo` ask for it. A place never photographed and empty
needs none. Never stand another place's photo in for it.

When the person sends a photo of a drawer or box, first confirm its contents against the
records. Then attach it in one step. **Look which way up it is first:** a photo that came in on
its side or upside down is turned before anything is cut — `--rotate 90|180|270` (clockwise) on
`ev photo add` or `ev photo cut`, every coordinate then a fraction of the turned photo; one
already attached turns for good with `ev photo rotate <ref> <n> <degrees>`, its crops with it
(never by hand: turned coordinates are easy to get backwards).

**A drawer with a grid is cut by its corners**:
`ev photo cut <photo> --place <drawer> --grid blx,bly,brx,bry,frx,fry,flx,fly` takes the grid's
four corners as fractions of the upright photo — back-left, back-right, front-right, front-left
— and cuts a crop for every placed box from the grid, with the photo's perspective, so no box is
left on an older photo. The corners are your estimate: run the same command with `--preview`
first (nothing is cut; every box is drawn framed on its cells) and look before cutting.
Otherwise `ev photo cut <photo> --place <holder> <box>=x,y,w,h
<item>=x,y,w,h …` puts the whole view on the holder and a crop on each box or item named
(`--place` is optional: a photo that is one crop of one thing, a card's front, is cut with
`ev photo cut <photo> <thing>=x,y,w,h` too, which gives the `sheet` and `--preview` that
`ev photo add <node> <photo> --crop x,y,w,h` does not); a crop named by hand wins over the
grid's for the same box. **Look at every crop you cut:** the cut (and its `--preview`) returns
a `sheet`, every crop small and labelled with its cell, to see that each shows the right thing,
and `marked`, the whole photo with each crop framed and numbered, to see that no frame cuts its
part off — the sheet is too small to show a cut edge. An estimate cuts edges off (a disc's rim,
a bit's shank) far more often than it takes in too much: give hand crops a margin with
`--pad 0.1`. Redo any crop that is wrong (a hand crop wins for its box). The coordinates are
your estimate, the check is what makes them right.

**A group photo goes whole on one node only — the place — and every thing in it gets its own
crop.** `ev photo add` refuses a whole photo that is already attached whole elsewhere; when it
does, cut the crop — do not reach for `--whole` to get past it. `shared_photos` in `ev todo`
lists any slip of this kind. A photo of what is inside a bag or box goes on the things in it once
they are recorded, not on the bag. Close-ups the person sends later (screw heads, labels) go to
the item, cropped to the thing itself, and a photo of an empty holder goes to the holder. Photos
are copied into `~/.ev/photos`; the original may then be deleted. **A photo that shows several
records is cropped per record — also when one record is later split into several:** remove the
whole photo from it and give each new record its own crop from every photo it appears in.
One `ev photo cut <file> <x>=<crop> <x>=<crop> <y>=<crop>…` gives a record as many crops as it
has things in the photo, all in one step.

**A tour is not finished on an old photo.** `ev review <place> --as toured` is refused while
the place or any placed box in its grid has no photo or one older than its last change
(`details.stale`); attach a current photo, or say an old one still holds with
`ev photo current <ref>`. **When a photo is old for a reason the records never saw** (the
place was emptied or rearranged before it was recorded, the person says "that photo is old"),
say so with `ev photo stale <ref> --why "<what changed>"`, never only in an observation or a
photo note: prose is read by no list, and the place keeps its old photo as current until a
newer one comes.

## Going somewhere

Whenever the person says they are going somewhere or meeting someone ("yarın Annemlere
gidiyorum", "Ayşe gelecek"), run `ev for <place>` with the Turkish case suffix removed
(`Annemlere` → `Annemler`) and tell them what to take, return and collect, with where each
thing is. Record new intentions as they come up: `ev edit X to=<place>` (take it there),
`ev edit X owner=<place>` (it is theirs), `ev lend X --to <place>` / `ev back X` (lent out).
Different names for the same household are aliases of one place (`ev place alias`); if two
places turn out to be the same, `ev place merge`.

## Show, don't describe

When the person asks which thing you mean, or you name something they may not recognise, run
`ev focus <x>` (add `--photo n` for a particular photo): their open `ev ui` jumps to it and
shows its photo full screen — usually the crop you cut. Say you did it.

## Let the person watch

Suggest `ev ui` in a second terminal at the start of a session: it is read-only and
refreshes on its own, highlighting whatever you just changed, so the person sees each
record land as you make it. `o` shows a photo full screen (`[` `]` step through them). The
details start with the node's `#id`: when the person says "#534", run commands on `#534` as it
is. Its History tab shows the same as `ev history <x> --contents`.

## Working through `ev`, not around it

Read results with `--text` (the readable output, also through a pipe); use the JSON when you
need ids to act on. Change many records at once with `ev edit --stdin` (NDJSON
`{"ref": …, "set": {…}}`, all or nothing), never with a loop of `ev edit` calls. **A script
that decides or writes around `ev`** — deriving fields in a loop, chaining lookups, collecting
file paths to check photos by hand — means `ev` lacks a verb: say so, and record it in the
repository's `BUGS.md`. Watch for this friction all the time: improving `ev` from how it is
used is part of every session.

## Exit codes

| Exit | Meaning | What you do |
|---|---|---|
| 0 | success | continue |
| 2 | malformed input | fix the command |
| 3 | no node matches | `ev find <text>`, then retry with the id |
| 4 | several nodes match | pick from `error.candidates` and retry with the id |
| 5 | a rule refused the change | read `error.message` / `error.details`; tell the person |
| 6 | database is newer than this `ev` | stop; ask the person to upgrade `ev` |

A partial name never resolves (`flipper` does not find "Flipper Zero"): search first,
then act by id. Codes and names compare case- and diacritic-insensitively, so `k4x4-07-u`
finds `K4x4-07-Ü`; a code's `-` and `_` are one, and a number's leading zeros do not count
(`S03_12` is `S3-12`). Record a code exactly as its label is printed; never relabel a box only to
change its separator.

`#N` alone is a node. After a record's word it is that record's own number, not a node:
`task #16` → `ev task show 16`, a purchase line `#1` → `ev buy show 1`, `doc #269` →
`ev doc show 269`, `need #3`, `cover #2`, `observed #22`. Those commands take `#16` or `16`.

## Useful reads

- `ev find <text> [--tag t] [--kind k] [--empty]` — where is it? `ev find --tag t` alone lists all tagged
  t: tag things that belong together but are scattered (`3d yazıcı`) so they can be gathered.
  `ev find --empty` lists the boxes known to be empty (their place toured, or something left
  them), worked out from the records: never tag a box empty. Boxes with nothing recorded only
  because nobody counted them come apart (`not_known`): open them before calling them empty.
  When the person says a box is empty (opened it, brought it empty), record it:
  `ev empty <box>… --note "…"`; that also counts it (toured), so it leaves the tour. A box called
  empty that turns out to hold things nobody counted is set back with `ev review <box> --as raw`:
  it is no longer known empty, and its contents wait for its tour.
- `ev show <ref>`, `ev tree [<ref>] [--depth n]`, `ev history <ref> [--contents]` (`--contents`
  adds what came in, went out or was added there).

Field reference and payload shapes: `REFERENCE.md` in the ev repository.

## Language and appearance

`ev ui` and the readable output are English or Turkish; JSON is always English. When the person
asks for the other language, or for a fixed light or dark look, change it for them with
`ev settings language tr|en|auto` or `ev settings theme dark|light|auto` — an open `ev ui`
follows within a second. `auto` is the default for both: the computer's language, and the
terminal's own light or dark background, followed live. `ev ui` reopens the tree as the person
left it (the same nodes open, headings closed and node selected); `ev settings resume off` if
they would rather start at the top.
