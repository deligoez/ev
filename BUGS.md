# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **A word that is a root of its own is cut to another root.** `altın` → `alt` (meets
  `altında`), `ünite` → `uni`: the shortest shorter form written somewhere wins, and nothing in
  the inventory's own words tells a root from a suffixed form of another. Expected: a root stays
  whole. Left from the "one noun gets two keys" entry, whose ending cases (`bağı`/`bağları`,
  `bacaklarında`, `bloğu`) are fixed for the next release; this part needs a dictionary and
  waits for Çözgü's embeddable core. Measure with `tools/measure/stems.py`.
- **Three kit rules no test holds in place.** `cargo mutants -f core/src/kits.rs` (2026-09-30)
  caught 65 of 68 mutants; the three that lived are gaps in the tests, not in the code:
  deleting the `(State::Gone, _)` arm of `tally` (`kits.rs:93`; a gone part would count as
  found), making the "a kit with that name exists" guard in `kit_add` always true
  (`kits.rs:128`; any other database error would be reported as a duplicate name), and
  `added > 0` → `>= 0` in `kit_link` (`kits.rs:168`; linking a part twice would write a
  second history event). Expected: a test that fails on each.
- **The readable output is barely tested.** `cargo llvm-cov nextest` (2026-09-30): lines
  79.6% over the workspace, but `cli/src/render.rs` 16.3%, 1,133 lines never run; the CLI
  tests mostly read JSON, while the skill tells the agent to read with `--text`. Expected: a
  test per output shape of `render::human`, at least for the ones the agent reads daily
  (`show`, `next`, `todo`, `suggest`, `regroup`, `tree`).
- **nextest now and then calls one CLI test leaky.** `db_flag_wins_over_env` (cli/tests)
  shows `LEAK` in about one run in five: its `ev` child's output pipe closes after nextest's
  leak timeout. The test passes and nothing it runs starts a process or thread; still
  unexplained. Expected: no leak, or a known cause.
