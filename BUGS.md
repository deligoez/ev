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
- **nextest now and then calls a CLI test leaky.** Over 25 full runs (2026-09-30), 7 runs
  showed one `LEAK`, each time a different test of `cli/tests/cli.rs` (five tests in all), all
  of them spawning the `ev` binary: the child's output pipe closed after nextest's leak
  timeout. The tests pass. Most likely the spawned process's teardown under a fully parallel
  run rather than anything a test leaves behind, but not shown. Expected: a known cause, or a
  `leak-timeout` in `.config/nextest.toml` chosen from a measurement.
- **Moving a sketched room leaves its outline in the old holder's frame.** A room's points are
  relative to its holder's top-left corner, and `ev move` does not translate them: when the two
  balconies moved out of their rooms to the home (2026-09-30), their outlines landed on the
  home's top-left corner until they were translated by hand (the room's origin added to every
  point, written back with `ev sketch --stdin`). Expected: a move keeps where a sketched node
  lies on the map, translating its points (and `at`) into the new holder's frame.
