# ak → ev: the purchases a money record holds

## Decision

**ak** (a sibling tool, its own repository and data folder) keeps the household's money: every
receipt, bill, statement and subscription. **ev** keeps what is a thing. Neither reads the
other's database; each asks the other only through its commands. Agreed with ak's development
agent on 2026-10-06, written into ak's spec §5 the same day (its commit 17c41a7); a change to it
is made in both repositories.

- **ak → ev:** `ak export --for ev | ev buy import --stdin`. ak sends the lines that are things
  or about things: durable, clothing, one-time digital, and services tied to a thing. A
  subscription, a bill or groceries stay in ak.
- **The key:** `source: "ak"`, and `key` the bare id of ak's payment (`412`) when the whole
  payment is one thing, or the payment's id and the item's place in it (`412.2`) when a payment
  holds several lines and only some are things (a pot on a grocery receipt). An item's place in
  its payment is fixed: ak follows its decisions by the item, not by the receipt's order. ak
  never changes nor reuses a payment id. ev keys a line by (`source`, `key`), so the same line
  imported again updates it and never adds a second.
- **Whole or items, never both** (agreed 2026-10-07, ak's spec §5): a payment goes to ev either
  whole (`412`) or as items (`412.1`, `412.2`). ak refuses marking the items of a payment sent
  whole, or the payment when an item of it was sent (`ev_whole_or_items`, exit 5), so an import,
  which never deletes, never leaves a stale line beside the new ones in ev.
- **One purchase, one line, in full.** A purchase paid in instalments is sent once: `paid` is the
  full price, `ordered_at` the day it was bought. The parts and the statements that billed
  them stay in ak. ev reckons today's money from the purchase day; a part's day would be wrong
  there.
- **`bucket` on every line** (`durable`, `clothing`, `digital`, `service`): the person's decision,
  kept in ak (`ak edit 412.2 ev=durable`, the agent proposing), so ev does not guess. Which lines
  go to ev is exactly the lines so marked.
- **The rest of a line as from any source:** `name`, `qty`, `paid`, `currency`, `status`, `shop`
  (ak's payee), `merchant`, `order` (ak's number, as the shop prints it), `ordered_at`,
  `billed_to` (the account's name), `category`, `raw`. REFERENCE has the NDJSON format.

## A purchase ev already has

Most of what ak would send, ev has read already: ak found its 708 shop orders and 554 App Store
receipts among ev's own adapters' lines, under the same order number, printed the same way in
every case (compared on all of them, 2026-10-07). Sent again as `source: "ak"`, each would be a
second open line for the same thing. Decided with ak's development agent:

- **ak's line stays its own row** (`source: "ak"`, its key), so the way back below works by key,
  never by matching text.
- **ev joins it to the line it already has**, with `same_as`, as it joins any purchase seen by
  two sources (REFERENCE, "One purchase seen by two sources"): the joined line counts as
  settled and is never offered for linking; the line it joins is the one linked to a thing.
- **By the order number, exactly:** ak's `order` equal to another source's `order_no`, six
  characters or more, whatever the shop is called on either side (ak's App Store is one source
  over two of ev's shops). One such line: joined. Several (an order of several lines): the one
  whose `paid` is the same, when exactly one is; else the one whose name clearly shares most
  words (ev's existing rule). An ak line already linked to a thing, or dismissed, is left as it
  is.
- **Anything less certain is not joined, and said:** the import's result lists it under
  `unjoined` (`{id, key, order, candidates}`: the lines of that order), for the agent to show
  the person, who joins it by hand (`ev buy join <line> <other>`), or dismisses it as
  `duplicate`, or leaves it as a purchase of its own. ak sends a multi-line order as items
  (`412.1`, `412.2`) where it knows them, so a whole `412` meets a multi-line order rarely.
- Only ak's lines are joined by order number: ak relays purchases another source may have read.
  Two of ev's own adapters seeing one order are left as today, so this changes nothing about
  lines already in the inventory.

## The way back

ak answers "when did the dishwasher's instalments end" by asking ev which of its payments a
thing was bought with, and the other way round:

- **thing → lines:** `ev show <ref> --json` → `purchases[]`, each with `source` and
  `source_key` (`--include-gone` for a thing that left). A thing linked to a shop's line that an
  ak line joined lists that line; `ev buy show <id>` → `joined` has the ak line's id.
- **line → thing:** `ev buy list --source ak --key <id> --json` → the payment's line, or its
  items' lines (`--key 412` finds `412.1` and `412.2` too; `412.2` that item alone), each with
  `linked: [{node: {id, name, path_text, state}, qty}]`; a joined line carries `joined_to`
  `{id, source, source_key, linked}`, the line it joins and what that is linked to.

## Phases

1. **The filters** (shipped): `ev buy list --source <s>` (exact) and `--key <k>` (that key and
   its items), in REFERENCE.
2. **The import** (shipped): `ev buy import --stdin` reads ak's export as written, a count sent
   as text included; a test imports ak's sample lines.
3. **The join:** ak's lines joined by order number on import, `unjoined` in the result,
   `ev buy join <line> <other>` (and `--clear`), `joined_to` on a joined line's row.
4. Later, with ak's agent: whether ev's mail and shop adapters that ak covers are retired.

## Not now

- ev sending anything to ak: the money is ak's, the thing is ev's, and a link between them is
  read, never copied.
- Splitting a purchase's price over its things beyond what `ev buy link --qty` already does.
- Joining two of ev's own sources by order number.
