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
   `ev pending` is the checklist.
3. **Nothing is finished until the person says so.** A photo of "the current state" is not
   "done". Never mark a bag, drawer or task complete — and never `gone` a record — on your own
   reading; ask. A mistaken `gone` is corrected with `ev restore X --correction "<why>"`.
4. **Leaving the home.** Set aside: `ev dispose <x> --as trash|give|sell`. Actually gone:
   `ev gone <x>` (or `ev gone <x> --as trash` when it is thrown out on the spot).
   `ev disposals` lists what is waiting in each pile.
5. **Uncertainty goes into `note`**, never guessed into a field. If you are not sure what
   something is, ask the person, then record the answer.
6. **Lost:** `ev lost <x>` keeps the last seen place; `ev lost` lists them; `ev found <x>`
   clears it in place; any move clears it too.

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

## Going somewhere

Whenever the person says they are going somewhere or meeting someone ("yarın Mahmutlara
gidiyorum", "Ayşe gelecek"), run `ev for <place>` with the Turkish case suffix removed
(`Mahmutlara` → `Mahmutlar`) and tell them what to take, return and collect, with where each
thing is. Record new intentions as they come up: `ev edit X to=<place>` (take it there),
`ev edit X owner=<place>` (it is theirs), `ev lend X --to <place>` / `ev back X` (lent out).
Different names for the same household are aliases of one place (`ev place alias`); if two
places turn out to be the same, `ev place merge`.

## Let the person watch

Suggest `ev ui` in a second terminal at the start of a session: it is read-only and
refreshes on its own, highlighting whatever you just changed, so the person sees each
record land as you make it.

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
