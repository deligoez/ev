use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use ev_core::{Disposition, Error, Inventory, Kind, NewNode, Result};
use serde_json::Value;

mod render;
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
    /// Return a candidate to active.
    Restore { reference: String },
    /// A node leaves the home; --as is required when it is not a candidate yet.
    Gone {
        reference: String,
        #[arg(long = "as")]
        disposition: Option<String>,
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
    /// A node's events, oldest first.
    History { reference: String },
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
    #[arg(long = "tag")]
    tags: Vec<String>,
    #[arg(long = "photo")]
    photos: Vec<String>,
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

fn run(cli: Cli) -> Result<Value> {
    let mut inv = Inventory::open(&db_path(cli.db)?)?;
    match cli.cmd {
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
        Cmd::Restore { reference } => inv.restore(&reference),
        Cmd::Gone {
            reference,
            disposition: d,
        } => inv.gone(&reference, d.as_deref().map(disposition).transpose()?),
        Cmd::Disposals { disposition: d } => {
            inv.disposals(d.as_deref().map(disposition).transpose()?)
        }
        Cmd::Lost { reference: Some(r) } => inv.mark_lost(&r),
        Cmd::Lost { reference: None } => inv.lost_list(),
        Cmd::Found { reference } => inv.found(&reference),
        Cmd::History { reference } => inv.history(&reference),
        Cmd::Ui => ui::run(inv).map(|()| Value::Null),
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
        tags: a.tags,
        photos: a.photos,
    })
}

fn warn_missing_photos<'a>(paths: impl Iterator<Item = &'a str>) {
    for p in paths {
        if !std::path::Path::new(p.trim()).exists() {
            eprintln!("warning: photo path does not exist: {p}");
        }
    }
}
