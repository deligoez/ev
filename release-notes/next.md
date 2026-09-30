Draft for the next release.

## Changed

- **`ev ui` reopens the tree as it was left.** It already went back to the node that was
  selected; now the nodes that were open stay open and the headings that were closed (the To
  do sections, Unknown place) stay closed, per database, in `ui-state.json`. A node gone since
  is dropped. `ev settings resume off` still starts at the top with the default tree.

## Fixed

- **A sketched room or piece of furniture moved to another holder stays where it lies on the
  map.** Its place and outline are written in its holder's frame, and a move left them there:
  two balconies moved out of their rooms to the home landed on the home's top-left corner. A
  move now translates them into the new holder's frame; when either frame is unknown (a holder
  on the way up has no place), the sketch is left as it was.
