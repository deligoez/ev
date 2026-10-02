//! `ev mcp`: the inventory as an MCP server over stdio (spec/mcp.md).
//!
//! Every tool goes through the CLI's own parser and dispatcher, so a command behaves the same
//! from a terminal and from an agent. Nothing but MCP messages may reach stdout.

use std::path::PathBuf;

use clap::Parser;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt, schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::Value;

use crate::{Cli, Cmd};

/// What an agent must know before it has read the skill. Claude Code keeps only the first
/// 2,048 characters of a server's instructions; a test holds this under that.
pub(crate) const INSTRUCTIONS: &str = "ev is a home inventory kept by talking: the person stands at \
the shelves and reports, you record through ev's tools, and either of you asks where something \
is. Everything is a node in one tree (home > room > furniture > container > item); codes like \
K4x4-07-Ü or S5-01 are the physical labels.

Start with `next`: today's task, its places and what else waits there. `todo` lists everything \
waiting. Find a thing (`find`, `show`) before acting on it; a partial name never resolves, so act \
by #id.

The `ev` tool runs any ev command: `args` is the command line without `ev`, e.g. [\"move\", \"#12\", \
\"--to\", \"S5-01\", \"--plan\"]; lines a command reads with --stdin go in `input`; [\"<command>\", \
\"--help\"] explains a command.

Rules that always hold:
- Nothing is finished on your own reading. Never record a move done, a place toured, a task done \
or a thing gone until the person says so: propose first, record when they confirm.
- Show what you read from a photo: frame each group with a numbered box (`photo cut ... --show`, \
`photo mark`) and talk by those numbers.
- What the person says about a thing goes into its note, in their words.
- A refused call explains what to change; read it before trying again.

The full process (tours, photos, placement, purchases) is the `ev` prompt or the resource \
ev://skill; fields and payloads are in ev://reference.";

/// Longer results are cut here, with a note on how to narrow the call.
const MAX_CHARS: usize = 50_000;

/// How a result comes back: the readable text (with #ids), or the JSON.
#[derive(Debug, Default, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Format {
    /// The readable output, as `ev --text` prints it.
    #[default]
    Text,
    /// The full JSON, also as structured content.
    Json,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct EvArgs {
    /// The command line without `ev`, one argument per item: ["show", "K4x4-07-A"].
    args: Vec<String>,
    /// What the command reads from standard input (`--stdin`): NDJSON lines.
    input: Option<String>,
    #[serde(default)]
    format: Format,
}

#[derive(Clone)]
pub(crate) struct Server {
    db: Option<PathBuf>,
}

impl Server {
    pub(crate) fn new(db: Option<PathBuf>) -> Self {
        Self { db }
    }

    /// Runs one command on a blocking thread, as `ev <args>` would.
    async fn call(
        &self,
        args: Vec<String>,
        input: Option<String>,
        format: Format,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || run_args(db, &args, input, format))
            .await
            .map_err(|e| McpError::internal_error(format!("the command panicked: {e}"), None))
    }
}

#[tool_router]
impl Server {
    #[tool(
        name = "ev",
        description = "Run any ev command. `args` is the command line without `ev`. Families: \
record and change (add, edit, split, move, done, cancel, dispose, gone, restore, lost, found); \
look (find, show, tree, history, next, todo, progress, suggest, regroup, audit, map, grid, \
themes); plan (task, observe, unobserve, review, goal, rule, facet, synonym, need); photos and \
documents (photo, doc, focus); purchases, coverage and value (buy, cover, value, link, money); \
kits, labels and errands (kit, label, lend, back, for). [\"<command>\", \"--help\"] shows a \
command's arguments. Lines a command reads with --stdin go in `input`. Text output by default; \
format json returns the full JSON.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn ev(&self, Parameters(p): Parameters<EvArgs>) -> Result<CallToolResult, McpError> {
        self.call(p.args, p.input, p.format).await
    }
}

#[tool_handler]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("ev", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

/// Serves until the client closes stdin.
pub(crate) fn serve(db: Option<PathBuf>) -> ev_core::Result<()> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| ev_core::Error::Internal(format!("cannot start the runtime: {e}")))?;
    runtime.block_on(async {
        let service = Server::new(db)
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| ev_core::Error::Internal(format!("cannot start the server: {e}")))?;
        service
            .waiting()
            .await
            .map_err(|e| ev_core::Error::Internal(format!("the server stopped: {e}")))?;
        Ok(())
    })
}

/// One tool call: parse `args` as the CLI would, refuse what cannot run here, run it with
/// `input` as its standard input, and shape the result.
pub(crate) fn run_args(
    db: Option<PathBuf>,
    args: &[String],
    input: Option<String>,
    format: Format,
) -> CallToolResult {
    let argv = std::iter::once("ev".to_string()).chain(args.iter().cloned());
    let mut cli = match Cli::try_parse_from(argv) {
        Ok(cli) => cli,
        Err(e) => {
            let text = e.render().to_string();
            return match e.kind() {
                clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                | clap::error::ErrorKind::DisplayVersion => {
                    CallToolResult::success(vec![ContentBlock::text(text)])
                }
                _ => CallToolResult::error(vec![ContentBlock::text(text)]),
            };
        }
    };
    let refused = |why: &str| CallToolResult::error(vec![ContentBlock::text(why.to_string())]);
    // The arguments, not the parsed flag: the parser also fills it from EV_DB.
    if args.iter().any(|a| a == "--db" || a.starts_with("--db=")) {
        return refused("this server keeps one inventory; leave --db out");
    }
    match cli.cmd {
        Cmd::Ui => return refused("`ui` is a terminal program; run `ev ui` in a terminal"),
        Cmd::Mcp => return refused("`mcp` is this server; it does not run inside itself"),
        _ => {}
    }
    if db.is_some() {
        cli.db = db;
    }
    match crate::with_input(input, || crate::run(cli)) {
        Ok(Value::Null) => CallToolResult::success(vec![ContentBlock::text("done")]),
        Ok(v) => shaped(&v, format, false),
        Err(e) => {
            let text = match format {
                Format::Text => crate::render::error(&e),
                Format::Json => e.to_json().to_string(),
            };
            CallToolResult::error(vec![ContentBlock::text(cut(text))])
        }
    }
}

/// A result as text or as JSON (then also as structured content).
fn shaped(v: &Value, format: Format, _images: bool) -> CallToolResult {
    match format {
        Format::Text => {
            CallToolResult::success(vec![ContentBlock::text(cut(crate::render::human(v)))])
        }
        Format::Json => {
            let mut r = CallToolResult::success(vec![ContentBlock::text(cut(
                serde_json::to_string_pretty(v).unwrap_or_default(),
            ))]);
            r.structured_content = Some(v.clone());
            r
        }
    }
}

/// Cuts a long result at `MAX_CHARS`, on a character boundary, and says so.
fn cut(text: String) -> String {
    if text.chars().count() <= MAX_CHARS {
        return text;
    }
    let mut out: String = text.chars().take(MAX_CHARS).collect();
    out.push_str(
        "\n… (cut at 50,000 characters; narrow the call: a reference, --depth, a tag or a kind)",
    );
    out
}
