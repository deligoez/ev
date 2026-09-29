Draft for the next release.

## Fixed

- **`ev regroup` no longer proposes a thing as its own better place.** A kit recorded as an
  item with its parts inside (the LiPo box with its Seeed board, the DVD writer with its
  cables) is a holder too, and it used to come out best for itself. Its own subtree is now
  never a candidate, as with `ev suggest --for`.
- **Colours count with the noun they describe.** A colour alone now weighs 0.3 in a query, and
  a colour followed by a noun is also matched as a pair (`yeşil LED`), so a green heat gun no
  longer lands in the green LED box while green and yellow LEDs still find theirs.
- **Noun compounds match as compounds.** A bare noun followed by a noun whose only ending is
  the 3rd-person possessive (`hesap makinesi`, `kablo bağı`, `kontrol kalemi`) is also matched
  as a two-word term, so "hesap makinesi" finds the calculator rather than the machine-screw
  box. A two-word term adds to the score but never on its own makes `regroup` flag a move.
