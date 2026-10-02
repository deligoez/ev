//! The schema, one migration per version; `Inventory::open` applies the ones a file lacks.

pub(super) const SCHEMA_V1: &str = "
BEGIN;
CREATE TABLE nodes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    parent_id INTEGER REFERENCES nodes(id),
    code TEXT,
    code_folded TEXT,
    address TEXT,
    qty INTEGER,
    note TEXT,
    theme TEXT,
    fill INTEGER,
    state TEXT NOT NULL DEFAULT 'active',
    disposition TEXT,
    lost INTEGER NOT NULL DEFAULT 0,
    pending_to INTEGER REFERENCES nodes(id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX nodes_parent ON nodes(parent_id);
CREATE TABLE tags (
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    tag TEXT NOT NULL,
    PRIMARY KEY (node_id, tag)
);
CREATE TABLE photos (
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    position INTEGER NOT NULL,
    path TEXT NOT NULL,
    PRIMARY KEY (node_id, position)
);
CREATE TABLE events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    at TEXT NOT NULL,
    type TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX events_node ON events(node_id);
PRAGMA user_version = 1;
COMMIT;
";

/// Places (spec §13): a person, a household or anywhere outside the tree, with aliases.
pub(super) const SCHEMA_V2: &str = "
BEGIN;
CREATE TABLE places (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE TABLE place_aliases (
    place_id INTEGER NOT NULL REFERENCES places(id),
    alias TEXT NOT NULL,
    alias_folded TEXT NOT NULL UNIQUE
);
ALTER TABLE nodes ADD COLUMN owner_place INTEGER REFERENCES places(id);
ALTER TABLE nodes ADD COLUMN with_place INTEGER REFERENCES places(id);
ALTER TABLE nodes ADD COLUMN to_place INTEGER REFERENCES places(id);
PRAGMA user_version = 2;
COMMIT;
";

/// Placement rules the agent must weigh on every suggestion (spec §14).
pub(super) const SCHEMA_V3: &str = "
BEGIN;
CREATE TABLE rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    text TEXT NOT NULL,
    created_at TEXT NOT NULL
);
PRAGMA user_version = 3;
COMMIT;
";

/// Holders whose contents were never inventoried (spec §15).
pub(super) const SCHEMA_V4: &str = "
BEGIN;
ALTER TABLE nodes ADD COLUMN unknown INTEGER NOT NULL DEFAULT 0;
PRAGMA user_version = 4;
COMMIT;
";

/// Photo provenance (spec §16): the stored original a crop was cut from, the crop, a note.
pub(super) const SCHEMA_V5: &str = "
BEGIN;
ALTER TABLE photos ADD COLUMN source TEXT;
ALTER TABLE photos ADD COLUMN crop TEXT;
ALTER TABLE photos ADD COLUMN note TEXT;
ALTER TABLE photos ADD COLUMN added_at TEXT;
PRAGMA user_version = 5;
COMMIT;
";

/// The tidy-up plan (spec §17): how far each place has been gone through, what was noticed
/// there, an ordered work list, and settings such as the household's goal.
pub(super) const SCHEMA_V6: &str = "
BEGIN;
CREATE TABLE reviews (
    node_id INTEGER PRIMARY KEY REFERENCES nodes(id),
    status TEXT NOT NULL,
    at TEXT NOT NULL,
    note TEXT
);
CREATE TABLE observations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    text TEXT NOT NULL,
    photo TEXT,
    at TEXT NOT NULL
);
CREATE INDEX observations_node ON observations(node_id);
CREATE TABLE tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    why TEXT NOT NULL,
    rank INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    note TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    closed_at TEXT
);
CREATE TABLE task_nodes (
    task_id INTEGER NOT NULL REFERENCES tasks(id),
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    PRIMARY KEY (task_id, node_id)
);
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
PRAGMA user_version = 6;
COMMIT;
";

/// Things to do that hang on one node (spec §18): a label to print, broken, a use-by date, a
/// sale in progress — one row per node and kind — and a list of things to buy or make.
pub(super) const SCHEMA_V7: &str = "
BEGIN;
CREATE TABLE marks (
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    kind TEXT NOT NULL,
    value TEXT,
    amount INTEGER,
    note TEXT,
    at TEXT NOT NULL,
    PRIMARY KEY (node_id, kind)
);
CREATE TABLE needs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    text TEXT NOT NULL,
    qty INTEGER,
    make INTEGER NOT NULL DEFAULT 0,
    for_node INTEGER REFERENCES nodes(id),
    status TEXT NOT NULL DEFAULT 'open',
    note TEXT,
    created_at TEXT NOT NULL,
    closed_at TEXT
);
PRAGMA user_version = 7;
COMMIT;
";

/// Gridfinity-style holders (spec §22): a drawer's grid, and the rectangle of cells each box
/// in it covers. Columns and rows are 0-based here; people read them as A… and 1…, row 1 at
/// the back.
pub(super) const SCHEMA_V8: &str = "
BEGIN;
CREATE TABLE grids (
    node_id INTEGER PRIMARY KEY REFERENCES nodes(id),
    cols INTEGER NOT NULL,
    rows INTEGER NOT NULL
);
CREATE TABLE cells (
    node_id INTEGER PRIMARY KEY REFERENCES nodes(id),
    col INTEGER NOT NULL,
    row INTEGER NOT NULL,
    width INTEGER NOT NULL,
    depth INTEGER NOT NULL
);
PRAGMA user_version = 8;
COMMIT;
";

/// A box's outer size (spec §23), `WxDxH` in the units the person uses (gridfinity units for
/// bins), so a fuller box can be matched with a bigger spare one.
pub(super) const SCHEMA_V9: &str = "
BEGIN;
ALTER TABLE nodes ADD COLUMN size TEXT;
CREATE TABLE synonyms (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    words TEXT NOT NULL,
    created_at TEXT NOT NULL
);
PRAGMA user_version = 9;
COMMIT;
";

/// Facets (spec §26): kinds of things kept apart, like modules and bare parts. A holder is in a
/// facet by carrying its name as a tag; `words` also tell a thing's facet from its name.
pub(super) const SCHEMA_V10: &str = "
BEGIN;
CREATE TABLE facets (
    name TEXT PRIMARY KEY,
    words TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);
PRAGMA user_version = 10;
COMMIT;
";

/// A grid photo keeps the grid's corners in it (spec §28), so its cells can be marked later by
/// name without measuring the photo again.
pub(super) const SCHEMA_V11: &str = "
BEGIN;
ALTER TABLE photos ADD COLUMN grid TEXT;
PRAGMA user_version = 11;
COMMIT;
";

/// Kits (spec §29): what a bought set should contain, part by part, and which records are
/// those parts, so what is still missing from it is read off the records.
pub(super) const SCHEMA_V12: &str = "
BEGIN;
CREATE TABLE kits (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    name_folded TEXT NOT NULL UNIQUE,
    copies INTEGER NOT NULL DEFAULT 1,
    note TEXT,
    created_at TEXT NOT NULL
);
CREATE TABLE kit_parts (
    kit_id INTEGER NOT NULL REFERENCES kits(id),
    position INTEGER NOT NULL,
    text TEXT NOT NULL,
    qty INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (kit_id, position)
);
CREATE TABLE kit_links (
    kit_id INTEGER NOT NULL,
    position INTEGER NOT NULL,
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    PRIMARY KEY (kit_id, position, node_id),
    FOREIGN KEY (kit_id, position) REFERENCES kit_parts(kit_id, position)
);
CREATE INDEX kit_links_node ON kit_links(node_id);
PRAGMA user_version = 12;
COMMIT;
";

/// A parking place (spec §30): things put in it wait for their final place, so placement does
/// not offer it as one and `ev todo` lists what waits there.
pub(super) const SCHEMA_V13: &str = "
BEGIN;
ALTER TABLE nodes ADD COLUMN temporary INTEGER NOT NULL DEFAULT 0;
PRAGMA user_version = 13;
COMMIT;
";

/// The sketch (spec §31): where a room lies in the home and a piece of furniture in the room,
/// in centimetres, and what stands on what (a Kallax on another).
pub(super) const SCHEMA_V14: &str = "
BEGIN;
CREATE TABLE sketches (
    node_id INTEGER PRIMARY KEY REFERENCES nodes(id),
    x REAL,
    y REAL,
    w REAL,
    d REAL,
    on_id INTEGER REFERENCES nodes(id)
);
PRAGMA user_version = 14;
COMMIT;
";

/// Schema 15: how a grid is seen, from above (a drawer: row 1 at the back) or from the front
/// (furniture and its compartments: row 1 at the top).
pub(super) const SCHEMA_V15: &str = "
BEGIN;
ALTER TABLE grids ADD COLUMN face TEXT NOT NULL DEFAULT 'above';
PRAGMA user_version = 15;
COMMIT;
";

/// Schema 16: a room's outline, and what a floor plan shows in a place that is no record here
/// (a bed, a door, a window), for finding one's way on the map.
pub(super) const SCHEMA_V16: &str = "
BEGIN;
ALTER TABLE sketches ADD COLUMN points TEXT;
CREATE TABLE sketch_marks (
    id INTEGER PRIMARY KEY,
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    x REAL NOT NULL,
    y REAL NOT NULL,
    w REAL NOT NULL,
    d REAL NOT NULL
);
CREATE INDEX sketch_marks_node ON sketch_marks(node_id);
PRAGMA user_version = 16;
COMMIT;
";

/// Schema 17: the plan's marks go. What a floor plan shows that is no record here (the
/// kitchen's cabinets, doors, windows) was drawn in rooms nobody asked for it in.
pub(super) const SCHEMA_V17: &str = "
BEGIN;
DROP TABLE sketch_marks;
PRAGMA user_version = 17;
COMMIT;
";

/// Schema 18: a regroup proposal the person declined: the thing stays in the holder it was in
/// when they said so, and regroup leaves it alone until it is moved.
pub(super) const SCHEMA_V18: &str = "
BEGIN;
CREATE TABLE declines (
    node_id INTEGER PRIMARY KEY REFERENCES nodes(id),
    holder_id INTEGER NOT NULL REFERENCES nodes(id),
    why TEXT,
    at TEXT NOT NULL
);
PRAGMA user_version = 18;
COMMIT;
";

/// Schema 19: one state for how far a place is counted, one for a place not known. The
/// `unknown` mark ("contents never counted") is what a place not yet counted already is, so it
/// goes; a thing with no place at all is lost, with no last-seen place.
pub(super) const SCHEMA_V19: &str = "
BEGIN;
ALTER TABLE nodes DROP COLUMN unknown;
UPDATE nodes SET lost = 1 WHERE parent_id IS NULL AND kind != 'home' AND state != 'gone';
PRAGMA user_version = 19;
COMMIT;
";

/// Schema 20: what a thing is beyond its name (purchases spec §3.1): its make, model and
/// serial, read off its label, so a purchase can be matched by an exact key.
pub(super) const SCHEMA_V20: &str = "
BEGIN;
ALTER TABLE nodes ADD COLUMN make TEXT;
ALTER TABLE nodes ADD COLUMN model TEXT;
ALTER TABLE nodes ADD COLUMN serial TEXT;
PRAGMA user_version = 20;
COMMIT;
";

/// Schema 21: documents (purchases spec §3.5), copied into the store and linked to what they
/// belong to: a node now, a purchase or a coverage later.
pub(super) const SCHEMA_V21: &str = "
BEGIN;
CREATE TABLE documents (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL,
    file TEXT NOT NULL UNIQUE,
    original_name TEXT,
    number TEXT,
    ettn TEXT,
    issued_at TEXT,
    issuer TEXT,
    note TEXT,
    added_at TEXT NOT NULL
);
CREATE TABLE document_links (
    document_id INTEGER NOT NULL REFERENCES documents(id),
    target TEXT NOT NULL,
    target_id INTEGER NOT NULL,
    at TEXT NOT NULL,
    PRIMARY KEY (document_id, target, target_id)
);
CREATE INDEX document_links_target ON document_links(target, target_id);
PRAGMA user_version = 21;
COMMIT;
";

/// Schema 22: purchases (purchases spec §3.2), lines of what was bought, linked to things on
/// the person's word; a product linked once is remembered for its next purchase. Amounts are in
/// minor units (kuruş, cents).
pub(super) const SCHEMA_V22: &str = "
BEGIN;
CREATE TABLE purchases (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source TEXT NOT NULL,
    source_key TEXT NOT NULL,
    shop TEXT,
    merchant TEXT,
    order_no TEXT,
    order_url TEXT,
    product_url TEXT,
    shop_sku TEXT,
    name TEXT NOT NULL,
    brand TEXT,
    category TEXT,
    ordered_at TEXT,
    delivered_at TEXT,
    qty INTEGER NOT NULL DEFAULT 1,
    paid INTEGER,
    currency TEXT,
    billed_to TEXT,
    status TEXT NOT NULL DEFAULT 'delivered',
    bucket TEXT NOT NULL DEFAULT 'durable',
    dismissed TEXT,
    why TEXT,
    raw TEXT,
    same_as INTEGER REFERENCES purchases(id),
    imported_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (source, source_key)
);
CREATE TABLE purchase_links (
    purchase_id INTEGER NOT NULL REFERENCES purchases(id),
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    qty INTEGER NOT NULL,
    at TEXT NOT NULL,
    PRIMARY KEY (purchase_id, node_id)
);
CREATE INDEX purchase_links_node ON purchase_links(node_id);
CREATE TABLE purchase_aliases (
    shop TEXT NOT NULL,
    shop_sku TEXT NOT NULL,
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    at TEXT NOT NULL,
    PRIMARY KEY (shop, shop_sku, node_id)
);
PRAGMA user_version = 22;
COMMIT;
";

/// Schema 23: coverage (purchases spec §3.6), warranties and insurance as one record, covering
/// one or more things; its status is computed, never stored.
pub(super) const SCHEMA_V23: &str = "
BEGIN;
CREATE TABLE coverages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL,
    issuer TEXT,
    number TEXT,
    starts TEXT NOT NULL DEFAULT 'delivery',
    start_date TEXT,
    after_id INTEGER REFERENCES coverages(id),
    term_n INTEGER,
    term_unit TEXT,
    usage TEXT,
    ends_on TEXT,
    premium INTEGER,
    deductible INTEGER,
    currency TEXT,
    scope TEXT,
    note TEXT,
    created_at TEXT NOT NULL
);
CREATE TABLE coverage_nodes (
    coverage_id INTEGER NOT NULL REFERENCES coverages(id),
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    PRIMARY KEY (coverage_id, node_id)
);
CREATE INDEX coverage_nodes_node ON coverage_nodes(node_id);
PRAGMA user_version = 23;
COMMIT;
";

/// Schema 24: money over time (purchases spec §3.9), a cached price index and exchange rates,
/// fetched outside `ev` and imported.
pub(super) const SCHEMA_V24: &str = "
BEGIN;
CREATE TABLE price_index (
    series TEXT NOT NULL,
    period TEXT NOT NULL,
    value REAL NOT NULL,
    source TEXT,
    fetched_at TEXT NOT NULL,
    PRIMARY KEY (series, period)
);
CREATE TABLE fx_rates (
    currency TEXT NOT NULL,
    home TEXT NOT NULL,
    day TEXT NOT NULL,
    rate REAL NOT NULL,
    source TEXT,
    fetched_at TEXT NOT NULL,
    PRIMARY KEY (currency, home, day)
);
PRAGMA user_version = 24;
COMMIT;
";

/// Schema 25: what a thing is worth (dated observations, never the purchase price), its links
/// with an archive copy, and what an adapter hangs on a purchase line for the thing it becomes
/// (purchases spec §3.3, §3.4, §6).
pub(super) const SCHEMA_V25: &str = "
BEGIN;
CREATE TABLE valuations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    amount INTEGER NOT NULL,
    currency TEXT NOT NULL,
    at TEXT NOT NULL,
    approximate INTEGER NOT NULL DEFAULT 0,
    source TEXT,
    note TEXT,
    added_at TEXT NOT NULL
);
CREATE INDEX valuations_node ON valuations(node_id, at);
CREATE TABLE links (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    kind TEXT NOT NULL,
    url TEXT NOT NULL,
    archive TEXT,
    note TEXT,
    added_at TEXT NOT NULL,
    UNIQUE (node_id, url)
);
CREATE TABLE purchase_attachments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    purchase_id INTEGER NOT NULL REFERENCES purchases(id),
    kind TEXT NOT NULL,
    data TEXT NOT NULL,
    brought_to INTEGER REFERENCES nodes(id),
    UNIQUE (purchase_id, kind, data)
);
PRAGMA user_version = 25;
COMMIT;
";

/// Schema 26: a purchase line the person said is not a given thing (purchases spec §5): it
/// stays open for others and is no longer offered to that one.
pub(super) const SCHEMA_V26: &str = "
BEGIN;
CREATE TABLE purchase_declines (
    purchase_id INTEGER NOT NULL REFERENCES purchases(id),
    node_id INTEGER NOT NULL REFERENCES nodes(id),
    why TEXT,
    at TEXT NOT NULL,
    PRIMARY KEY (purchase_id, node_id)
);
PRAGMA user_version = 26;
COMMIT;
";

/// A line's pack size: how many units each bought quantity holds (an 8-pack of cells, a set),
/// so one line can be linked to every record its units went into.
pub(super) const SCHEMA_V27: &str = "
BEGIN;
ALTER TABLE purchases ADD COLUMN pack INTEGER NOT NULL DEFAULT 1 CHECK (pack >= 1);
PRAGMA user_version = 27;
COMMIT;
";

/// A task's due date (`YYYY-MM-DD`): a day the person wants it done by, so a date said in
/// conversation is no longer prose in its reason.
pub(super) const SCHEMA_V28: &str = "
BEGIN;
ALTER TABLE tasks ADD COLUMN due TEXT;
PRAGMA user_version = 28;
COMMIT;
";

/// Schema 29: files in the store are kept relative to the database's directory (`photos/…`,
/// `docs/…`), so the data directory can move; `ev_store` (store/files.rs) leaves paths
/// outside it and web addresses as they are.
pub(super) const SCHEMA_V29: &str = "
BEGIN;
UPDATE photos SET path = ev_store(path), source = ev_store(source);
UPDATE documents SET file = ev_store(file);
UPDATE links SET archive = ev_store(archive) WHERE archive IS NOT NULL;
PRAGMA user_version = 29;
COMMIT;
";
