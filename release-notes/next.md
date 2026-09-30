Draft for the next release.

## Changed

- **`ev ui` reopens the tree as it was left.** It already went back to the node that was
  selected; now the nodes that were open stay open and the headings that were closed (the To
  do sections, Unknown place) stay closed, per database, in `ui-state.json`. A node gone since
  is dropped. `ev settings resume off` still starts at the top with the default tree.
