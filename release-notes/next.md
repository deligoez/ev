Draft for the next release.

## `ev ui`: a sidebar of lists instead of top tabs

Ten top tabs were already too many, and more lists are coming (purchases by bucket, repairs and
maintenance, people, the lists behind the statistics). `ev ui` now has three panes: a sidebar of
lists grouped under headings (HOME, HISTORY, INSIGHT), the list chosen, and the selected record's
details, whose tabs are now the only tab strip on screen (spec/ui-sidebar.md).

- Every list in the sidebar has the digit that opens it and, where one number says it, how many
  it holds, counted by building the list itself so the two never disagree. The digits follow the
  sidebar: 1 layout, 2 to do, 3 pending, 4 leaving, 5 lost, 6 errands, 7 past, 8 statistics,
  0 settings; 9 is kept for the purchases.
- `Tab` moves between the panes, and the one the keys go to has a coloured border. In the
  sidebar the arrows open each list as they pass it; in the details `j`/`k` scroll and `h`/`l`
  step the tabs.
- The layout follows the width, since the person often works from a phone: the full sidebar
  from 120 columns, a rail of digits and counts from 90, hidden below that (`b` lays it over the
  list), and one pane at a time below 70. `b` hides or shows it at any width and is remembered.
- `:` opens a palette that finds any list, or any record by code, name or `#id`, typed with or
  without Turkish letters (`kayip` finds Kayıp). `Esc` steps back through what was opened, a
  list or a record jumped to, and the top line says where it goes; with nothing behind it quits
  as before.
- When the record selected in a list leaves it (made, found, gone by another process), its
  neighbour is selected and the status line says which record left, instead of the selection
  jumping to the top.

## Fixes

- A place toured, then changed in the same second, could not be toured again: the check
  compared times kept to the second. It compares the order of the events now. A faster machine
  (CI) hit it; one at home could have too, with an agent writing quickly.
