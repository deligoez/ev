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
- **The key:** `source: "ak"`, and `key` the bare id of ak's payment. ak never changes nor reuses
  a payment id. ev keys a line by (`source`, `key`), so the same line imported again updates it
  and never adds a second.
- **One purchase, one line, in full.** A purchase paid in instalments is sent once: `paid` is the
  full price, `ordered_at` the day it was bought. The parts and the statements that billed
  them stay in ak. ev reckons today's money from the purchase day; a part's day would be wrong
  there.
- **`bucket` on every line** (`durable`, `clothing`, `digital`, `service`): ak knows which lines
  are things, so ev does not guess.
- **The rest of a line as from any source:** `shop`, `order` (as the shop prints it), `name`,
  `qty`, `currency`, `billed_to` (the card's holder or account, when ak knows it). REFERENCE has
  the NDJSON format.

## The way back

ak answers "when did the dishwasher's instalments end" by asking ev which of its payments a
thing was bought with, and the other way round:

- **thing → lines:** `ev show <ref> --json` → `purchases[]`, each with `source` and
  `source_key` (`--include-gone` for a thing that left).
- **line → thing:** `ev buy list --source ak --key <id> --json` → the line with `linked:
  [{node: {id, name, path_text, state}, qty}]`. Until those two filters exist, `ev buy list
  --json` carries the same fields for every line.

## Phases

1. **The filters:** `ev buy list --source <s>` and `--key <k>` (exact, as given), in REFERENCE
   and the skill.
2. **The import** is in place already (`ev buy import --stdin`). When ak's export exists (its
   Phase 3, after bank accounts and income), a test imports a sample of it here, and the
   mail and shop sources that ak then covers are retired from ev's side with ak's agent.

## Not now

- ev sending anything to ak: the money is ak's, the thing is ev's, and a link between them is
  read, never copied.
- Splitting a purchase's price over its things beyond what `ev buy link --qty` already does.
