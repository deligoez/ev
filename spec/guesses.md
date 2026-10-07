# What is only a guess

## Decision (2026-10-08)

The inventory agent keeps two kinds of uncertainty in prose today (its report):

- **Things known only from the person's words** before their place is toured ("the Pi 400 is in
  the cabinet", a Mac mini, a USB disk). They live as observations on a place, so `ev find pi
  400` misses them, and nothing says to check them when the place is toured.
- **Facts it assumed**: a date written as "büyük ihtimalle", a coverage end assumed one year from
  purchase. A reminder then trusts the guess.

ev gets one verb for both: **`ev guess`**. A guess is said, shown and cleared; it is never
decided by ev (the agent proposes, the person confirms).

- **A record that is a guess:** `ev guess <ref>… [--note "<who said it, how sure>"]`. The record
  is a real record (findable, placeable, linkable to a purchase) carrying the mark. `ev add …
  --guess "<note>"` makes one in a step: what the person said is in the cabinet is added there,
  as a guess.
- **A field that is a guess:** `ev guess <ref> --field came --note "assumed from the box"`. The
  fields: `came`, `left`, `make`, `model`, `serial`, `qty`, `size`, and `cover:<id>` for the end
  of one of its coverages. Several `--field` at once.
- **Clearing:** `ev guess <ref> --clear` clears the record's guess, `--clear --field came` one
  field's. Setting a field again with `ev edit` (or `ev cover edit` for a coverage) clears its
  guess: a value said again is the person's word.
- **Where it shows:**
  - `ev show`: `guess` (`{note, at}`) and `guessed` (`[{field, note, at}]`); `ev ui` shows them
    on the summary, dimmed, with the note.
  - `ev find`: a result that is a guess carries `guess: true` and the text marks it `(guess)`,
    beside `place_count`.
  - `ev todo`: a `guesses` list, each record or field still a guess with its note, the place's
    tour being the moment to check them.
  - `ev review <place> toured`: the answer lists the guesses inside the place (`guesses`), so
    the agent asks about each: seen (clear the guess), not there (`ev gone --as mistake --why`
    or `ev lost`), or still unsure (leave it).
  - `ev cover list`: a coverage whose end is a guess says so (`end_guessed: true`).
- **Stored as marks** (`guess`, `guess:<field>`): no change of schema.

## Why

A guess written in prose cannot be found, counted or checked. Marking it keeps the record useful
now (it is found, it can wait for its purchase) and makes the tour the moment it is settled. One
verb for records and fields, since the agent meets both in the same sentence of the person's.

## Not now

- A scale of certainty (likely, unlikely): a guess is a guess until someone looks; the note says
  how sure.
- Guessed purchases or prices: a purchase line comes from a shop or the person, not a guess.
- Clearing guesses on a tour without the agent: the tour lists them, the person answers.
