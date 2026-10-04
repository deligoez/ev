# Codes compare by what a label means, not how it is printed

## Decision

Two codes are the same code when they differ only in:

- case and diacritics (as before: `K4x4-07-Ü` is `k4x4-07-u`);
- the separator: `-` and `_` are one character (`S3-11` is `S3_11`);
- leading zeros of a number (`S03_12` is `S3-12`; `GF2x1_007` is `GF2x1-7`).

That holds for finding a node by its code, for a code being free (a second `S3-12` is refused
when `S3_12` is in use), and for a series: `code=S3-*` and `code=S3_*` continue one series
whichever separator and padding its labels have, and the new code is written as given (the
prefix as typed, the number padded like the series' longest).

A record keeps its code exactly as its label is printed; nothing is rewritten.

## Why

The household's label maker cannot print a hyphen with its current cassette: it stalls at the
character. New labels come out with `_`, the old ones have `-`, and a new box of the `S3` series
was printed `S03_12`. Relabelling every box to one separator is work for nothing: the label is
what the person reads, and ev only has to know `S3_11` and `S3-11` are one box. A leading zero
on a printed number means nothing either, so `S03` reads as `S3`.

## Model

Schema 32: `nodes.code_folded` is recomputed with the new folding; `code` is untouched. Measured
on one household's copy (110 codes, 75 with `-`, 33 with `_`): no two live codes become one.

## Not now

- Any other look-alike: `O` and `0`, `l` and `1`, a space for a separator.
