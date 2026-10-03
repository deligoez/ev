# Output: as little as needed, all of it JSON

## Decision

Every `ev` command answers in JSON when it is read by a program (a pipe, `--json`, the MCP
server), and the answer carries what the reader needs to act, once. Text (`--text`, a terminal)
stays the readable rendering of the same value.

The rules, in the order they were measured to matter:

1. **JSON is one line.** No indentation, on the command line and over MCP. A human reading JSON
   pipes it through `jq`; everyone else reads `--text`.
2. **A row names its place once.** The short form of a node (a row in a list: `children`,
   `results`, a task's `nodes`, a suggestion's `holder`, …) carries `path_text` and no `path`
   segments. `lost` appears only when it is true, like `disposition` and `qty`. The node a
   payload is about (`node` in `show` and every command that returns it) keeps `path`, the
   ancestors with their ids.
3. **A field with no value is left out.** Below the top level of a payload, a `null` field is
   not printed: a missing field reads as null. Top-level keys stay, so a payload's sections are
   always there to look for.
4. **The node payload leaves out what it does not have.** `show`, and every command that
   answers with the node it changed (`add`, `move`, `review`, `lend`, …), prints `node` and only
   the sections with something in them: no `"kits": []`, no `"grid": null`. The node's flags
   `lost` and `temporary` appear only when true, as on a row.
   An **edit** answers with what it changed: `node` as a row and `changed`
   (`{field: {before, after}}`). The caller wrote the rest; a long note came back whole on every
   `note=+…`, which the inventory agent reported (it already knew all of it).
5. **A list is a list of rows.** `buy list` prints a line's own fields and counts of what came
   with it; `buy show <id>` has the attachments, the documents, the raw file and the links.
6. **Errors are JSON too.** A mistyped argument or a missing value, which the argument parser
   catches before ev runs, prints the same `{"error": {"code": 2, "kind": "usage", …}}` as every
   other error in JSON mode. Only `--help` and `--version` stay text: they are read, not parsed.

## Why

Measured on one household's copy (~670 records, ~900 purchase lines), JSON piped:

| Command | Before | Where the bytes went |
|---|---|---|
| `buy list` | 1.9 MB | attachments, documents, raw paths and URLs on every one of ~900 lines |
| `regroup` | 500 KB | indentation; `path` segments on every holder and item |
| `tree` | 340 KB | indentation (60% of it) |
| `find kablo` | 55 KB | `path` segments: 55% of the rows |
| `show <box>` | 23 KB | children's `path` segments (a child's path is the box's plus itself); 15 of 22 sections empty |

An agent pays for every byte it reads, and its context is where the household's work happens.
Every one of these was a repeat: the segments say again what `path_text` says (a segment shows a
code or a name, exactly as `path_text` does), indentation says nothing, and an empty section
says what its absence says. The ancestors' ids were the one thing only `path` had; they stay on
the node a payload is about, where they are used (to step up to the parent), and a row's own id
is enough to ask for its path.

## Not changed

- What a command computes. `suggest` keeps `containers` (every holder, so the agent can choose
  past the best few) and `regroup` keeps `matched` (why a holder scores): they are the answer,
  not a repeat of it.
- Empty lists at the top level (`{"results": []}`): an empty answer is still an answer.
- `false` and `0`: they are values (`exists: false` on a photo whose file is missing).

## Phases

1. Compact JSON on the command line and over MCP.
2. Rows without `path` segments; `lost` only when true.
3. Null fields left out below the top level.
4. The node payload without empty sections.
5. `buy list` rows.
6. Usage errors as JSON.

Each phase updates `REFERENCE.md` (output shapes), the tests that read the old shapes, and
`release-notes/next.md`.
