Draft for the next release.

## Changed

- **`ev cell` no longer takes `--recode`.** It renamed each placed box after its cell
  (`<holder code>-<back-left cell>`), which fits a code that says where a box is. Movable boxes
  now carry serial codes (`GF1x2-003`, `S5-01`) that stay on their labels wherever the box
  goes, so a box keeps its code when `ev cell` moves it. `ev recode A=X B=Y …` still swaps or
  rotates codes when that is really wanted.

## Maintenance

- The quality gate adds `cargo shear` (unused dependencies, and `.rs` files no module
  declares) and runs the tests with `cargo nextest`, and clippy now fails on a function over
  300 lines. The large source files are split into modules (`ui/`, `store/`, `placement/`),
  moved line for line, and `tools/deadcode.sh` lists dead code across both crates.
