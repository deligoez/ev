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

- **How it left, and when, where and for how much:** a `departures` row per gone record, all of
  it optional: `at` (a partial date as the person can give it: `2016`, `2016-06`,
  `2016-06-14`), `place_id` (where it was then), `price` and `currency` (what a sale brought),
  `via` (the channel: a marketplace, a shop's trade-in, a friend), `note`. With no `at`, the
  day of the `gone` event stands for it when the leaving was seen (`ev gone`); a past thing
  added already gone with no `at` left when nothing says, and is counted apart from the years.
  The event keeps the moment the record was written, so the history still tells both.
- **When it came:** `nodes.came_at`, a partial date, for a thing recorded long after it arrived;
  a linked purchase's order date stands for it when not given.
- **How it left:** the dispositions gain three values, for what the person actually says. Like
  `used`, only `gone` takes them (nothing is set aside to be stolen):
  - `left`: left behind (at a move, in a former home),
  - `stolen`,
  - `unknown`: "sold or thrown out, I am not sure". It must stay sayable without guessing.

  Broken and thrown out stays `trash` with a `--why`; given away stays `give`.
- **A sale in progress** (`ev sale --listed --price … --where …`) is carried into the row when the
  thing goes `--as sell`: where it was listed becomes what it went through. Its asking price is
  not carried: what a thing asked is not what it brought, and only `ev sold` says that.
- **Where it was:** `--where <place>`: a named place (`ev place add "Eski ev"`), reused for every
  thing left there, with aliases as today. A former home is a place, not a node: nothing is
  toured there.

## Verbs

- `ev gone <x> --as <how> [--at <date>] [--why "…"]`: a past date for a thing recorded now.
- `ev add "<name>" --gone <how> [--at <date>] [--came <date>] [--where <place>] [--make …]
  [--model …]`: a past thing recorded in one step, in no holder, so never in the tree.
  `--stdin` lines take `gone`, `at`, `came` and `where` too, so a batch from mails goes in at
  once.
- `ev gone <x> --as <how> [--at <date>] [--where <place>] [--why "…"]`: a thing recorded now
  that left long ago, with the day it left.
- `ev edit <x> came=<date>`: when it came, for any record.
- `ev sold <x> --price <n> [--currency EUR] [--at <date>] [--via "…"] [--note "…"]`: what a
  sale brought, on a thing gone (or set aside) as `sell`. A sale mail goes on it with `ev doc
  add <mail> --kind other --for <x>`.
- `ev buy link <line> <x>` works on a gone record: the line is settled and leaves the open
  lists. Today a line can only be dismissed, which says "never a thing", the wrong fact.
- `ev past [--name <word>] [--where <place>]`: the past things, last gone first: name, came and
  left, how, where, paid (its linked purchases) and got (its sale); and per year how many left
  and the money paid for them and got for them, by currency. Mistakes, joined portions and
  digitized papers are no past belongings and are left out.
- `ev past --year <y>`: **what was ours in that year**: every record, past or present, that came
  by the end of that year and had not left before it began. A record whose coming is not known
  (no `came`, no linked purchase) cannot be placed in a year: they are counted apart, never
  guessed in.

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
  is not the sale price. The person's word is the authority, so these become a sale by hand
  (`ev sold`), the mail attached as a document when it helps.

## The skill

- Record past things only on the person's word, and only those they want kept: the person
  decides case by case. Consumables and cables are the weakest candidates, never ruled out.
  Ask once when a source turns a batch up, and record the answer ("these phones yes, the cables
  no").
- Never guess how a thing left: `--as unknown` when the person is unsure.
- A partial date is a date (`--at 2016`); never invent a day.
- Old photos of a past thing are attached like any photo.
- A repair or replacement part bought for a thing (a screen for an old phone, a battery for a
  tablet) is a purchase of that thing: link the line to it.

## Phases

1. Schema 34: `departures`, `nodes.came_at`. The three new hows; `ev gone --at --where`; `ev add
   --gone --at --came --where` (and `--stdin`); `ev edit came=`; `ev buy link` on a gone
   record. A schema change: the real database is backed up before the build is installed.
2. `ev sold`; `ev gone --as sell` carries a sale in progress into the departure.
3. `ev past` (the list, the years, `--year`) and the "past" section of `ev stats`.
4. In `ev ui`: a Past tab, apart from the inventory.
5. Sale lines from a marketplace adapter (`type: sale`), linked like purchases.

## Decided with the person (2026-10-05)

1. **What is worth a record** is the person's call, case by case. Consumables are the weakest
   candidates, but "never" is not a rule.
2. **"What was ours then"** is the view that matters most (`ev past --year`); the other reads
   are ev's to choose.
3. **No successor chain.** Phones that followed each other are only the same kind, later.
4. **The other home** where some things are kept today is a question of the present, not of
   past belongings: a few things there are outside the home (`owner=`/`with=` and a place);
   more would need a home of their own in the tree. Not part of this spec.
5. **Old photos** of a past thing can be attached, as to any record.
6. **A tab of its own** in `ev ui`, never mixed with the inventory.
7. **`ev past` is everything that left**, not only what was recorded long after: what goes on a
   tour today (a carton thrown out, a cell used up) is listed with the things remembered from
   years ago. The year headings keep them apart, and "what left this year, and what it
   brought" is worth reading on its own.

## Not now

- A successor chain (`replaces=`): decided against.
- Reading sale prices from question-and-answer mails on its own: the person's word decides.
- Any reminder or task about past things.
