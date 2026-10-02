//! `ev mcp`: the inventory as an MCP server over stdio (spec/mcp.md).
//!
//! Every tool goes through the CLI's own parser and dispatcher, so a command behaves the same
//! from a terminal and from an agent. Nothing but MCP messages may reach stdout.

use std::path::{Path, PathBuf};

use clap::Parser;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, GetPromptRequestParams, GetPromptResponse, GetPromptResult,
    Implementation, ListPromptsResult, ListResourcesResult, PaginatedRequestParams, Prompt,
    PromptMessage, ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource,
    ResourceContents, Role, ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt, schemars, tool, tool_handler,
    tool_router,
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

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct FormatOnly {
    #[serde(default)]
    format: Format,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct FindArgs {
    /// Words to look for, in any order (stems, synonyms and typos match); may be left out with
    /// `tag` or `kind` to list all of them.
    text: Option<String>,
    /// Only things with this tag.
    tag: Option<String>,
    /// Only this kind: home, room, furniture, container or item.
    kind: Option<String>,
    /// Also things that left the home.
    include_gone: Option<bool>,
    #[serde(default)]
    format: Format,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ShowArgs {
    /// A code, an exact name or `#id` (a partial name never resolves).
    #[serde(rename = "ref")]
    reference: String,
    /// A node that left the home, by id.
    include_gone: Option<bool>,
    #[serde(default)]
    format: Format,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SuggestArgs {
    /// What the thing is: its name, part code and kind of thing, in Turkish and English when
    /// both are used ("KY-018 LDR ışık sensörü modülü").
    text: Option<String>,
    /// An existing node instead of a text: its own name, note and tags are the query.
    #[serde(rename = "for")]
    for_ref: Option<String>,
    #[serde(default)]
    format: Format,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct HistoryArgs {
    /// A code, an exact name or `#id`.
    #[serde(rename = "ref")]
    reference: String,
    /// Also what came in, went out or was added there.
    contents: Option<bool>,
    #[serde(default)]
    format: Format,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct TreeArgs {
    /// The node to start from; every home when left out.
    #[serde(rename = "ref")]
    reference: Option<String>,
    /// How many levels down.
    depth: Option<usize>,
    #[serde(default)]
    format: Format,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct PhotoArgs {
    /// A code, an exact name or `#id`.
    #[serde(rename = "ref")]
    reference: String,
    /// Which photo, from 1 (the oldest); the newest when left out.
    n: Option<usize>,
}

/// `["--flag", value]` when the value is given.
fn flag(name: &str, value: Option<String>) -> Vec<String> {
    value.map_or_else(Vec::new, |v| vec![name.to_string(), v])
}

/// `["--flag"]` when true.
fn switch(name: &str, on: Option<bool>) -> Vec<String> {
    if on == Some(true) {
        vec![name.to_string()]
    } else {
        Vec::new()
    }
}

/// A command, its options, then `--` and its positional values, so a value that starts with
/// `-` is never read as a flag.
fn argv(cmd: &str, options: Vec<Vec<String>>, positional: Vec<String>) -> Vec<String> {
    let mut out = vec![cmd.to_string()];
    out.extend(options.into_iter().flatten());
    if !positional.is_empty() {
        out.push("--".into());
        out.extend(positional);
    }
    out
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

    #[tool(
        description = "Where to pick up: the task to work on (one due within a day goes first), \
each of its places with what is planned to arrive and what else waits there (photos, labels, \
unclear names, coverage questions), notes on the order, progress and the places no task covers. \
Call it first in every session.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn next(
        &self,
        Parameters(p): Parameters<FormatOnly>,
    ) -> Result<CallToolResult, McpError> {
        self.call(argv("next", vec![], vec![]), None, p.format)
            .await
    }

    #[tool(
        description = "Everything waiting, by kind with counts: tasks, planned moves, errands, \
things leaving, labels to print, things to buy, repairs, use-by dates, lost things, places not \
counted, photos needed now, unclear records, coverage and value questions.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn todo(
        &self,
        Parameters(p): Parameters<FormatOnly>,
    ) -> Result<CallToolResult, McpError> {
        self.call(argv("todo", vec![], vec![]), None, p.format)
            .await
    }

    #[tool(
        description = "Where is it? Word search over name, code, make, model, serial, note, theme \
and tags, best match first, each with its #id and full path. Act on a result by its #id.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn find(&self, Parameters(p): Parameters<FindArgs>) -> Result<CallToolResult, McpError> {
        let args = argv(
            "find",
            vec![
                flag("--tag", p.tag),
                flag("--kind", p.kind),
                switch("--include-gone", p.include_gone),
            ],
            p.text.into_iter().collect(),
        );
        self.call(args, None, p.format).await
    }

    #[tool(
        description = "One node: its fields, path, what is in it, its photos and documents, planned \
move, marks, tasks and observations. Read a place this way before proposing it.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn show(&self, Parameters(p): Parameters<ShowArgs>) -> Result<CallToolResult, McpError> {
        let args = argv(
            "show",
            vec![switch("--include-gone", p.include_gone)],
            vec![p.reference],
        );
        self.call(args, None, p.format).await
    }

    #[tool(
        description = "Where should this go: the rules, where similar things are with the words \
that matched and why, and whether nothing here is this kind of thing (new_group_likely). \
Give `text` for a new thing or `for` for one already recorded.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn suggest(
        &self,
        Parameters(p): Parameters<SuggestArgs>,
    ) -> Result<CallToolResult, McpError> {
        let words = p
            .text
            .map(|t| t.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default();
        let args = argv("suggest", vec![flag("--for", p.for_ref)], words);
        self.call(args, None, p.format).await
    }

    #[tool(
        description = "What happened to a node, oldest first; with `contents`, also what came in, \
went out or was added there.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn history(
        &self,
        Parameters(p): Parameters<HistoryArgs>,
    ) -> Result<CallToolResult, McpError> {
        let args = argv(
            "history",
            vec![switch("--contents", p.contents)],
            vec![p.reference],
        );
        self.call(args, None, p.format).await
    }

    #[tool(
        description = "A node's photo as an image: the newest by default, or the n-th from the \
oldest. For agents that cannot open the photo files themselves.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn photo(
        &self,
        Parameters(p): Parameters<PhotoArgs>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || photo_result(db, p.reference, p.n))
            .await
            .map_err(|e| McpError::internal_error(format!("the photo panicked: {e}"), None))
    }

    #[tool(
        description = "The tree under a node, or every home; `depth` limits how far down. On a \
whole home this is long: start from a room or a piece of furniture.",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn tree(&self, Parameters(p): Parameters<TreeArgs>) -> Result<CallToolResult, McpError> {
        let args = argv(
            "tree",
            vec![flag("--depth", p.depth.map(|d| d.to_string()))],
            p.reference.into_iter().collect(),
        );
        self.call(args, None, p.format).await
    }
}

/// The skill and the reference of this version, for clients with no skills of their own.
const SKILL: &str = include_str!("../../skills/ev/SKILL.md");
const REFERENCE: &str = include_str!("../../REFERENCE.md");

/// The resources: (uri, name, description, text).
const RESOURCES: [(&str, &str, &str, &str); 2] = [
    (
        "ev://skill",
        "skill",
        "How to keep the inventory with the person: the conversation loop, tours, photos, \
         placement, purchases. Read it before the first tour.",
        SKILL,
    ),
    (
        "ev://reference",
        "reference",
        "Every command, field and payload of this version of ev.",
        REFERENCE,
    ),
];

#[tool_handler]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new("ev", env!("CARGO_PKG_VERSION")))
        .with_instructions(INSTRUCTIONS)
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        Ok(ListPromptsResult::with_all_items(vec![Prompt::new(
            "ev",
            Some("Keep the home inventory with ev: how the conversation, tours and photos go."),
            None,
        )]))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, McpError> {
        if request.name != "ev" {
            return Err(McpError::invalid_params(
                format!("no prompt `{}`; there is `ev`", request.name),
                None,
            ));
        }
        Ok(GetPromptResponse::Complete(GetPromptResult::new(vec![
            PromptMessage::new_text(Role::User, SKILL),
        ])))
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult::with_all_items(
            RESOURCES
                .iter()
                .map(|(uri, name, about, _)| {
                    Resource::new(*uri, *name)
                        .with_description(*about)
                        .with_mime_type("text/markdown")
                })
                .collect(),
        ))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let Some((uri, _, _, text)) = RESOURCES.iter().find(|r| r.0 == request.uri) else {
            return Err(McpError::invalid_params(
                format!(
                    "no resource `{}`; there are ev://skill and ev://reference",
                    request.uri
                ),
                None,
            ));
        };
        Ok(ReadResourceResponse::Complete(ReadResourceResult::new(
            vec![ResourceContents::text(*text, *uri).with_mime_type("text/markdown")],
        )))
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
        Ok(v) => shaped(&v, format),
        Err(e) => {
            let text = match format {
                Format::Text => crate::render::error(&e),
                Format::Json => e.to_json().to_string(),
            };
            CallToolResult::error(vec![ContentBlock::text(cut(text))])
        }
    }
}

/// A result as text or as JSON (then also as structured content), followed by the pictures it
/// made: a numbered photo (`marked`) and a contact sheet of crops (`sheet`), so the person sees
/// them in the conversation even with no `ev ui` open.
fn shaped(v: &Value, format: Format) -> CallToolResult {
    let body = match format {
        Format::Text => crate::render::human(v),
        Format::Json => serde_json::to_string_pretty(v).unwrap_or_default(),
    };
    let mut content = vec![ContentBlock::text(cut(body))];
    for key in ["marked", "sheet"] {
        if let Some(block) = v[key].as_str().and_then(|p| image_block(Path::new(p))) {
            content.push(block);
        }
    }
    let mut r = CallToolResult::success(content);
    if let Format::Json = format {
        r.structured_content = Some(v.clone());
    }
    r
}

/// The long side of a picture sent to an agent: about what a model reads at full detail.
const LONG_SIDE: u32 = 1568;

/// A picture as an image block: upright, JPEG, its long side at most `LONG_SIDE` pixels.
/// `None` when the file is missing or not a picture.
fn image_block(path: &Path) -> Option<ContentBlock> {
    let img = ev_core::open_upright(path).ok()?;
    let img = if img.width().max(img.height()) > LONG_SIDE {
        img.resize(LONG_SIDE, LONG_SIDE, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let mut buf = std::io::Cursor::new(Vec::new());
    img.to_rgb8()
        .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut buf, 85,
        ))
        .ok()?;
    Some(ContentBlock::image(
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, buf.into_inner()),
        "image/jpeg",
    ))
}

/// The `photo` tool: a node's n-th photo (the newest by default) as an image, with a line
/// saying which node and which photo.
fn photo_result(db: Option<PathBuf>, reference: String, n: Option<usize>) -> CallToolResult {
    let listed = run_args(
        db,
        &["photo".into(), "list".into(), "--".into(), reference],
        None,
        Format::Json,
    );
    let Some(v) = listed
        .structured_content
        .clone()
        .filter(|_| listed.is_error != Some(true))
    else {
        return listed;
    };
    let photos = v["photos"].as_array().cloned().unwrap_or_default();
    let node = &v["node"];
    let name = format!(
        "#{} {}",
        node["id"],
        node["name"].as_str().unwrap_or_default()
    );
    let refuse = |why: String| CallToolResult::error(vec![ContentBlock::text(why)]);
    if photos.is_empty() {
        return refuse(format!("{name} has no photo"));
    }
    let n = n.unwrap_or(photos.len());
    let Some(p) = photos.get(n.wrapping_sub(1)) else {
        return refuse(format!(
            "{name} has {} photo(s); ask for 1 to {}",
            photos.len(),
            photos.len()
        ));
    };
    let path = p["path"].as_str().unwrap_or_default();
    let Some(block) = image_block(Path::new(path)) else {
        return refuse(format!(
            "the file of photo {n} of {name} cannot be read: {path}"
        ));
    };
    let note = p["note"]
        .as_str()
        .map(|t| format!(" — {t}"))
        .unwrap_or_default();
    CallToolResult::success(vec![
        ContentBlock::text(format!("{name}: photo {n} of {}{note}", photos.len())),
        block,
    ])
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
