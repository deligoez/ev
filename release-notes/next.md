Draft for the next release.

## Changed

- **`ev cell` no longer takes `--recode`.** It renamed each placed box after its cell
  (`<holder code>-<back-left cell>`), which fits a code that says where a box is. Movable boxes
  now carry serial codes (`GF1x2-003`, `S5-01`) that stay on their labels wherever the box
  goes, so a box keeps its code when `ev cell` moves it. `ev recode A=X B=Y …` still swaps or
  rotates codes when that is really wanted.

## Fixed

- **`ev next` and `ev todo` said "null being counted".** The summary they print copied every
  count of `ev progress` but the places being counted, so the readable output showed `null`
  and the JSON had no `counting`. Both now carry it.

## Maintenance

- The quality gate adds `cargo shear` (unused dependencies, and `.rs` files no module
  declares) and runs the tests with `cargo nextest`, and clippy now fails on a function over
  300 lines. The large source files are split into modules (`ui/`, `store/`, `placement/`),
  moved line for line, and `tools/deadcode.sh` lists dead code across both crates.
- The readable output has tests of its own, one per shape the agent reads daily (`show`,
  `next`, `todo`, `suggest`, `regroup`, `tree`), which fail on any `null` in the text:
  `render.rs` coverage went from 16% to 41%. Three kit rules no test held (found by
  `cargo mutants`) have tests too; every mutant of `kits.rs` is now caught.
