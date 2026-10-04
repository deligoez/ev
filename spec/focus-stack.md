# A series of marked photos in `ev ui`, numbered by ev, closed by the person

## Decision

Pictures sent to a running `ev ui` — `ev focus --file`, and the ones `ev photo mark` and
`ev photo cut` show on their own — join a **series** instead of replacing what is on screen.

- **The person ends a series**, in `ev ui`, with `X` (close series): the whole series is gone.
  `Esc`/`o` only hide it, and `m` brings it back, so the person can look around in the tree and
  return. Whatever is sent after a close starts a new series; the agent never decides where a
  series starts or ends.
- **ev numbers the frames.** A series hands out numbers in order: `photo mark`'s numbered labels
  (`1=…`, `2 → A6=…`) and `photo cut`'s frames go on from the next free number, so the numbers
  on one series' photos never repeat. After a close they start at 1 again. The agent writes its
  own labels from 1 per photo and quotes the numbers ev returns (`marks`, `legend`).
- **A photo marked again** — a frame fixed — takes its own place in the series and keeps its
  numbers. A photo cut after it was marked does the same, so the cut's frames carry the numbers
  the person was already told. Record ids stay in the cut's legend, never on screen: a cut's
  preview goes to the screen only when it frames a grid's boxes on their cells.
- **A mark that points at frames already numbered** — a placement proposal's destination
  (`ev photo mark <drawer> 4=A6`, frame 4 goes to A6) — takes `--keep-numbers`.
- Each picture is titled with its note and `n/total` in the series.
- `ev focus --list` reads the series: its pictures, their notes and each one's frames (number →
  where it is, or which record). `ev focus --clear` ends it too, for an agent the person asked.

## Why

A batch is talked through over several commands: the marked photos of a batch, then a cut per
photo, each showing its own numbered copy. Each replaced the set, so the person saw only the last
picture while the others had not been talked about; numbers started at 1 on every photo, and a
cut's preview showed record ids where the agent's table said 1–5. The person answers "3 at",
"5 ver": a number must mean one frame in what is on their screen.

## Data model

The request file beside the database (`ev.db-focus.json`) holds the series:

```json
{"files": ["/…/a-marked-1.jpg", "/…/b-numbered-2.jpg"], "notes": ["parts", "drawer"],
 "frames": [[{"n": 1, "at": "0.1,0.1,0.2,0.2"}], [{"n": 2, "ref": {…}, "crop": "…"}]],
 "next": 3, "show": 1, "note": "drawer", "since": "…", "at": "…"}
```

- `files`, `notes` and `frames` run in parallel; `note` is this request's note (older readers).
- `show` is the index of the first picture this request sent; `next` the next free number;
  `since` when the series began.
- A picture's place is its photo: folder and file name without `-<kind>-<ms>` (copies are named
  `<photo>-<kind>-<ms>.jpg`). Pictures no longer on disk drop out when the series is written.
- A node request (`ev focus <ref>`) keeps the series beside `id`.
- `X` in `ev ui` removes the file: the one thing the UI writes, and it is no inventory data.

Answers stay small: `{"focus": {"files": [this request's], "note", "series": n, "next", "at"}}`.

## Not now

- A cap on a series' length.
- Numbers for `ev focus --file` pictures: ev did not draw them, so it does not know them.
