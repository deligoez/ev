# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **Words alone can reach the top of `ev buy for --toured`.** Shared words are capped at 30
  points, twice the bar of 15, so a line that only shares a few rare words with a thing ranks
  above real matches: a Belkin extension cord was offered for a TP4056 charging module, a
  bathroom cabinet for vinyl screw-cover stickers "for the bathroom cabinet" (2026-10-02, on a
  copy of the real inventory). Expected: a line with no shared code, brand or model needs more
  than words to be offered first, or the toured list sorts code and brand matches above word
  matches.
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
