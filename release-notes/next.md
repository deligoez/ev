Draft for the next release.

## `ev ui`: a sidebar of lists instead of top tabs

Ten top tabs were already too many, and more lists are coming (purchases by bucket, repairs and
maintenance, people, the lists behind the statistics). `ev ui` now has three panes: a sidebar of
lists grouped under headings (HOME, HISTORY, INSIGHT), the list chosen, and the selected record's
details, whose tabs are now the only tab strip on screen (spec/ui-sidebar.md).

- Every list in the sidebar has the digit that opens it and, where one number says it, how many
  it holds, counted by building the list itself so the two never disagree. The digits follow the
  sidebar: 1 layout, 2 to do, 3 pending, 4 leaving, 5 lost, 6 errands, 7 past, 8 statistics,
  9 purchases, 0 settings.
- The purchases have lists of their own: every line on `9`, and one list per bucket (durable,
  clothing, digital, service). An order's lines sit under one heading with the order's total,
  the title adds up what is shown per currency, `f` narrows to open, linked or dismissed lines
  and `/` to lines with the words typed, live. The details are the line as `ev buy show`
  writes it, and Enter opens the thing it is linked to in the tree. Until now the purchases
  were only reachable from the command line.
- A figure on the Statistics list that counts a list is marked `›`, and `Enter` opens that
  list: a year's or a shop's purchase lines (the list's title says which), the To do list from
  the counting, the past from what left. `Esc` comes straight back to the figure.
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

## The series grid fills the screen

The marked photo series' grid made every row as tall as a 4:3 picture of the tile's width, and
what was left under the last row that fit stayed empty: about a fifth of a tall screen. The
rows that fit now share that height (a short series takes the whole height in the rows it
needs), so the usual portrait photo is drawn taller, and a picture sits centred in its tile.

## A series knows which place it is about

A batch of one drawer's photos joined the series still open from the previous drawer, so its
frames were numbered 16–32 while its pictures were f16–f34, and "16" and "f16" named different
things; nothing had told the agent that a new batch had begun. A marked photo series now
remembers the place its first picture was about (the place `ev photo mark`, `ev photo cut
--place` or `ev photo add` named, or `ev focus --file … --for <place>`). A picture about another
place still joins the series, since closing it is the person's call, but the answer says so with
`series_about: {about, now}`, for the agent to ask whether to close the series first, and the
picture's title in `ev ui` names both places (spec/series-batches.md).

## Errors with ids

An error now can carry a stable id and its values, so the text output can say it in the
reader's language and an agent can match on it rather than on an English sentence
(spec/error-ids.md, decided with the person for ev and ak alike). The JSON adds `id`, `values`
and `at` (where it happened, such as a batch line, kept apart from the sentence); `code`,
`kind` and the English `message` are as before. The errors every command meets first come
first: a reference that matches nothing, several things, or a thing gone or joined to
another. Then every refusal (exit 5), every request written wrong (exit 2) and every "not
found" (exit 3) got one: 292 ids in all. Four errors keep an English sentence for now,
three of them because they pass a library's own error on; a test keeps that number from
growing.

## Purchase lines by source and key

ak, the household's money tool, sends ev the lines that are things, keyed by its payment
(`412`), or by the payment and the item's place in it (`412.2`) when only some lines of a
receipt are things (spec/ak.md). To answer which thing a payment bought, `ev buy list` takes
`--source ak` and `--key 412`, which finds the payment's own line or its items' lines;
`--key 412.2` finds that item alone.

## Faster

- `ev ui` started and refreshed most of a second slower once the sidebar counted the purchase
  lines, since it read every line for it, at start and again on every change made elsewhere.
  It now counts them in the database (`buy_counts`), and reads the lines only while a purchase
  list is open.
- `ev buy list` looked up every line's joined lines with a scan of the whole table, three
  times a line: 0.47 s for one household's ~1,840 lines, 0.11 s now. The index it needed is
  added the first time a file is opened; no schema version depends on it, so an older ev still
  opens the file.

## Fixes

- A frame `ev photo mark` drew on a grid's cells ran along the cells on the floor of the drawer,
  so on a tall box (`size` higher than 1) its top edge sat inside the box's rim, which a photo
  from above sees leaning out. Cells that are exactly one placed box's are now framed out to its
  rim, the way `ev photo cut --grid` already cropped it.

- `ev find` compared a code as written: a box coded `X5_13` was not found by `ev find X5-13`,
  and listing a series with `ev find "X5-"` left out its `X5_` labels, so an agent proposed a
  code already in use as the next free one. A code is now matched as references match it, `-`
  and `_` one and the padding of its numbers aside, as `ev show` and the next free code of a
  series already did.

- A place toured, then changed in the same second, could not be toured again: the check
  compared times kept to the second. It compares the order of the events now. A faster machine
  (CI) hit it; one at home could have too, with an agent writing quickly.
