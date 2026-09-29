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
  state is missing, or whose contents changed after their newest photo — things moved out count
  too. A new photo of the place takes it off the list, and so does a crop attached to the place
  itself (a box cut out of a drawer photo); crops on the things inside do not. `ev ui` shows them
  under Fotoğraf gerekli.
- **Rotate a photo in `ev ui`.** `r` turns the photo on screen 90° clockwise and `R`
  counter-clockwise, full screen or on the photo panel. It is view-only: the turn is kept per
  photo until `ev ui` quits, and neither the photo file nor the database is changed.
- **"The photo is still fine."** `ev photo current <ref>` takes a place off the photo-needed
  list after a small change (one thing taken out), until the next change.
- **A group photo cannot end up on every thing in it.** `ev photo add` refuses a whole
  photo that is already attached whole to another node (exit 5, naming the nodes): the things in
  a drawer or box photo get `--crop`s, and the whole view belongs to the place. `--whole` is the
  deliberate exception. `ev todo` and the Yapılacak tab list any whole photo shared by several
  records, so older slips of this kind show up too.
- **Codes move with boxes.** `ev recode A=X B=Y …` gives several nodes new codes in one step,
  checking uniqueness against the codes they end up with, so codes can be swapped or rotated when
  boxes trade places in a grid. `ev edit code=` still refuses a code that is in use, which made a
  rotation need a throwaway code. Every changed code needs its label printed again. All or
  nothing.
