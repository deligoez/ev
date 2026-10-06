# Vehicles, and leaving a home

## Decision

A household's car is a thing it owns and a place things are kept, at once: bought, insured,
valued and one day sold like a thing, and with a glovebox and a boot whose contents are toured
like a drawer's. A home is left the way a box is: when nothing of ours is in it any more.
Decided with the person on 2026-10-07, through the inventory agent.

- **A new kind, `vehicle`**, at the top of the tree beside the homes, never inside one.
  - It holds: its compartments (glovebox, boot, door pockets) are `container`s under it, and
    their contents are toured like any place's.
  - It has a thing's life: `make`, `model` (the model year goes here: "Model 1.6
    2026"), `serial` (the VIN), `code` (the plate, the label printed on it; `ev recode` when a
    plate changes, the history keeping the old one), `came`, purchase links, coverage
    (traffic insurance and comprehensive insurance are two `insurance` coverages), values,
    documents and photos.
  - It leaves as a thing does: `ev gone --as sell|trade|stolen|unknown|trash` (scrapped is
    `trash` with a `--why`), `ev sold --via`, and `ev past`. Like any holder, only once nothing
    active is left in it.
- **Two guards**, so a vehicle does not distort the home:
  - placement (`ev suggest`, `ev regroup`, the tour's proposals) never proposes moving a
    household thing into a vehicle, or out of one into the home: what goes in the car is the
    person's call;
  - `ev stats` counts vehicles apart from the home's things: a car's price would otherwise
    swamp the value of everything else and lead the dearest five.
- **In use or spare** (a thing kept in several places, spec/portions): units directly in a
  vehicle are in use, as units in an item are. Its compartments are containers, so what is kept
  in a glovebox is spare: spare bulbs are spares. A single record (a fire extinguisher, a
  first-aid kit) is neither, as today.
- **A home is left when it is empty.** `ev gone <home> --as moved` (a rented or lent home moved
  out of; `moved` is a home's only), or `--as sell` with `ev sold` for one that was owned.
  - Its rooms are its structure and leave with it.
  - Everything else must be moved (`ev move … <new home>`) or gone (`--as left` for what stayed
    behind) first. The refusal lists what is left, by room, with the commands, so leaving a home
    is a short guided batch.
  - A home left keeps its record: `came`, `address`, the day it was left, what it cost and
    brought; it is a past thing.
- **Homes and vehicles lead the past.** `ev past` (and Past in `ev ui`) gives them first, in a
  section of their own: came and left, the address or the plate, what was paid and got, each in
  today's money too. `ev past --year <y>` lists them first: which home we lived in and which car
  we had that year. The tree shows only the current ones, as today.

## Why

The person asked for vehicles and past homes together, and decided:
1. a car is a container at the level of a home, not inside one;
2. its code is its plate;
3. its compartments are holders, toured like any place;
4. its purchase, its insurance and its papers belong to it;
5. a car that was sold shows in the past with what it cost and what it brought;
6. running costs (fuel, tolls, tax, servicing payments) stay in ak, the money tool;
7. "a past home is like a disposed thing: nothing may remain in it."

ev already holds most of it: a holder that is not empty cannot go (`still_holds`), and a thing's
life (`came`, purchases, coverage, values, `ev gone`, `ev sold`, `ev past`) works for any record
a purchase can be linked to. What it lacks is a kind that is both a top-level holder and a
thing, and a way to leave a home.

## Data model

No schema change: `kind` and `disposition` are text, checked by ev, not by the database.
- `kind` gains `vehicle`. An older ev cannot read a database that holds one.
- `disposition` gains `moved`, taken by a home only, and only by `ev gone`.

## Verbs

- `ev add "<name>" --kind vehicle [--code <plate>] [--make …] [--model …] [--serial <VIN>]
  [--came <date>]`: a vehicle, at the top; `--in` is refused. `ev add … --gone sell --at …`
  records a past one in one step, as for any past thing.
- `ev add "Torpido" --kind container --in <plate>`: a compartment, as any holder.
- `ev gone <home> --as moved [--at <date>]`: refused while anything but its rooms is active in
  it, listing what is left by room; its rooms go with it.

## Phases

1. The `vehicle` kind: at the top only, a holder with a thing's life; the two guards; in use
   directly in it. README, REFERENCE, the skill.
2. Leaving a home: `moved`, rooms leaving with it, the guided refusal.
3. The past: homes and vehicles first in `ev past`, `--year` and Past in `ev ui`.

## Open, with the person

- **Former homes as records.** Today a former home is a place (`--where`). Turning it into a
  home record that was left would let `ev show` say what we had there and left behind there,
  and `ev past --year` say where we lived. The person asked what it would be good for, since
  nothing of ours remains in one; on hold until they decide.
- **Service and inspection** (odometer, the two-yearly inspection as a due date): ak keeps what
  it cost; whether ev keeps what was done to the car is the person's call.

## Not now

- ak tagging its payments with a vehicle: when it comes, by the vehicle's id (`#id`), never by
  its plate, which can change.
