# A kit bought as one purchase line

## Decision

A kit can be linked to the purchase line it was bought as: `ev kit add … --purchase <line>`, or
`ev kit purchase <kit> <line>` for a kit recorded already (`--clear` takes it back). Then:

1. **The line is settled by the kit.** Nothing of it is left open (`open_qty` 0), so it leaves
   `ev buy list --open`, the purchases to link in `ev todo` and the back-fill. `ev buy show`
   and `ev buy list` name the kit on the line (`kits`).
2. **Every record linked to the kit's parts sees that purchase.** `ev show` lists it under
   `purchases` with `kit` (the kit's name) instead of a quantity of its own, and through it the
   invoice and the other documents that came with the line (`ev buy show`).
3. **A part asks no purchase question.** `ev add`, `ev found` and an edit of make or model offer
   no purchase candidates for a record linked to a kit with a purchase, and `ev buy for --toured`
   counts it as linked: the set's line is its purchase, and the lines offered instead were
   unrelated ones.

One line per kit. A kit of several copies bought on one line (`qty 2`) is still that one line.

## Why

A set of 128 bits and accessories, bought as one line, was recorded as a kit with its parts
linked to the records. The records had no purchase: linking each part to the line one by one
means nothing, and `ev buy pack` would make the line 136 units to hand out. Every part was
offered unrelated lines as candidates, while the set's own line was never offered and stayed
open forever.

## Model

Schema 31: `kits.purchase_id INTEGER REFERENCES purchases(id)`, null for a kit with no
purchase. A `kit_purchase` event `{kit, purchase}` (null when cleared) goes on every record
linked to the kit, so each one's history says where its purchase came from.

## Not now

- A coverage (warranty) of the set: `ev buy bring` brings a line's coverage to one thing; a kit's
  parts reading it through the kit is a later step, once a set with a warranty asks for it.
- A kit bought over several lines.
- What a part cost: the line's price is the set's, not shared out over its parts.
