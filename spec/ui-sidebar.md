# ev ui: a sidebar of lists instead of top tabs

## Decision

`ev ui` grew to ten top tabs, and more lists are coming: purchases (all, and the durable,
clothing, digital and service buckets), repairs and maintenance, people, and the lists behind
every figure on the Statistics page. A tab bar stops scaling at about seven. The UI moves to
**three panes**: a sidebar of lists on the left, the chosen list in the middle, the selected
record's details on the right (decided with the person, 2026-10-06).

- **No top tabs.** The top line keeps the brand and says where you are: the section and list
  (`Home › Pending`), later the drill-down path (`Statistics › Purchases by year › 2024`).
- **The only tab strip left is the details pane's** (Summary, Photos, Documents, …): tabs choose
  a facet of the selected record, the sidebar chooses the collection. Two tab strips that both
  want the same keys are what the research found to confuse most.
- **The sidebar has two levels at most:** uppercase section headings, and lists under them. The
  place tree is a list (Layout), shown in the middle pane, never inside the sidebar.
- **Each list shows its count, right-aligned,** taken from the same query that fills the list,
  so the two never disagree. Lists without a single meaningful count (Layout, To do,
  Statistics, Settings) show none.
- **Sections now:**

  | Section | Lists (digit) |
  |---|---|
  | HOME | 1 Layout · 2 To do · 3 Pending · 4 Leaving · 5 Lost · 6 Errands |
  | PURCHASES (phase 3) | 9 All · Durable · Clothing · Digital · Service |
  | UPKEEP (with spec/repairs.md) | Repairs · Maintenance |
  | PEOPLE (with the people feature) | one list per person billed or bought for |
  | HISTORY | 7 Gone |
  | INSIGHT | 8 Statistics |
  | (bottom) | Search · 0 Settings |

- **Keys.** Digits open the lists that carry one, in the order above (the person has no strong
  habit with the old order, so the digits follow the sidebar). `Tab` / `Shift+Tab` move the focus
  sidebar → list → details; the focused pane has a bold border. In the sidebar, `↑` `↓` / `j` `k`
  move and open the list at once (a preview, as in a mail client), `Enter` / `→` / `l` go into
  the list. In the details, `j` `k` scroll and `h` `l` step its tabs. `b` hides or shows the
  sidebar and is remembered. `/` keeps searching; its results are the Search list.
- **The width follows the terminal** (the person often works from a phone over Remote Control,
  so the narrow layouts are a primary case):

  | Width | Layout |
  |---|---|
  | ≥ 120 columns | sidebar (24 columns) + list + details |
  | 90–119 | the sidebar shrinks to a rail: digit and count per list |
  | 70–89 | list + details; `b` opens the sidebar over the list as a drawer |
  | < 70 | one pane at a time, the focused one; `Tab` moves to the next |

  `b` overrides the automatic choice at every width.
- **The mouse:** a click on a list in the sidebar opens it; the dividers drag as before.

## Why

Research on 15 ratatui apps' source (rainfrog, gobang, ATAC, slumber, openapi-tui, spotatui,
gitui, kdash, …) and on lazygit, k9s, aerc, neomutt, together with the inventory agent's study of
desktop apps (Apple's sidebar guidelines, Things, OmniFocus, Linear, Under My Roof):

- Apps that switch lists with top tabs stay at 5–7 of them; apps with 15+ collections use a
  grouped source list on the left (aerc 22 columns, serie 26, slumber 30).
- Almost all of them use tabs in the content area for facets of the selected record.
- A width in columns, clamped, beats a percentage: 15% of 80 columns is 12, of 250 is 37.
- The common faults: focus that is not visible, digits claimed by two tab strips, counts that
  disagree with their lists (neomutt, Snipe-IT), selection lost on a live refresh.

## Phases

1. **The shell.** The sidebar with sections, digits and counts; focus between the three panes;
   the four width layouts and `b`; the top line as a path; the ten current tabs become the
   sidebar's lists, no new list yet. Remembered in `ui-state.json`: whether the sidebar is shown.
2. **Moving between lists.** `:` opens a palette that finds any list, place code or `#id`, with
   Turkish and ASCII spellings (`kayip` finds `Kayıp`); `Esc` steps back through what was opened
   (a back stack), and the top line shows the path. A refresh keeps the selected record by id,
   and says so when it left the list.
3. **Purchases.** The PURCHASES section: All and the four buckets, each a list of purchase lines
   with its own columns, filters (shop, year, linked / not linked / declined), grouping by order,
   totals, and the line's details on the right (the inventory agent's needs list).
4. **Statistics drill-down.** `Enter` on a figure opens the list behind it, pushed on the back
   stack; a figure and that list come from one query.
5. **Upkeep and people** join the sidebar as those features land.

Phases 1–4 shipped on 2026-10-06. As built, phase 3 filters by where a line stands (`f`: all,
open, linked, dismissed) and by words (`/`, live); a year and a shop narrow a list only when a
figure of the statistics opened it. Phase 4 marks a figure with a list behind it `›`; the
figures that open one are the purchase lines (all, a year's, a shop's), the counting (To do)
and what left (the past).

Still open from the inventory agent's needs list for the purchases, for when they are asked
for: a filter by month, currency, source or account; sorting by amount; the line's history and
its candidate things on the right; product images shown in the line's details; the views
"open durable lines by probable place", "suspected duplicates" and "lines of things that left";
a money panel on a thing's details (purchases, repairs, coverage premium, value, sale).

## The palette's commands (decided 2026-10-07)

`:` also runs the screen's own commands, not only goes somewhere: one palette, as in k9s and
Helix, with VS Code's prefix to narrow it.

- **One key, one box.** Typing finds lists, then commands, then records. A query starting with
  `>` finds commands only. No second palette and no `Ctrl+P`: `:` is the habit to keep.
- **One table of commands** (`cli/src/ui/commands.rs`): each command's name, its key and when it
  applies. Running a command presses that key, so the palette can never do something the key
  does not, and a key can never be renamed without the palette following. The idea is
  ratatui-labs' `ratatui-action` (an application names its capabilities once and every surface
  reads them); its crates are experiments, so ev keeps its own table of a few dozen lines.
- **Every line shows its key**, right-aligned and dimmed. The palette teaches the keys: whoever
  finds "Map of the home" there learns `M`.
- **Only what applies now is offered**, with the same conditions as the help line: the tree's
  open/close commands on the tree, the photo commands when a photo is shown, the filter on a
  purchase list.
- **Only the screen's commands.** Views, panes, dividers, photos, copying, the map, quitting.
  Nothing that writes to the inventory: the agent proposes, the person confirms (the principle in
  CLAUDE.md), and `ev ui` stays a place to look.

Why: research on ratatui and other terminal apps (ratatui-labs' `ratatui-command-palette` and
`ratatui-action`, `ratada`, k9s, Helix, lazygit-style tools with `Ctrl+P`). The palette that
already found any list or record was one step from a command palette; the help line cannot hold
every key on a narrow screen, and the palette can.

Not now: the details tabs as commands (`H`/`L` and a click choose them); a `?` screen listing the
whole table, until someone asks for it.

## Not now

- Reordering or pinning lists by the person (Snipe-IT, Linear favourites): the order above is
  fixed until someone asks.
- Hiding empty lists: they show a dimmed `0`, so a digit always opens the same list.
- A tree widget in the sidebar: two levels are enough.
