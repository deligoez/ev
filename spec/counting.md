# Counting follows the work, and what is left is named

## Decision

A place becomes "being counted" (`counting`) when work in it starts, never because a task about
it was started. Starting, stopping or closing a task changes no place's count. When a place is
counted, ev names what is still not counted around it: in the same piece of furniture and in the
same room, each with the tasks it is in, or none. `ev progress <place>` reads one piece of
furniture or one room on its own.

This replaces §32.3 of spec/0.1.0.md ("starting a task marks its places `counting`; closing the
task … puts what was not back to `raw`").

## Why

The person believed a piece of furniture was done except one drawer; nine of its drawers had
never been opened. Starting a task about them had marked all nine `counting` at once, and an
earlier task had done the same to shelves nobody had looked at, so progress read "14 being
counted" for places no one had touched. Meanwhile a drawer of that same furniture that belonged
to no task sat in a global "unplanned" list, never shown next to the furniture being toured, and
the task in progress kept leading `ev next` without saying what else was left there. The person
calls knowing each container's state ev's main job; this was the opposite.

## What counts as work in a place

A not-counted place (`raw`, inherited from nothing above it) becomes `counting` the moment one of
these is written for it or for something inside it:

| Event | Counts when |
|---|---|
| `photo` | on the place itself or on anything inside it: opening a drawer to photograph it is work in it |
| `create`, `move`, `done`, `gone`, `found`, `cell`, `split`, `portion_in`, `portion_out`, `more_of`, `edit` | on something inside the place, not on the place itself: making a new drawer record, moving a box from one shelf to another, or re-coding a drawer is not opening it |

Every other event (a document linked, a label printed, a task changed, a review, a coverage, a
purchase linked) leaves the count as it was. Only `raw` places change: a `toured` or `kept`
place stays as it is (its "changed since" already says what moved). The change is an ordinary
`review` event, `{as: "counting", by: <event type>}`, so the history says why.

Nothing else marks a place: not a task start, not a task close. A place marked `counting` stays
so until it is `toured`, `kept`, or set back with `ev review <place> --as raw` on the person's
word.

## What is left, named

- **After a review:** `ev review <place> --as toured|kept` adds `left_here`:
  `{furniture: {node, places: [...]}, room: {node, places: [...]}}`, the places not counted
  (`raw` or `counting`) in the same piece of furniture and in the same room, each
  `{node, status, tasks: [{id, title}]}` (empty `tasks`: in no task). Either part is null when
  there is none, or nothing is left in it. The text says "Bu mobilyada sayılmamış N yer kaldı"
  with each place and its task.
- **In `ev next`:** while a task is in progress, `left_nearby` lists the places not counted in the
  furniture its places are in that are not in that task, each with its tasks: the drawer of the
  same cabinet that no task holds shows up while the cabinet is being toured.
- **`ev progress <place>`:** only the places inside a piece of furniture or a room (or the
  place itself, when it is one), with the same counts as `ev progress`, and each place's
  `tasks`. `ev progress` without a place is as before, each place now carrying its `tasks` too.

## Data

No schema change. `counting` is the same row in `reviews` it always was; what changes is who
writes it.

## Phases

1. Work marks a place `counting`; task start and close mark nothing.
2. `ev progress <place>`, and `tasks` on every place of `ev progress`.
3. `left_here` after a review; `left_nearby` in `ev next`.

## Not now

- A separate "planned" state: a task already says a place is planned, and `ev progress` now
  shows each place's tasks.
- Clearing the `counting` marks earlier task starts left: that is the person's call, place by
  place (`ev review <place> --as raw`), since some of those places may have been worked in.
