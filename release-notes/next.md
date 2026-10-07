Draft for the next release.

## `:` runs commands too

The palette that finds any list or record in `ev ui` now also runs the screen's own commands:
type a few letters of "map", "rotate" or "two levels" and `Enter` does what the key would. Each
command shows its key at the right edge, so the palette teaches the keys that the help line has
no room for on a narrow screen. A query starting with `>` finds commands only, and only those
that apply where you are: the tree's commands on the tree, the photo's when a photo is shown,
the filter on a purchase list. Every command is one entry of a single table (its name, its key,
when it applies), and running one presses that key, so the palette and the keys cannot drift
apart. Nothing in it writes to the inventory.

## A crash of `ev ui` leaves the terminal clean

When `ev ui` panicked, ratatui put the screen and the keyboard back, but the mouse reports and
the light/dark reports that `ev ui` had turned on itself stayed on, so the shell went on
printing escape codes at every mouse move. A panic now turns both off before ratatui restores
the rest, the same way a normal exit does.
