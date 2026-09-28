Draft notes for the next batched release, planned as v0.4.0; rename to `v0.4.0.md` when
releasing.

## Fixed

- **`ev ui`'s places tab is now called "Götür/İade".** "Yerler" did not say what the tab is for:
  what to take somewhere, what to return, and what we lent out.
- **`ev audit` no longer lists disposal sets as holders without a theme.** A dock being sold
  holds its cables, but it is not a place to put things.
- **`ev suggest` matches the start of words, not fragments.** "boş" no longer finds "Bosch";
  a query word of four letters or more still finds its suffixed forms ("vida" → "vidası").
- **`ev audit` groups Turkish word forms.** "vida", "vidası" and "vidalar" were three rows of
  `spread`; now they are one, named by the shortest form, with every merged form under `forms`.
  A form joins a stem only when that stem is a word of its own or shared by two forms, so "kutu"
  stays "kutu" and "kartuşu" does not fall into "kart".
- **A gone node can still be annotated.** After `ev gone 243`, `ev edit 243 note=…` exited 3 as
  if the node never existed. Its id now reaches it for the note — the only field that may change
  on a gone node — and the error for a gone id says how to still see it (`show --include-gone`,
  `history`).

## New

- **Photos live in ev.** `ev photo add` copies a photo into `~/.ev/photos` (hash-named, so the
  same photo is stored once) and attaches it; `ev photo list|remove`, and `ev photo adopt` pulls
  in photos older records still point to outside the store.
- **Crops.** `ev photo add <item> <drawer photo> --crop x,y,w,h` attaches just that part of a
  drawer photo and remembers the original, so one photo can illustrate every box in a drawer.
- **Photos in `ev ui`.** The details pane shows the selected node's photo — through Ghostty's (and
  other terminals') graphics protocol, or half-blocks elsewhere. `[` `]` or the wheel over the
  photo step through a node's photos.
- **Full-screen photos in `ev ui`.** `o` (or a click on the photo) fills the terminal with the
  current photo, titled with the node and the photo's note; `[` `]` step, Esc closes. `O` opens it
  in the system viewer instead — the same `o` / `O` split terminal file managers use.
- **Clearing a search in `ev ui`.** On the Ara tab, `x`, Esc or a click on the list title
  (`✕ temizle`) drops the query and its results; Esc no longer quits while there is a search to
  clear. In the search box, Esc first empties the typed text and Ctrl-U clears it.
- **Why it left.** `ev gone X --why "<text>"` records the reason in the event and on the note in
  the same step.
- **Contents unknown.** `ev edit X unknown=true` (or `ev add --unknown`) marks a holder whose
  contents were never inventoried, so an empty count is not read as free space. `suggest` and the
  tree mark it, and `ev audit` lists every such holder under `unknown`.
- **Correcting a mistaken `gone`.** `ev restore X --correction "<why>"` brings a node recorded as
  gone by mistake back to active; the reason stays in its history. A plain `restore` still only
  returns a candidate.

## Upgrade notes

- **Schema version 5** (the `unknown` flag, then photo provenance), migrated on open.
