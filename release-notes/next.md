Draft notes for the next batched release; rename to the tag's version when releasing.

## New

- **Point at a thing in `ev ui`.** `ev focus <ref> [--photo n]` makes a running `ev ui` jump
  to the node in the tree and show that photo full screen (the last one by default, usually the
  crop just cut), so "which one do you mean?" is answered on screen. Each request is shown once;
  `--clear` withdraws it.
- **Closing a record that was a mistake.** `ev gone <ref> --as mistake --why "…"` closes a
  record that should never have existed — a misreading from a photo, a duplicate — keeping its
  history without reporting it as thrown away. It cannot be set aside with `dispose`, and the
  reason is required.
- **The known current state of each place.** `ev todo` lists places whose picture of the current
  state is missing, or whose contents changed after their newest whole-view photo — things
  moved out count too. A new photo of the place takes it off the list; crops do not count, as
  they picture one thing. `ev ui` shows them under Fotoğraf gerekli.
