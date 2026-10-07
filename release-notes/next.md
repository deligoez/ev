Draft for the next release.

## Vehicles

A household's car is now a record of its own kind, `vehicle`, at the top of the tree beside the
homes (spec/vehicles-homes.md). It holds its compartments, a glovebox or a boot recorded as
containers under it and toured like any drawer, and it has a thing's life: make, model with its
year, the VIN as its serial and its plate as its code, purchases, insurance as coverage,
values, documents, and, once sold, `ev sold` and a place in `ev past` and `ev past --year`. A
past car is added in one step with its last plate. It is never inside a home and never lost
(a stolen car is gone, `--as stolen`), and its plate needs no label: it is printed on it
already. Two guards keep it from distorting the home: placement never proposes moving a
household thing into a car or out of one (asked about a car, it weighs that car's places only),
and `ev stats` counts vehicles apart (`vehicles`, each with its cost), so a car's price no
longer swamps the value of the home's things. Units fitted directly in a car count as in use,
those kept in its glovebox as spares. `ev ui` marks a vehicle ▭ and opens it on the first
screen.

## Leaving a home

A home is now left the way a box is: when nothing of ours is in it any more. `ev gone <home>
--as moved` closes a home moved out of, `--as sell` an owned one that was sold; its rooms are
its structure and leave with it, as a car's glovebox and boot do when the car is sold. Anything
else still in it refuses, listed by the room it is in, with the two ways to empty it: move it
where it goes, or say it stayed behind (`--as left`).

## Former homes

The homes lived in before the inventory are records now, for an address history with dates and
for where past things were left. `ev add "<name>" --kind home --gone moved --came 2012-11 --at
2015-04 --address "…"` adds one already left, and its papers attach to it like any record's. A
thing left there names it with `--where`, by its name or `#id`, and is recorded inside it:
`ev show <home> --include-gone` lists what we had and left there, and `ev past --where` finds
it. A place used for a former home until now becomes one with `ev place home <place>`: what
was left there moves into the new home record, and the place goes; a place that is another
household's (with errands) is refused.
