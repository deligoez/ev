use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use ev_core::{Disposition, Error, Inventory, Kind, NewNode, Result};
use serde_json::Value;

mod i18n;
mod input;
mod render;
mod settings;
mod theme;
mod ui;

/// Agent-first home inventory.
#[derive(Parser)]
#[command(name = "ev", version, about)]
struct Cli {
    /// Force JSON output even on a terminal.
    #[arg(long, global = true)]
    json: bool,

    /// Database file; wins over EV_DB. Defaults to ~/.ev/ev.db.
    #[arg(long, global = true, env = "EV_DB")]
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
    /// Folded search over name, code, note, theme and tags.
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
    },
    /// Change fields: name, code, kind, address, qty, note, theme, fill, tags=+x/-x, photos=+p/-p.
    Edit {
        reference: String,
        #[arg(required = true)]
        assignments: Vec<String>,
    },
    /// Give several nodes new codes at once: swap or rotate codes when boxes change places.
    Recode {
        /// <ref>=<new code>; an empty code clears it.
        #[arg(required = true)]
        pairs: Vec<String>,
    },
    /// Move now, or plan a move with --plan.
    Move {
        reference: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        plan: bool,
    },
    /// List pending moves.
    Pending,
    /// Apply a node's pending move.
    Done { reference: String },
    /// Drop a node's pending move.
    Cancel { reference: String },
    /// Mark a node as a candidate to leave: trash, give or sell.
    Dispose {
        reference: String,
        #[arg(long = "as")]
        disposition: String,
    },
    /// Return a candidate to active; with --correction, undo a gone recorded by mistake.
    Restore {
        reference: String,
        #[arg(long)]
        correction: Option<String>,
    },
    /// A node leaves the home; --as is required when it is not a candidate yet.
    /// --as mistake (with --why) closes a record that should never have existed.
    Gone {
        reference: String,
        #[arg(long = "as")]
        disposition: Option<String>,
        /// Why it left; recorded in the event and appended to the note.
        #[arg(long)]
        why: Option<String>,
    },
    /// Every candidate, grouped by disposition.
    Disposals {
        #[arg(long = "as")]
        disposition: Option<String>,
    },
    /// Mark a node lost, or list lost nodes when no reference is given.
    Lost { reference: Option<String> },
    /// Clear a node's lost flag where it was last seen.
    Found { reference: String },
    /// A node's events, oldest first; `--contents` adds what came in, went out or was added.
    History {
        reference: String,
        #[arg(long)]
        contents: bool,
    },
    /// Read-only terminal browser that follows the database as it changes.
    Ui,
    /// Lend a node of ours to a place; it stays in the tree where it returns to.
    Lend {
        reference: String,
        #[arg(long)]
        to: String,
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
    /// Regrouping hints under a place: things that fit better elsewhere, strays with a themed
    /// home, full boxes and bigger spares, sparse boxes to merge, mixed boxes, unknown fill.
    Regroup { reference: Option<String> },
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
    Unobserve { id: i64 },
    /// Mark how far a place has been gone through: toured, kept (leave as is) or raw.
    Review {
        reference: String,
        #[arg(long = "as")]
        status: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Every place to go through, and how far each one is.
    Progress,
    /// The ordered work list.
    #[command(subcommand)]
    Task(TaskCmd),
    /// Where to pick up: the current task with its places, progress and unplanned places.
    Next,
    /// Everything waiting, in one list: tasks, moves, errands, disposals, labels, needs, repairs,
    /// use-by dates, lost things, uninventoried and changed places, unclear records.
    Todo,
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
    },
    /// Things to buy or make.
    #[command(subcommand)]
    Need(NeedCmd),
    /// A holder laid out in cells (a gridfinity drawer): show its map, or set its size.
    Grid {
        reference: String,
        #[arg(long, requires = "rows")]
        cols: Option<i64>,
        #[arg(long, requires = "cols")]
        rows: Option<i64>,
        #[arg(long, conflicts_with_all = ["cols", "rows"])]
        clear: bool,
    },
    /// Place boxes in their holder's grid: `<ref>=A3` or `<ref>=A3-B4`, several at once;
    /// `<ref>=` takes one out. --recode names each box `<holder code>-<back-left cell>`.
    Cell {
        #[arg(required = true)]
        pairs: Vec<String>,
        #[arg(long)]
        recode: bool,
    },
    /// Show or change display settings: `language en|tr|auto`, `theme dark|light|auto`,
    /// `resume on|off` (`ev ui` reopens on the node it was on).
    Settings {
        name: Option<String>,
        value: Option<String>,
    },
    /// Make a running `ev ui` show a node and one of its photos (the last by default).
    Focus {
        reference: Option<String>,
        #[arg(long)]
        photo: Option<usize>,
        #[arg(long, conflicts_with = "reference")]
        clear: bool,
        /// Show pictures that are no record (marked photos) instead of a node; repeat for
        /// several, stepped through with `[` `]`.
        #[arg(long, conflicts_with_all = ["reference", "clear"])]
        file: Vec<PathBuf>,
        /// The title shown over --file.
        #[arg(long, requires = "file")]
        note: Option<String>,
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
    Remove { id: i64 },
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
        id: i64,
        #[arg(long)]
        note: Option<String>,
    },
    /// It is no longer needed.
    Drop {
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
    },
    /// Unfinished tasks in order; --all adds finished and dropped ones.
    List {
        #[arg(long)]
        all: bool,
    },
    /// One task.
    Show { id: i64 },
    /// Start working on a task (one at a time).
    Start { id: i64 },
    /// Close a task as done; only when the person says so.
    Done {
        id: i64,
        #[arg(long)]
        note: Option<String>,
    },
    /// Close a task without doing it.
    Drop {
        id: i64,
        #[arg(long)]
        note: Option<String>,
    },
    /// Reopen a closed task.
    Reopen { id: i64 },
    /// Change title, reason, position or places (--on / --off).
    Edit {
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
    },
}

#[derive(Subcommand)]
enum PhotoCmd {
    /// Copy a photo into the store and attach it; --crop x,y,w,h (fractions 0–1) attaches a cut-out.
    Add {
        reference: String,
        file: PathBuf,
        #[arg(long)]
        crop: Option<String>,
        #[arg(long)]
        note: Option<String>,
        /// Attach the whole photo even though it is already attached whole to another node.
        #[arg(long, conflicts_with = "crop")]
        whole: bool,
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
        /// temporary copy, to check the corners by eye; with a note, also show it in a running
        /// `ev ui`.
        #[arg(long, num_args = 0..=1)]
        preview: Option<Option<String>>,
    },
    /// Draw numbered marks on a copy of a photo, to show which thing is meant and where it goes:
    /// `<label>=x,y,w,h` (fractions of the upright photo) or `<label>=A6` (cells of the grid,
    /// when TARGET is a place). The copy is temporary: not stored, not attached.
    Mark {
        /// A photo file, or a place whose newest whole photo is marked.
        target: String,
        #[arg(required = true)]
        marks: Vec<String>,
        /// The grid's corners in the photo, when it did not keep them (see `photo cut --grid`).
        #[arg(long)]
        grid: Option<String>,
        /// Where to write the marked copy; a scratch folder otherwise.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also show it full screen in a running `ev ui`, titled with this note.
        #[arg(long)]
        show: Option<String>,
    },
    /// A node's photos, numbered from 1.
    List { reference: String },
    /// Detach the n-th photo of a node.
    Remove { reference: String, n: usize },
    /// Copy every photo still referenced outside the store into it.
    Adopt,
    /// The newest photo still shows the place well enough; drop it from the photo-needed list.
    Current { reference: String },
}

#[derive(Subcommand)]
enum RuleCmd {
    /// Add a rule in plain words.
    Add { text: String },
    /// Every rule with its id.
    List,
    /// Remove a rule by id.
    Remove { id: i64 },
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
    /// Its contents have not been inventoried yet.
    #[arg(long)]
    unknown: bool,
    /// NDJSON file, one node per line.
    #[arg(long, conflicts_with = "stdin")]
    batch: Option<PathBuf>,
    /// Read NDJSON lines from stdin.
    #[arg(long)]
    stdin: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let json = cli.json || !std::io::stdout().is_terminal();
    // JSON has no words to translate; skip reading the settings (and the system language).
    if !json {
        ui::set_language_from_settings();
    }
    match run(cli) {
        Ok(Value::Null) => ExitCode::SUCCESS,
        Ok(value) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&value).unwrap_or_default()
                );
            } else {
                print!("{}", render::human(&value));
            }
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

fn db_path(flag: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = flag {
        return Ok(p);
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
        (Some(n), Some(_)) => {
            return Err(Error::Usage(format!(
                "unknown setting `{n}`; there are language, theme and resume"
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

fn run(cli: Cli) -> Result<Value> {
    if let Cmd::Settings { name, value } = cli.cmd {
        return settings_cmd(name, value);
    }
    let db = db_path(cli.db)?;
    let mut inv = Inventory::open(&db)?;
    match cli.cmd {
        Cmd::Settings { .. } => unreachable!("settings are handled before the database opens"),
        Cmd::Grid {
            reference,
            cols,
            rows,
            clear,
        } => match (cols, rows, clear) {
            (Some(c), Some(r), _) => inv.grid_set(&reference, c, r),
            (_, _, true) => inv.grid_clear(&reference),
            _ => inv.grid(&reference),
        },
        Cmd::Cell { pairs, recode } => {
            let pairs = pairs
                .iter()
                .map(|p| {
                    p.split_once('=')
                        .map(|(r, c)| (r.trim().to_string(), c.to_string()))
                        .ok_or_else(|| Error::Usage(format!("`{p}` is not <ref>=<cells>")))
                })
                .collect::<Result<Vec<_>>>()?;
            inv.cells_set(&pairs, recode)
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
        } => {
            let kind = kind.map(|k| k.parse::<Kind>()).transpose()?;
            inv.find(&text, tag.as_deref(), kind, include_gone)
        }
        Cmd::Edit {
            reference,
            assignments,
        } => {
            warn_missing_photos(
                assignments
                    .iter()
                    .filter_map(|a| a.strip_prefix("photos=+")),
            );
            inv.edit(&reference, &assignments)
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
            reference,
            to,
            plan,
        } => inv.move_to(&reference, &to, plan),
        Cmd::Pending => inv.pending(),
        Cmd::Done { reference } => inv.done(&reference),
        Cmd::Cancel { reference } => inv.cancel(&reference),
        Cmd::Dispose {
            reference,
            disposition: d,
        } => inv.dispose(&reference, disposition(&d)?),
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
        } => inv.gone_because(
            &reference,
            d.as_deref().map(disposition).transpose()?,
            why.as_deref(),
        ),
        Cmd::Disposals { disposition: d } => {
            inv.disposals(d.as_deref().map(disposition).transpose()?)
        }
        Cmd::Lost { reference: Some(r) } => inv.mark_lost(&r),
        Cmd::Lost { reference: None } => inv.lost_list(),
        Cmd::Found { reference } => inv.found(&reference),
        Cmd::History {
            reference,
            contents: false,
        } => inv.history(&reference),
        Cmd::History {
            reference,
            contents: true,
        } => inv.history_with_contents(&reference),
        Cmd::Ui => ui::run(inv, &db).map(|()| Value::Null),
        Cmd::Lend { reference, to } => inv.lend(&reference, &to),
        Cmd::Back { reference } => inv.back(&reference),
        Cmd::For { place } => inv.errands(place.as_deref()),
        Cmd::Place(PlaceCmd::Add { name, aliases }) => inv.place_add(&name, &aliases),
        Cmd::Place(PlaceCmd::Alias { place, alias }) => inv.place_alias(&place, &alias),
        Cmd::Place(PlaceCmd::List) => inv.place_list(),
        Cmd::Place(PlaceCmd::Merge { from, into }) => inv.place_merge(&from, &into),
        Cmd::Suggest { text, tag, for_ref } => {
            inv.suggest_with(&text.join(" "), tag.as_deref(), for_ref.as_deref())
        }
        Cmd::Regroup { reference } => inv.regroup(reference.as_deref()),
        Cmd::Themes { reference } => inv.themes(reference.as_deref()),
        Cmd::Synonym(SynonymCmd::Add { words }) => inv.synonym_add(&words.join(", ")),
        Cmd::Synonym(SynonymCmd::List) => inv.synonym_list(),
        Cmd::Synonym(SynonymCmd::Remove { id }) => inv.synonym_remove(id),
        Cmd::Facet(FacetCmd::Add { name, words }) => inv.facet_add(&name, words.as_deref()),
        Cmd::Facet(FacetCmd::List) => inv.facet_list(),
        Cmd::Facet(FacetCmd::Remove { name }) => inv.facet_remove(&name),
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
        Cmd::Progress => inv.progress(),
        Cmd::Next => inv.next(),
        Cmd::Todo => inv.todo(),
        Cmd::Focus { file, note, .. } if !file.is_empty() => inv.focus_file(&file, note.as_deref()),
        Cmd::Focus {
            reference,
            photo,
            clear,
            ..
        } => {
            if reference.is_none() && !clear {
                return Err(Error::Usage("name a node, --file, or --clear".into()));
            }
            inv.focus(reference.as_deref(), photo)
        }
        Cmd::Label { references, needed } => inv.label(&references, !needed),
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
            inv.sale(&reference, status, price, place.as_deref())
        }
        Cmd::Need(NeedCmd::Add {
            text,
            qty,
            make,
            for_ref,
            note,
        }) => inv.need_add(&text, qty, make, for_ref.as_deref(), note.as_deref()),
        Cmd::Need(NeedCmd::List { all }) => inv.need_list(all),
        Cmd::Need(NeedCmd::Got { id, note }) => inv.need_close(id, true, note.as_deref()),
        Cmd::Need(NeedCmd::Drop { id, note }) => inv.need_close(id, false, note.as_deref()),
        Cmd::Task(TaskCmd::Add { title, why, on, at }) => inv.task_add(&title, &why, &on, at),
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
        }) => inv.task_edit(id, title.as_deref(), why.as_deref(), &on, &off, at),
        Cmd::Audit => inv.audit(),
        Cmd::Rule(RuleCmd::Add { text }) => inv.rule_add(&text),
        Cmd::Rule(RuleCmd::List) => inv.rule_list(),
        Cmd::Rule(RuleCmd::Remove { id }) => inv.rule_remove(id),
        Cmd::Photo(PhotoCmd::Add {
            reference,
            file,
            crop,
            note,
            whole,
        }) => {
            let crop = crop.map(|c| c.parse::<ev_core::Crop>()).transpose()?;
            inv.photo_add_with(&reference, &file, crop, note.as_deref(), whole)
        }
        Cmd::Photo(PhotoCmd::Mark {
            target,
            marks,
            grid,
            out,
            show,
        }) => {
            let grid = grid
                .as_deref()
                .map(str::parse::<ev_core::GridCorners>)
                .transpose()?;
            let marks = marks
                .iter()
                .map(|m| {
                    let (label, at) = m.split_once('=').ok_or_else(|| {
                        Error::Usage(format!("`{m}` is not <label>=x,y,w,h or <label>=<cell>"))
                    })?;
                    Ok((label.trim().to_string(), at.trim().to_string()))
                })
                .collect::<Result<Vec<_>>>()?;
            let mut v = inv.photo_mark(&target, &marks, grid.as_ref(), out.as_deref())?;
            if let (Some(note), Some(path)) = (show, v["marked"].as_str().map(PathBuf::from)) {
                v["shown"] = inv.focus_file(&[path], Some(&note))?["focus"].clone();
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
        }) => {
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
                    Ok((r.trim().to_string(), c.parse::<ev_core::Crop>()?))
                })
                .collect::<Result<Vec<_>>>()?;
            if preview.is_some() {
                let mut v =
                    inv.photo_cut_preview(&file, place.as_deref(), &crops, grid.as_ref(), None)?;
                if let (Some(Some(n)), Some(path)) =
                    (preview, v["preview"].as_str().map(PathBuf::from))
                {
                    v["shown"] = inv.focus_file(&[path], Some(&n))?["focus"].clone();
                }
                return Ok(v);
            }
            inv.photo_cut(
                &file,
                place.as_deref(),
                &crops,
                note.as_deref(),
                grid.as_ref(),
            )
        }
        Cmd::Photo(PhotoCmd::List { reference }) => inv.photo_list(&reference),
        Cmd::Photo(PhotoCmd::Remove { reference, n }) => inv.photo_remove(&reference, n),
        Cmd::Photo(PhotoCmd::Adopt) => inv.photo_adopt(),
        Cmd::Photo(PhotoCmd::Current { reference }) => inv.photo_current(&reference),
    }
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
            None => {
                let mut s = String::new();
                std::io::stdin()
                    .read_to_string(&mut s)
                    .map_err(|e| Error::Usage(format!("cannot read stdin: {e}")))?;
                s
            }
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
    let name = a
        .name
        .ok_or_else(|| Error::Usage("a name is required".into()))?;
    let kind = a
        .kind
        .ok_or_else(|| Error::Usage("--kind is required".into()))?;
    warn_missing_photos(a.photos.iter().map(String::as_str));
    inv.add(NewNode {
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
        unknown: a.unknown,
    })
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
}
