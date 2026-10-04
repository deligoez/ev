# A thing that waits for another

## Decision

A record can wait for another: `ev edit <thing> waits_for=<other>` (`waits_for=` clears it). It
says the thing's place is to be settled when the other is at hand — glue sticks parked until
the glue gun, recorded lost, turns up.

- **When the awaited one turns up, ev says so.** `ev found <other>` (and `--in`) answers with
  `waiting`: the records waiting for it, so the agent asks the person where they go now ("#793,
  #794 were waiting for this: put them next to it?"). Nothing is moved on its own.
- **`ev show <other>`** lists `waited_for_by`; `ev show <thing>` lists `waits_for`.
- **`ev todo`** shows, on each parked (`temporary`) row, what it waits for.
- **The wait ends when the thing moves**, as the `temporary` mark does: it has found its place.
  `waits_for=` ends it by hand.

The thing waits for a record, any record, lost or not; not for itself, and not for one inside
itself (it would never turn up separately).

## Why

The person parked the sticks and asked whether a task could make sure their place is reconsidered
when the gun is found. A task has no "when X is found"; the only link was prose in the gun's
note, read by nothing. A relation between the two records is read by `found`, `show` and `todo`.

## Model

Schema 33: `nodes.waits_for INTEGER REFERENCES nodes(id)`. An `edit` event records it like any
field.

## Not now

- Waiting for something other than a record (a purchase to arrive, a date).
- Waiting for several records at once.
