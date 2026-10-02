# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **No way to say "not this one" to a candidate.** When the person answers that a purchase line
  is not the thing in hand (an RFID reader offered a USB card-reader hub, a short screw offered
  a 3,5×30 chipboard screw, 2026-10-02), nothing records it: the line stays open, rightly, since
  it may be another thing's, but `ev buy for --toured` and `ev add` offer it to the same thing
  again. Expected: a per-thing decline (`ev buy decline <line> <ref>`, like `ev regroup
  --decline`) that keeps the line open for others and drops it from that thing's candidates.
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
