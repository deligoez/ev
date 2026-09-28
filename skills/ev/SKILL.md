---
name: ev
description: Agent-first home inventory. Use when the user talks about where things are at home (shelves, drawers, boxes, storage, pantry, car), wants to record, find, move, give away, sell or throw out belongings, or mentions `ev`, labels like `K4x4-07-Ü` or `S5-01`, or a lost item.
---

# ev — home inventory for an agent in conversation

The person stands at the shelves and reports; you record through `ev`; either of you asks
`ev` where something is. `ev` prints JSON when piped, so read its stdout as JSON and its
stderr as the error channel.

## Model in one paragraph

Everything is a node in one tree: `home` › `room` (rooms may nest) › `furniture` ›
`container` › `item`. `kind` is only a label — any node can hold other nodes. Codes are
what is printed on the physical labels (`K4x4-07-Ü`, `S5-01`): use them exactly as the
person gives them, never invent one. A node leaves in two steps (`dispose --as` then
`gone`) or one (`gone --as`). A node whose place is unknown is `lost`.

## The conversation loop

1. **Record what the person reports.** Enter a box and its contents in one call with
   `ev add --stdin` (NDJSON; a line with `"key":"b"` can be referenced by later lines as
   `"in":"@b"`). Batches are all or nothing.
2. **Propose, don't assume a move happened.** Plan it with `ev move <x> --to <y> --plan`,
   tell the person where it goes, and only after they say they did it run `ev done <x>`.
   `ev pending` is the checklist. A planned move is also how you park a thing whose right
   place is not settled yet: plan it towards the likeliest holder, and when you later go
   through that holder, bring up everything planned to arrive there so the person decides
   whether it really belongs.
3. **Nothing is finished until the person says so.** A photo of "the current state" is not
   "done". Never mark a bag, drawer or task complete — and never `gone` a record — on your own
   reading; ask. A mistaken `gone` is corrected with `ev restore X --correction "<why>"`.
4. **Leaving the home.** Set aside: `ev dispose <x> --as trash|give|sell`. Actually gone:
   `ev gone <x>` (or `ev gone <x> --as trash` when it is thrown out on the spot). Record
   what the person said about it with `--why "<text>"` ("probably thrown out" is a reason,
   not certainty). A gone node is still reachable by id: `ev show <id> --include-gone`,
   `ev history <id>`, and `ev edit <id> note=…` to annotate it later.
   `ev disposals` lists what is waiting in each pile. A record that was never real (misread
   from a photo, entered twice) is closed with `ev gone <x> --as mistake --why "<what>"`, not
   as trash.
5. **What the person tells you about a thing goes into its `note`** — what it is for, where it
   came from, why they keep it ("a wrench that came with the kitchen tap, kept because it is
   often needed"). That knowledge is what makes a later decision possible; record it the moment
   they say it, in their words, with the date.
6. **Uncertainty goes into `note`**, never guessed into a field. If you are not sure what
   something is, ask the person, then record the answer.
7. **Lost:** `ev lost <x>` keeps the last seen place; `ev lost` lists them; `ev found <x>`
   clears it in place; any move clears it too.

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
- **Work one task.** `ev task start <id>`, go through its places box by box, bring up
  everything under `arriving`, and add new tasks when you find work elsewhere.
- **Close only on the person's word.** When they say the place is done: `ev review <place>
  --as toured` and `ev task done <id>`; then run `ev next` again and say what comes next.
  A toured place that changed later shows in `progress` as `changed_since`; mention it.

## Everything waiting: `ev todo`

When the person asks what is left, or a session starts with no clear task, run `ev todo` and
summarise by kind with counts ("19 tasks, 8 moves, 2 things to return to Mahmutlar, 8 things
leaving, 1 thing to buy"). Most of it is state on the records — never copy it into tasks; it
leaves the list when you record the thing itself (`done`, `gone`, `back`, `found`, …).
Record the kinds that have their own verbs as you meet them:

- **A code you set** needs a label: after adding or re-coding a holder, say it goes on the
  label list (`ev label` prints it for the label maker); `ev label <ref>` only when the person
  says the label is on.
- **Something to buy or make** (a box ran out, a battery is low, a gridfinity bin to print):
  `ev need add "<what>" [--qty n] [--make] [--for <place>]`; `ev need got <id>` when it
  arrived, then record it in the tree.
- **Broken:** `ev broken <x> --note "<what is wrong>"`; `ev fixed <x>`; if it will not be fixed,
  propose `ev dispose`.
- **A use-by date** seen on a package or photo: `ev expires <x> 2026-07`.
- **Selling:** after `ev dispose <x> --as sell`, `ev sale <x> --listed --price n --where …`,
  then `--reserved`; when it is sold, `ev gone <x>`.
- **Photo of the current state:** every place should have a whole-view photo of how it is now.
  `photos` in `ev todo` lists the places without one or changed since; after a move or a
  tour, ask for a fresh photo of each place it touched and attach it with
  `ev photo add <place> <file> --note "son hali, <date>"`. Older photos stay as history.
- **Unclear records** are names still guessed ("belirsiz", "muhtemelen"): ask about them when
  the person is at that place, then rename.

## Proposals the person can answer by number

Every table or list of proposed actions starts each row with a number (`#` column), so the
person can answer "did 1 and 3". Act only on the numbers they name; ask about the rest.

## Where should this go?

Never answer from memory. Run `ev suggest "<what it is>"` first and decide over its whole
output: the rules, where alike things already are, and every holder in `containers`
(`complete.containers` says how many; all of them are there). Say what you weighed —
"looked at N holders, rules X and Y; alike things are in A" — so the person can see the
answer was not recalled. Prefer putting a thing with its kind; mind the rules; offer an
empty or lightly filled holder when nothing alike exists. When the person corrects a
placement, record the reason as `ev rule add` or a container `theme`, so the next session
knows it. Run `ev audit` now and then to find alike things split up and holders without a
theme, and give holders a theme whenever you learn what they are for.

## Photos

When the person sends a photo of a drawer or box, first confirm its contents against the
records. Then `ev photo add <holder> <photo>` for the whole view, and for each box or item in
it `ev photo add <node> <photo> --crop x,y,w,h` with fractions of the upright photo. **Look at
every crop you cut** (open the stored file) and redo it if it shows the wrong thing — the crop
coordinates are your estimate, the check is what makes them right. Close-ups the person sends
later (screw heads, labels) go to the item, cropped to the thing itself, and a photo of an
empty holder goes to the holder. Photos are copied into
`~/.ev/photos`; the original may then be deleted.

## Going somewhere

Whenever the person says they are going somewhere or meeting someone ("yarın Mahmutlara
gidiyorum", "Ayşe gelecek"), run `ev for <place>` with the Turkish case suffix removed
(`Mahmutlara` → `Mahmutlar`) and tell them what to take, return and collect, with where each
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
record land as you make it. To look at a photo they press `o` (full screen, `[` `]` to step
through the node's photos) or `O` (system viewer); on the search tab `x` clears the search.

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
finds `K4x4-07-Ü`.

## Useful reads

- `ev find <text> [--tag t] [--kind k]` — where is it?
- `ev show <ref>` — one node, its path, children, pending move, disposition.
- `ev tree [<ref>] [--depth n]` — the whole picture.
- `ev history <ref>` — what happened to it.

Field reference and payload shapes: `REFERENCE.md` in the ev repository.
