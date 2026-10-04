Draft for the next release.

## The marked photo series

Pictures sent to `ev ui` no longer replace each other. A batch is talked through over several
commands — the marked photos, then a cut per photo — and each one used to take the screen, so the
person was left with the last picture while the others had not been talked about. Now everything
shown (`ev photo mark`, `ev photo cut` and its preview, `ev focus --file`) joins one **marked photo
series**, stepped with `[` `]`, each picture titled with its own note and `n/total`.

The person owns the series: `Esc` only hides it and `m` brings it back, so they can look around in
the tree and return; `X` closes it, and whatever the agent sends next starts a new one. The agent
never decides where a series starts or ends.

ev owns the numbers. Within a series a frame's number is unique until the series is closed, so
the person can answer "3 at, 7 ver" across every photo on screen:

- `photo mark` counts each photo's labels from 1 and draws the frame's number in the series: the
  second photo's `1=… 2=…` draws `3` and `4`. The marks returned carry the numbers drawn.
- A photo marked again (a frame fixed) takes its own place in the series and keeps its numbers; a
  frame added to it takes the next free number. Nothing is renumbered.
- A cut of a photo marked in the series draws the numbers it was marked with, so the cut's frames
  match what the person was already told. Record ids stay in the legend; the preview goes to the
  screen only when it frames a grid's boxes on their cells.
- `--keep-numbers` on `photo mark` draws numbers as given, for a placement proposal's destination
  (`4=A6`: frame 4 goes to A6).
- `ev focus --list` reads the series: each picture, its note, its frames (number → where, or which
  record) and the next free number. `ev focus --clear` still closes it from outside.

The design is in `spec/focus-stack.md`.

## Frames read on red things

Marked frames and their number plates are edged in dark. A red frame on a red box or a roll of red
tape used to disappear into it.
