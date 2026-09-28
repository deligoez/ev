# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **`ev suggest` matches word fragments both ways.** "boş" matched "Bosch" because `bos` is inside
  `bosch`; a query word should match the start of a word, not any substring in either direction.
  Seen 2026-09-29.
- **Uninventoried boxes read as empty.** A container whose contents were never recorded (the two
  cardboard boxes) shows 0 items, the same as a truly empty box. A flag such as
  `contents: unknown` would keep `suggest` from offering it as free space.
- **`ev audit` groups words, not stems.** `vida`/`vidası` and `kablolar`/`kablosu` show up as
  separate words; a light Turkish suffix stripper would merge them. Low priority: the agent
  reads the list and merges them itself.
