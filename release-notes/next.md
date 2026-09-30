Draft for the next release.

## Changed

- **`ev ui` reopens the tree as it was left.** It already went back to the node that was
  selected; now the nodes that were open stay open and the headings that were closed (the To
  do sections, Unknown place) stay closed, per database, in `ui-state.json`. A node gone since
  is dropped. `ev settings resume off` still starts at the top with the default tree.
- **A mark before each row in `ev ui` says its kind:** ⌂ home, ◫ room, ▥ furniture, □ box,
  · thing, in the tree, the Contents tab and the lists. A box and the things in it differed
  only by a code; now they read apart at a glance. Plain one-cell shapes, so every terminal
  draws them the same width.
- **A settled row turns green in `ev ui`**, from its mark to its name. Settled means counted,
  with nothing in `ev todo` hanging on it or on anything in it (no task, planned move out or in,
  disposal, label, photo, unclear name, …) and no observation waiting. A drawer turns green
  once every box in it has, and so on up to the home, so the tree reads as a map of what is
  finished. The name's colour now says only this; the kind is left to the mark, so furniture
  names lose their own colour, a quantity (`×3`) is no longer green, and a settled row drops
  its `[counted]` tag.

## Fixed

- **A sketched room or piece of furniture moved to another holder stays where it lies on the
  map.** Its place and outline are written in its holder's frame, and a move left them there:
  two balconies moved out of their rooms to the home landed on the home's top-left corner. A
  move now translates them into the new holder's frame; when either frame is unknown (a holder
  on the way up has no place), the sketch is left as it was.
