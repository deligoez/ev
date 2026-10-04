# Codes compare by what a label means, not how it is printed

## Decision

Two codes are the same code when they differ only in:

- case and diacritics (as before: `K4x4-07-Ü` is `k4x4-07-u`);
- the separator: `-` and `_` are one character (`S5-11` is `S5_11`);
- leading zeros of a number (`S05_12` is `S5-12`; `G1x1_007` is `G1x1-7`).

That holds for finding a node by its code, for a code being free (a second `S5-12` is refused
when `S5_12` is in use), and for a series: `code=S5-*` and `code=S5_*` continue one series
whichever separator and padding its labels have, and the new code is written as given (the
prefix as typed, the number padded like the series' longest).

A record keeps its code exactly as its label is printed; nothing is rewritten.

## Why

The household's label maker cannot print a hyphen with its current cassette: it stalls at the
character. New labels come out with `_`, the old ones have `-`, and a new box of the `S5` series
was printed `S05_12`. Relabelling every box to one separator is work for nothing: the label is
what the person reads, and ev only has to know `S5_11` and `S5-11` are one box. A leading zero
on a printed number means nothing either, so `S05` reads as `S5`.

## Model

Schema 32: `nodes.code_folded` is recomputed with the new folding; `code` is untouched. Measured
on one household's copy (110 codes, 75 with `-`, 33 with `_`): no two live codes become one.

## Not now

- Any other look-alike: `O` and `0`, `l` and `1`, a space for a separator.
