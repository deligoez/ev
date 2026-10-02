Draft for the next release.

## New

- **A cut shows what it recognised** (`ev photo cut … --show`). Every cut and `--preview` now
  also draws `marked`: the whole photo with a numbered red frame on each crop, numbered from 1
  in the order the crops were given and then the grid's boxes, with bare numbers as labels. Its
  `legend` (`[{n, ref, crop}]`) says which record each number is, and the text output lists it
  (`1  #484 …`). `--show` sends the numbered photo to a running `ev ui` as `ev focus --file`
  does, titled with `--note` or else with each number and its code or name. So an agent that
  cuts a photo among records shows the person what it read in the same command, instead of
  having to remember a separate `ev photo mark` and `ev focus`. Sending stays opt-in: a cut can
  come from a script, and a request replaces the picture the person may be looking at.

## Fixed

- **`ev photo mark` drew a long label so large it covered the frames.** Labels like
  `"1 LR44 Mettzchrom x17 (#484)"` on a phone photo came out as full-width red bars over the
  parts and over each other. A label is now no wider than its frame (or an eighth of the photo,
  so a number on a small frame stays legible): a long one is drawn smaller, down to a third of
  the photo's size, and broken onto up to three lines at spaces. It goes above its frame, else
  below it, else inside it, at the first place that stays in the photo and off every other label
  and frame, and all frames are drawn before any label, so no frame line crosses a label.
