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
  open. Text results follow `ev settings language`, read on every call. A refused command is a
  tool error with the CLI's message. A text result over 50,000 characters is cut with a note on
  how to narrow the call; a JSON result that long is refused with the same advice, since cut
  JSON does not parse. The instructions stay under the 2,048 characters Claude Code keeps; the
  skill is also the prompt `ev` and the resource `ev://skill`, and the reference
  `ev://reference`. Nine tools take about 9 KB of definitions.

## Fixed

- **Writes at the same moment failed with "database is locked".** A command that read and then
  wrote got the lock refused at once, without the five-second wait, when another write got in
  between. Rare from one terminal, but an MCP client sends calls in parallel: in QA, seven of
  eight parallel writes failed. Writes now take the lock up front and wait their turn, for up
  to 30 seconds instead of 5: a burst of two MCP servers and the CLI (60 writes among 40 reads)
  lost 27 writes before and none after.
- **`ev photo mark`, `ev focus` and `ev money needs` printed JSON as their text.** `photo
  mark` now lists each label and where it is, then the numbered copy; `focus` says what `ev ui`
  was asked to show, or that the request is cleared; `money needs` names the index, the month
  it starts from and every missing exchange rate.
- **A record id written as ev prints it was refused.** ev writes purchase lines, tasks,
  documents and observations as `#12`, but `ev buy show #1` or `ev task done #21` failed with
  "invalid digit". An agent copies the id as printed; every record id now takes `#12` or `12`.
- **`ev … | head` panicked** with "failed printing to stdout: Broken pipe" when the reader
  stopped before ev finished writing. A closed pipe is now ignored.
