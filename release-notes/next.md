Draft notes for the next batched release; rename to the tag's version when releasing.

## Fixed

- **`ev ui`'s places tab is now called "Götür/İade".** "Yerler" did not say what the tab is for:
  what to take somewhere, what to return, and what we lent out.
- **`ev audit` no longer lists disposal sets as holders without a theme.** A dock being sold
  holds its cables, but it is not a place to put things.
- **`ev suggest` matches the start of words, not fragments.** "boş" no longer finds "Bosch";
  a query word of four letters or more still finds its suffixed forms ("vida" → "vidası").

## New

- **Contents unknown.** `ev edit X unknown=true` (or `ev add --unknown`) marks a holder whose
  contents were never inventoried, so an empty count is not read as free space. `suggest` and the
  tree mark it, and `ev audit` lists every such holder under `unknown`.
- **Correcting a mistaken `gone`.** `ev restore X --correction "<why>"` brings a node recorded as
  gone by mistake back to active; the reason stays in its history. A plain `restore` still only
  returns a candidate.

## Upgrade notes

- **Schema version 4** (the `unknown` flag), migrated on open.
