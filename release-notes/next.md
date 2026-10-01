Draft for the next release.

## New

- **A thing's make, model and serial number** (`ev edit X make=Bosch model="GSB 13 RE"
  serial=…`, or `--make/--model/--serial` on `ev add`). Read off the label while the thing is in
  hand, they are what finds it again: `ev find` searches them as strongly as a code, so
  `ev find gsb 13` finds a record named just "Matkap". `ev show` and the details in `ev ui` list
  them. The first step of the purchases spec (`spec/purchases.md`): a model code is the exact
  key a purchase and its invoice will be matched by.
- **Documents: invoices, warranty certificates, manuals** (`ev doc add <file> --kind invoice
  --for <ref> --issued 2024-05-03 --issuer <shop> --number <no>`). The file is copied into `docs/`
  beside the database, named by content hash like photos, so the original may be deleted; the
  same file added again is the same document with one more link. `ev show` and `ev ui` list a
  thing's documents; `ev doc list`, `show`, `link` and `unlink` manage them.

- **Purchases: what was bought, linked to things on the person's word** (`ev buy import`,
  `add`, `list`, `show`, `link`, `unlink`, `dismiss`). A line never creates a thing. An adapter
  turns a shop's export into NDJSON (`tools/purchases/hepsiburada.py | ev buy import --stdin`);
  importing again changes nothing, cancelled and consumable lines are skipped, and the shop's
  invoices come along, hung on their order's lines. A linked thing shows its purchases and
  reaches their invoices in `ev show` and `ev ui`. Amounts are exact (minor units).
- **The purchase a thing could be, asked while it is in hand.** `ev add` lists up to three
  purchase lines that could be the new record, and `ev buy for <ref>` ranks them for any record,
  each with its reasons: a shared model code, the brand, the record's own `model`, rarer shared
  words; two numbers of one unit that differ (125 kHz against 13,56 MHz) rule a line out. Measured
  on one shop's 158 lines against 451 records: each known right match ranked first, and about one
  record in thirty got a wrong first offer.

- **Warranties and insurance, with their status** (`ev cover add <ref> --kind manufacturer
  --term 2y`, `list`, `show`, `remove`). One record covers one or more things; it starts at the
  linked purchase's delivery, on a date, or when another coverage ends (an extended warranty).
  Its end, days left and status (active, ending, ended, undetermined) are computed, and a
  statutory or manufacturer warranty runs longer by the days its thing spent broken. A linked
  durable purchase proposes a two-year statutory warranty, recorded only when confirmed.
  `ev todo` lists coverages ending within 60 days and counts valuable things with none.
- **"Don't track this" and "not now"** (`ev track <ref> value|coverage no|later`), on a thing or
  a whole holder: the question is never raised again on its own.
- **Inventory settings** in the database: `ev settings inventory` (home country and currency,
  price index, the valuable threshold, the warning window).
- **A price in today's money** (`ev money needs`, piped through `tools/money/fetch.py` into
  `ev money import --stdin`). A purchase shows what it cost grown by the home price index to the latest month
  ("≈ 825.67 TRY in 2026-08 money"); one in another currency is first turned into the home
  currency at its day's rate. The index and rates are cached in the database and fetched by the
  tool with no key (Eurostat HICP, the World Bank for countries Eurostat lacks, the Central Bank
  of Türkiye or the ECB for rates), so `ev` stays offline; `ev money status` says when the index
  is stale. The valuable count in `ev todo` uses today's money.
- **What a thing is worth, with its date** (`ev value <ref> 2500 --source "listing" --at
  2026-09-20`). The latest observation is the current value and shows in `ev show` and `ev ui`;
  older ones stay as history. The purchase price is never a value. `ev todo` counts bought things
  with no value and lists the dearest; a value, like a "don't track" decision, closes the
  question.
- **Links with an archive** (`ev link add <ref> <url> --kind manual --archive <file|url>`): a
  product, manual, support or driver page; a saved copy goes into the document store, so the
  link survives the page.
- **`ev todo` counts the open purchase lines** still to be linked to a thing.
- **What came with a purchase comes along on request** (`ev buy bring <line> <ref>`). An
  adapter can hang a link, a value or a warranty on a line; after the line is linked to a thing,
  one command makes them the thing's own. `ev buy show` lists them.
- **One purchase seen by two sources is joined** on import (`same_as`): another app's record
  whose order page names a shop's order number points at that shop's line, narrowed by the
  product key, or by a clearly closest name, when the order has several lines. The joined line
  counts as settled; its documents and attachments come with the line it joins. Measured on one
  household's Under My Roof records against its shop exports: 36 records joined, each to the
  right line.
- **Purchase adapters for fifteen shops and Under My Roof** (`tools/purchases/`): AliExpress,
  Amazon.com.tr, Amazon.de, Decathlon, GittiGidiyor, Hepsiburada, idefix, IKEA, Kitapyurdu,
  n11, Robo90, Robotistan, sahibinden, Trendyol, Vivense, each reading a saved export, and
  `umr.py`, which reads a copy of Under My Roof's store: each item with its value, warranties,
  info link, receipts and attachments.
- **`ev split` offers each new part's purchase**, as `ev add` does.
- **`ev sale --condition new|like-new|used`**: what a buyer is told, the only place a condition
  is recorded.

## Upgrade notes

- The database moves to schema 25 (node identity columns, documents, purchases, coverage, the
  price index and rate caches, values, links) the first time this version opens it; an older
  `ev` then refuses it.

## Changed

- **`ev ui` reopens the tree as it was left.** It already went back to the node that was
  selected; now the nodes that were open stay open and the headings that were closed (the To
  do sections, Unknown place) stay closed, per database, in `ui-state.json`. A node gone since
  is dropped. `ev settings resume off` still starts at the top with the default tree.
- **A mark before each row in `ev ui` says its kind:** ⌂ home, ◫ room, ▥ furniture, □ box,
  · thing, in the tree, the Contents tab and the lists. A box and the things in it differed
  only by a code; now they read apart at a glance. Plain one-cell shapes, so every terminal
  draws them the same width.
- **A settled row turns green in `ev ui`**, from its mark to its name. Settled means counted,
  with nothing in `ev todo` hanging on it or on anything in it (no task, planned move out or in,
  disposal, label, photo, unclear name, …) and no observation waiting. A drawer turns green
  once every box in it has, and so on up to the home, so the tree reads as a map of what is
  finished. The name's colour now says only this; the kind is left to the mark, so furniture
  names lose their own colour, a quantity (`×3`) is no longer green, and a settled row drops
  its `[counted]` tag.
- **`ev find` and `/` in `ev ui` search by words, not by one exact string.** Every word of
  the query has to turn up on the record (name, code, tags, theme or note) but in any order, so
  `led kırmızı` finds "Kırmızı LED, 5 mm". A word also finds its Turkish stem (`kırmızılar`),
  and the synonym groups of `ev synonym` count. A word that matches nothing as written is tried
  again with a typo or two (`kirmzi`); a word that does match never drags in look-alikes
  (`kablo` does not bring `tablo`). Results come best first: a match in the name or code above
  one in tags, theme or note.

## Fixed

- **A sketched room or piece of furniture moved to another holder stays where it lies on the
  map.** Its place and outline are written in its holder's frame, and a move left them there:
  two balconies moved out of their rooms to the home landed on the home's top-left corner. A
  move now translates them into the new holder's frame; when either frame is unknown (a holder
  on the way up has no place), the sketch is left as it was.
- **`ev photo add` and `ev photo list` read as text.** With `--text` (or at a terminal) they
  printed the whole JSON. They now print the node, then each photo by its number with its file,
  its crop when only part of the photo shows the node, and its note.
- **A record closed as a mistake no longer dates its place's photo.** Closing a placeholder
  with `gone --as mistake` counted as a thing leaving, so the drawer's photo taken minutes before
  was refused as out of date when its tour was closed. A mistaken record was never there; only a
  thing that really goes (trash, give, sell, return) still dates the photo.
