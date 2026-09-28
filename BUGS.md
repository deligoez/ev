# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **`ev for` lists a node twice when `to` equals `owner`.** The org stand parts belong to
  Mahmutlar (`owner`); adding `to=Mahmutlar` put each of them under both `take` and `return`.
  Expected: one entry — returning a thing to its owner already means taking it there — or
  `edit` refusing `to=` equal to the owner with a hint that `return` covers it.
