# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **`ev audit` lists disposal sets as holders without a theme.** A dock set being sold holds its
  cables, so it counts as a holder; candidates (and their parts) should be left out of
  `no_theme`. Seen 2026-09-28.
- **`ev audit` groups words, not stems.** `vida`/`vidası` and `kablolar`/`kablosu` show up as
  separate words; a light Turkish suffix stripper would merge them. Low priority: the agent
  reads the list and merges them itself.
