# CLAUDE.md — developing ev

This repository is **ev**, an agent-first home inventory (Rust: `core` + `cli`, SQLite). You are
the development agent: you design, build, test and release ev. The inventory itself is kept by a
separate agent working in `~/.ev` (the data repository); it uses ev, reports what it lacks, and
never builds it. See **The two repositories** below.

## Language

**Every message to the person is in Turkish**, with proper diacritics. Everything in the
repository is English: code, comments, tests, specs, docs, commit messages, release notes.
Content shown to the person by `ev` (Turkish UI text) goes through `cli/src/i18n.rs`.

## This repository is PUBLIC

github.com/deligoez/ev. **No personal data, ever:** no real purchase prices, order numbers,
addresses, employer names, family or household names, ids or names from the real inventory,
and nothing about the person's own servers (host names, ports, where the data repository lives).
Tests, docs and examples use invented data (`Annemler`, `Ayşe`, round prices like 1999).
Measurements are described generally ("one household's ~1,170 lines from 14 shops"). Raw
purchase data (`~/.ev/purchases/`) never enters any repository. Scan the diff before a release.

## Workflow

- **Commits through `hc`**, loaded with the `hc` skill each time: one unit (a change + its green
  test) per commit, the test in its own commit right after, one commit per new test, Conventional
  Commits (`feat(mcp): …`, `fix(edit): …`, `test: …`, `docs(reference): …`). Commit after each
  finished unit, locally. No Claude attribution anywhere.
- **Push in batches, never to find out** (decided with the person, 2026-10-06): every push runs
  CI on GitHub, and CI is not where a mistake is discovered. Push the commits of a finished
  piece of work together (a phase, a fix with its tests, a docs pass), not one push per commit,
  and only after the full gate below passed locally on that exact `HEAD` — run as one `&&`
  chain whose exit status is checked, never through a pipe (`… | tail` hides a failure: a
  commit with a clippy error went out that way). **CI red is a stop:** no further push until
  the cause is understood and the fix is green locally. A test that passes here and fails
  there is a real bug (timing, time zone, a faster machine), not noise: reproduce it. CI
  cancels a run that a newer push to the same branch overtakes.
- **Quality gate** before every push, on the commits pushed:
  `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo shear && cargo nextest run --workspace`.
  Run it as `tools/gate.sh`: it checks the committed `HEAD` in a worktree of its own
  (`target/gate`, its own target directory), so work goes on in the main tree while it runs and
  nothing edited meanwhile leaks into what is checked (it did once: an edit made during a gate
  failed a test that had nothing to do with it). Between pushes, each commit that touches code
  passes `cargo clippy --all-targets -- -D warnings` and the tests of what it changed.
  A test reads every backticked `ev …` command in README, REFERENCE, the skill and
  `release-notes/next.md` and fails on one the CLI lacks; `i18n` tests fail on a text without a
  Turkish translation or a duplicate key. A test that takes seconds on its own is a finding: one
  question to macOS cost every UI test ten seconds until it was found (fixed 2026-10-06).
- **Specs** live in `spec/` (`purchases.md`, `mcp.md`, …): decision, why, data model, phases, not
  now. Write the spec first for a feature of any size, then implement phase by phase without
  stopping, unless the person asks otherwise. Design discussions end up in the spec, not in chat.
- **Docs move with the code:** README (user-facing; a feature not in README is not shipped),
  `REFERENCE.md` (every command, field, payload), `skills/ev/SKILL.md` (the agent's process), and
  `release-notes/next.md` (English, explanatory: what changed and why, not a commit list).
- File work through **codedbpro** (load its skill); Markdown edits with `patch`/`replace`.

## BUGS.md and releases

- **BUGS.md** collects what is wrong or missing: what happened, where, what was expected. Entries
  come from you and from the inventory agent (it appends here and commits with hc; that is the
  only file it touches in this repository). Write a finding the moment you see it, without
  asking. Before declaring something missing, check `ev --help`, `ev <cmd> --help` and REFERENCE.
- **A fixed entry moves to `release-notes/next.md`.**
- **Releases are batched and only when the person asks.** Steps: check README, REFERENCE and the
  skill against `next.md`; rename `next.md` to `vX.Y.Z.md` with an intro paragraph and create a
  new `next.md` containing exactly `Draft for the next release.` (a test reads it); bump
  `version` in `core/Cargo.toml` and `cli/Cargo.toml`; gate; scan the diff since the last tag for
  personal data; commit `chore(release): vX.Y.Z`; tag; push the branch and the tag. The tag push
  runs cargo-dist (`release.yml`): binaries, GitHub release with the notes, Homebrew formula in
  `deligoez/homebrew-tap` (`brew install deligoez/tap/ev`). Watch it with `gh run watch`.
- After a release: install it locally (below) and update the skill (below).

## Installing for the inventory agent

The person and the inventory agent use the ev installed on this Mac, and **between releases
they use the development build, never the last release**: the inventory agent is on this
machine, so a fix reaches it the moment it is pushed, not when a release is cut. After every
pushed change to code, install it right away and tell the inventory agent what changed (and the
skill lines to re-read, when the skill changed):

```bash
CARGO_TARGET_DIR=$PWD/target/install cargo install --locked --path cli --force
```

`~/.cargo/bin/ev` comes first on PATH and shadows the Homebrew one. `ev --version` tells them
apart: a build of a release tag prints `0.24.0`, any other build `0.24.0 (dev v0.24.0-9-g…)`
(`cli/build.rs`); check it after installing. A change of schema still waits for the steps below
before it is installed.

(Stale `ev-core` "method not found": `cargo clean --target-dir target/install`.)

**A schema change migrates the real inventory the moment the new binary opens it.** Before
installing a build with a new schema version (or a change to how the file is opened):
1. ask the inventory agent to stop writing and commit; make sure `~/.ev` has nothing uncommitted
   (it is a git repository; the inventory agent commits every change), and
2. copy the database with SQLite, not `cp`:
   `sqlite3 ~/.ev/ev.db ".backup '$HOME/.ev/ev.db.bak-<date>-v<old version>'"`. The inventory runs
   in write-ahead-log mode: a write may sit in `ev.db-wal` until its command ends, and `.backup`
   copies a consistent whole while `cp` of the file alone may not.

Never run tests, measurements or experiments against `~/.ev/ev.db`: use a copy or a temporary
database (`--db`, `EV_DB`). Never import shop data into the real database on your own.

**The skill** is installed from this repository through npx, never copied by hand:

```bash
npx -y skills@1.5.26 add deligoez/ev -g -a claude-code -a opencode -y -s ev
```

Run it after every pushed change to `skills/ev/SKILL.md`, check
`cmp ~/.agents/skills/ev/SKILL.md skills/ev/SKILL.md`, then **read the installed file** (at least
the changed sections; all of it after a compaction): the Skill tool shows a stale copy once
loaded, and rules were missed because of that.

## The two repositories

| | This repository (`~/Developer/deligoez/projects/ev`) | The data (`~/.ev`) |
|---|---|---|
| Who | the development agent (you) | the inventory agent |
| What | code, tests, specs, docs, skill, BUGS.md | `ev.db`, photos, documents, its own CLAUDE.md |
| Remote | GitHub `deligoez/ev`, public, branch `main` | private; its own CLAUDE.md says where |
| Commits | hc, per unit, pushed | hc after every inventory change, pushed |

How work flows between them: the inventory agent meets friction while keeping the inventory and
writes it into BUGS.md here; you fix it, release when asked, install it locally and update the
skill; the inventory agent picks the new behaviour up through the skill. Process rules for
keeping the inventory belong in `skills/ev/SKILL.md` (so every agent gets them) or in
`~/.ev/CLAUDE.md` (household specifics), not here.

## The sibling: ak

**ak** (`~/Developer/deligoez/projects/ak`, data in `~/.ak`) keeps the household's money; ev keeps
what is a thing (decided 2026-10-06; ak's `spec/0.1.0.md`). Every receipt, bill and subscription
is ak's; ev gets only the lines that are things (durable, clothing, one-time digital, and services
linked to a thing) through `ak export --for ev | ev buy import --stdin`, keyed `source: "ak"`.
Neither reads the other's database. ev's side of that contract goes into `spec/ak.md` when it is
agreed; a change to it is made in both repositories, with ak's development agent.

## Design principles (decided with the person)

- **ev never owns the model.** The person brings the agent and its subscription; ev gives it
  tools (CLI + skill, and `ev mcp`). No chat built into ev, no API key, no reuse of
  subscription logins (spec/mcp.md).
- **The agent proposes, the person confirms.** Nothing in ev marks a move done, a place toured
  or a thing gone on its own; commands that close things exist for the agent to run on the
  person's word.
- **Facts, not scores.** ev computes signals (due dates, what waits in a place, stale photos) and
  shows them; it never reorders the person's plan by a formula.
- **Friction is a missing verb.** When an agent has to script around ev (a loop of `ev edit`,
  chaining ids, collecting photo paths), ev lacks a command: add it.
- **Do not add what nobody asked for** (to a screen, a map, a record): propose first.
- **Every record shown by the place it is in**; photos are numbered and framed so the person can
  answer by number.

## Facts that save time

- MCP: `ev mcp` (rmcp 3.5, stdio), spec/mcp.md; tests in `cli/tests/mcp.rs` drive it through an
  MCP client. Claude Code keeps only 2,048 characters of server instructions (a test holds it).
- `cli/src/main.rs` `run()` is the one dispatcher; the MCP server parses with the same `Cli`.
  Commands read standard input only through `read_input()` (the MCP server's stdin is its protocol).
- The real database's schema history is in the backups' names under `~/.ev`; the current schema
  version is `SCHEMA_VERSION` in `core/src/store.rs`.
- `.config/nextest.toml` raises the leak timeout for the CLI tests (measured, see the comment).
