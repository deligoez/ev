# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **A root the inventory never inflects is still cut to a shorter written word.** `kapı` →
  `kap`, `veri` → `ver`, `mini` → `min`, `yani` → `yan`, `boya` → `boy`, `powerline` →
  `power`: the word reads as that word plus a valid ending (`kap`+`ı`), and no other written
  form of it shows it is a root. The cases the inventory does show (`altın`, `ünite`, `pense`,
  `cıvata`) and those an ending cannot follow (`varta`, `sabun`, `güneş`, `türkiye`) are fixed
  for the next release. Expected: a root stays whole. The rest needs a dictionary and waits for
  Çözgü's embeddable core. Measure with `tools/measure/stems.py` (1,736/1,919 after the fix).
- **Photo and document paths are stored absolute.** `photos.path` and `documents.file` hold
  `<data dir>/photos/<hash>.jpg`, not a path relative to the store, so a data directory that
  moves (another Mac, another user name, a copy opened with `--db`) points back to the old
  place: a copy of the inventory used for QA showed and marked the real store's photos.
  Expected: files inside the store are kept relative to the database's directory and resolved
  on read; paths outside it (not yet adopted) stay absolute. Needs a schema migration.
