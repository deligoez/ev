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

## The way back

ak answers "when did the dishwasher's instalments end" by asking ev which of its payments a
thing was bought with, and the other way round:

- **thing → lines:** `ev show <ref> --json` → `purchases[]`, each with `source` and
  `source_key` (`--include-gone` for a thing that left).
- **line → thing:** `ev buy list --source ak --key <id> --json` → the payment's line, or its
  items' lines (`--key 412` finds `412.1` and `412.2` too; `412.2` that item alone), each with
  `linked: [{node: {id, name, path_text, state}, qty}]`.

## Phases

1. **The filters** (shipped): `ev buy list --source <s>` (exact) and `--key <k>` (that key and
   its items), in REFERENCE.
2. **The import** is in place already (`ev buy import --stdin`). When ak's export exists (its
   Phase 3, after bank accounts and income), a test imports a sample of it here, and the
   mail and shop sources that ak then covers are retired from ev's side with ak's agent.

## Not now

- ev sending anything to ak: the money is ak's, the thing is ev's, and a link between them is
  read, never copied.
- Splitting a purchase's price over its things beyond what `ev buy link --qty` already does.
