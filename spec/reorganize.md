# Reorganizing a piece of furniture: themes are provisional

## Decision

Once every drawer of a piece of furniture is toured, the person wants to reorganize it: merge
drawers, split one, move parts between them. The themes written while touring describe what each
drawer holds today; they must not anchor the new layout. `ev regroup` and `ev suggest` lean on
today's themes and look at one holder at a time, so they defend the present.

`ev layout <furniture>` looks across every place in a piece of furniture (a Kallax, a set of
drawers) at once, reading what the places **hold**, not their themes. It reports facts, best
first within each list, and never moves anything:

- **`spread`**: kinds of thing kept in several places (cables in four drawers), with how many are
  in each place. A thing's kind is the end of its name before the first comma or dash: a Turkish
  name ends in what the thing is (`Kablo, USB-C` is a cable). When the last two words are a noun
  compound (`lens kapağı`, `güç adaptörü`, `test kablosu`) the compound is the kind: the last
  word alone grouped lens caps with pen caps and a camera body with a lock nut's `gövdesi`.
  Shared words alone grouped a `kutulu` adaptor with a `kutulu` sensor.
- **`overlap`**: pairs of places whose contents read alike, whatever their themes say, with the
  words they share and how much of each it is. Colour words never count.
- **`merge`**: nearly empty places (few things, or a low fill) and a place with room whose
  contents read like theirs.
- **`split`**: full or mixed places (fill ≥ 90, or no group holding most of what is in it),
  with their groups.

`ev layout <furniture> --propose` drafts a new layout from the contents alone: every kind of three
or more things is a group; the groups, largest first, each go to the place that already holds most
of it and is not yet given a group (else the emptiest place left), and the place takes the kind as
its theme. More kinds than places: the smaller ones stay where they are (`kept`). The draft leaves
alone what moves with something else: a thing in a box of its own inside the place (a themed or
labelled bin moves as one, if at all), a thing inside another thing (a device's leads), and a part
of a kit. A parking place (`temporary`) gets no group; what waits there goes to its kind's place.
On one household's 32-drawer Kallax of ~400 records these rules took the draft from 153 moves and
a theme for every drawer to 17 moves and 9 themes. It answers with the moves that layout needs
(`thing`, `from`, `to`) and the proposed theme of each place. Nothing is applied: the agent shows
it, the person changes it, and the moves are recorded as `ev move … --plan` and the themes with
`ev edit theme=` on their word.

The skill says it in one line: **a theme describes the present, not the plan.**

## Why

The person asked not to let the labels block sensible moves. A report that starts from contents
shows where kinds of thing are scattered and which drawers are really one, and a drafted layout
gives the conversation something to change instead of defending each drawer as it is. Facts, not
scores: ev groups and counts; the person and the agent decide.

## Phases

1. `ev layout <furniture>`: `spread`, `overlap`, `merge`, `split`.
2. `--propose`: the drafted layout and its moves.
3. Skill: when to run it (after the last drawer of a piece of furniture is toured), how to show
   it (marked photos of the places named), and the rule above.

## Not now

- Applying a layout in one command: the moves are planned one by one on the person's word, as
  every move is.
- Layouts across several pieces of furniture or rooms.
