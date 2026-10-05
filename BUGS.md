# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **A root the inventory never inflects is still cut to a shorter written word.** `kapı` →
  `kap`, `veri` → `ver`, `mini` → `min`, `yani` → `yan`, `boya` → `boy`, `powerline` →
  `power`: the word reads as that word plus a valid ending (`kap`+`ı`), and no other written
  form of it shows it is a root. The cases the inventory does show (`altın`, `ünite`, `pense`,
  `cıvata`) and those an ending cannot follow (`varta`, `sabun`, `güneş`, `türkiye`) were fixed
  in v0.18.0. Expected: a root stays whole. The rest needs a dictionary and waits for
  Çözgü's embeddable core. Measure with `tools/measure/stems.py` (1,736/1,919 after the fix).
- **Cell frames drift on tall boxes.** On a drawer photo with its grid corners kept, the frames
  drawn for 1x2 boxes sat a little low on the top edge (perspective). Minor. (Reported by the
  inventory agent.)
- **`ev layout --propose` drafts from places nobody has counted.** On a Kallax where 10 of 32
  drawers are still `raw`, 15 of the draft's 17 moves take things out of seven uncounted
  drawers (recorded from photos only), and two uncounted drawers get themes; neither
  `ev layout` nor `--propose` says that part of the furniture is not toured. Records of an
  untoured place are guesses (the skill says not to act on them before its tour). Expected:
  a line naming the untoured places, and moves from or themes for them left out or marked
  as guesses. (QA round 2.)
- **`ev layout`: a dash or an opening comma still picks the wrong kind.** `USB-A — mini USB
  kablo` reads as kind `usb` (cut at the dash), and `USB-A, USB-C, Lightning kablolar` as
  `usb` (cut at the first comma, though the name ends in what it is). So three USB cables form
  a kind of their own and the draft sends a USB-C cable to the "usb" drawer, not to the drawer
  it themes "kablo". Plural and singular of one compound also land apart: `Şarj adaptörleri`
  joins `adaptörü` (with display adapters and measuring-tool attachments, themed onto the
  antenna drawer) while `Güç adaptörü` is its own kind in another drawer. Expected: the head
  noun of the whole name (the last noun before a comma that ends a list, not the first word),
  a dash not cutting a name, and `adaptörleri` / `adaptörü` of one compound read as one. (QA
  round 2.)
- **`ev layout` spread rows say how many, never which.** `spread[].places[].things` is a count,
  so checking why `usb` or `adaptörü` became a kind meant listing each drawer's tree and
  guessing which names fed it. Expected: the record ids (or a `--why` that names them).
  (QA round 2.)
- **A series photo marked in place loses the person's note.** `ev focus --file a.jpg="<the
  person's words>"` (f1), then `ev photo mark f1 1=… --show "two joysticks"`: f1's title is now
  the agent's `--show`, and `ev photo add #12 f1` attaches the photo with that title as its
  note, not the note it was sent with (the skill: "each keeps the note it was sent with").
  Expected: the sent note stays the photo's note; the mark's title only titles the frames.
  (QA round 2.)
- **JSON left-overs after the noise fix.** `ev next --json`: every place still carries
  `tracking: {}`, and the drawers of a compartment `cells: []`; each sibling repeats its
  holder's whole grid as `parent_grid` (1.4 KB on each of three drawers), so a nine-place task
  is still 63 KB. `ev photo add #12 f1` answers with every photo of #12 (eight here) instead of
  the one added. `ev focus f2` answers without the `f` it showed. Expected: empty fields left
  out, the holder's grid once (or not in `next`), the new photo only, and `f` echoed. Minor.
  (QA round 2.)
- **`ev kit drop` miscounts in its refusal.** After `ev kit drop 1 37`, the parts are numbered
  1–36 and 38 (numbers kept, fine), and `ev kit drop 1 99` answers "part 99 does not exist; the
  kit has 37", though 38 is a real part. Expected: "the kit's parts are 1–36, 38" or the
  highest number. Minor. (QA round 2.)
