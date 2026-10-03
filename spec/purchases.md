# ev — what a thing cost, what proves it, what still covers it

Status: **implemented**, released in v0.17.0 and refined since (pack sizes, declined candidates,
the toured back-fill). Written 2026-10-01 from a day of design work with the person (a
purchase-history export across fourteen shops, a review of the Under My Roof data model and of
inflation sources). Phases are in §9; what changed after the first draft is in §11 and §12.

## 1. Decision

`ev` learns five things about a thing, beside where it is:

1. **Identity:** `make`, `model` and `serial` fields on a node.
2. **Purchases:** lines of what was bought (shop, order, date, quantity, price, currency, links,
   invoice). They are their own records and **never create a node**. A line is linked to a node
   on the person's word.
3. **Documents:** invoices, warranty certificates, manuals, service forms, copied into `ev`'s
   own store like photos.
4. **Coverage:** warranties and insurance, as one kind of record with a computed status.
5. **Value:** dated observations of what a thing is worth today. A purchase price can also be
   shown in **today's money**.

Shops, Under My Roof and inflation data are read by **adapters outside the binary** that write
NDJSON; `ev` imports that and stays offline.

## 2. Why

- **The invoice is the main reason a household keeps an inventory.** When a thing breaks, the
  first questions are: is it under warranty, where is the invoice, where was it bought. Today
  `ev` can answer none of them.
- **Bought is not "still here".** One household's history, ~1,170 lines from 14 sources, holds
  consumables, clothing, returns, gifts for others and digital codes. Lines must never become
  nodes on their own. A node is still recorded only when the thing is seen in its place.
- **Matching needs exact keys, and the records lack them.** On a 124-line sample, plain word
  overlap put about 5 right matches among the best 25. A shared model code was near-certain;
  shared generic words ("kart", "led", "64 gb") were wrong. Few records carried a model code.
  Hence `make`, `model` and `serial` as fields, read off the label while the person holds the
  thing.
- **Pages die.** Product pages of a closed marketplace survive only in the Wayback Machine.
  Listings 404 and old models redirect to a category. Information about a thing has to outlive
  the shop's page.
- **What people actually filled in.** In a four-year Under My Roof inventory of 321 items, these
  fields were used:
  - Value 221, an info URL 186, Purchased From 145, Price 130, an order URL 84
  - make/model 81/53, 17 receipts, 11 warranties

  Serial, barcode, condition, heir, repairs, maintenance and insurance were **never** used.
  This spec takes the first group and leaves most of the second (§10).
- **A price from 2016 is not a price from today.** Comparing old prices with a threshold in
  today's money treats every old purchase as cheap. In a high-inflation currency the gap is an
  order of magnitude.

## 3. Data model

### 3.1 Identity on the node

`make`, `model`, `serial`: optional text fields, set with `ev edit X make=… model=… serial=…`.
They are searched by `ev find` (name weight) and are exact keys for purchase ranking (§5).
**No barcode or EAN field:** where a shop gives one, it stays in the raw source data.

### 3.2 Purchases

`purchases`, one row per purchased line:

| Field | Notes |
|---|---|
| `source`, `source_key` | identity: the adapter's name and its stable key for the line; re-import updates, never duplicates |
| `shop`, `merchant` | where it was bought; the marketplace seller when there is one |
| `order_no` | optional; kept because customer service asks for it, never a todo when missing |
| `order_url` | the order's page at the shop, when there is one |
| `product_url` | the product page at the time of purchase (see §3.4 for pages that die) |
| `shop_sku` | the shop's product key (ASIN, SKU, article number) |
| `name`, `brand`, `category` | as the shop wrote them |
| `ordered_at`, `delivered_at` | dates; warranty starts at delivery |
| `qty`, `paid`, `currency` | `paid` is the line total actually paid, tax included; unit price is derived |
| `billed_to` | who the invoice names when it is not the household, e.g. a company; information only (§3.8) |
| `status` | `delivered`, `returned`; cancelled lines are not imported |
| `bucket` | `durable`, `clothing`, `digital`; set by the adapter, editable. `consumable` lines are not imported for now (§11) |
| `dismissed`, `why` | a reason when a line will never be a node: `consumed`, `given`, `returned`, `elsewhere`, `not-mine`, `duplicate` |
| `raw` | path to the raw record under `~/.ev/purchases/<source>/`; never copied |
| `same_as` | another line that is the same purchase seen by a second source (§6) |

- `purchase_links (purchase_id, node_id, qty, at)`. A line's open quantity is `qty` minus the
  linked quantities; a line is settled when nothing is open or it is dismissed.
- `purchase_aliases (shop, shop_sku) → node`: every confirmed link is remembered, so the same
  product bought again is offered first for the same kind of record.
- **A manual purchase** (bought in a shop, a gift, from a person) is a line of its own with
  `source = manual`, entered with `ev buy add`.

### 3.3 Value observations

`valuations (node_id, amount, currency, at, source, note)`. "What it is worth today": a
second-hand listing, a shop's price, an appraisal. The latest observation is the current value.
The purchase price is never a value.

### 3.4 Links

`links (node_id, kind, url, archive, added_at, note)`. `kind`: `info` (a product page),
`manual`, `support`, `driver`, `other`. A node may have many. `archive` is where the page
survives when the URL dies: a saved copy under `~/.ev/` (the raw product page an adapter
downloaded) or a Wayback Machine address.

### 3.5 Documents

`documents (id, kind, file, original_name, number, ettn, issued_at, issuer, note, added_at)`:
- `kind`: `invoice`, `warranty`, `manual`, `service`, `appraisal`, `policy`, `other`.
- `file` is a copy in `~/.ev/docs/`, named by content hash like `~/.ev/photos/`. The raw source
  folder can be deleted and the document lives on.
- `ettn` is the e-Archive invoice UUID, verifiable at the tax authority.
- `document_links (document_id, purchase_id | node_id | coverage_id)`. An invoice belongs to
  purchase lines: an adapter links it to every line of its order, and a node reaches it through
  its purchase. A manual, a warranty certificate or an appraisal belongs to the node or to a
  coverage.

### 3.6 Coverage: warranty and insurance

`coverages`:

| Field | Values |
|---|---|
| `kind` | `statutory` (the legal minimum), `manufacturer`, `extended`, `store`, `insurance` |
| `issuer` | brand, importer, distributor, insurer |
| `number` | warranty or policy number |
| `starts` | `delivery` (the default: the linked purchase's `delivered_at`), a date, or `after:<coverage>` (an extended warranty that begins when the manufacturer's ends) |
| `term` | `n` days, weeks, months or years; `lifetime`; or a usage limit in text (`5000 h`, `60000 km`), with the time term beside it |
| `ends_on` | explicit end; required for insurance, derived otherwise |
| `premium`, `deductible`, `scope` | insurance only, optional ("screen breakage, theft") |
| `note` | free text |

- `coverage_nodes (coverage_id, node_id)`: one coverage can cover several things (a household
  contents policy), and a thing can have several (manufacturer + extended + insurance).
- **Status is computed, never stored:** `active`, `ending` (within the warning window, 60 days
  by default), `ended`, `undetermined` (no start can be derived).
- **Repair time extends the legal and manufacturer terms.** Turkish rules add the time a thing
  spent in repair. `ev broken` → `ev fixed` intervals are added to the end of `statutory` and
  `manufacturer` coverages.
- **A coverage proposal is a computation, not a record.** A durable linked purchase proposes a
  `statutory` coverage: delivery + 2 years. That is the Turkish minimum, but only for goods on
  the regulation's list, so it is a proposal the person confirms. A product page stating
  "10 yıl garanti" proposes a `manufacturer` coverage. Confirming writes the record.

### 3.7 Tracking decisions

Value and coverage are **tracked subjects** of every thing:

1. **Data means tracked.** A node with a valuation, or a coverage, is tracked. Nothing is
   asked.
2. **No data means an open question.** It counts in `ev todo` (§4.3).
3. **The person can close the question:** "do not track" or "not now". Both are recorded as a
   mark (`marks.kind = value_skip | coverage_skip`, `value = no | later`, `note` = why). `ev`
   **never raises either again on its own**: a closed question comes back only when the person
   asks the agent. The two differ only in what the record says.
4. **Entering data clears the decision.**
5. **A decision on a holder covers its contents,** including things put there later ("no values
   for the screw boxes"). A thing inside with data of its own stays tracked.
6. **The coverage question is asked by default only for valuable things:** a purchase whose
   price in today's money (§3.9) is at least the threshold (§3.8). The value question is open
   for every thing, ordered by today's-money price.

### 3.8 Settings and ownership

Inventory settings live in the database (`settings` table), changed with `ev settings`:

| Setting | Default |
|---|---|
| `home_country` | `TR` |
| `home_currency` | `TRY` |
| `price_index` | the home country's Eurostat HICP series; World Bank annual CPI when Eurostat has none |
| `valuable_threshold` | `1000` (in the home currency) |
| `coverage_warning_days` | `60` |

Ownership stays as it is: every thing is the household's unless `owner=<place>` says otherwise.
A purchase's `billed_to` never changes the owner. If a company-invoiced thing is the company's,
the person says `owner=…`. Everything recorded in `ev` is tracked, whoever owns it.

### 3.9 Money over time

- **One country.** An inventory has one home country, currency and price index. Amounts may be
  in any currency (a purchase abroad).
- **Today's money** = amount → home currency at the **purchase day's** rate → × index(latest
  month) / index(purchase month). A monthly index level, never chained yearly rates.
- **What it is for:** showing an old purchase in today's money ("1,000 TRY (2024-05) ≈ 1,880
  TRY in 2026-08 money") and the valuable threshold.
- **What it is not:** an estimate of a thing's value. Measured on real purchases of products
  still sold, the index missed today's shop price by −50% to +100% per product: a detergent
  had halved, a perfume had doubled. Value stays an observation (§3.3).
- Every computed figure names its index, month and rate day.
- **Caches:** `price_index (series, month, value, source, fetched_at)` and
  `fx_rates (currency, day, rate, source, fetched_at)`. A computation older than 45 days says
  so ("with the 2026-06 index") and asks for a refresh.
- **Sources,** each opened and measured 2026-10-01, all without a key:

  | Source | Gives | Covers |
  |---|---|---|
  | Eurostat `prc_hicp_minr` | monthly, ~1 month behind | 46 areas, incl. EU, TR, UK, US, CH, NO |
  | World Bank `FP.CPI.TOTL` | annual, a year behind | every country |
  | TCMB daily XML | rates against TRY | |
  | ECB data portal | daily rates against EUR | 40+ currencies |

  Turkey's series is TÜİK's harmonised index; its annual rates match the official TÜFE within
  0.1 point. Three other series were found silently stopped (FRED's OECD copies, Eurostat's
  old HICP, DBnomics' IMF mirror). That is why the series is a setting, never a constant.
- **Another index** (an independent estimate, a national office not covered above) is entered
  by the person or found by the agent, with its source recorded.

## 4. Verbs

### 4.1 New verbs

| Verb | Does |
|---|---|
| `ev buy import <file>` / `--stdin` | import purchase NDJSON (§6); idempotent; reports new / updated / unchanged |
| `ev buy add --shop s --date d --paid n [--currency c] [--order o] [--url u] [--for <node>]` | a manual purchase |
| `ev buy list [--open] [--bucket b] [--shop s] [--since d]` | lines; `--open` = not linked, not dismissed |
| `ev buy for <node>` | ranked purchase candidates with the reasons (§5) |
| `ev buy link <line> <node> [--qty n]` / `unlink` | on the person's word; linking offers the line's attachments (§6) |
| `ev buy dismiss <line> --as <reason> [--why …]` | settle a line that will never be a node |
| `ev buy show <line>` | one line with its links, documents, raw path |
| `ev doc add <file> --kind k [--for <node>] [--purchase <line>] [--coverage <id>] [--number n] [--issued d]` | copy a document into the store and link it |
| `ev doc list [<node>]` / `ev doc show <id>` | documents, with what they belong to |
| `ev cover add <node>… --kind k [--term 2y] [--from delivery\|<date>\|after:<id>] [--ends d] [--issuer i] [--number n] …` | a coverage |
| `ev cover list [--ending]` / `ev cover show <id>` | coverages with computed status |
| `ev value <node> <amount> [--currency c] [--at d] [--source s]` | a valuation; with only `<node>`, its history |
| `ev link add <node> <url> [--kind k] [--archive a]` / `list` / `remove` | links |
| `ev track <node> value\|coverage no\|later [--why …]` / `yes` | close or reopen a tracked question (§3.7) |
| `ev money import <file>` / `ev money status` | import index and rate NDJSON; show what is cached and how old |

### 4.2 Changed verbs

- `ev edit`: `make`, `model`, `serial`.
- `ev show`: identity fields; linked purchases (date, shop, paid, today's money); latest value
  with its date; links; documents; coverages with status.
- `ev add` / `ev split`: print up to three purchase candidates above a threshold for each new
  record, so the agent can ask while the person holds the thing (§5).
- `ev broken X`: also prints X's coverages, invoice, shop and order URL.
- `ev sale --listed`: takes `--condition new|like-new|used`. The only place a condition is
  recorded (§10).
- `ev settings`: the inventory settings of §3.8.
- `ev ui`: a node's details show its purchases, today's money and latest value; lists do not.
- `ev find`: searches `make`, `model`, `serial`.

### 4.3 `ev todo`

New kinds, **as counts**, never as one item per thing:

- `values`: things with no valuation and no decision, the count plus the five highest in today's
  money.
- `coverage`: valuable things with no coverage and no decision.
- `coverage_ending`: coverages within the warning window, one item each (these are few and
  time-bound).
- `purchases`: open durable lines, as a count.

Questions are asked one by one only where they matter: during a tour, most expensive first;
when a node is opened in `ev ui` (one line); at `ev broken` and `ev sale`.

## 5. Ranking purchase candidates

What separates a right match from a wrong one (measured on the 124-line sample):

1. **Exact keys first:** a remembered alias `(shop, shop_sku)`; the node's `model` or `serial`
   in the line's name.
2. **Model codes,** tokens mixing letters and digits (`LR1130`, `TB-X306F`), shared between
   the line and the node's name or note: near-certain.
3. **Brand** shared with the node's `make`, name or note.
4. **Conflicting numbers with the same unit** (125 kHz vs 13.56 MHz, 16 GB vs 64 GB) demote a
   candidate however many words match. Units are converted before comparing (mm vs cm was
   missed in the prototype).
5. **Generic words** count little, as in `ev suggest`. A `durable` line is preferred; a fully
   linked line is not offered.

The ranking reuses `ev`'s tokenizer, stems and synonyms. The prototype raised a button cell to
first on two shared codes and dropped an RFID near-miss on a frequency conflict.

## 6. Sources and adapters

- An adapter is a script that reads one source's raw export and writes NDJSON. It lives with
  the data, `~/.ev/purchases/<source>/adapter.py`, written by the inventory agent (§13);
  `examples/purchases/` is the worked example. Sources: each shop, and **Under My Roof**
  (read-only from its local SQLite; never written).
- An adapter emits typed objects: `purchase` (§3.2 fields), and `document`, `link`, `valuation`
  and `coverage` objects that **hang on a purchase** by its `(source, source_key)`.
- Nothing reaches a node until the purchase is linked. Linking offers the attachments in one
  question: "bring along the invoice, 1 link, a value of 2,500 TRY (approximate date), a 2-year
  warranty?"
- An Under My Roof item becomes one purchase line (`source = umr`) carrying its price, dates,
  URLs, receipts, attachments, warranties and value. Its value has no date of its own: the
  item's last-modified date is used and marked approximate.
- **The same purchase from two sources** (Under My Roof and the shop) is joined by `same_as`.
  The keys:
  - the order number inside an order URL (24 of 25 matched in the sample);
  - otherwise the shop key inside a product URL (ASIN, SKU, content id).

  The rest are offered as candidates.
- `tools/money/` fetches index and rate series (§3.9) into NDJSON for `ev money import`.
- Raw data stays under `~/.ev/purchases/<source>/` (mode 700), outside every repository: raw
  orders carry addresses and phone numbers.

## 7. Storage

New tables: `purchases`, `purchase_links`, `purchase_aliases`, `valuations`, `links`,
`documents`, `document_links`, `coverages`, `coverage_nodes`, `price_index`, `fx_rates`. New
node columns: `make`, `model`, `serial`. New settings keys (§3.8). New mark kinds: `value_skip`,
`coverage_skip`. New store: `~/.ev/docs/`. A schema migration as for earlier versions, with a
backup before it.

## 8. The skill

The ev skill gains:
- **Read the label:** while the person holds a thing, read its make, model and serial off the
  label or photo into those fields.
- **Ask about a purchase at record time:** when `ev add` prints candidates, ask "is this the one
  bought on …?".
- **The tracking rule (§3.7):** ask once; respect "no" and "not now"; never raise a closed
  question again unless asked.
- **When a thing breaks:** run `ev broken` and read out its coverage and invoice.
- **Write values with a source and a date.**

## 9. Phases

Each phase ends with the gate green and one measured check.

| # | Phase | Check |
|---|---|---|
| 1 | Identity fields; documents (`ev doc`, store, links to nodes); `ev show` lists them | a real invoice PDF attached and shown; `ev find` hits a model code |
| 2 | Purchases: schema, `buy import/add/list/show/link/unlink/dismiss`, invoices through purchases; the first shop adapter | a shop's export imported twice with no duplicates; every imported invoice reachable from its lines |
| 3 | Candidates at record time and `ev buy for` (§5) | share of the known right matches ranked first; wrong candidates shown per new record |
| 4 | Coverage, proposals, tracking decisions, `ev todo` kinds, `ev broken` | a linked purchase proposes its statutory coverage; a holder-level "no" silences its contents |
| 5 | Money: settings, `tools/money`, caches, today's money in `ev show`, the threshold | today's money for a known purchase matches a hand computation from the same index and rate |
| 6 | Valuations, links with archive copies; the Under My Roof adapter; remaining shop adapters | an Under My Roof item's order URL joins its shop line through `same_as` |

## 10. Non-goals

| Not taken | Why |
|---|---|
| Barcode / EAN field | the only use is an exact key, and the shops' raw data already has it; revisit if phone scanning comes |
| Condition scale (excellent … poor) | subjective and goes stale; dated photos show the state, `broken` the function; asked only at sale |
| Insurance claims | coverage records hold the policy; a claim is a note or a document for now |
| Depreciation | valuations give the real figure with a date |
| Maintenance tasks, consumables | later; consumable purchase lines are not even imported yet (§11) |
| Custom fields | the schema stays fixed; anything else goes in `note` |
| Per-currency inflation | one country per inventory (§3.9) |
| Country-specific index logic | the series is a setting; an alternative index is entered like any other |

## 11. Decided after the first draft (2026-10-01)

1. **Consumables are not imported for now.** Adapters still classify each line, but drop
   `consumable` lines instead of emitting them. The bucket stays in the format so they can be
   imported later without a format change.
2. **Prices show in a node's details in `ev ui`**: purchases, today's money, the latest value.
   Lists and the tree show no prices.
3. **A one-off back-fill:** `ev buy for` over open durable lines against existing records,
   answered in numbered batches.
4. **A node's name comes from the person, not the shop.** It says what the thing is, then its
   make and model: `Darbeli matkap, Bosch GSB 13 RE`, in the inventory's existing style
   (`Kırmızı LED, 5 mm`). The rule:
   - **Never renamed automatically.**
   - When a purchase is linked and the node has no `make` / `model`, the same question offers
     them, read from the line's brand and model code.
   - When the name lacks the make and model, the same question offers that name.
   - A shop's long title ("Bosch Professional GSB 13 RE Darbeli Matkap (600 W, Mandren Çapı …)")
     never becomes the name. It stays on the purchase line.

## 12. Decided while back-filling (2026-10-02)

1. **Under My Roof is not imported.** It is not a shop: its records were typed by the person,
   so they confirm a match, they are not purchases of their own. The `umr` adapter
   stays as a reader; what it gives is used to confirm a shop line, never imported as lines.
2. **Back-fill only toured places.** A record in a place not toured yet is still a guess; it
   is matched to a purchase during that place's tour, not before. The one-off back-fill of
   §11.3 covers things whose place has been toured.

## 13. Adapters live with the data (2026-10-03)

1. **ev ships the contract, not the shops.** An adapter depends on how one shop's pages look
   and on how the agent saved them (its `RECIPE.md`, its `raw/` layout); it changes with them,
   not with ev's releases. So the shop adapters moved out of this repository to the data side,
   `~/.ev/purchases/<shop>/adapter.py`, kept by the inventory agent. The fifteen written during
   the back-fill are in the `v0.21.0` tag for anyone who wants to start from one.
2. **One worked example stays**, `examples/purchases/`: an adapter over an invented export and
   a README on writing one for a new shop. A test runs it into `ev buy import`, so the example
   an agent copies from is always one that works.
3. **The adapter emits the `image` lines.** It knows which picture is which line; matching
   saved file names to lines afterwards (`images.py`) failed for three shops whose file names
   carried another id.
4. **Pictures are brought by the agent, never on their own**, and every bring says what it
   brought (`ev buy bring --type image`, `--all` for the back-fill); the agent tells the
   person. Where the person said pictures always come along, that rule is the household's
   (`~/.ev/CLAUDE.md`), not a setting in ev.
