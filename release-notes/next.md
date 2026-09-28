Draft notes for the next batched release, planned as v0.4.0; rename to `v0.4.0.md` when
releasing.

## Fixed

- **`ev ui`'s places tab is now called "Götür/İade".** "Yerler" did not say what the tab is for:
  what to take somewhere, what to return, and what we lent out.
- **`ev audit` no longer lists disposal sets as holders without a theme.** A dock being sold
  holds its cables, but it is not a place to put things.
- **`ev suggest` matches the start of words, not fragments.** "boş" no longer finds "Bosch";
  a query word of four letters or more still finds its suffixed forms ("vida" → "vidası").

## New

- **Photos live in ev.** `ev photo add` copies a photo into `~/.ev/photos` (hash-named, so the
  same photo is stored once) and attaches it; `ev photo list|remove`, and `ev photo adopt` pulls
  in photos older records still point to outside the store.
- **Crops.** `ev photo add <item> <drawer photo> --crop x,y,w,h` attaches just that part of a
  drawer photo and remembers the original, so one photo can illustrate every box in a drawer.
- **Photos in `ev ui`.** The details pane shows the selected node's photo — through Ghostty's (and
  other terminals') graphics protocol, or half-blocks elsewhere. `[` `]` step through photos, `o`
  opens one in the system viewer.

- **Contents unknown.** `ev edit X unknown=true` (or `ev add --unknown`) marks a holder whose
  contents were never inventoried, so an empty count is not read as free space. `suggest` and the
  tree mark it, and `ev audit` lists every such holder under `unknown`.
- **Correcting a mistaken `gone`.** `ev restore X --correction "<why>"` brings a node recorded as
  gone by mistake back to active; the reason stays in its history. A plain `restore` still only
  returns a candidate.

## Upgrade notes

- **Schema version 5** (the `unknown` flag, then photo provenance), migrated on open.
