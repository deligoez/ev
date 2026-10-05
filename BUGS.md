# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **A root the inventory never inflects is still cut to a shorter written word.** `kapı` →
  `kap`, `veri` → `ver`, `mini` → `min`, `yani` → `yan`, `boya` → `boy`, `powerline` →
  `power`: the word reads as that word plus a valid ending (`kap`+`ı`), and no other written
  form of it shows it is a root. The cases the inventory does show (`altın`, `ünite`, `pense`,
  `cıvata`) and those an ending cannot follow (`varta`, `sabun`, `güneş`, `türkiye`) were fixed
  in v0.18.0. Expected: a root stays whole. The rest needs a dictionary and waits for
  Çözgü's embeddable core. Measure with `tools/measure/stems.py` (1,736/1,919 after the fix).
- **Cell frames drift on tall boxes.** On a drawer photo with its grid corners kept, the frames
  drawn for 1x2 boxes sat a little low on the top edge (perspective). Minor. (Reported by the
  inventory agent.)
- **Starting a task marks its places "counting" before anyone looks.** `ev task start` writes a
  `counting` review on every place of the task at once (`plan.rs`, the `doing` branch). A
  nine-drawer task turned nine unopened drawers into "sayılıyor", and progress read "14
  counting" for places nobody had opened; the person believed a piece of furniture was nearly
  done when nine drawers had never been toured. Expected: a place becomes counting when work on
  it starts (a photo, an add or edit inside it, a review), or a distinct "planned" state; the
  task alone says nothing about a place. (Reported by the inventory agent; the person calls it
  critical.)
- **Finishing the last place of a task in a piece of furniture names nothing left there.** An
  in-progress task keeps winning `ev next`, and when its last place in a furniture is reviewed
  nothing says "this furniture still has N places not counted, held by tasks X, Y, and one by
  none"; an unplanned drawer of the very furniture being toured sat in `unplanned` the whole
  time. Expected: the review (and `ev next`) names the furniture's (and room's) remaining
  uncounted places with their tasks, unplanned ones included. (Reported by the inventory agent.)
- **No scoped progress.** `ev progress` takes no place: "what is left in K4x4" needs the whole
  list filtered by hand. Expected: `ev progress <furniture|room>`, each place with its state
  (counted, counting, not counted) and its task. (Reported by the inventory agent.)
