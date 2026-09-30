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
- **Record-only changes make a place's photo stale.** Measured 2026-10-01 closing the K4x4-02-A
  tour: two minutes after its current photo, the drawer's theme was edited and a placeholder
  record in it was closed with `gone --as mistake`. `ev review --as toured` then refused the
  photo as older than what it shows (`reason: changed`), though nothing in the drawer moved; the
  way out was `ev photo current`. Expected: only changes a photo could show (a thing added,
  moved in or out, gone for real, split, a cell set) date the photo; a theme, a note, a tag or a
  record closed as a mistake do not.
