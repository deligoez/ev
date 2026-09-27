# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **`ev disposals` lists the parts of a set as separate rows.** A dock set disposed together with
  its adapter and cables shows four rows under `sell`, and a DVD writer shows its cable under
  `give`. Expected: only the outermost candidate of a set is listed, with its parts nested or
  counted, since they leave together (spec §12.1). Seen 2026-09-27 on the real inventory.
- **Release job is slow (~3 min).** GoReleaser re-runs `cargo test` that CI already ran, and the
  Rust toolchain and target builds are not cached. Drop the test hook and add a Rust cache.
