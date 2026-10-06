Draft for the next release.

## Past belongings

Purchase mails reach back a decade and more, and they turn up things that are no longer in the
home: old phones, a console sold years ago, a screen left behind at a move. Until now these lines
stayed open forever, since there was no thing to link them to, and recording the thing now would
have stamped "gone today" and "here since today" on it, both false. ev can now record such a
thing the way it is remembered (spec/past-belongings.md).

- **In one step, in no place:** `ev add "<name>" --gone <how> --at 2019-05 --came 2016 --where
  "<former home>"`. Dates are partial (a year, a month or a day), so nobody has to invent a day.
  Batch lines take `gone`, `at`, `came` and `where` too.
- **Three new ways of leaving,** for what people actually say: `left` (left behind), `stolen`, and
  `unknown` ("sold or thrown out, I am not sure"). Only `ev gone` takes them.
- **A thing recorded now that left long ago:** `ev gone <x> --at 2016 --where "<place>"`; the
  `gone` event still keeps the moment it was written.
- **When a thing came:** `ev edit <x> came=2014-03`, on any record; a linked purchase's order date
  stands for it when not given.
- **When nobody knows when it left,** a past thing added without `--at` is listed under "when not
  known", apart from the years, rather than in the year it was typed in.
- **A gone record can be completed:** `ev edit` on a gone record now takes what the thing was
  (`name`, `came`, `qty`, `make`, `model`, `serial`, as well as the note), so ten panels thrown
  out are recorded as ten. Where it stands still cannot change. Documents and photos attach to
  a gone record by its id (`ev doc add … --for #512`, `ev photo add #512 …`): a sale mail or an
  old photo is part of what it was.
- **How a thing left reads in the past tense:** "(gone: sold)", "thrown out", "given away", "how
  not known", instead of the word for setting it aside ("sell").
- **What a sale brought:** `ev sold <x> --price 1500 [--currency EUR] [--via "…"]`. A sale that
  was listed carries where it was listed into the record, but not its asking price, which is not
  what the sale brought.
- **Purchases settle:** `ev buy link` and `ev buy add --for` work on a gone record, so a line of a
  past thing leaves the open lists for the right reason instead of being dismissed.
- **A swap is a way of leaving:** `--as trade` (or `--gone trade`) with `--traded-for <what came>`,
  or `ev traded <x> --for <y>` later; the record that came shows what went for it. A schema
  change (35).
- **Reading it back:** `ev past` lists the past things in two lists, what was remembered first
  and then what left the inventory on a tour (once real use filled one list with this week's
  cartons, the remembered things drowned), each by year with the money paid for them and
  got for them; `ev past --year 2018` shows what was ours that year, present things included,
  and counts apart what nothing dates instead of guessing it in. `ev stats` has a past section,
  and `ev ui` a Past tab on `0`, whose details say when each thing came and how it left. The tree,
  the to-do list and every count of today never see a past thing.

The schema moves to version 35 (a `departures` table with `traded_for`, and `nodes.came_at`); the
database is migrated when the new build first opens it, and the migration reaches the database
file at once, even while `ev ui` is open.

## Being counted means work has begun, and what is left is named

A person believed a cabinet was done except one drawer, when nine of its drawers had never been
opened: starting a task about them had marked all nine "being counted" at once, and a drawer of
the same cabinet that no task held sat in a list far from the cabinet being toured
(spec/counting.md).

- **Starting a task marks nothing.** A place becomes "being counted" at the first work in it
  while a task on it (or on its cabinet) is in progress: a photo, a thing recorded, moved,
  edited, gone or found inside it. Work outside a tour, such as one thing put away on an errand,
  counts nothing. Closing a task no longer puts begun places back to "not counted".
- **What is left is named.** `ev review <place> --as toured|kept` answers with the places of the
  same cabinet and room still not counted, each with its task or "in no task"; `ev next` lists,
  while a task is in progress, the places of its cabinet that the task does not cover.
- **One cabinet at a time:** `ev progress <place>` reads one piece of furniture or one room, and
  every place in `ev progress` now carries its open tasks. A task on a cabinet plans its drawers,
  so they no longer show as places no task covers.

## What was paid for and is never a thing

A developer membership, a game key, a diet programme: purchase mails hold over a thousand such
receipts, and a purchase line could only be linked to a thing or dismissed as consumed, so the
money either waited forever or read as eaten. Lines now take a `service` bucket beside
`digital` (`ev buy add --bucket`, or the adapter's `bucket`); neither ever waits to be linked
(`ev buy list --open` leaves them out), and `ev stats` shows what each bucket was paid, things,
clothing, digital and services apart.

## A warranty bought as its own line

An appliance order often carries an extended warranty as a second line. That line could only be
linked to the appliance, where it read as part of its price, or dismissed, where the money
vanished. `ev cover purchase <coverage> <line>` (or `ev cover add … --purchase <line>`) makes it
the coverage's line: the line is settled, the coverage shows what it was bought as, and its
price becomes the premium when none was given. Lines settled by a kit are now also left out of
`ev todo`'s and `ev stats`' open purchase counts, which still counted them.

## The final photo, checked against the records

`ev review <place> --as toured` answers with `photo_check`: the records in the place that its
newest whole photo shows (a crop of theirs was cut from it) and those it does not yet, so the
agent says which record is which thing before the person calls a place done. Nothing is refused
over it: closing a tour stays the person's word.

## Smaller fixes

Found by three QA rounds on a copy of a real inventory before this release.

- **Today is the local day.** Due dates, use-by dates, values, the day a thing was seen leaving
  and the busiest day in `ev stats` were read in UTC, so anything done after midnight but before
  the offset landed on the day before.
- **Leaving:** things set aside to trade or to return are in `ev disposals` and `ev todo`; a part
  sold from a listing keeps where it was listed, and a thing that leaves is no longer on sale;
  a sale said again keeps its currency, and one said before the thing leaves shows; a past thing
  taken back with `--correction` has a place not known instead of none; `ev restore` of a gone
  record says to add `--correction`.
- **Dates remembered:** a date still to come, or a coming after the leaving, is refused, and when
  and where a gone thing left can be said again (`ev edit <id> left=… left_in=…`). A past thing
  takes no `--to`, `--temporary` or `--code`, and `--where` names a place by its name, not a
  record or an id. A leaving is checked against the purchase that brought the thing too, not
  only a `came` said by hand. A date said after the leaving (`ev sold --at`, `left=`) dates it
  but leaves the thing in the list of those seen leaving, and a price said while waiting to
  sell is dropped when the thing is given or thrown out instead. Values and purchases entered
  by hand are not dated in the future either.
- **Swaps:** a thing is never traded for itself or for a place, nothing is half written when a
  swap is refused, and a swap drops a sale price said before. Only a thing given, sold or traded
  can be a swap; what came must still have been ours when the swap was and not before it; a swap
  said again with nothing new is refused; a gone thing is traded by its name too.
- **Purchases:** one open count in `ev todo` and `ev stats`, which `ev buy list --open --bucket
  durable` lists (returned, digital and service lines wait for nothing; lines settled by a kit no
  longer count); a line linked to a thing, dismissed, or already a coverage's is not taken by a
  coverage, checked before the coverage is written, and a coverage's premium and its currency
  follow its line; a link on the person's word takes back an earlier decline; a line bought
  after its thing left is refused, and no longer offered by `ev buy for`; a purchase entered by
  hand without a currency is in the home one; `--bucket` is checked. A purchase is linked to a
  thing only: never to a home, a room, a mistake, a joined portion or a digitized paper. A past
  thing's lines can be declined; a line linked to the thing cannot. A link of no units, or of a
  service, says why.
- **Numbers:** the dearest things in `ev stats` show what they cost per currency; two lines in
  different currencies were added as one. The last 30 days no longer count a past thing as
  added, nor a leaving taken back as a correction, a mistake or a joined portion as gone. `ev
  past --year` counts a thing here today as ours this year, and one gone in a year as ours
  that year, whenever it came.
- **A command that changes nothing says so:** a dropped task is not done without being reopened,
  an open task is not reopened, a done or dropped task is not closed again, a decline is not
  cleared where there is none, nor a coverage's line, and a recode to the same code says it
  changed nothing.
- **Tasks and places:** a closed task starts only once reopened, and a start says which task it
  stopped; `ev progress` of a thing is refused and of a box reads its place. A thing is never
  reviewed as a place, a place never photographed has no photo to call current, and drawers
  labelled after their cabinet began to be counted are not counting until work in them starts.
- **Photos:** a file ev cannot open as an image is refused when it is added, not when it is
  cut later.
- **MCP:** a batch file named as standard input (`--batch /dev/stdin`) reads the call's `input`
  instead of hanging the server, and a call with no command is an error.
- **Text:** fields, dismissal reasons, coverage terms, need states, marks and history events read
  in words; moments read as local days; amounts in the reader's way; `ev fixed` needs something
  broken, and `ev photo current` drops the out-of-date mark. `ev broken` said again keeps the
  note; `ev suggest` gives scores as the reader writes numbers and names where a word was found;
  several sketched at once read as text; a note of several lines keeps its lines aligned; a
  closed task shows its due date without days left; a coverage shows its premium and `ev cover
  list` what each covers; a purchase marks a linked thing that left; history shows a sale's date
  and an emptied list as a dash; a portion that joined another names it; a thing that left is
  proposed no warranty.
