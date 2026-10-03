# Writing a purchase adapter

ev does not read any shop. A shop's order history reaches ev through an **adapter**: a small
script, written by the agent that keeps the inventory, that reads what it saved from one shop
and prints one JSON object per line for `ev buy import --stdin`. ev ships the contract (the
import lines in [REFERENCE.md](../../REFERENCE.md#import-lines)) and this worked example; the
adapters for the shops a household actually uses live with that household's data, next to the
raw exports they read, and change when the shop's pages change.

`example-shop.py` reads `orders.json`, an invented export, and shows each kind of line once:

```bash
examples/purchases/example-shop.py | ev buy import --stdin
```

## The steps

1. **Save the raw export first, outside every repository** (`~/.ev/purchases/<shop>/raw/`):
   the order pages or the JSON the shop's site loads, the invoices, the product pictures. Write
   down how you saved it (`RECIPE.md` next to it), so the next fetch is the same. Raw orders
   carry addresses and prices: they never enter a repository.
2. **One `purchase` line per product as sold.** `source` is the shop, `key` is stable across
   fetches (`<order>:<sku>`), so a re-import updates the line instead of adding a second one.
   Dates are `YYYY-MM-DD`: `ordered_at` is the day of the order, `delivered_at` the day it came.
   `paid` is the line total, as a plain number (`1999.00`).
3. **Drop what never was a purchase** (a cancelled order, a returned line: or keep it with
   `status`), and guess a `bucket` (`consumable` for what is used up) so the matcher skips it.
4. **Hang what came with a line on it**, by its `key`: the product page (`link`), the shop's
   warranty (`coverage`), the invoice (`document`, on every line of its order), the product
   pictures (`image`). The adapter knows which picture is which line; name it here rather than
   matching file names later. Paths are absolute.
5. **Check before importing:** print the lines, read a few, count them against the shop's
   order list. Import only when the person asks.

Nothing an adapter emits reaches a thing in the tree: a line is linked to a thing on the
person's word (`ev buy link`), and what hangs on it comes along with `ev buy bring`.
