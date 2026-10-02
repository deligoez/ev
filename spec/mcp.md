# ev — an MCP server, so any agent can use the inventory

Status: **implemented** (unreleased), 2026-10-02. Written 2026-10-02 after a survey of the official Rust
SDK, the MCP specification (revision 2026-07-28), Anthropic's guidance on tools for agents and on
skills next to MCP, and the client configurations of Claude Code, Codex and OpenCode. Phases are
in §8. Remote access (a phone, an HTTP transport, OAuth) is out of scope (§9).

## 1. Decision

`ev mcp` runs an MCP server over stdio. It opens the same inventory the CLI opens and exposes:

1. **One general tool, `ev`,** that runs any `ev` command from its arguments, so nothing the CLI
   can do is missing.
2. **A few read-only tools** for what every session starts with and what is asked most:
   `next`, `todo`, `find`, `show`, `suggest`, `history`, `tree`.
3. **`photo`,** a read-only tool that returns a node's photo as an image, for agents that cannot
   open files.
4. **Instructions** of at most 2,048 characters: what ev is, when to reach for it, and the few
   rules that must hold even without the skill.
5. **The skill as a prompt and a resource** (`ev://skill`), and the reference as a resource
   (`ev://reference`), for clients that have no skills of their own.

The skill stays the place for the process. It is rewritten to name both ways in: the CLI and the
MCP tools.

## 2. Why

- **The person brings the agent, and with it the subscription.** A product cannot offer
  claude.ai login inside itself; an MCP client the person already uses (Claude Code, Codex,
  OpenCode, Claude Desktop) can call ev with no key and no cost on ev's side. Today only an
  agent with a shell and the skill can use ev.
- **One tool per command would not fit.** ev has 60 top-level commands, many with subcommands.
  Tool definitions take context, and agents pick worse among many similar tools. The guidance
  is one tool per job, not per endpoint. The general tool keeps everything reachable at the
  cost of one definition; the read tools give the commonest calls a schema of their own and a
  read-only mark, so a client can let them run without asking.
- **The same code path as the CLI.** The general tool parses its arguments with the CLI's own
  parser and runs the CLI's own dispatcher. A command behaves the same in both, and the tests
  of one cover the other.
- **MCP carries connectivity, the skill carries expertise.** Most of the 38 KB skill is process:
  nothing is done until the person says so, every photo is shown framed and numbered, a planned
  move is not a done move. No tool description holds that, and Claude Code cuts server
  instructions at 2,048 characters (measured, not documented). The instructions carry only
  what an agent must know before it has read the skill.
- **An image in a tool result is shown to the person.** Claude Code displays a returned image
  inline in the conversation. A numbered photo can reach the person even with no `ev ui` open.

## 3. Transport and process

- **stdio only.** The client starts `ev mcp`; the server reads JSON-RPC from stdin and writes
  only MCP messages to stdout. Anything else goes to stderr. It exits when stdin closes.
- **`ev mcp --db <file>`** (or `EV_DB`) fixes the inventory for the whole server. A tool call
  cannot name another one: arguments containing `--db` are refused.
- **Each call opens the inventory, runs, and closes it**, as a CLI call does, on a blocking
  thread. Other ev processes (the CLI, `ev ui`) can use the same file at the same time, as they
  do today.
- **Standard input of a command comes from the call, never from the process.** The commands
  that read stdin (`add --stdin`, `edit --stdin`, `sketch --stdin`, `buy import --stdin`,
  `money import --stdin`) take the tool's `input` text instead. Reading the process's stdin
  inside the server would consume the protocol stream.
- **Refused through the general tool:** `ui` (a terminal program) and `mcp` itself.
- **Language:** the text output follows `ev settings language`, as in the terminal.

## 4. Tools

Every tool takes `format`: `text` (default) or `json`.

- `text`: one text block with the readable output of `--text`. It carries `#id`s, so an agent
  can act on it.
- `json`: one text block with the JSON, and the same JSON as `structuredContent`.

A result longer than 50,000 characters is cut there, and the cut says so and suggests how to
narrow the call (`--depth`, a reference, a tag).

**Errors.** A refused or failed command is a tool result with `isError: true` and the message of
the CLI's error output: usage (CLI exit 2), not found (3), ambiguous with its candidates (4),
refused by a rule with its details (5), database newer than this ev (6). The agent reads it and
corrects the call. Only a malformed MCP request is a protocol error.

| Tool | Arguments | Same as | Annotations |
|---|---|---|---|
| `ev` | `args: [string]`, `input?: string` | `ev <args…>` (with `input` as stdin) | not read-only, destructive |
| `next` | — | `ev next` | read-only, idempotent |
| `todo` | — | `ev todo` | read-only, idempotent |
| `find` | `text?`, `tag?`, `kind?`, `include_gone?` | `ev find` | read-only, idempotent |
| `show` | `ref`, `include_gone?` | `ev show` | read-only, idempotent |
| `suggest` | `text?` or `for?` | `ev suggest` | read-only, idempotent |
| `history` | `ref`, `contents?` | `ev history` | read-only, idempotent |
| `tree` | `ref?`, `depth?` | `ev tree` | read-only, idempotent |
| `photo` | `ref`, `n?` | the n-th photo of a node (the newest by default) | read-only, idempotent |

- **`ev`'s description** lists the command families in one line each and says how to learn the
  rest: `args: ["--help"]` or `args: ["<command>", "--help"]`, whose help text comes back as
  the result.
- **Images.** `photo` returns the photo as an image block (JPEG, its long side at most 1,568
  pixels) and a text line saying which node and which photo. When a result of `ev` carries a
  numbered photo (`marked` from `photo cut` or `photo mark`) or a contact sheet (`sheet`), the
  image goes into the result too, after the text.
- **Annotations are hints, not security.** A client asks before running `ev`; it may let the
  read tools run freely. Nothing in ev relies on that.

## 5. Instructions

At most 2,048 characters (a test holds the limit). In this order:

1. What ev is: a home inventory kept by talking; the person reports, the agent records.
2. Start with `next`; `todo` for everything waiting; `find` or `show` before acting on a thing.
3. The rules that hold without the skill: never mark anything done, moved or gone on your own
   reading, ask; propose a move and record it as done only when the person says it is done;
   show every photo framed and numbered before talking about it.
4. Where the full process is: the `ev` prompt, or the resource `ev://skill`.

## 6. Prompts and resources

- **Prompt `ev`:** one user message carrying the skill's text, for clients that surface prompts
  as commands (`/ev`).
- **Resource `ev://skill`** (`text/markdown`): the skill. **Resource `ev://reference`**: the
  reference (fields, payloads, every command).
- Both are compiled into the binary from `skills/ev/SKILL.md` and `REFERENCE.md`, so the server
  always serves the text of its own version.

## 7. The skill

The skill stays the process document, and becomes independent of the way in:

- A short section, **"Two ways in: the CLI and MCP"**: every `ev …` in the skill is also the
  `ev` tool with the same arguments; `--stdin` text goes in `input`; the read tools exist for
  the commonest reads; a numbered photo comes back as an image in the tool result.
- The rest of the skill keeps naming commands as `ev …`: one notation, two transports.

## 8. Phases

1. **Input from the call.** One helper reads standard input for every command; the MCP server
   sets the text a call gives. Tests: each `--stdin` command still reads the process's stdin
   from the CLI.
2. **`ev mcp` with the general tool and the instructions.** `rmcp` (official SDK) on tokio,
   stdio. Tests through an MCP client over a child process: tools are listed in a fixed order;
   `ev` runs a command and returns its text; `format: json` returns `structuredContent`; a
   refused command returns `isError: true` with the message; `ui`, `mcp` and `--db` are
   refused; `input` reaches `add --stdin`; help text comes back; instructions are at most 2,048
   characters.
3. **The read tools.** Each builds the same arguments the CLI would get and goes through the
   same runner. Tests: each matches the CLI's output for the same arguments.
4. **Images.** `photo` and the images of `marked` and `sheet`. Tests: an image block of type
   JPEG with the expected long side.
5. **Prompt and resources.** Tests: listed, and their text is the skill's and the reference's.
6. **Documents.** README (connecting an agent: the commands for Claude Code, Codex and
   OpenCode), REFERENCE (§3–§6 as reference), the skill (§7), release notes.

## 9. Not now

- **Remote use** (a phone, the Claude app): it needs the Streamable HTTP transport, a public
  address and OAuth. The inventory holds addresses, invoices and scans of IDs; exposing it is a
  separate decision.
- **Tools for every command** with their own schemas: only if a measured session shows the
  general tool is used badly.
- **Images sent by the person through an MCP chat:** the photo reaches the model, not the tool,
  so a chat app cannot hand a photo to `photo cut`. Agents with a file system (Claude Code)
  pass a path, as today.
- **Server-initiated notifications** (a changed inventory): `ev ui` already watches the file.
