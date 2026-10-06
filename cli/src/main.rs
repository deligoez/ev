use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use ev_core::{Disposition, Error, Inventory, Kind, NewDoc, NewNode, Result};
use serde_json::{Value, json};

mod history;
mod i18n;
mod input;
mod mapview;
mod mcp;
mod render;
mod settings;
mod theme;
mod ui;

/// Agent-first home inventory.
#[derive(Parser)]
#[command(name = "ev", version = env!("EV_VERSION"), about)]
struct Cli {
    /// Force JSON output even on a terminal.
    #[arg(long, global = true)]
    json: bool,

    /// Force the readable text output even through a pipe (to read a result, not parse it).
    #[arg(
        long = "text",
        id = "text_output",
        global = true,
        conflicts_with = "json"
    )]
    text_output: bool,

    /// Database file; wins over EV_DB. Defaults to ~/.ev/ev.db.
    // EV_DB is read in `db_path`, not through clap's `env`: clap names an argument an
    // environment variable fills in every usage line, as if `--db` were required.
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a node, or many with --batch/--stdin (all or none).
    Add(Box<AddArgs>),
    /// One node with its path, children, pending move and disposition.
    Show {
        reference: String,
        #[arg(long)]
        include_gone: bool,
    },
    /// The subtree under a node, or every home.
    Tree {
        reference: Option<String>,
        #[arg(long)]
        depth: Option<usize>,
    },
    /// Word search over name, code, make, model, serial, note, theme and tags: every word in any order, by stem, synonym or with a typo.
    Find {
        /// What to look for; may be left out with --tag or --kind, to list all of them.
        #[arg(default_value = "")]
        text: String,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        include_gone: bool,
        /// Only the containers nothing is in, worked out from the records (no tag to keep).
        #[arg(long)]
        empty: bool,
    },
    /// Change fields: name, code, kind, address, qty, note, theme, fill, make, model, serial, tags=+x/-x, photos=+p/-p.
    Edit {
        #[arg(required_unless_present = "stdin")]
        reference: Option<String>,
        assignments: Vec<String>,
        /// Edit several records at once, all or none: NDJSON lines
        /// `{"ref": "#551", "set": {"size": "1x2x1.5", "tags": ["+modül"], "note": null}}`
        /// (an array sets the field once per item; null clears it).
        #[arg(long, conflicts_with_all = ["reference", "assignments"])]
        stdin: bool,
    },
    /// Split one record into several kinds of thing: each `<name>=<qty>` becomes a new record
    /// beside it (same place, kind, tags). By default the parts are what each unit is made of
    /// (3 sets → `card=3` `cable=3`) and the original keeps its count; with --take they are
    /// some of its units (2 of 4 cells are another make) and come off its count. --rename and
    /// --qty set the original. The history links them both ways; photos stay on the original,
    /// to be cropped per part.
    Split {
        reference: String,
        /// `<name>=<qty>`, or just `<name>` for a record without a count.
        #[arg(required = true)]
        parts: Vec<String>,
        /// A new name for the original, for the part it keeps.
        #[arg(long)]
        rename: Option<String>,
        /// The original's count after the split.
        #[arg(long)]
        qty: Option<i64>,
        /// The parts are some of the original's units: their counts come off its count.
        #[arg(long, conflicts_with = "qty")]
        take: bool,
    },
    /// Give several nodes new codes at once: swap or rotate codes when boxes change places.
    Recode {
        /// <ref>=<new code>; an empty code clears it.
        #[arg(required = true)]
        pairs: Vec<String>,
    },
    /// Move now, or plan a move with --plan; several records at once go to one place, all or
    /// none. With --qty (one record), only that many of a counted item: they become a portion
    /// of the same thing in the new place (joining one already there), and the rest stay.
    Move {
        #[arg(required = true)]
        references: Vec<String>,
        #[arg(long)]
        to: String,
        #[arg(long)]
        plan: bool,
        /// How many of the record's units move; all of them by default.
        #[arg(long)]
        qty: Option<i64>,
    },
    /// Records made separately are one thing kept in several places: what it is comes from the
    /// first (a make or model that differs is refused), and records in the same place join.
    Join {
        #[arg(required = true, num_args = 2..)]
        references: Vec<String>,
    },
    /// A portion of a thing kept in several places is a thing of its own after all.
    Unjoin { reference: String },
    /// List pending moves.
    Pending,
    /// Apply a node's pending move.
    Done { reference: String },
    /// Drop a node's pending move.
    Cancel { reference: String },
    /// Mark a node as a candidate to leave: trash, give, sell, or digitize (photograph it,
    /// then throw the paper out; `gone` needs a photo or document on it first).
    Dispose {
        reference: String,
        #[arg(long = "as")]
        disposition: String,
        /// Shred it rather than throw it out whole (a name, a number, a barcode on it); trash
        /// or digitize only.
        #[arg(long)]
        shred: bool,
        /// Only this many of a counted item: they are set apart, the rest stay.
        #[arg(long)]
        qty: Option<i64>,
        /// What the person said about letting it go; added to its note, as with `gone --why`.
        #[arg(long)]
        why: Option<String>,
    },
    /// Return a candidate to active; with --correction, undo a gone recorded by mistake.
    Restore {
        reference: String,
        /// Why the gone record was not really gone; takes back a `gone` recorded by mistake
        /// (give the record's id).
        #[arg(long)]
        correction: Option<String>,
    },
    /// A node leaves the home; --as is required when it is not a candidate yet.
    /// --as used: it was used up (a tape run out, a dead cell). --as left (left behind),
    /// stolen, or unknown (sold or thrown out, not sure). --as mistake (with --why) closes a
    /// record that should never have existed.
    Gone {
        reference: String,
        /// How it left: trash, give, sell, trade, return, used, digitize, left, stolen,
        /// unknown or mistake; a thing set aside leaves as it was set aside.
        #[arg(long = "as")]
        disposition: Option<String>,
        /// Why it left; recorded in the event and appended to the note.
        #[arg(long)]
        why: Option<String>,
        /// It was shredded rather than thrown out whole; trash or digitize only.
        #[arg(long)]
        shred: bool,
        /// Only this many of a counted item leave; the rest stay.
        #[arg(long)]
        qty: Option<i64>,
        /// When it left, as remembered: 2016, 2016-06 or 2016-06-14 (else today).
        #[arg(long)]
        at: Option<String>,
        /// Where it was then: a place (a former home), made when new.
        #[arg(long = "where")]
        place: Option<String>,
        /// With --as trade: what came in exchange, when it is recorded.
        #[arg(long)]
        traded_for: Option<String>,
    },
    /// A thing gone, left as a trade (swapped for something else); --for links what came in
    /// exchange. Also corrects one first recorded as given or sold, and links what came later.
    Traded {
        reference: String,
        #[arg(long = "for")]
        for_: Option<String>,
    },
    /// What a sale brought, on a thing gone (or set aside) as sell: --price 1500 [--currency
    /// EUR] [--at 2019-05] [--via "a marketplace"] [--note "…"]. Said again, it is corrected.
    Sold {
        reference: String,
        #[arg(long)]
        price: String,
        /// A currency code; the home currency when not given.
        #[arg(long)]
        currency: Option<String>,
        /// When it was sold, as remembered: 2019, 2019-05 or 2019-05-14.
        #[arg(long)]
        at: Option<String>,
        /// Through what: a marketplace, a shop's trade-in, a friend.
        #[arg(long)]
        via: Option<String>,
        #[arg(long)]
        note: Option<String>,
    },
    /// The past belongings, last gone first, with per year how many left and the money paid
    /// for them and got for them; --year Y: what was ours in that year.
    Past {
        /// Only those whose name holds this word.
        #[arg(long, conflicts_with = "year")]
        name: Option<String>,
        /// Only those left in this place.
        #[arg(long = "where", conflicts_with = "year")]
        place: Option<String>,
        /// Every thing, past or present, that was ours in this year.
        #[arg(long)]
        year: Option<i32>,
    },
    /// Every candidate, grouped by disposition.
    Disposals {
        /// One pile: trash, digitize, give, sell, trade or return.
        #[arg(long = "as")]
        disposition: Option<String>,
    },
    /// Mark a node lost, or list lost nodes when no reference is given.
    Lost {
        reference: Option<String>,
        /// Only this many of a counted item are missing; the rest are where they were.
        #[arg(long, requires = "reference")]
        qty: Option<i64>,
    },
    /// A lost node is found: where it was last seen, or `--in <place>` where it turned up.
    Found {
        reference: String,
        #[arg(long = "in")]
        place: Option<String>,
    },
    /// A node's events, oldest first; `--contents` adds what came in, went out or was added.
    History {
        reference: String,
        #[arg(long)]
        contents: bool,
    },
    /// Read-only terminal browser that follows the database as it changes.
    Ui,
    /// Serve the inventory to an agent over MCP (stdio): `claude mcp add ev -- ev mcp`.
    Mcp,
    /// Lend a node of ours to a place; it stays in the tree where it returns to.
    Lend {
        reference: String,
        #[arg(long)]
        to: String,
        /// Only this many of a counted item are lent; they rejoin the rest on `ev back`.
        #[arg(long)]
        qty: Option<i64>,
    },
    /// A lent node came back.
    Back { reference: String },
    /// What to take to, return to or collect from a place; every place when none is given.
    For { place: Option<String> },
    /// Places: people, households or anywhere outside the tree, with aliases.
    #[command(subcommand)]
    Place(PlaceCmd),
    /// Where could this go: rules, where similar things are, and every place that can hold it.
    Suggest {
        /// What the thing is; add category words in both languages for a better match.
        text: Vec<String>,
        #[arg(long)]
        tag: Option<String>,
        /// Use an existing node's own name, note and tags (it does not vote for itself).
        #[arg(long = "for")]
        for_ref: Option<String>,
    },
    /// A piece of furniture read across all its places at once, from what they hold, themes
    /// left aside: kinds spread over several places, places that read alike, nearly empty ones
    /// to merge, full or mixed ones to split. `--propose` drafts a layout from the contents
    /// alone, with its moves and themes; nothing is moved.
    Layout {
        reference: String,
        #[arg(long)]
        propose: bool,
    },
    /// Regrouping hints under a place: things that fit better elsewhere, strays with a themed
    /// home, full boxes and bigger spares, sparse boxes to merge, mixed boxes, unknown fill.
    /// `--decline <thing> [--why]` records a "no" to moving it: it stays where it is and is
    /// left out of later runs until it is moved; `--allow <thing>` takes that back.
    Regroup {
        reference: Option<String>,
        #[arg(long, conflicts_with_all = ["reference", "allow"])]
        decline: Option<String>,
        #[arg(long, requires = "decline")]
        why: Option<String>,
        #[arg(long, conflicts_with_all = ["reference", "decline"])]
        allow: Option<String>,
    },
    /// Holders with things in them and no theme, with what a theme could be read from: their
    /// contents, the words those share, and the themed holder they read most like.
    Themes { reference: Option<String> },
    /// Words that mean the same thing when placing: `ev synonym add ldr "ışık sensörü"`.
    #[command(subcommand)]
    Synonym(SynonymCmd),
    /// Kinds of things kept apart when placing (modules and bare parts, novels and technical
    /// books): `ev facet add modül --words "modül, kart"`, then tag holders with it.
    #[command(subcommand)]
    Facet(FacetCmd),
    /// What a bought set should contain, part by part, and which records are those parts:
    /// `ev kit show <kit>` says what is found, lost and still missing.
    #[command(subcommand)]
    Kit(KitCmd),
    /// Where the inventory could be tidier: alike things split up, holders without a theme,
    /// items lying loose in a room or on furniture.
    Audit,
    /// Placement rules weighed on every suggestion.
    #[command(subcommand)]
    Rule(RuleCmd),
    /// Photos of a node, kept in ev's own store.
    #[command(subcommand)]
    Photo(PhotoCmd),
    /// What the household wants from ev: organize (tidy up with a plan) or track (records only).
    Goal { goal: Option<String> },
    /// Note something noticed about a place, optionally tied to its n-th photo.
    Observe {
        reference: String,
        text: String,
        #[arg(long)]
        photo: Option<usize>,
    },
    /// Remove an observation by id.
    Unobserve {
        #[arg(value_parser = record_id)]
        id: i64,
    },
    /// Mark how far a place has been counted: counting (its tour has begun), toured (counted),
    /// kept (left as it is) or raw (not counted, the default).
    Review {
        reference: String,
        #[arg(long = "as")]
        status: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Every place to go through, how far each one is and the tasks it is in; with a place (a
    /// piece of furniture, a room), only the places inside it.
    Progress { place: Option<String> },
    /// The ordered work list.
    #[command(subcommand)]
    Task(TaskCmd),
    /// Where to pick up: the current task with its places, progress and unplanned places.
    Next,
    /// Everything waiting, in one list: tasks, moves, errands, disposals, labels, needs, repairs,
    /// use-by dates, lost things, uninventoried and changed places, unclear records.
    Todo,
    /// Numbers about the home on one page: how much there is, what it cost, how far the counting
    /// has come, the purchases, the last 30 days, the boxes, coverages, tags, the oldest things.
    Stats,
    /// On the person's word, these boxes are empty (opened, nothing inside), though their place
    /// was never toured: `find --empty`, `suggest` and `regroup` then count them as empty.
    Empty {
        #[arg(required = true)]
        references: Vec<String>,
        /// What the person said.
        #[arg(long)]
        note: Option<String>,
    },
    /// Labels to print; with references, mark those printed (or --needed again).
    Label {
        references: Vec<String>,
        #[arg(long)]
        needed: bool,
    },
    /// Mark a node broken, with what is wrong.
    Broken {
        reference: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// A broken node was fixed.
    Fixed { reference: String },
    /// Record a use-by date (YYYY-MM-DD or YYYY-MM), or --clear it.
    Expires {
        reference: String,
        date: Option<String>,
        #[arg(long, conflicts_with = "date")]
        clear: bool,
    },
    /// Where a sale stands for a sell candidate: --listed, --reserved or --clear.
    Sale {
        reference: String,
        #[arg(long, group = "sale_state")]
        listed: bool,
        #[arg(long, group = "sale_state")]
        reserved: bool,
        #[arg(long, group = "sale_state")]
        clear: bool,
        #[arg(long)]
        price: Option<i64>,
        #[arg(long = "where")]
        place: Option<String>,
        /// What the buyer is told: new, like-new or used. Recorded only with a sale.
        #[arg(long, conflicts_with = "clear")]
        condition: Option<String>,
    },
    /// Things to buy or make.
    #[command(subcommand)]
    Need(NeedCmd),
    /// Documents kept in ev's own store: invoices, warranty certificates, manuals, service forms,
    /// appraisals, policies. `ev doc add <file> --kind invoice --for <ref>` copies the file in.
    #[command(subcommand)]
    Doc(DocCmd),
    /// Warranties and insurance (coverage), with a computed status: `ev cover add <ref> --kind
    /// manufacturer --term 2y`; `ev cover list --ending`.
    #[command(subcommand)]
    Cover(CoverCmd),
    /// Money over time: the cached price index and exchange rates that give a purchase price in
    /// today's money. `ev money needs | tools/money/fetch.py | ev money import --stdin`.
    #[command(subcommand)]
    Money(MoneyCmd),
    /// Close or reopen a tracked question about a thing: `ev track <ref> value|coverage
    /// no|later|yes [--why …]`. `no` and `later` are never raised again by ev on its own; a
    /// decision on a holder covers what is in it.
    Track {
        reference: String,
        subject: String,
        decision: String,
        #[arg(long)]
        why: Option<String>,
    },
    /// What a thing is worth, observed on a date: a second-hand listing, a shop's price, an
    /// appraisal. Never the purchase price. With only `<ref>`, its observations, newest first.
    Value {
        reference: String,
        /// The amount, e.g. 2500 or 2.499,90.
        amount: Option<String>,
        /// Defaults to the home currency.
        #[arg(long, requires = "amount")]
        currency: Option<String>,
        /// When it was observed: YYYY-MM-DD; today by default.
        #[arg(long, requires = "amount")]
        at: Option<String>,
        /// Where the figure comes from (sahibinden listing, appraisal, shop page).
        #[arg(long, requires = "amount")]
        source: Option<String>,
        #[arg(long, requires = "amount")]
        note: Option<String>,
        /// The date is a guess.
        #[arg(long, requires = "amount")]
        approximate: bool,
        /// Remove an observation recorded by mistake, by its id.
        #[arg(long, conflicts_with = "amount", value_parser = record_id)]
        remove: Option<i64>,
    },
    /// A thing's links: its product page, manual, support or driver page, with an archive copy
    /// for when the page dies.
    #[command(subcommand)]
    Link(LinkCmd),
    /// Purchases: lines of what was bought, linked to things on the person's word. A line never
    /// creates a thing. `ev buy import` takes an adapter's NDJSON; `ev buy add` one by hand.
    #[command(subcommand)]
    Buy(BuyCmd),
    /// A holder laid out in cells (a gridfinity drawer, a Kallax): show its map, or set its
    /// size — for several holders at once when they are alike (`ev grid K4x4-01 K4x4-02 …
    /// --cols 1 --rows 2`).
    Grid {
        #[arg(required = true)]
        references: Vec<String>,
        #[arg(long, requires = "rows")]
        cols: Option<i64>,
        #[arg(long, requires = "cols")]
        rows: Option<i64>,
        /// How the grid is seen: `above` (a drawer, row 1 at the back) or `front` (furniture
        /// and its compartments, row 1 at the top).
        #[arg(long, value_parser = ["above", "front"])]
        face: Option<String>,
        #[arg(long, conflicts_with_all = ["cols", "rows", "face"])]
        clear: bool,
    },
    /// A sketch in centimetres, seen from above: `--size w,d` for a room (or the home), `--at
    /// x,y` for where it lies in its holder (its top-left corner), or beside another thing in the
    /// same holder (`--right-of`, `--left-of`, `--above`, `--below`, slid along that side by
    /// `--offset`); `--points "x,y x,y …"` for a room that is not a rectangle; `--on <ref>` for
    /// furniture standing on another. With no option, show it. `--stdin` takes NDJSON lines
    /// `{"ref": …, "points": [[x, y], …]}` with the same fields, all or none.
    Sketch {
        #[arg(required_unless_present = "stdin")]
        reference: Option<String>,
        #[arg(long)]
        at: Option<String>,
        #[arg(long)]
        size: Option<String>,
        #[arg(long)]
        on: Option<String>,
        #[arg(long, conflicts_with_all = ["at", "size"])]
        points: Option<String>,
        #[arg(long, conflicts_with_all = ["at", "points", "left_of", "above", "below"])]
        right_of: Option<String>,
        #[arg(long, conflicts_with_all = ["at", "points", "above", "below"])]
        left_of: Option<String>,
        #[arg(long, conflicts_with_all = ["at", "points", "below"])]
        above: Option<String>,
        #[arg(long, conflicts_with_all = ["at", "points"])]
        below: Option<String>,
        /// Centimetres along the side it is placed beside, from the other's top or left edge.
        #[arg(long, allow_negative_numbers = true)]
        offset: Option<f64>,
        #[arg(long, conflicts_with_all = ["at", "size", "on", "points", "right_of", "left_of", "above", "below", "offset"])]
        clear: bool,
        /// NDJSON lines from standard input, applied in order, all or none.
        #[arg(long, conflicts_with_all = ["reference", "at", "size", "on", "points", "right_of", "left_of", "above", "below", "offset", "clear"])]
        stdin: bool,
    },
    /// The map of a place (the home without one): its contents as tiles laid out by its grid,
    /// its sketch, or on their own; a stack of furniture front on, top first.
    Map { reference: Option<String> },
    /// Place boxes in their holder's grid: `<ref>=A3` or `<ref>=A3-B4`, several at once;
    /// `<ref>=` takes one out. A box keeps its code: it is the box's serial label.
    Cell {
        #[arg(required = true)]
        pairs: Vec<String>,
    },
    /// Show or change display settings: `language en|tr|auto`, `theme dark|light|auto`,
    /// `resume on|off` (`ev ui` reopens on the node it was on).
    Settings {
        name: Option<String>,
        value: Option<String>,
    },
    /// Make a running `ev ui` show a node and one of its photos (the last by default), or add
    /// marked photos to its series (`--file`; `--list` reads it, `--clear` ends it).
    Focus {
        reference: Option<String>,
        #[arg(long)]
        photo: Option<usize>,
        #[arg(long, conflicts_with = "reference")]
        clear: bool,
        /// Show pictures that are no record (marked photos) instead of a node; repeat for
        /// several, stepped through with `[` `]`. `a.jpg=<note>` titles that one on its own.
        #[arg(long, conflicts_with_all = ["reference", "clear"])]
        file: Vec<String>,
        /// The title shown over --file.
        #[arg(long, requires = "file")]
        note: Option<String>,
        /// The series of marked photos in `ev ui`: each picture, its note and its frames
        /// (number → where, or which record), and the next free number.
        #[arg(long, conflicts_with_all = ["reference", "clear", "file"])]
        list: bool,
    },
}

#[derive(Subcommand)]
enum SynonymCmd {
    /// A group of words or phrases that mean the same thing: `ldr "ışık sensörü" fotodirenç`.
    Add {
        #[arg(required = true)]
        words: Vec<String>,
    },
    /// Every synonym group.
    List,
    /// Drop a group by id.
    Remove {
        #[arg(value_parser = record_id)]
        id: i64,
    },
}

#[derive(Subcommand)]
enum FacetCmd {
    /// A kind of thing kept apart (`modül --words "modül, kart"`); holders join it by carrying
    /// its name as a tag. Adding an existing facet replaces its words.
    Add {
        name: String,
        /// Comma-separated words that also tell a thing's facet from its name.
        #[arg(long)]
        words: Option<String>,
    },
    /// Every facet with its words and the holders tagged with it.
    List,
    /// Drop a facet; the tags stay on the holders.
    Remove { name: String },
}

#[derive(Subcommand)]
enum KitCmd {
    /// Record a kit: its name, how many were bought (--copies), and its parts, each
    /// `--part "<name>"` or `--part "<name>=<how many in one copy>"`.
    Add {
        name: String,
        #[arg(long)]
        copies: Option<i64>,
        #[arg(long)]
        note: Option<String>,
        #[arg(long = "part")]
        parts: Vec<String>,
        /// The purchase line the whole kit was bought as (see `ev kit purchase`).
        #[arg(long, value_parser = record_id)]
        purchase: Option<i64>,
    },
    /// The purchase line the whole kit was bought as: it settles the line, and every record
    /// linked to the kit sees it as its purchase and is offered no other. `--clear` takes it back.
    Purchase {
        kit: String,
        #[arg(value_parser = record_id, required_unless_present = "clear")]
        line: Option<i64>,
        #[arg(long, conflicts_with = "line")]
        clear: bool,
    },
    /// Add parts to the end of a kit's list: `<name>` or `<name>=<how many in one copy>`.
    Part {
        kit: String,
        #[arg(required = true)]
        parts: Vec<String>,
    },
    /// Take part N off the kit's list (entered by mistake); refused while records are linked
    /// to it. The other parts keep their numbers.
    Drop { kit: String, n: i64 },
    /// Name part N anew: `<name>` or `<name>=<how many in one copy>`; its records stay linked.
    Rename { kit: String, n: i64, part: String },
    /// These records are part N of the kit (numbered as `ev kit show` lists them).
    Link {
        kit: String,
        n: i64,
        #[arg(required = true)]
        references: Vec<String>,
    },
    /// This record is not part N after all.
    Unlink {
        kit: String,
        n: i64,
        reference: String,
    },
    /// Every kit with how many of its parts are found, lost and still open.
    List,
    /// One kit part by part: expected, the records that are it and where, found, lost, open.
    Show { kit: String },
    /// Drop a kit and its links; the records stay.
    Remove { kit: String },
}

/// The NDJSON of `ev edit --stdin`: `{"ref": …, "set": {field: value}}` per line, turned into
/// `field=value` assignments. A string or number is one assignment, an array one per item,
/// null an empty value (which clears the field). Blank lines are skipped.
fn edit_lines(text: &str) -> Result<Vec<(String, Vec<String>)>> {
    let mut lines = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        if raw.trim().is_empty() {
            continue;
        }
        let at = |msg: String| Error::Usage(msg).at_line(i + 1);
        let v: Value =
            serde_json::from_str(raw).map_err(|e| at(format!("not a JSON object: {e}")))?;
        let reference = match &v["ref"] {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => return Err(at("`ref` is missing (a name, code, id or #id)".into())),
        };
        let set = v["set"]
            .as_object()
            .ok_or_else(|| at("`set` is missing: an object of field: value".into()))?;
        let scalar = |field: &str, v: &Value| match v {
            Value::String(s) => Ok(format!("{field}={s}")),
            Value::Number(n) => Ok(format!("{field}={n}")),
            Value::Bool(b) => Ok(format!("{field}={b}")),
            Value::Null => Ok(format!("{field}=")),
            _ => Err(at(format!("`{field}`: a value is text, a number or null"))),
        };
        let mut assignments = Vec::new();
        for (field, value) in set {
            match value {
                // `"tags": ["a", "-b"]`: each is added unless it says otherwise, as a list of
                // tags reads in NDJSON; `"tags": "+a"` is the same as `["a"]`.
                Value::Array(items) if field == "tags" || field == "photos" => {
                    for item in items {
                        let a = scalar(field, item)?;
                        let v = &a[field.len() + 1..];
                        assignments.push(if v.starts_with(['+', '-']) {
                            a
                        } else {
                            format!("{field}=+{v}")
                        });
                    }
                }
                Value::Array(items) => {
                    for item in items {
                        assignments.push(scalar(field, item)?);
                    }
                }
                other => assignments.push(scalar(field, other)?),
            }
        }
        lines.push((reference, assignments));
    }
    Ok(lines)
}

/// `<name>` or `<name>=<count>`: a kit part and how many come in one copy (1 by default).
fn kit_parts(parts: &[String]) -> Result<Vec<(String, i64)>> {
    parts
        .iter()
        .map(|p| match p.rsplit_once('=') {
            Some((name, n)) => n
                .trim()
                .parse::<i64>()
                .map(|n| (name.trim().to_string(), n))
                .map_err(|_| Error::Usage(format!("`{p}`: the count after = is not a number"))),
            None => Ok((p.trim().to_string(), 1)),
        })
        .collect()
}
#[derive(Args)]
struct CoverAddArgs {
    #[arg(required = true)]
    references: Vec<String>,
    /// statutory, manufacturer, extended, store or insurance.
    #[arg(long)]
    kind: String,
    /// 2y, 18m, 6w, 90d or lifetime.
    #[arg(long)]
    term: Option<String>,
    /// When it starts: delivery (the default: the linked purchase's), a date, or
    /// after:<coverage id> (an extended warranty after the manufacturer's).
    #[arg(long)]
    from: Option<String>,
    /// An explicit end date (an insurance policy's).
    #[arg(long)]
    ends: Option<String>,
    /// A usage limit beside the time term: 5000 h, 60000 km.
    #[arg(long)]
    usage: Option<String>,
    /// Brand, importer, distributor or insurer.
    #[arg(long)]
    issuer: Option<String>,
    /// The warranty or policy number.
    #[arg(long)]
    number: Option<String>,
    #[arg(long)]
    premium: Option<String>,
    #[arg(long)]
    deductible: Option<String>,
    #[arg(long)]
    currency: Option<String>,
    /// What an insurance covers: screen breakage, theft.
    #[arg(long)]
    scope: Option<String>,
    #[arg(long)]
    note: Option<String>,
    /// The purchase line it was bought as (an extended warranty sold on its own): it settles
    /// the line, and its price is the premium when none is given.
    #[arg(long, value_parser = record_id)]
    purchase: Option<i64>,
}

#[derive(Subcommand)]
enum CoverCmd {
    /// A warranty or an insurance for one or more things.
    Add(Box<CoverAddArgs>),
    /// Every coverage; --ending only those ending within the warning window.
    List {
        #[arg(long)]
        ending: bool,
    },
    /// One coverage with its things, documents and status.
    Show {
        #[arg(value_parser = record_id)]
        id: i64,
    },
    /// Remove a coverage recorded by mistake; its documents stay.
    Remove {
        #[arg(value_parser = record_id)]
        id: i64,
    },
    /// The purchase line a coverage was bought as (an extended warranty sold as a line of its
    /// own): it settles the line, the coverage shows it, and its price is the premium when none
    /// was given. `--clear` takes it back.
    Purchase {
        #[arg(value_parser = record_id)]
        coverage: i64,
        #[arg(value_parser = record_id, required_unless_present = "clear")]
        line: Option<i64>,
        #[arg(long, conflicts_with = "line")]
        clear: bool,
    },
}

#[derive(Subcommand)]
enum MoneyCmd {
    /// What to fetch: the index series from the earliest purchase month, and the rates of
    /// foreign-currency purchase days not cached.
    Needs,
    /// Import index and rate lines (NDJSON) from tools/money.
    Import {
        #[arg(conflicts_with = "stdin")]
        file: Option<PathBuf>,
        #[arg(long)]
        stdin: bool,
    },
    /// What is cached and whether the index is stale.
    Status,
}

#[derive(Subcommand)]
enum BuyCmd {
    /// Import an adapter's NDJSON (`purchase` and `document` lines); importing the same lines
    /// again changes nothing. All or nothing.
    Import {
        #[arg(conflicts_with = "stdin")]
        file: Option<PathBuf>,
        #[arg(long)]
        stdin: bool,
    },
    /// A purchase entered by hand: bought in a shop, a gift, from a person.
    Add {
        name: String,
        #[arg(long)]
        shop: Option<String>,
        /// When it was bought, YYYY-MM-DD.
        #[arg(long)]
        date: Option<String>,
        /// What was paid for the whole line, e.g. 1234.56.
        #[arg(long)]
        paid: Option<String>,
        #[arg(long)]
        currency: Option<String>,
        #[arg(long, default_value_t = 1)]
        qty: i64,
        /// Units in each bought quantity (an 8-pack, a set), for linking them to several things.
        #[arg(long, default_value_t = 1)]
        pack: i64,
        /// The order number, for customer service.
        #[arg(long)]
        order: Option<String>,
        #[arg(long)]
        order_url: Option<String>,
        /// The product's page.
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        brand: Option<String>,
        /// durable (the default), clothing, digital (a licence, a game key, a membership) or
        /// service (a diet programme, a repair): what is never a thing in the home is not
        /// waiting to be linked.
        #[arg(long)]
        bucket: Option<String>,
        /// Link it to this thing at once.
        #[arg(long = "for")]
        for_ref: Option<String>,
    },
    /// Purchase lines, newest first.
    List {
        /// Only lines with something left to link and not dismissed.
        #[arg(long)]
        open: bool,
        #[arg(long)]
        bucket: Option<String>,
        #[arg(long)]
        shop: Option<String>,
        /// Bought on or after YYYY-MM-DD.
        #[arg(long)]
        since: Option<String>,
        /// Only lines with every word of it in the name, shop, brand, product code or order
        /// number: is there a purchase of X, before or without a record.
        #[arg(long)]
        query: Option<String>,
    },
    /// One line with what it is linked to and its documents.
    Show {
        #[arg(value_parser = record_id)]
        id: i64,
    },
    /// Purchase lines that could be this thing, best first, with the reasons; `--toured`: every
    /// unlinked thing in a toured place with the one line that could be it (the back-fill).
    For {
        #[arg(required_unless_present = "toured", conflicts_with = "toured")]
        reference: Option<String>,
        #[arg(long)]
        toured: bool,
    },
    /// Link a line to a thing on the person's word (all that is left of it by default; on a line
    /// in packs, as many units as the thing stands for).
    Link {
        #[arg(value_parser = record_id)]
        id: i64,
        reference: String,
        #[arg(long)]
        qty: Option<i64>,
    },
    /// Units in each bought quantity of a line (an 8-pack, a set), so its units can be linked
    /// to several things.
    Pack {
        #[arg(value_parser = record_id)]
        id: i64,
        pack: i64,
    },
    /// Undo a link.
    Unlink {
        #[arg(value_parser = record_id)]
        id: i64,
        reference: String,
    },
    /// Settle a line that will never be a thing: consumed, given, returned, elsewhere,
    /// not-mine, duplicate; `--clear` takes that back.
    Dismiss {
        #[arg(value_parser = record_id)]
        id: i64,
        #[arg(long = "as", required_unless_present = "clear")]
        reason: Option<String>,
        #[arg(long)]
        why: Option<String>,
        #[arg(long, conflicts_with = "reason")]
        clear: bool,
    },
    /// The person's "not this one": the line is not this thing. It stays open for others and is
    /// no longer offered to this one; `--clear` takes that back.
    Decline {
        #[arg(value_parser = record_id)]
        id: i64,
        reference: String,
        #[arg(long)]
        why: Option<String>,
        #[arg(long, conflicts_with = "why")]
        clear: bool,
    },
    /// Bring what came with a line (a link, a value, a warranty, a product image) to a thing it
    /// is linked to. Says what it brought and what it left, and why.
    Bring {
        #[arg(value_parser = record_id, required_unless_present = "all")]
        id: Option<i64>,
        #[arg(required_unless_present = "all")]
        reference: Option<String>,
        /// Every linked line, each to the thing it is linked to: the back-fill. A line
        /// linked to several things, or to a thing that is gone, is left and named.
        #[arg(long, conflicts_with_all = ["id", "reference", "only"])]
        all: bool,
        /// Only these attachments, by id (repeatable or comma-separated); all not yet brought
        /// by default.
        #[arg(long, value_parser = record_id, value_delimiter = ',')]
        only: Vec<i64>,
        /// Only attachments of these types: link, valuation, coverage, image (repeatable or
        /// comma-separated).
        #[arg(long = "type", value_delimiter = ',')]
        r#type: Vec<String>,
    },
}

#[derive(Subcommand)]
enum LinkCmd {
    /// Add a link, or update the kind, archive and note of the same address.
    Add {
        reference: String,
        url: String,
        /// info, manual, support, driver or other.
        #[arg(long, default_value = "info")]
        kind: String,
        /// Where the page survives: a saved file (copied into the store) or a Wayback address.
        #[arg(long)]
        archive: Option<String>,
        #[arg(long)]
        note: Option<String>,
    },
    /// A thing's links.
    List { reference: String },
    /// Remove a link by its id.
    Remove {
        #[arg(value_parser = record_id)]
        id: i64,
    },
}

#[derive(Subcommand)]
enum DocCmd {
    /// Copy a file into the document store and link it to things; the same file again is the
    /// same document, only the new links are added.
    Add {
        file: PathBuf,
        /// invoice, warranty, manual, service, appraisal, policy, scan (a copy of a paper
        /// thrown out once copied) or other.
        #[arg(long)]
        kind: String,
        /// What it belongs to (repeatable).
        #[arg(long = "for")]
        for_refs: Vec<String>,
        /// The document's number (invoice no, policy no).
        #[arg(long)]
        number: Option<String>,
        /// An e-Archive invoice's UUID.
        #[arg(long)]
        ettn: Option<String>,
        /// When it was issued: YYYY-MM-DD, YYYY-MM or YYYY.
        #[arg(long)]
        issued: Option<String>,
        /// Who issued it (a shop, a brand, a service).
        #[arg(long)]
        issuer: Option<String>,
        #[arg(long)]
        note: Option<String>,
        /// The coverage it proves (a warranty certificate, a policy).
        #[arg(long, value_parser = record_id)]
        coverage: Option<i64>,
    },
    /// Every document, or those of one thing.
    List { reference: Option<String> },
    /// One document with what it belongs to.
    Show {
        #[arg(value_parser = record_id)]
        id: i64,
    },
    /// Link a stored document to one more thing.
    Link {
        #[arg(value_parser = record_id)]
        id: i64,
        reference: String,
    },
    /// Take a document off a thing; it stays in the store.
    Unlink {
        #[arg(value_parser = record_id)]
        id: i64,
        reference: String,
    },
}

#[derive(Subcommand)]
enum NeedCmd {
    /// Something to buy (or --make, e.g. 3D print), optionally for a place.
    Add {
        text: String,
        #[arg(long)]
        qty: Option<i64>,
        #[arg(long)]
        make: bool,
        #[arg(long = "for")]
        for_ref: Option<String>,
        #[arg(long)]
        note: Option<String>,
    },
    /// Open needs; --all adds closed ones.
    List {
        #[arg(long)]
        all: bool,
    },
    /// It was bought or made.
    Got {
        #[arg(value_parser = record_id)]
        id: i64,
        #[arg(long)]
        note: Option<String>,
    },
    /// It is no longer needed.
    Drop {
        #[arg(value_parser = record_id)]
        id: i64,
        #[arg(long)]
        note: Option<String>,
    },
}

#[derive(Subcommand)]
enum TaskCmd {
    /// Add a task with the reason it matters; --at puts it at that position.
    Add {
        title: String,
        #[arg(long)]
        why: String,
        /// A place the task is about; repeatable.
        #[arg(long = "on")]
        on: Vec<String>,
        #[arg(long)]
        at: Option<usize>,
        /// The day the person wants it done by (YYYY-MM-DD); `ev next` puts it first a day
        /// before.
        #[arg(long)]
        due: Option<String>,
    },
    /// Unfinished tasks in order; --all adds finished and dropped ones.
    List {
        #[arg(long)]
        all: bool,
    },
    /// One task.
    Show {
        #[arg(value_parser = record_id)]
        id: i64,
    },
    /// Start working on a task (one at a time).
    Start {
        #[arg(value_parser = record_id)]
        id: i64,
    },
    /// Close a task as done; only when the person says so.
    Done {
        #[arg(value_parser = record_id)]
        id: i64,
        #[arg(long)]
        note: Option<String>,
    },
    /// Close a task without doing it.
    Drop {
        #[arg(value_parser = record_id)]
        id: i64,
        #[arg(long)]
        note: Option<String>,
    },
    /// Reopen a closed task.
    Reopen {
        #[arg(value_parser = record_id)]
        id: i64,
    },
    /// Change title, reason, position, due date (--due none clears it) or places (--on / --off).
    Edit {
        #[arg(value_parser = record_id)]
        id: i64,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        why: Option<String>,
        #[arg(long = "on")]
        on: Vec<String>,
        #[arg(long = "off")]
        off: Vec<String>,
        #[arg(long)]
        at: Option<usize>,
        #[arg(long)]
        due: Option<String>,
    },
}

#[derive(Subcommand)]
enum PhotoCmd {
    /// Copy a photo into the store and attach it; --crop x,y,w,h (fractions 0–1) attaches a cut-out.
    /// To check a crop before it is made, cut it with `ev photo cut <file> <ref>=x,y,w,h --preview`
    /// instead: the same crop, with a sheet and the photo framed.
    Add {
        #[arg(required_unless_present = "stdin")]
        reference: Option<String>,
        /// One or more photos: files, or `f12` (a picture of the marked photo series, attached
        /// with the note it was sent with and not sent again).
        #[arg(required_unless_present = "stdin")]
        files: Vec<PathBuf>,
        #[arg(long)]
        crop: Option<String>,
        #[arg(long)]
        note: Option<String>,
        /// Attach the whole photo even though it is already attached whole to another node.
        #[arg(long, conflicts_with = "crop")]
        whole: bool,
        /// Turn the photo clockwise first (90, 180 or 270) and store it turned; --crop is a
        /// fraction of the turned photo.
        #[arg(long)]
        rotate: Option<u16>,
        /// Grow --crop on each side by this fraction of its own size (0.1: a tenth).
        #[arg(long, requires = "crop")]
        pad: Option<f64>,
        /// Do not send it to the marked photo series in a running `ev ui`, which it joins by
        /// default, unframed and titled with --note (else with the record).
        #[arg(long)]
        no_show: bool,
        /// Attach many at once, one JSON object a line on standard input:
        /// `{"ref": "#12", "photo": "f2", "note": "…"}` (`photo` a file or `f12`).
        #[arg(long, conflicts_with_all = ["reference", "files", "crop", "rotate", "note"])]
        stdin: bool,
    },
    /// Cut one photo up among several nodes at once: `<ref>=x,y,w,h` for each, and the whole
    /// photo on --place (the drawer or box it shows). All or nothing.
    Cut {
        file: PathBuf,
        #[arg(required_unless_present = "place")]
        pieces: Vec<String>,
        #[arg(long)]
        place: Option<String>,
        #[arg(long)]
        note: Option<String>,
        /// The --place grid's corners in the photo, back-left, back-right, front-right,
        /// front-left, as x,y fractions: every box placed in the grid gets its crop from them.
        #[arg(long)]
        grid: Option<String>,
        /// Cut nothing: draw every crop it would make (each grid box on its cells) on a
        /// temporary copy, to check the corners by eye; the note titles it in `ev ui`.
        #[arg(long, num_args = 0..=1)]
        preview: Option<Option<String>>,
        /// Shown by default: kept so older calls still parse.
        #[arg(long, hide = true)]
        show: bool,
        /// Do not send the numbered photo (and the preview) to a running `ev ui`, which a cut
        /// does by default, titled with --note (else with what each number is).
        #[arg(long, conflicts_with = "show")]
        no_show: bool,
        /// Turn the photo clockwise first (90, 180 or 270) and store it turned; the crops and
        /// --grid are fractions of the turned photo.
        #[arg(long)]
        rotate: Option<u16>,
        /// Grow every crop named by hand on each side by this fraction of its own size (0.1:
        /// a tenth), so an edge the estimate cut off stays in; the grid's crops have a margin.
        #[arg(long)]
        pad: Option<f64>,
    },
    /// Draw numbered marks on a copy of a photo, to show which thing is meant and where it goes:
    /// `<label>=x,y,w,h` (fractions of the upright photo) or `<label>=A6` (cells of the grid,
    /// when TARGET is a place). The copy is temporary: not stored, not attached.
    Mark {
        /// A photo file, or a place whose newest whole photo is marked.
        target: String,
        #[arg(required_unless_present = "codes")]
        marks: Vec<String>,
        /// The grid's corners in the photo, when it did not keep them (see `photo cut --grid`).
        #[arg(long)]
        grid: Option<String>,
        /// Where to write the marked copy; a scratch folder otherwise.
        #[arg(long)]
        out: Option<PathBuf>,
        /// The title it is shown under in a running `ev ui` (the labels by default).
        #[arg(long)]
        show: Option<String>,
        /// Do not send it to a running `ev ui`, which it is by default.
        #[arg(long, conflicts_with = "show")]
        no_show: bool,
        /// Also draw each placed box's code on its own cells (TARGET a place with a grid): which
        /// label goes on which box.
        #[arg(long)]
        codes: bool,
        /// Draw the numbers as given. A shown mark's numbered labels (`1`, `2 → A6`) otherwise
        /// go on from the series in `ev ui`; this is for marks that point at frames already
        /// numbered there (a destination: `4=A6`, frame 4 goes to A6).
        #[arg(long, conflicts_with = "no_show")]
        keep_numbers: bool,
    },
    /// A node's photos, numbered from 1.
    List { reference: String },
    /// Detach the n-th photo of a node.
    Remove { reference: String, n: usize },
    /// Turn a node's n-th photo clockwise for good, with every crop cut from it on any record
    /// (re-cut to show the same part upright) and its grid corners; a crop turns its source
    /// photo.
    Rotate {
        reference: String,
        n: usize,
        /// 90, 180 or 270, clockwise.
        degrees: u16,
    },
    /// Copy every photo still referenced outside the store into it.
    Adopt,
    /// The newest photo still shows the place well enough; drop it from the photo-needed list.
    Current { reference: String },
    /// The newest photo no longer shows the place, though the records saw no change (it was
    /// emptied before it was recorded); keep it on the photo-needed list until a newer photo.
    Stale {
        reference: String,
        /// What is different now.
        #[arg(long)]
        why: Option<String>,
    },
}

#[derive(Subcommand)]
enum RuleCmd {
    /// Add a rule in plain words.
    Add { text: String },
    /// Every rule with its id.
    List,
    /// Remove a rule by id.
    Remove {
        #[arg(value_parser = record_id)]
        id: i64,
    },
}

#[derive(Subcommand)]
enum PlaceCmd {
    /// Create a place with optional aliases.
    Add {
        name: String,
        #[arg(long = "alias")]
        aliases: Vec<String>,
    },
    /// Give an existing place another name.
    Alias { place: String, alias: String },
    /// Every place with its aliases and open errands.
    List,
    /// Fold one place into another; references and aliases move.
    Merge { from: String, into: String },
}

#[derive(Args)]
struct AddArgs {
    name: Option<String>,
    #[arg(long)]
    kind: Option<String>,
    #[arg(long = "in")]
    parent: Option<String>,
    #[arg(long)]
    lost: bool,
    #[arg(long)]
    code: Option<String>,
    #[arg(long)]
    address: Option<String>,
    #[arg(long)]
    qty: Option<i64>,
    #[arg(long)]
    note: Option<String>,
    #[arg(long)]
    theme: Option<String>,
    #[arg(long)]
    fill: Option<i64>,
    /// Outer size WxDxH, e.g. 1x2x0.5 for a gridfinity bin.
    #[arg(long)]
    size: Option<String>,
    #[arg(long = "tag")]
    tags: Vec<String>,
    #[arg(long = "photo")]
    photos: Vec<String>,
    /// Place this node should be taken to.
    #[arg(long)]
    to: Option<String>,
    /// Place this node belongs to when it is not ours.
    #[arg(long)]
    owner: Option<String>,
    /// A parking place: what is put in it waits for its final place.
    #[arg(long)]
    temporary: bool,
    /// Make, as on the label (Bosch).
    #[arg(long)]
    make: Option<String>,
    /// Model, as on the label (GSB 13 RE).
    #[arg(long)]
    model: Option<String>,
    /// Serial number, as on the label.
    #[arg(long)]
    serial: Option<String>,
    /// More of a thing already recorded: its name, kind, make, model, size and tags come from
    /// this record, and the new units are a portion of the same thing (joining one already in
    /// the --in place).
    #[arg(long, conflicts_with_all = ["name", "kind", "make", "model", "serial"])]
    of: Option<String>,
    /// A past thing, recorded already gone: how it left (sell, give, trash, used, trade, left,
    /// stolen, unknown). It is in no holder; --kind defaults to item.
    #[arg(long, conflicts_with_all = ["parent", "lost", "of"])]
    gone: Option<String>,
    /// With --gone: when it left, as remembered (2016, 2016-06, 2016-06-14).
    #[arg(long, requires = "gone")]
    at: Option<String>,
    /// With --gone: where it was then, a place (a former home), made when new.
    #[arg(long = "where", requires = "gone")]
    place: Option<String>,
    /// With --gone trade: what came in exchange, when it is recorded.
    #[arg(long, requires = "gone")]
    traded_for: Option<String>,
    /// When it came, as remembered (2014, 2014-03).
    #[arg(long)]
    came: Option<String>,
    /// NDJSON file, one node per line.
    #[arg(long, conflicts_with = "stdin")]
    batch: Option<PathBuf>,
    /// Read NDJSON lines from stdin.
    #[arg(long)]
    stdin: bool,
}

/// A record's id as ev prints it (`#12`) or bare (`12`): a purchase line, a task, a document,
/// an observation. Node references are not ids here; they resolve through the inventory.
fn record_id(s: &str) -> std::result::Result<i64, String> {
    s.strip_prefix('#')
        .unwrap_or(s)
        .parse()
        .map_err(|_| "expected a record id such as 12 or #12".to_string())
}

/// Whether the parser's error is a page to read (`--help`, `--version`) rather than a mistake.
fn is_page(e: &clap::Error) -> bool {
    matches!(
        e.kind(),
        clap::error::ErrorKind::DisplayHelp
            | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            | clap::error::ErrorKind::DisplayVersion
    )
}

/// A mistyped argument as ev's own usage error, so JSON mode answers in JSON (spec/output.md).
fn usage_error(e: &clap::Error) -> Error {
    let text = e.render().to_string();
    Error::Usage(text.trim().trim_start_matches("error: ").to_string())
}

/// `make=Optika` given to a command that takes `--make`: `ev edit`'s form used out of habit. The
/// hint names the flag, so the next try is right.
fn flag_hint(args: &[String]) -> Option<String> {
    use clap::CommandFactory;
    let cmd = Cli::command();
    let (i, sub) = args
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(i, a)| cmd.find_subcommand(a).map(|s| (i, s)))?;
    args[i + 1..].iter().find_map(|a| {
        let (k, v) = a.split_once('=')?;
        let flag = k.replace('_', "-");
        sub.get_arguments()
            .any(|x| x.get_long() == Some(flag.as_str()))
            .then(|| {
                format!(
                    "`{a}`: `ev {}` takes it as `--{flag} {v}` (key=value is `ev edit`'s form)",
                    sub.get_name()
                )
            })
    })
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) if is_page(&e) => e.exit(),
        Err(e) => {
            // Decided from the arguments as written: they did not parse.
            let args: Vec<String> = std::env::args().collect();
            let json = args.iter().any(|a| a == "--json")
                || (!args.iter().any(|a| a == "--text") && !std::io::stdout().is_terminal());
            let hint = flag_hint(&args);
            if !json {
                match hint {
                    Some(h) => {
                        eprintln!("error: {h}");
                        return ExitCode::from(2);
                    }
                    None => e.exit(),
                }
            }
            let err = hint.map_or_else(|| usage_error(&e), Error::Usage);
            eprintln!("{}", err.to_json());
            return ExitCode::from(err.code() as u8);
        }
    };
    if let Cmd::Mcp = cli.cmd {
        // Each call sets the person's language on the thread it runs on (mcp::run_args).
        return match mcp::serve(cli.db) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprint!("{}", render::error(&e));
                ExitCode::from(e.code() as u8)
            }
        };
    }
    let json = cli.json || (!cli.text_output && !std::io::stdout().is_terminal());
    // JSON has no words to translate; skip reading the settings (and the system language).
    if !json {
        ui::set_language_from_settings();
    }
    match run(cli) {
        Ok(Value::Null) => ExitCode::SUCCESS,
        Ok(value) => {
            let out = if json {
                render::json(&value) + "\n"
            } else {
                render::human(&value)
            };
            // A reader that stops early (`ev buy list | head`) closes the pipe; that is not an
            // error of the command, and print! would panic on it.
            let _ = std::io::Write::write_all(&mut std::io::stdout().lock(), out.as_bytes());
            ExitCode::SUCCESS
        }
        Err(e) => {
            if json {
                eprintln!("{}", e.to_json());
            } else {
                eprint!("{}", render::error(&e));
            }
            ExitCode::from(e.code() as u8)
        }
    }
}

thread_local! {
    /// Standard input as a command sees it. `None`: the process's own (the CLI). `Some(text)`:
    /// the text an MCP call gave, or `Some(None)` when it gave none — the MCP server's own stdin
    /// is the protocol stream, and reading it would corrupt the session.
    static INPUT: std::cell::RefCell<Option<Option<String>>> = const { std::cell::RefCell::new(None) };
}

/// Runs `f` with `text` as the standard input every command reads (`--stdin`); `None` means
/// none was given, which such a command then refuses.
fn with_input<T>(text: Option<String>, f: impl FnOnce() -> T) -> T {
    INPUT.with(|i| *i.borrow_mut() = Some(text));
    let out = f();
    INPUT.with(|i| *i.borrow_mut() = None);
    out
}

/// What a command reads as standard input: the process's, or the text set by `with_input`.
fn read_input() -> Result<String> {
    match INPUT.with(|i| i.borrow_mut().as_mut().map(Option::take)) {
        Some(Some(text)) => Ok(text),
        Some(None) => Err(Error::Usage(
            "this command reads lines from standard input; pass them as the call's `input`".into(),
        )),
        None => {
            let mut s = String::new();
            std::io::stdin()
                .read_to_string(&mut s)
                .map_err(|e| Error::Usage(format!("cannot read stdin: {e}")))?;
            Ok(s)
        }
    }
}

fn db_path(flag: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = flag {
        return Ok(p);
    }
    if let Some(p) = std::env::var_os("EV_DB").filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| Error::Internal("HOME is not set; pass --db".into()))?;
    Ok(PathBuf::from(home).join(".ev").join("ev.db"))
}

fn disposition(s: &str) -> Result<Disposition> {
    s.parse()
}

/// Settings live outside the database, so this runs without opening it.
fn settings_cmd(name: Option<String>, value: Option<String>) -> Result<Value> {
    use settings::{LangPref, Settings, ThemePref};
    let path =
        Settings::path().ok_or_else(|| Error::Internal("HOME is not set; set EV_CONFIG".into()))?;
    let mut s = Settings::load_from(&path);
    match (name.as_deref(), value.as_deref()) {
        (None, _) => {}
        (Some(n), None) => {
            return Err(Error::Usage(format!(
                "give a value: ev settings {n} <value>"
            )));
        }
        (Some("language" | "lang"), Some(v)) => {
            s.language = LangPref::parse(v)
                .ok_or_else(|| Error::Usage(format!("language is en, tr or auto, not `{v}`")))?;
        }
        (Some("theme" | "appearance"), Some(v)) => {
            s.theme = ThemePref::parse(v)
                .ok_or_else(|| Error::Usage(format!("theme is dark, light or auto, not `{v}`")))?;
        }
        (Some("resume"), Some(v)) => {
            s.resume = settings::parse_switch(v)
                .ok_or_else(|| Error::Usage(format!("resume is on or off, not `{v}`")))?;
        }
        (Some("series_tile"), Some(v)) => {
            s.series_tile = v
                .parse::<u16>()
                .ok()
                .filter(|w| (settings::SERIES_TILE_MIN..=400).contains(w))
                .ok_or_else(|| {
                    Error::Usage(format!(
                        "series_tile is a width in terminal cells, {} to 400, not `{v}`",
                        settings::SERIES_TILE_MIN
                    ))
                })?;
        }
        (Some(n), Some(_)) => {
            return Err(Error::Usage(format!(
                "unknown setting `{n}`; there are language, theme, resume and series_tile"
            )));
        }
    }
    if value.is_some() {
        s.save_to(&path)
            .map_err(|e| Error::Internal(format!("cannot write {}: {e}", path.display())))?;
        i18n::set_lang(s.language.effective());
    }
    Ok(s.to_json(Some(&path)))
}

#[expect(
    clippy::too_many_lines,
    reason = "one match arm per subcommand; long, but flat"
)]
fn run(cli: Cli) -> Result<Value> {
    // The inventory's own settings live in its database; the display settings do not.
    let inventory_setting = |n: &Option<String>| {
        n.as_deref().is_some_and(|n| {
            n == "inventory" || ev_core::INVENTORY_SETTINGS.iter().any(|(k, _)| *k == n)
        })
    };
    if let Cmd::Settings { name, value } = &cli.cmd
        && !inventory_setting(name)
    {
        return settings_cmd(name.clone(), value.clone());
    }
    let db = db_path(cli.db)?;
    let mut inv = Inventory::open(&db)?;
    match cli.cmd {
        Cmd::Settings { name, value } => {
            let name = name.filter(|n| n != "inventory");
            inv.inventory_settings(name.as_deref(), value.as_deref())
        }
        Cmd::Grid {
            references,
            cols,
            rows,
            face,
            clear,
        } => {
            let refs = references.as_slice();
            let changes = cols.is_some() || face.is_some();
            if (clear || !changes) && refs.len() > 1 {
                return Err(Error::Usage(
                    "several holders take --cols and --rows or --face; show or clear one at a time"
                        .into(),
                ));
            }
            if clear {
                return inv.grid_clear(&refs[0]);
            }
            if let (Some(c), Some(r)) = (cols, rows) {
                inv.grid_set_many(refs, c, r)?;
            }
            if let Some(f) = &face {
                inv.grid_face(refs, f)?;
            }
            match refs {
                [one] => inv.grid(one),
                many => Ok(serde_json::json!({
                    "grids": many.iter().map(|r| inv.grid(r)).collect::<Result<Vec<_>>>()?
                })),
            }
        }
        Cmd::Sketch {
            reference,
            at,
            size,
            on,
            points,
            right_of,
            left_of,
            above,
            below,
            offset,
            clear,
            stdin,
        } => {
            if stdin {
                let text = read_input()?;
                return inv.sketch_many(&text);
            }
            let reference = reference.unwrap_or_default();
            let change = ev_core::SketchChange {
                reference: reference.clone(),
                at: at.map(|s| ev_core::parse_pair(&s, "--at")).transpose()?,
                size: size
                    .map(|s| ev_core::parse_pair(&s, "--size"))
                    .transpose()?,
                points: points.map(|s| ev_core::parse_points(&s)).transpose()?,
                on,
                right_of,
                left_of,
                above,
                below,
                offset,
                clear,
            };
            let asked = change.at.is_some()
                || change.size.is_some()
                || change.points.is_some()
                || change.on.is_some()
                || change.right_of.is_some()
                || change.left_of.is_some()
                || change.above.is_some()
                || change.below.is_some()
                || change.offset.is_some()
                || clear;
            if asked {
                inv.sketch_set(&change)
            } else {
                Ok(
                    serde_json::json!({ "node": inv.show(&reference, false)?["node"], "sketch": inv.sketch(&reference)? }),
                )
            }
        }
        Cmd::Map { reference } => inv.map(reference.as_deref()),
        Cmd::Cell { pairs } => {
            let pairs = pairs
                .iter()
                .map(|p| {
                    p.split_once('=')
                        .map(|(r, c)| (r.trim().to_string(), c.to_string()))
                        .ok_or_else(|| Error::Usage(format!("`{p}` is not <ref>=<cells>")))
                })
                .collect::<Result<Vec<_>>>()?;
            inv.cells_set(&pairs)
        }
        Cmd::Add(a) => add(&mut inv, *a),
        Cmd::Show {
            reference,
            include_gone,
        } => inv.show(&reference, include_gone),
        Cmd::Tree { reference, depth } => inv.tree(reference.as_deref(), depth),
        Cmd::Find {
            text,
            tag,
            kind,
            include_gone,
            empty,
        } => {
            let kind = kind.map(|k| k.parse::<Kind>()).transpose()?;
            inv.find_with(&text, tag.as_deref(), kind, include_gone, empty)
        }
        Cmd::Edit { stdin: true, .. } => {
            let text = read_input()?;
            inv.edit_batch(&edit_lines(&text)?)
        }
        Cmd::Edit {
            reference,
            assignments,
            ..
        } => {
            if assignments.is_empty() {
                return Err(Error::Usage("give at least one field=value".into()));
            }
            warn_missing_photos(
                assignments
                    .iter()
                    .filter_map(|a| a.strip_prefix("photos=+")),
            );
            inv.edit(reference.as_deref().unwrap_or_default(), &assignments)
        }
        Cmd::Split {
            reference,
            parts,
            rename,
            qty,
            take,
        } => {
            let parts = parts
                .iter()
                .map(|p| match p.rsplit_once('=') {
                    Some((name, q)) => q
                        .trim()
                        .parse::<i64>()
                        .map(|q| (name.trim().to_string(), Some(q)))
                        .map_err(|_| {
                            Error::Usage(format!("`{p}`: the count after = is not a number"))
                        }),
                    None => Ok((p.trim().to_string(), None)),
                })
                .collect::<Result<Vec<_>>>()?;
            inv.split_with(&reference, &parts, rename.as_deref(), qty, take)
        }
        Cmd::Recode { pairs } => {
            let pairs = pairs
                .iter()
                .map(|p| {
                    p.split_once('=')
                        .map(|(r, c)| (r.trim().to_string(), c.to_string()))
                        .ok_or_else(|| Error::Usage(format!("`{p}` is not <ref>=<code>")))
                })
                .collect::<Result<Vec<_>>>()?;
            inv.recode(&pairs)
        }
        Cmd::Move {
            references,
            to,
            plan,
            qty,
        } => match references.as_slice() {
            [one] => inv.move_qty(one, &to, plan, qty),
            _ if qty.is_some() => Err(Error::Usage(
                "--qty moves part of one record; name one".into(),
            )),
            many => inv.move_many(many, &to, plan),
        },
        Cmd::Join { references } => inv.join(&references),
        Cmd::Unjoin { reference } => inv.unjoin(&reference),
        Cmd::Pending => inv.pending(),
        Cmd::Done { reference } => inv.done(&reference),
        Cmd::Cancel { reference } => inv.cancel(&reference),
        Cmd::Dispose {
            reference,
            disposition: d,
            shred,
            qty,
            why,
        } => inv.dispose_qty(&reference, disposition(&d)?, shred, qty, why.as_deref()),
        Cmd::Restore {
            reference,
            correction: Some(why),
        } => inv.correct_gone(&reference, &why),
        Cmd::Restore {
            reference,
            correction: None,
        } => inv.restore(&reference),
        Cmd::Gone {
            reference,
            disposition: d,
            why,
            shred,
            qty,
            at,
            place,
            traded_for,
        } => {
            traded_check(&inv, Some(&reference), d.as_deref(), traded_for.as_deref())?;
            let v = inv.gone_left(
                &reference,
                d.as_deref().map(disposition).transpose()?,
                why.as_deref(),
                shred,
                qty,
                at.as_deref(),
                place.as_deref(),
            )?;
            // A swap: what came in exchange (spec/past-belongings.md).
            match traded_for {
                Some(t) => inv.traded(&format!("#{}", v["node"]["id"]), Some(&t)),
                None => Ok(v),
            }
        }
        Cmd::Traded { reference, for_ } => inv.traded(&reference, for_.as_deref()),
        Cmd::Sold {
            reference,
            price,
            currency,
            at,
            via,
            note,
        } => inv.sold(
            &reference,
            &price,
            currency.as_deref(),
            at.as_deref(),
            via.as_deref(),
            note.as_deref(),
        ),
        Cmd::Past {
            year: Some(y), ..
        } => inv.past_year(y),
        Cmd::Past {
            name,
            place,
            year: None,
        } => inv.past(name.as_deref(), place.as_deref()),
        Cmd::Disposals { disposition: d } => {
            // Only what a thing can be set aside as has a pile.
            const PILES: [&str; 6] = ["trash", "digitize", "give", "sell", "trade", "return"];
            if let Some(x) = d.as_deref()
                && !PILES.contains(&x.trim())
            {
                return Err(Error::Usage(format!(
                    "no pile `{x}`; use {}",
                    PILES.join(", ")
                )));
            }
            inv.disposals(d.as_deref().map(disposition).transpose()?)
        }
        Cmd::Lost {
            reference: Some(r),
            qty,
        } => inv.mark_lost_qty(&r, qty),
        Cmd::Lost {
            reference: None, ..
        } => inv.lost_list(),
        Cmd::Found {
            reference,
            place: None,
        } => inv.found(&reference),
        Cmd::Found {
            reference,
            place: Some(p),
        } => inv.found_in(&reference, &p),
        Cmd::History {
            reference,
            contents: false,
        } => inv.history(&reference),
        Cmd::History {
            reference,
            contents: true,
        } => inv.history_with_contents(&reference),
        Cmd::Ui => ui::run(inv, &db).map(|()| Value::Null),
        // Handled in `main`, before an inventory is opened.
        Cmd::Mcp => Err(Error::Usage("`ev mcp` serves over stdio; run it on its own".into())),
        Cmd::Lend { reference, to, qty } => inv.lend_qty(&reference, &to, qty),
        Cmd::Back { reference } => inv.back(&reference),
        Cmd::For { place } => inv.errands(place.as_deref()),
        Cmd::Place(PlaceCmd::Add { name, aliases }) => inv.place_add(&name, &aliases),
        Cmd::Place(PlaceCmd::Alias { place, alias }) => inv.place_alias(&place, &alias),
        Cmd::Place(PlaceCmd::List) => inv.place_list(),
        Cmd::Place(PlaceCmd::Merge { from, into }) => inv.place_merge(&from, &into),
        Cmd::Suggest { text, tag, for_ref } => {
            inv.suggest_with(&text.join(" "), tag.as_deref(), for_ref.as_deref())
        }
        Cmd::Layout { reference, propose } => inv.layout(&reference, propose),
        Cmd::Regroup {
            reference,
            decline,
            why,
            allow,
        } => match (decline, allow) {
            (Some(d), _) => inv.regroup_decline(&d, why.as_deref()),
            (_, Some(a)) => inv.regroup_allow(&a),
            _ => inv.regroup(reference.as_deref()),
        },
        Cmd::Themes { reference } => inv.themes(reference.as_deref()),
        Cmd::Synonym(SynonymCmd::Add { words }) => inv.synonym_add(&words.join(", ")),
        Cmd::Synonym(SynonymCmd::List) => inv.synonym_list(),
        Cmd::Synonym(SynonymCmd::Remove { id }) => inv.synonym_remove(id),
        Cmd::Facet(FacetCmd::Add { name, words }) => inv.facet_add(&name, words.as_deref()),
        Cmd::Facet(FacetCmd::List) => inv.facet_list(),
        Cmd::Facet(FacetCmd::Remove { name }) => inv.facet_remove(&name),
        Cmd::Kit(KitCmd::Add {
            name,
            copies,
            note,
            parts,
            purchase,
        }) => inv.kit_add(
            &name,
            copies,
            note.as_deref(),
            &kit_parts(&parts)?,
            purchase,
        ),
        Cmd::Kit(KitCmd::Purchase { kit, line, .. }) => inv.kit_purchase(&kit, line),
        Cmd::Kit(KitCmd::Part { kit, parts }) => inv.kit_parts_add(&kit, &kit_parts(&parts)?),
        Cmd::Kit(KitCmd::Drop { kit, n }) => inv.kit_part_drop(&kit, n),
        Cmd::Kit(KitCmd::Rename { kit, n, part }) => {
            let (text, qty) = kit_parts(&[part])?.remove(0);
            inv.kit_part_set(&kit, n, &text, qty)
        }
        Cmd::Kit(KitCmd::Link { kit, n, references }) => inv.kit_link(&kit, n, &references),
        Cmd::Kit(KitCmd::Unlink { kit, n, reference }) => inv.kit_unlink(&kit, n, &reference),
        Cmd::Kit(KitCmd::List) => inv.kit_list(),
        Cmd::Kit(KitCmd::Show { kit }) => inv.kit_show(&kit),
        Cmd::Kit(KitCmd::Remove { kit }) => inv.kit_remove(&kit),
        Cmd::Goal { goal } => inv.goal(goal.as_deref()),
        Cmd::Observe {
            reference,
            text,
            photo,
        } => inv.observe(&reference, &text, photo),
        Cmd::Unobserve { id } => inv.unobserve(id),
        Cmd::Review {
            reference,
            status,
            note,
        } => inv.review(&reference, &status, note.as_deref()),
        Cmd::Progress { place } => inv.progress_in(place.as_deref()),
        Cmd::Next => inv.next(),
        Cmd::Todo => inv.todo(),
        Cmd::Stats => inv.stats(),
        Cmd::Focus { list: true, .. } => inv.focus_list(),
        Cmd::Focus { file, note, .. } if !file.is_empty() => {
            // `a.jpg=<note>` gives that picture a note of its own; a name that is a file stays
            // one. `f12` is the series' picture, sent again.
            let mut each: Vec<(PathBuf, Option<String>)> = Vec::new();
            for f in &file {
                let (p, n) = match f.split_once('=') {
                    Some((p, n)) if !Path::new(f).is_file() => (p, Some(n.to_string())),
                    _ => (f.as_str(), None),
                };
                each.push((photo_arg(&inv, Path::new(p))?, n));
            }
            inv.focus_noted(&each, note.as_deref())
        }
        Cmd::Focus {
            reference,
            photo,
            clear,
            ..
        } => {
            if reference.is_none() && !clear {
                return Err(Error::Usage("name a node, --file, --list or --clear".into()));
            }
            // `f12` names a picture of the marked photo series, not a record (`#12` is one).
            if let Some(n) = reference.as_deref().and_then(ev_core::series_number) {
                return inv.focus_picture(n);
            }
            inv.focus(reference.as_deref(), photo)
        }
        Cmd::Label { references, needed } => inv.label(&references, !needed),
        Cmd::Empty { references, note } => inv.mark_empty(&references, note.as_deref()),
        Cmd::Broken { reference, note } => inv.broken(&reference, note.as_deref(), false),
        Cmd::Fixed { reference } => inv.broken(&reference, None, true),
        Cmd::Expires {
            reference,
            date,
            clear,
        } => {
            if date.is_none() && !clear {
                return Err(Error::Usage("give a date or --clear".into()));
            }
            inv.expires(&reference, date.as_deref())
        }
        Cmd::Sale {
            reference,
            listed,
            reserved,
            clear,
            price,
            place,
            condition,
        } => {
            if !(listed || reserved || clear) {
                return Err(Error::Usage("say --listed, --reserved or --clear".into()));
            }
            let status = if listed {
                Some("listed")
            } else if reserved {
                Some("reserved")
            } else {
                None
            };
            inv.sale(
                &reference,
                status,
                price,
                place.as_deref(),
                condition.as_deref(),
            )
        }
        Cmd::Need(NeedCmd::Add {
            text,
            qty,
            make,
            for_ref,
            note,
        }) => inv.need_add(&text, qty, make, for_ref.as_deref(), note.as_deref()),
        Cmd::Need(NeedCmd::List { all }) => inv.need_list(all),
        Cmd::Doc(DocCmd::Add {
            file,
            kind,
            for_refs,
            number,
            ettn,
            issued,
            issuer,
            note,
            coverage,
        }) => {
            let v = inv.doc_add(
                &file,
                &NewDoc {
                    kind,
                    number,
                    ettn,
                    issued,
                    issuer,
                    note,
                },
                &for_refs,
            )?;
            match (coverage, v["document"]["id"].as_i64()) {
                (Some(c), Some(d)) => inv.doc_attach_coverage(d, c),
                _ => Ok(v),
            }
        }
        Cmd::Doc(DocCmd::List { reference }) => inv.doc_list(reference.as_deref()),
        Cmd::Buy(BuyCmd::Import { file, stdin }) => {
            let text = match (file, stdin) {
                (Some(f), false) => std::fs::read_to_string(&f)
                    .map_err(|e| Error::Usage(format!("{}: {e}", f.display())))?,
                (None, true) => read_input()?,
                _ => return Err(Error::Usage("give a file or --stdin".into())),
            };
            inv.buy_import(&text)
        }
        Cmd::Buy(BuyCmd::Add {
            name,
            shop,
            date,
            paid,
            currency,
            qty,
            pack,
            order,
            order_url,
            url,
            brand,
            bucket,
            for_ref,
        }) => inv.buy_add(
            &serde_json::json!({
                "name": name, "shop": shop, "ordered_at": date, "paid": paid,
                "currency": currency, "qty": qty, "pack": pack, "order": order, "order_url": order_url,
                "product_url": url, "brand": brand, "bucket": bucket,
            }),
            for_ref.as_deref(),
        ),
        Cmd::Buy(BuyCmd::List {
            open,
            bucket,
            shop,
            since,
            query,
        }) => inv.buy_list_matching(
            open,
            bucket.as_deref(),
            shop.as_deref(),
            since.as_deref(),
            query.as_deref(),
        ),
        Cmd::Cover(CoverCmd::Add(a)) => {
            let a = *a;
            // The line is checked before the coverage is written, so a wrong id leaves nothing.
            if let Some(l) = a.purchase {
                inv.cover_line_check(l, None)?;
            }
            let v = inv.cover_add(
                &a.references,
                &ev_core::NewCoverage {
                    kind: a.kind,
                    issuer: a.issuer,
                    number: a.number,
                    from: a.from,
                    term: a.term,
                    usage: a.usage,
                    ends: a.ends,
                    premium: a.premium,
                    deductible: a.deductible,
                    currency: a.currency,
                    scope: a.scope,
                    note: a.note,
                },
            )?;
            match (a.purchase, v["coverage"]["id"].as_i64()) {
                (Some(l), Some(c)) => inv.cover_purchase(c, Some(l)),
                _ => Ok(v),
            }
        }
        Cmd::Cover(CoverCmd::List { ending }) => inv.cover_list(ending),
        Cmd::Money(MoneyCmd::Needs) => inv.money_needs(),
        Cmd::Money(MoneyCmd::Status) => inv.money_status(),
        Cmd::Money(MoneyCmd::Import { file, stdin }) => {
            let text = match (file, stdin) {
                (Some(f), false) => std::fs::read_to_string(&f)
                    .map_err(|e| Error::Usage(format!("{}: {e}", f.display())))?,
                (None, true) => read_input()?,
                _ => return Err(Error::Usage("give a file or --stdin".into())),
            };
            inv.money_import(&text)
        }
        Cmd::Cover(CoverCmd::Show { id }) => inv.cover_show(id),
        Cmd::Cover(CoverCmd::Remove { id }) => inv.cover_remove(id),
        Cmd::Cover(CoverCmd::Purchase { coverage, line, .. }) => inv.cover_purchase(coverage, line),
        Cmd::Track {
            reference,
            subject,
            decision,
            why,
        } => inv.track(&reference, &subject, &decision, why.as_deref()),
        Cmd::Value {
            remove: Some(id), ..
        } => inv.value_remove(id),
        Cmd::Value {
            reference,
            amount,
            currency,
            at,
            source,
            note,
            approximate,
            remove: None,
        } => {
            let new = amount.map(|amount| ev_core::NewValuation {
                amount,
                currency,
                at,
                approximate,
                source,
                note,
            });
            inv.value(&reference, new.as_ref())
        }
        Cmd::Link(LinkCmd::Add {
            reference,
            url,
            kind,
            archive,
            note,
        }) => inv.link_add(&reference, &url, &kind, archive.as_deref(), note.as_deref()),
        Cmd::Link(LinkCmd::List { reference }) => inv.link_list(&reference),
        Cmd::Link(LinkCmd::Remove { id }) => inv.link_remove(id),
        Cmd::Buy(BuyCmd::Show { id }) => inv.buy_show(id),
        Cmd::Buy(BuyCmd::For { reference, .. }) => match reference {
            Some(r) => inv.buy_for(&r),
            None => inv.buy_backfill(),
        },
        Cmd::Buy(BuyCmd::Link { id, reference, qty }) => inv.buy_link(id, &reference, qty),
        Cmd::Buy(BuyCmd::Pack { id, pack }) => inv.buy_pack(id, pack),
        Cmd::Buy(BuyCmd::Unlink { id, reference }) => inv.buy_unlink(id, &reference),
        Cmd::Buy(BuyCmd::Decline {
            id,
            reference,
            why,
            clear,
        }) => inv.buy_decline(id, &reference, why.as_deref(), clear),
        Cmd::Buy(BuyCmd::Bring {
            id,
            reference,
            all,
            only,
            r#type,
        }) => match (id, reference) {
            (Some(id), Some(reference)) if !all => inv.buy_bring(id, &reference, &only, &r#type),
            _ => inv.buy_bring_all(&r#type),
        },
        Cmd::Buy(BuyCmd::Dismiss {
            id,
            reason,
            why,
            clear,
        }) => inv.buy_dismiss(
            id,
            if clear { None } else { reason.as_deref() },
            why.as_deref(),
        ),
        Cmd::Doc(DocCmd::Show { id }) => inv.doc_show(id),
        Cmd::Doc(DocCmd::Link { id, reference }) => inv.doc_link(id, &reference),
        Cmd::Doc(DocCmd::Unlink { id, reference }) => inv.doc_unlink(id, &reference),
        Cmd::Need(NeedCmd::Got { id, note }) => inv.need_close(id, true, note.as_deref()),
        Cmd::Need(NeedCmd::Drop { id, note }) => inv.need_close(id, false, note.as_deref()),
        Cmd::Task(TaskCmd::Add {
            title,
            why,
            on,
            at,
            due,
        }) => inv.task_add_with(&title, &why, &on, at, due.as_deref()),
        Cmd::Task(TaskCmd::List { all }) => inv.task_list(all),
        Cmd::Task(TaskCmd::Show { id }) => inv.task_show(id),
        Cmd::Task(TaskCmd::Start { id }) => inv.task_set(id, "doing", None),
        Cmd::Task(TaskCmd::Done { id, note }) => inv.task_set(id, "done", note.as_deref()),
        Cmd::Task(TaskCmd::Drop { id, note }) => inv.task_set(id, "dropped", note.as_deref()),
        Cmd::Task(TaskCmd::Reopen { id }) => inv.task_set(id, "open", None),
        Cmd::Task(TaskCmd::Edit {
            id,
            title,
            why,
            on,
            off,
            at,
            due,
        }) => {
            // The date first: a malformed one stops the edit before anything changes.
            if let Some(d) = &due {
                inv.task_due(id, Some(d))?;
            }
            inv.task_edit(id, title.as_deref(), why.as_deref(), &on, &off, at)
        }
        Cmd::Audit => inv.audit(),
        Cmd::Rule(RuleCmd::Add { text }) => inv.rule_add(&text),
        Cmd::Rule(RuleCmd::List) => inv.rule_list(),
        Cmd::Rule(RuleCmd::Remove { id }) => inv.rule_remove(id),
        Cmd::Photo(PhotoCmd::Add {
            reference,
            files,
            crop,
            note,
            whole,
            rotate,
            pad,
            no_show,
            stdin,
        }) => {
            let pad = pad_of(pad)?;
            // (record, photo as given, its note)
            let jobs: Vec<(String, PathBuf, Option<String>)> = if stdin {
                read_input()?
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| {
                        let v: Value = serde_json::from_str(l)
                            .map_err(|e| Error::Usage(format!("`{l}` is no JSON line: {e}")))?;
                        let field = |k: &str| {
                            v[k].as_str()
                                .map(str::to_string)
                                .ok_or_else(|| Error::Usage(format!("each line needs `{k}`: {l}")))
                        };
                        Ok((
                            field("ref")?,
                            PathBuf::from(field("photo")?),
                            v["note"].as_str().map(str::to_string),
                        ))
                    })
                    .collect::<Result<_>>()?
            } else {
                let r = reference.unwrap_or_default();
                files
                    .iter()
                    .map(|f| (r.clone(), f.clone(), note.clone()))
                    .collect()
            };
            if jobs.len() > 1 && (crop.is_some() || rotate.is_some()) {
                return Err(Error::Usage("--crop and --rotate take one photo".into()));
            }
            // Every record and photo checked before anything is attached, the whole-photo rule
            // too, also between the photos of this call: a refusal leaves nothing half done and
            // names the line or photo it is about. A series picture (`f12`) keeps the note it
            // was sent with.
            let cut_or_whole = crop.is_some() || whole;
            let mut ready = Vec::new();
            let mut wholes: std::collections::HashMap<PathBuf, (i64, String)> =
                std::collections::HashMap::new();
            for (i, (r, given, own)) in jobs.iter().enumerate() {
                let what = if stdin {
                    format!("line {}", i + 1)
                } else {
                    given.display().to_string()
                };
                let mut check = || -> Result<_> {
                    let series = given
                        .to_str()
                        .and_then(ev_core::series_number)
                        .filter(|_| !given.exists());
                    let file = photo_arg(&inv, given)?;
                    if !file.is_file() {
                        return Err(Error::NotFound(format!("no file {}", given.display())));
                    }
                    if rotate.is_some() {
                        // One photo, turned when attached: its own add checks it.
                        inv.resolve(r, false)?;
                    } else {
                        let (id, stored) = inv.photo_add_check(r, &file, cut_or_whole)?;
                        if !cut_or_whole {
                            if let Some((_, first)) = wholes.get(&stored).filter(|(o, _)| *o != id) {
                                return Err(Error::Refused {
                                    message: format!(
                                        "the same photo as {first}, whole on another record; \
                                         cut it with `ev photo cut`, or pass --whole"
                                    ),
                                    details: Value::Null,
                                });
                            }
                            wholes.insert(stored, (id, what.clone()));
                        }
                    }
                    let note = match (own, series) {
                        (Some(n), _) => Some(n.clone()),
                        (None, Some(n)) => inv.series_note(n)?,
                        (None, None) => None,
                    };
                    Ok((r.clone(), file, note, series.is_some(), given.clone()))
                };
                ready.push(check().map_err(|e| e.prefixed(&what))?);
            }
            let crop = crop
                .map(|c| c.parse::<ev_core::Crop>().map(|c| c.padded(pad)))
                .transpose()?;
            let single = ready.len() == 1;
            let (mut added, mut show) = (Vec::new(), Vec::new());
            let mut last = Value::Null;
            for (r, file, note, from_series, given) in ready {
                let file = match rotate {
                    Some(d) => inv.turned_copy(&file, d)?,
                    None => file,
                };
                let v = inv.photo_add_with(&r, &file, crop, note.as_deref(), whole)?;
                let photo = v["photos"].as_array().and_then(|p| p.last()).cloned();
                // The photo joins the marked photo series in `ev ui`, unframed: every photo the
                // person sends is shown, framed or not (spec/focus-stack.md). One taken from
                // the series is there already.
                if let (false, false, Some(path)) =
                    (no_show, from_series, photo.as_ref().and_then(|p| p["path"].as_str()))
                {
                    let title = note
                        .clone()
                        .unwrap_or_else(|| crate::render::label(&v["node"]));
                    show.push((PathBuf::from(path), Some(title)));
                }
                added.push(json!({
                    "node": v["node"],
                    "photo": photo.as_ref().map(|p| p["n"].clone()),
                    "from": given.to_string_lossy(),
                }));
                last = v;
            }
            let shown = if show.is_empty() {
                Value::Null
            } else {
                // One picture is titled as the request too, as `shown.note` says it.
                let one = (show.len() == 1).then(|| show[0].1.clone()).flatten();
                inv.focus_noted(&show, one.as_deref())?["focus"].clone()
            };
            if single {
                // Only the photo added, with its number; every photo is `ev photo list`.
                if let Some(p) = last["photos"].as_array().and_then(|p| p.last()).cloned() {
                    last["photos"] = json!([p]);
                }
                if !shown.is_null() {
                    last["shown"] = shown;
                }
                return Ok(last);
            }
            let mut v = json!({ "added": added });
            if !shown.is_null() {
                v["shown"] = shown;
            }
            Ok(v)
        }
        Cmd::Photo(PhotoCmd::Mark {
            target,
            marks,
            grid,
            out,
            show,
            no_show,
            codes,
            keep_numbers,
        }) => {
            // `f12`: the series' picture, marked again on the photo it was drawn on.
            let target = match ev_core::series_number(&target) {
                Some(n) if !Path::new(&target).exists() => {
                    inv.series_photo(n)?.to_string_lossy().into_owned()
                }
                _ => target,
            };
            let grid = grid
                .as_deref()
                .map(str::parse::<ev_core::GridCorners>)
                .transpose()?;
            let mut marks = marks
                .iter()
                .map(|m| {
                    let (label, at) = m.split_once('=').ok_or_else(|| {
                        Error::Usage(format!("`{m}` is not <label>=x,y,w,h or <label>=<cell>"))
                    })?;
                    Ok((label.trim().to_string(), at.trim().to_string()))
                })
                .collect::<Result<Vec<_>>>()?;
            // Each placed box's code on its own cells: which label goes on which box.
            if codes {
                let g = inv.grid(&target)?;
                let before = marks.len();
                for b in g["grid"]["boxes"].as_array().into_iter().flatten() {
                    if let (Some(code), Some(cells)) = (b["code"].as_str(), b["cells"].as_str()) {
                        marks.push((code.to_string(), cells.to_string()));
                    }
                }
                if marks.len() == before {
                    return Err(Error::Usage(format!(
                        "no box with a code is placed in {target}'s grid"
                    )));
                }
            }
            // Shown in the person's `ev ui` unless asked not to, as part of the series there:
            // its numbers go on from the series' (spec/focus-stack.md).
            if no_show {
                return inv.photo_mark(&target, &marks, grid.as_ref(), out.as_deref());
            }
            let mut v = inv.photo_mark_numbered(
                &target,
                &marks,
                grid.as_ref(),
                out.as_deref(),
                keep_numbers,
            )?;
            if let Some(path) = v["marked"].as_str().map(PathBuf::from) {
                // Titled with --show's note, or else with the labels as drawn.
                let note = show.unwrap_or_else(|| {
                    v["marks"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|m| m["label"].as_str())
                        .collect::<Vec<_>>()
                        .join(" · ")
                });
                let frames = v["frames"].as_array().cloned().unwrap_or_default();
                let source = PathBuf::from(v["source"].as_str().unwrap_or_default());
                v["shown"] =
                    inv.focus_drawn(&[path], Some(&note), &frames, &source)?["focus"].clone();
            }
            if let Some(o) = v.as_object_mut() {
                o.remove("frames");
            }
            Ok(v)
        }
        Cmd::Photo(PhotoCmd::Cut {
            file,
            pieces,
            place,
            note,
            grid,
            preview,
            show: _,
            no_show,
            rotate,
            pad,
        }) => {
            let pad = pad_of(pad)?;
            let file = photo_arg(&inv, &file)?;
            let file = match rotate {
                Some(d) => inv.turned_copy(&file, d)?,
                None => file,
            };
            let grid = grid
                .as_deref()
                .map(str::parse::<ev_core::GridCorners>)
                .transpose()?;
            let crops = pieces
                .iter()
                .map(|p| {
                    let (r, c) = p
                        .rsplit_once('=')
                        .ok_or_else(|| Error::Usage(format!("`{p}` is not <ref>=x,y,w,h")))?;
                    Ok((r.trim().to_string(), c.parse::<ev_core::Crop>()?.padded(pad)))
                })
                .collect::<Result<Vec<_>>>()?;
            // Shown frames take their numbers in the series in `ev ui` — the ones this photo was
            // marked with there, then the next free ones — so they match what the person was
            // told (spec/focus-stack.md).
            let series = !no_show;
            let (mut v, title) = match &preview {
                Some(title) => (
                    inv.photo_cut_preview_in(
                        &file,
                        place.as_deref(),
                        &crops,
                        grid.as_ref(),
                        None,
                        series,
                    )?,
                    title.clone(),
                ),
                None => (
                    inv.photo_cut_in(
                        &file,
                        place.as_deref(),
                        &crops,
                        note.as_deref(),
                        grid.as_ref(),
                        series,
                    )?,
                    None,
                ),
            };
            // What the cut drew goes to the person's `ev ui` unless asked not to: the numbered
            // photo, and the preview when it framed a grid's boxes on their cells (otherwise it
            // frames the same crops under record ids, not the numbers the person is told).
            let path = |k: &str| v[k].as_str().map(PathBuf::from);
            let mut files: Vec<PathBuf> = Vec::new();
            if !no_show {
                if preview.is_some() && grid.is_some() {
                    files.extend(path("preview"));
                }
                files.extend(path("marked"));
            }
            if !files.is_empty() {
                let note = title.or(note).unwrap_or_else(|| legend_note(&v["legend"]));
                let frames = v["legend"].as_array().cloned().unwrap_or_default();
                v["shown"] =
                    inv.focus_drawn(&files, Some(&note), &frames, &file)?["focus"].clone();
            }
            Ok(v)
        }
        Cmd::Photo(PhotoCmd::List { reference }) => inv.photo_list(&reference),
        Cmd::Photo(PhotoCmd::Remove { reference, n }) => inv.photo_remove(&reference, n),
        Cmd::Photo(PhotoCmd::Rotate {
            reference,
            n,
            degrees,
        }) => inv.photo_rotate(&reference, n, degrees),
        Cmd::Photo(PhotoCmd::Adopt) => inv.photo_adopt(),
        Cmd::Photo(PhotoCmd::Current { reference }) => inv.photo_current(&reference),
        Cmd::Photo(PhotoCmd::Stale { reference, why }) => {
            inv.photo_stale_mark(&reference, why.as_deref())
        }
    }
}

/// `--pad`: a fraction 0–1 of a crop's own size added on every side; none by default.
fn pad_of(pad: Option<f64>) -> Result<f64> {
    match pad {
        None => Ok(0.0),
        Some(p) if (0.0..=1.0).contains(&p) => Ok(p),
        Some(p) => Err(Error::Usage(format!(
            "--pad is a fraction of the crop's size, 0 to 1, not {p}"
        ))),
    }
}

/// A photo given to a command: a file, or `f12`, the marked photo series' twelfth picture as
/// the photo it was drawn on. A file of that name wins.
fn photo_arg(inv: &Inventory, given: &Path) -> Result<PathBuf> {
    match given.to_str().and_then(ev_core::series_number) {
        Some(n) if !given.exists() => inv.series_photo(n),
        _ => Ok(given.to_path_buf()),
    }
}
/// The title of a numbered photo shown without a note: what each number is, by code or else by
/// name, so the person reads the numbers on the photo without the agent's table
/// (`1 Düğme pil · 2 D-A1`).
fn legend_note(legend: &Value) -> String {
    legend
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| {
            let r = &e["ref"];
            let what = r["code"].as_str().or(r["name"].as_str()).unwrap_or("?");
            format!("{} {what}", e["n"])
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

fn add(inv: &mut Inventory, a: AddArgs) -> Result<Value> {
    if a.batch.is_some() || a.stdin {
        if a.name.is_some() {
            return Err(Error::Usage(
                "give either a name or --batch/--stdin, not both".into(),
            ));
        }
        let text = match &a.batch {
            Some(path) => std::fs::read_to_string(path)
                .map_err(|e| Error::Usage(format!("cannot read {}: {e}", path.display())))?,
            None => read_input()?,
        };
        let mut lines = Vec::new();
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let node: NewNode = serde_json::from_str(line)
                .map_err(|e| Error::Usage(format!("line {}: {e}", i + 1)))?;
            lines.push(node);
        }
        if lines.is_empty() {
            return Err(Error::Usage("the batch is empty".into()));
        }
        warn_missing_photos(
            lines
                .iter()
                .flat_map(|l| l.photos.iter().map(String::as_str)),
        );
        return inv.add_batch(lines);
    }
    // With --of the thing names itself.
    let (name, kind) = match a.of {
        Some(_) => (String::new(), String::new()),
        None => (
            a.name
                .ok_or_else(|| Error::Usage("a name is required".into()))?,
            // A past thing is a thing, unless said otherwise.
            a.kind
                .or_else(|| a.gone.as_ref().map(|_| "item".to_string()))
                .ok_or_else(|| Error::Usage("--kind is required".into()))?,
        ),
    };
    warn_missing_photos(a.photos.iter().map(String::as_str));
    traded_check(inv, None, a.gone.as_deref(), a.traded_for.as_deref())?;
    let added = inv.add(NewNode {
        key: None,
        name,
        kind,
        parent: a.parent,
        lost: a.lost,
        code: a.code,
        address: a.address,
        qty: a.qty,
        note: a.note,
        theme: a.theme,
        fill: a.fill,
        size: a.size,
        tags: a.tags,
        photos: a.photos,
        to: a.to,
        owner: a.owner,
        temporary: a.temporary,
        make: a.make,
        model: a.model,
        serial: a.serial,
        of: a.of,
        gone: a.gone,
        at: a.at,
        came: a.came,
        place: a.place,
    })?;
    // A swap: what came in exchange (spec/past-belongings.md).
    match a.traded_for {
        Some(t) => inv.traded(&format!("#{}", added["node"]["id"]), Some(&t)),
        None => Ok(added),
    }
}

/// `--traded-for` goes with a trade only, and names a record that exists, checked before
/// anything is written so a refused swap leaves nothing half recorded.
/// `--traded-for` goes with a trade only (said, or the thing set aside to trade), names a thing
/// that exists and is not the one leaving, all checked before anything is written so a refused
/// swap leaves nothing half recorded.
fn traded_check(
    inv: &Inventory,
    leaving: Option<&str>,
    how: Option<&str>,
    traded_for: Option<&str>,
) -> Result<()> {
    let Some(t) = traded_for else { return Ok(()) };
    let leaving = leaving.map(|r| inv.resolve(r, false)).transpose()?;
    let set_aside = leaving
        .map(|id| inv.node(id))
        .transpose()?
        .and_then(|n| n.disposition);
    let trade = match how {
        Some(h) => h.trim() == "trade",
        None => set_aside == Some(Disposition::Trade),
    };
    if !trade {
        return Err(Error::Usage(
            "--traded-for goes with a trade: `--as trade` (or `--gone trade`)".into(),
        ));
    }
    let other = inv.resolve(t, true)?;
    if leaving == Some(other) {
        return Err(Error::Usage("a thing is not traded for itself".into()));
    }
    if matches!(
        inv.node(other)?.kind,
        ev_core::Kind::Home | ev_core::Kind::Room
    ) {
        return Err(Error::Usage(format!(
            "#{other} is a place; a thing is traded for a thing"
        )));
    }
    Ok(())
}

fn warn_missing_photos<'a>(paths: impl Iterator<Item = &'a str>) {
    for p in paths {
        if !std::path::Path::new(p.trim()).exists() {
            eprintln!("warning: photo path does not exist: {p}");
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    /// Every `ev …` the documentation shows, as written between backticks.
    fn documented() -> Vec<(&'static str, String)> {
        let docs = [
            (
                "skills/ev/SKILL.md",
                include_str!("../../skills/ev/SKILL.md"),
            ),
            ("README.md", include_str!("../../README.md")),
            ("REFERENCE.md", include_str!("../../REFERENCE.md")),
            (
                "release-notes/next.md",
                include_str!("../../release-notes/next.md"),
            ),
        ];
        let mut out = Vec::new();
        for (name, text) in docs {
            for (i, chunk) in text.split('`').enumerate() {
                let snippet = chunk.split_whitespace().collect::<Vec<_>>().join(" ");
                if i % 2 == 1 && snippet.starts_with("ev ") {
                    out.push((name, snippet));
                }
            }
        }
        out
    }

    /// The documented commands name real subcommands and only flags those subcommands take:
    /// a flag renamed in the code (`review --status` for `--as`) breaks this instead of the
    /// person or agent following the docs.
    #[test]
    fn every_documented_command_uses_real_subcommands_and_flags() {
        let root = super::Cli::command();
        let mut problems = Vec::new();
        let snippets = documented();
        // A check that finds nothing to check passes for the wrong reason.
        assert!(
            snippets.len() > 100,
            "only {} commands found",
            snippets.len()
        );
        let flagged = snippets
            .iter()
            .filter(|(_, s)| s.starts_with("ev review ") && s.contains("--as"))
            .count();
        assert!(flagged > 0, "the tour-gate example is among them");
        for (doc, snippet) in snippets {
            let words: Vec<&str> = snippet.split(' ').skip(1).collect();
            let mut cmd = &root;
            let mut rest = &words[..];
            while let Some((w, tail)) = rest.split_first() {
                match cmd.find_subcommand(w) {
                    Some(sub) => {
                        cmd = sub;
                        rest = tail;
                    }
                    None => break,
                }
            }
            if std::ptr::eq(cmd, &root) {
                // `ev ui`, `ev <x>` written as prose, or a subcommand that does not exist.
                let first = words.first().copied().unwrap_or("");
                let prose = first.is_empty()
                    || first.starts_with(['<', '[', '-', '#', '"', '…'])
                    || !first.chars().all(|c| c.is_ascii_lowercase());
                if !prose {
                    problems.push(format!("{doc}: `{snippet}`: no subcommand `{first}`"));
                }
                continue;
            }
            let flags: Vec<String> = cmd
                .get_arguments()
                .filter_map(|a| a.get_long().map(str::to_string))
                .chain(["help".to_string(), "json".to_string(), "db".to_string()])
                .collect();
            for w in rest {
                let Some(flag) = w.strip_prefix("--") else {
                    continue;
                };
                let flag = flag
                    .split(['=', '…', ']', ')', ',', '|', '\\'])
                    .next()
                    .unwrap_or("");
                if !flag.is_empty() && !flags.iter().any(|f| f == flag) {
                    problems.push(format!(
                        "{doc}: `{snippet}`: `{}` takes no --{flag}",
                        cmd.get_name()
                    ));
                }
            }
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    #[test]
    fn a_list_of_tags_in_ndjson_adds_each_unless_it_says_otherwise() {
        let lines =
            super::edit_lines(r#"{"ref":"Kutu","set":{"tags":["vida","-m3","+uzun"]}}"#).unwrap();
        assert_eq!(lines[0].1, ["tags=+vida", "tags=-m3", "tags=+uzun"]);
    }
}
