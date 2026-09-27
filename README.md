# ev

Agent-first home inventory. An AI agent records what a person reports at the shelves and
answers "where is it?" — homes, rooms, furniture, boxes and items in one tree, with planned
moves, a give/sell/trash pipeline, lost items and a full history.

```bash
cargo install --path cli        # installs the `ev` binary
ev add Ev --kind home
ev add Salon --kind room --in Ev
ev find flipper
```

The database lives at `~/.ev/ev.db` (`--db` or `EV_DB` to change it). JSON when piped,
readable text on a terminal.

- `spec/0.1.0.md` — the decisions this release implements
- `REFERENCE.md` — commands, fields, payload shapes
- `skills/ev/SKILL.md` — how an agent should use it

Workspace: `core` (model, rules, SQLite) and `cli` (the `ev` binary). A Tauri app is
expected to reuse `core` later.

Quality gate: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
