# ev — one thing kept in several places

Status: **implemented** (unreleased), 2026-10-03; the person settled the three open choices (a
note and photos are a portion's own, in use is read from the holder's kind, only items are
spread) before it was built. Written after the person asked for a better answer than copying
a record: 20 rechargeable AA cells, 2 in a flashlight, 2 in a toy, 16 spare in a drawer, are one
thing, not three records typed three times. Phases are in §8.

## 1. Decision

A thing can be kept in several places. Each place holds a **portion** of it: an ordinary record
with its own place and count. The portions of one thing share what the thing *is* (name, make,
model, size, tags) and are tied by one key, so ev can answer the questions copies cannot: how
many are there in all, where are they, how many are in use and how many spare, and do the
purchases account for them.

The person never manages portions. They say what happened to *some* of a thing, and the verbs
they already use take a count:

```
ev move <AA cells> --qty 2 --to <flashlight>      2 go into the flashlight; 18 stay
ev gone <AA cells in the toy> --as trash           the toy's 2 are dead
ev add --of <AA cells> --qty 4 --in <label maker>  4 more of the same, found in the label maker
ev move <flashlight's AA> --to <drawer>            back to the drawer: they join the 16 there
```

## 2. Why

| Way | What goes wrong |
|---|---|
| Copies (`ev add --like`, what every inventory app does) | The identity drifts: a model fixed on one copy is wrong on the others. The total is a sum over names that may not match. A purchase links to one copy; the others look unbought. Every move of 2 out of 20 means a new copy by hand, and moving them back leaves two records in one drawer. |
| One record with a side table of places | Everything in ev that is about a place assumes a record has one: the tree, a place's contents, tours and counting, photos framed by place, moves, lending, lost, labels, grid cells, sketches. A side table would need its own version of each. |
| A product catalog (identity in a table of its own, records point to it) | Every reader of a record (search and its stemming index, paths, placement, regrouping, audit, purchase matching, `ev ui`, more than a dozen modules) reads the name, make and model from the record. Moving them out rewrites all of them for no gain the person sees. |
| **Portions: records tied by a key, identity kept equal** | Everything that works on a record works on a portion unchanged. What is new is small: the key, keeping identity equal, counting across portions, and joining portions that meet in one place. |

Measured on one household (461 live items): 99 records already stand for several identical
things (560 units in all), 16 items sit inside another item (a device holding its cells), and
the same name already appears in two places twice with 41 of 102 places counted. Copies would
grow with every tour.

## 3. Data model

```sql
ALTER TABLE nodes ADD COLUMN thing INTEGER REFERENCES nodes(id);
CREATE INDEX nodes_thing ON nodes(thing);
```

- `thing` is the id of the thing's first record. It is NULL while a thing is kept in one place;
  the first time a part of it goes elsewhere, the original and the new portion both get it.
  "The thing" is every record with the same `thing`, gone ones included.
- **Shared (the identity):** `name`, `kind`, `make`, `model`, `size`, tags. They are equal on
  every live portion: setting one on any portion sets it on all of them in the same
  transaction, with an `edit` event on each (`via` the one named). This is the invariant a test
  holds.
- **Per portion (where it is and how it is):** place, `qty`, `note` (the person's words about
  *these*: "the flashlight's, charged in September"), photos (a crop shows the portion where it
  is), code, address, cells, sketch, lost, pending move, lending and owner, state and
  disposition, marks (broken, use-by date, label, sale), temporary.
- **Read across the thing:** purchase links, documents, coverage, valuations, web links,
  declined purchase candidates and kit links are written on the portion named, as today, and
  read over every portion of the thing. Nothing has to move when portions split or join, and a
  portion that left keeps what was linked to it.
- Only items are spread (`kind = item`). A record with a `serial` is one unit and is never
  spread. A holder with things inside it moves whole: which unit would hold them?

## 4. Verbs

### 4.1 A count on the verbs that already exist

`--qty <n>` acts on *n* of a portion. *n* below the portion's count splits them off as a new
portion first; *n* equal to it acts on the whole portion; more is refused (exit 5). An absent
count means one.

| Verb | With `--qty n` |
|---|---|
| `ev move <ref> --qty n --to <place>` | *n* go to the place, the rest stay |
| `ev move <ref> --qty n --to <place> --plan` | *n* are set apart now, in the same place, with the planned move; `ev done` takes them |
| `ev lend <ref> --qty n --to <place>` | *n* are lent |
| `ev dispose <ref> --qty n --as …` | *n* become candidates to leave |
| `ev gone <ref> --qty n --as …` | *n* leave |
| `ev lost <ref> --qty n` | *n* are missing from where they were |

A portion split off copies the source's state, disposition and owner, never its photos, note,
marks or links.

### 4.2 Joining in one place

A thing has at most one portion **of a kind** in a place. When a portion arrives (a move, a
`done`, a `found --in`, an `add --of`) where a live portion of the same thing already is, and
both are in the same condition (active, not lost, not lent, no pending move, same disposition
and owner), the counts add up in the one already there. The arriving record ends: state `gone`,
disposition `merged` (internal; never given on the command line), event `merged {into, qty}`,
and `joined {from, qty}` on the receiving portion. Its photos and history stay with it; its
task and kit links move to the receiving portion. Portions in different conditions stay apart:
the two lent to a neighbour are not the spare ones.

### 4.3 New verbs

- **`ev add --of <ref> [--qty n] --in <place>`**: more of a thing already recorded: a new portion
  with the thing's identity (or *n* more on its portion already there). For a tour that finds 4
  more cells in a drawer, and for the second lot that today starts as an empty record. Also a
  field of `ev add --stdin` lines: `{"of": "<ref>", "qty": 4, "in": "<place>"}`.
- **`ev join <ref> <ref>…`**: records made separately are one thing: copies made before this
  existed, or a pair `ev audit` lists as alike. The identity comes from the first; a make or
  model that differs is refused (exit 5) with both values, so the agent asks the person and
  edits first. Portions that end up in one place join (§4.2).
- **`ev unjoin <ref>`**: a portion is a thing of its own after all. It keeps its identity and
  leaves the key. A thing left with one portion keeps its key: purchases and documents linked
  on its gone or merged portions are read through it, and dropping the key would cut them off.
  With no other live portion, `ev show` shows the thing only while units gone beside it need
  accounting for.

## 5. What ev shows

- **`ev show` of a portion** adds the thing: `thing: ×20 in 3 places · in use 4 · spare 16`, then
  each other live portion with its path and count. A portion inside an item (a device, a toy)
  is **in use**; one in a container, on furniture or in a room is **spare**. Purchases,
  documents, coverage, values and links come from the whole thing, each marked with the portion
  it was linked on when that is not this one.
- **Accounted for**, a fact under the purchases, not a score: units bought (the thing's
  purchase links), here (live portions), and gone (portions gone as trash, give, sell; merged
  ones are not counted). `bought 20 · here 18 · gone 2` reads as settled; `bought 20 · here 16 ·
  gone 2` says 2 are unaccounted for; more here than bought says a purchase is missing, and is
  when purchase candidates are offered (§6).
- **`ev find`** groups the portions of one thing in the text: one line for the thing with its
  total and number of places, then each portion indented. In JSON each result keeps its own
  record and gains `thing: {id, total, places, in_use, spare}`; agents keep acting by the
  portion's id.
- **`ev ui`**: the Summary tab shows the total as a badge and lists the other places; a key
  jumps to the next portion of the same thing. The tree is unchanged: each portion stands in its
  place, as a record does today.
- **`ev history`** of a portion shows the splits and joins it took part in, each naming the
  other portion.

## 6. Elsewhere in ev

- **Purchase candidates** exclude a line already linked to any portion of the thing. `ev add
  --of` and a split offer candidates only when the thing has more units here than its purchases
  account for.
- **`ev audit`** stops listing the portions of one thing as "alike things split up": they are
  spread on purpose. A row whose names are the same thing word for word gains the hint `ev join`.
- **`ev regroup`** treats a portion inside an item as in use, not as a stray.
- **Kits**: a kit part linked to a portion is linked to the thing.

## 7. Events

`portion_out {qty, to}` on the source and `portion_in {from, qty}` on the new portion; `merged`
and `joined` (§4.2); `join {thing}` and `unjoin` for the new verbs. An identity edit applied
through another portion is an `edit` with `via`.

## 8. Phases

Each phase ends with the gate green and one measured check.

| # | Phase | Check |
|---|---|---|
| 1 | Schema 30 (`thing`); `ev move --qty` now and with `--plan`, `done`; joining on arrival; shared identity on `edit`; `ev show` and `ev ui` show the thing (total, places, in use, spare) | 20 cells: 2 to a flashlight, 2 to a toy, the flashlight's back to the drawer: 3 portions, then 2, totals right at every step; a model edited on one is on all |
| 2 | `ev add --of`, `ev join`, `ev unjoin`; audit stops listing portions and hints `ev join` | the household's two same-name pairs joined on the person's word, their totals right |
| 3 | `--qty` on `lend`, `dispose`, `gone`, `lost`; `found --in` joins | 2 of 20 thrown out: 18 here, `gone 2` |
| 4 | Purchases, documents, coverage, values and links read across the thing; accounted for; candidates by units | a 10-pack linked on one portion shows on all; 14 recorded offers a second purchase |
| 5 | `ev find` grouped; the `ev ui` jump key; README, REFERENCE, skill, release notes | an agent asked "where are my AA cells" answers from one call |

## 9. Not now

| Not taken | Why |
|---|---|
| Consumption and reorder (`need` raised when spare falls below a level) | the accounting in §5 comes first; a level is the person's to set once they see the counts |
| Spreading containers, furniture or serial-numbered things | an empty box is rarely counted in several places; a serial is one unit |
| Portions that differ in identity (variants: two colours) | that is two things; `ev unjoin` and two records say it |
| A history of the whole thing in one view | each portion's history names the other side; ask again when it is missed |
