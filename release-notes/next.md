Draft for the next release.

## Changed

- **`ev cell` no longer takes `--recode`.** It renamed each placed box after its cell
  (`<holder code>-<back-left cell>`), which fits a code that says where a box is. Movable boxes
  now carry serial codes (`GF1x2-003`, `S5-01`) that stay on their labels wherever the box
  goes, so a box keeps its code when `ev cell` moves it. `ev recode A=X B=Y …` still swaps or
  rotates codes when that is really wanted.
