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
- **What a sale brought:** `ev sold <x> --price 1500 [--currency EUR] [--via "…"]`. A sale that
  was listed carries where it was listed into the record, but not its asking price, which is not
  what the sale brought.
- **Purchases settle:** `ev buy link` and `ev buy add --for` work on a gone record, so a line of a
  past thing leaves the open lists for the right reason instead of being dismissed.
- **Reading it back:** `ev past` lists the past things by year with the money paid for them and
  got for them; `ev past --year 2018` shows what was ours that year, present things included,
  and counts apart what nothing dates instead of guessing it in. `ev stats` has a past section,
  and `ev ui` a Past tab on `0`, whose details say when each thing came and how it left. The tree,
  the to-do list and every count of today never see a past thing.

The schema moves to version 34 (a `departures` table and `nodes.came_at`); the database is
migrated when the new build first opens it.
