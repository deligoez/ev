# Measuring placement and stems

Two measurements that tell whether a change to matching is better or only different. The
scripts are here; the answer keys are the author's own inventory and stay out of the
repository, in `~/.ev/measure/` (`EV_MEASURE` to move it).

## Stems: `stems.py`

Does `Lexicon::key` give each word the stem a person would? The key is a list of words where
ev's stem and a Turkish morphology tool's differed, each judged by hand
(`stems-judged.tsv`: `surface  ev_key  other_key  verdict  why`, verdict `çözgü`, `ev` or
anything else to skip), and the stems ev gave when it was judged (`stems-baseline.tsv`,
`surface  key`). Words not in the judged list are expected to keep their baseline stem.

    python3 tools/measure/stems.py

It dumps the current stems with the ignored `dump_keys` test (the vocabulary is the
baseline's own words) and prints `right/total  fixed n  broke n` with the words that changed.
A "broke" can be a correct stem the baseline did not have; read the list.

## Placement: `placement.py`

Does each placed item score best in the holder it is in, or in the one a person said it
belongs to? The key (`placement-key.json`) lists the moves a review decided (item name prefix
→ target label prefix), and items left undecided; every other placed item belongs where it
is.

    python3 tools/measure/placement.py <ev> <db> [<ev> <db> ...]

Runs `ev suggest --for <item>` for every placed item, counts an item right when its best
holder is where it belongs, and prints `right/total` per run, so an old and a new build can
be compared on the same copy of a database. Never point it at the live database while a
build with a newer schema is around: opening migrates.
