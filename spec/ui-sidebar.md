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

## Not now

- Reordering or pinning lists by the person (Snipe-IT, Linear favourites): the order above is
  fixed until someone asks.
- Hiding empty lists: they show a dimmed `0`, so a digit always opens the same list.
- A tree widget in the sidebar: two levels are enough.
