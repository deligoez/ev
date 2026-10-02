Draft for the next release.

## New

- **`ev mcp`: any agent can keep the inventory.** An MCP server over stdio, so an MCP client
  the person already uses (Claude Code, Codex, OpenCode, Claude Desktop, Cursor) works with ev
  on their own subscription: `claude mcp add ev -- ev mcp`. One general tool, `ev`, runs any
  command from its arguments through the CLI's own parser and dispatcher, so nothing the CLI
  does is missing and both behave alike; lines a command reads with `--stdin` come in the
  call's `input`. Read-only tools cover the common reads (`next`, `todo`, `find`, `show`,
  `suggest`, `history`, `tree`), so a client can let them run unasked. `photo` returns a
  node's photo as an image, and a numbered photo a command makes (`photo cut --show`, `photo
  mark`) comes back as an image too: the person sees it in the conversation with no `ev ui`
  open. A refused command is a tool error with the CLI's message. The instructions stay under
  the 2,048 characters Claude Code keeps; the skill is also the prompt `ev` and the resource
  `ev://skill`, and the reference `ev://reference`. Nine tools take about 9 KB of definitions.
