# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **A root the inventory never inflects is still cut to a shorter written word.** `kapı` →
  `kap`, `veri` → `ver`, `mini` → `min`, `yani` → `yan`, `boya` → `boy`, `powerline` →
  `power`: the word reads as that word plus a valid ending (`kap`+`ı`), and no other written
  form of it shows it is a root. The cases the inventory does show (`altın`, `ünite`, `pense`,
  `cıvata`) and those an ending cannot follow (`varta`, `sabun`, `güneş`, `türkiye`) are fixed
  for the next release. Expected: a root stays whole. The rest needs a dictionary and waits for
  Çözgü's embeddable core. Measure with `tools/measure/stems.py` (1,736/1,919 after the fix).
- **nextest now and then calls a CLI test leaky.** Over 25 full runs (2026-09-30), 7 runs
  showed one `LEAK`, each time a different test of `cli/tests/cli.rs` (five tests in all), all
  of them spawning the `ev` binary: the child's output pipe closed after nextest's leak
  timeout. The tests pass. Most likely the spawned process's teardown under a fully parallel
  run rather than anything a test leaves behind, but not shown. Expected: a known cause, or a
  `leak-timeout` in `.config/nextest.toml` chosen from a measurement.
