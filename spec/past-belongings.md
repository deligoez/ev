# Past belongings: what was ours and left

## Decision

A thing that left the home long ago can be recorded the way it is remembered: in one step, with
the day (or month, or year) it left, how it left, and what it brought in when it was sold. It is an
ordinary record in state `gone`, so it never counts in the tree, the tour, `ev todo` or the
inventory's numbers, and it can hold what any record holds: purchases, documents, photos, a note.
Past things are read on their own: `ev past` lists them, by year and kind, with money out (what
was paid) and money in (what a sale brought).

Nothing is recorded until the person decides it is worth it. A clear "we do not record these" is a
valid outcome, and nothing in ev asks about past things on its own.

## Why

Purchase mails reach back a decade and more, and they turn up things that are no longer here: old
phones, a console set sold years ago, a keyboard sold, a screen left behind at a move, cables used
up. Today these lines stay open in `ev buy list --open` and in `ev todo`'s purchase count forever,
since there is no thing to link them to, and recording the thing now would stamp "gone today" and
"in the home since today" on it, both false. The person asked whether to keep them "since I bought
it once", and whether a marketplace's sale mails could be a source too.

## What exists today

- `ev gone <x> --as trash|give|sell|used|digitize|mistake|return` closes a record, stamped now.
- `ev sale <x> --listed|--reserved --price n --where …` describes a sale **in progress**, on a
  thing set aside to sell; the sale itself leaves no price or date on the record.
- `ev place add|alias|merge` names other households, for errands; a place is not a node.
- `ev buy link` links a purchase line to a record; a line nothing is linked to stays open.

## Data model

- **When it left:** `nodes.left_at`, a partial date as the person can give it: `2016`,
  `2016-06`, `2016-06-14`. Set by `ev gone` (today when not given). The `gone` event keeps the
  moment it was recorded, so the history still tells when the record was written.
- **When it came:** `nodes.came_at`, a partial date, for a thing recorded long after it arrived;
  a linked purchase's date stands for it when not given.
- **How it left:** the dispositions gain three values, for what the person actually says:
  - `left`: left behind (at a move, in a former home),
  - `stolen`,
  - `unknown`: "sold or thrown out, I am not sure". It must stay sayable without guessing.

  Broken and thrown out stays `trash` with a `--why`; given away stays `give`.
- **A sale after the fact:** a `departures` row per record: `price`, `currency`, `via` (the
  channel: a marketplace, a shop's trade-in, a friend), `at` (partial date), `note`. One row per
  record; a sale in progress (`ev sale`) becomes this row when the thing is `gone --as sell`, so
  the asked price is not lost.
- **Where it was:** `--where <place>`: a named place (`ev place add "Eski ev"`), reused for every
  thing left there, with aliases as today. A former home is a place, not a node: nothing is
  toured there.

## Verbs

- `ev gone <x> --as <how> [--at <date>] [--why "…"]`: a past date for a thing recorded now.
- `ev add "<name>" --gone <how> [--at <date>] [--came <date>] [--where <place>] [--make …]
  [--model …]`: a past thing recorded in one step, never placed in the tree. `--stdin` lines
  take `gone`, `at`, `came` and `where` too, so a batch from mails goes in at once.
- `ev sold <x> --price <n> [--currency EUR] [--at <date>] [--via "…"] [--note "…"]`: what a
  sale brought, on a thing gone (or going) as `sell`. A sale mail goes on it with `ev doc add
  <mail> --kind receipt --for <x>`.
- `ev buy link <line> <x>` works on a gone record: the line is settled and leaves the open
  lists. Today a line can only be dismissed, which says "never a thing", the wrong fact.
- `ev past [--year <y>] [--kind <word>] [--where <place>]`: the past things, newest gone first:
  name, came and left, how, where, paid (linked purchases) and got (`departures`), and per year
  what went out and came in, in the money of its day and in today's (spec/purchases.md §3.9).

## Not the same as

- **A record that was never real** stays `--as mistake`.
- **A thing given to a household that keeps it** stays `give`, and `ev for` keeps working as
  today.
- **Today's inventory** never counts a past thing: `ev stats` gains a "past" section of its own,
  and the tree, `ev todo`, `ev progress`, `ev suggest` and placement never see one.

## Sources

- **Purchase mails and exports** (already imported): the line gives the date bought, the price
  and the shop; linking it to the past record settles it.
- **A marketplace's sale mails:** an adapter beside the raw export, as for purchases
  (spec/purchases.md §6), emits `sale` lines: title, item number, price, shipping, currency,
  date paid. `ev buy import` keeps them as lines of their own kind (money in), linked to a
  record with `ev buy link` like a purchase, and `ev sold` reads its facts from a linked sale
  line.
- **A marketplace's question-and-answer mails** and **the person's memory**: the asked price
  is not the sale price. The person's word is the authority, so these become a `departures` row
  by hand (`ev sold`), the mail attached as a document when it helps.

## The skill

- Record past things only on the person's word, and only those they want kept. Ask once, when a
  source turns them up, and record the answer ("these phones yes, the cables no").
- Never guess how a thing left: `--as unknown` when the person is unsure.
- A partial date is a date (`--at 2016`); never invent a day.
- A repair or replacement part bought for a thing (a screen for an old phone, a battery for a
  tablet) is a purchase of that thing: link the line to it.

## Phases

1. `left_at`, `came_at`, the three new hows, `ev gone --at`, `ev add --gone` (and `--stdin`),
   `ev buy link` on a gone record. Schema change: back up first (CLAUDE.md).
2. `departures` and `ev sold`; `ev gone --as sell` carries a sale in progress into it.
3. `ev past` and the "past" section of `ev stats`.
4. Sale lines from a marketplace adapter (`type: sale`), linked like purchases.
5. In `ev ui`: a Past tab, if the person wants one.

## Open questions for the person

1. Which past things are worth a record: by kind (devices, furniture, bikes, consoles,
   cameras; never consumables or cables), by price, or case by case?
2. How should the list read: by kind, by year, money in and out per year, "what did I own in
   a given year"?
3. Is a phone that came after another a **successor** of it (a link: this one replaced that
   one), or only "the same kind, later"? Only a confirmed link would be recorded.
4. Is the other home where some things are kept today a home of the inventory (toured, in the
   tree) or a place outside it?
5. Do old photos of past things exist, and should they be attached?
6. Should past things appear in `ev ui` at all (a tab of their own), or only in `ev past`?

## Not now

- A successor chain (`replaces=`) until question 3 is answered.
- Reading sale prices from question-and-answer mails on its own: the person's word decides.
- Any reminder or task about past things.
