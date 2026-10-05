# Reorganizing a piece of furniture: themes are provisional

## Decision

Once every drawer of a piece of furniture is toured, the person wants to reorganize it: merge
drawers, split one, move parts between them. The themes written while touring describe what each
drawer holds today; they must not anchor the new layout. `ev regroup` and `ev suggest` lean on
today's themes and look at one holder at a time, so they defend the present.

`ev layout <furniture>` looks across every place in a piece of furniture (a Kallax, a set of
drawers) at once, reading what the places **hold**, not their themes. It reports facts, best
first within each list, and never moves anything:

- **`spread`**: kinds of thing kept in several places (cables in four drawers): groups of things
  that read alike (shared stems and synonyms of their names and tags, as `ev themes` finds them),
  with how many are in each place.
- **`overlap`**: pairs of places whose contents read alike, whatever their themes say, with the
  words they share and how much of each it is.
- **`merge`**: nearly empty places (few things, or a low fill) and a place with room whose
  contents read like theirs.
- **`split`**: full or mixed places (fill ≥ 90, or no group holding most of what is in it),
  with their groups.

`ev layout <furniture> --propose` drafts a new layout from the contents alone: the groups, largest
first, each given the place that already holds most of it and has room for it (else the emptiest
one), and a theme drawn from the group's own words. It answers with the moves that layout needs
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
