# Statistics

Status: **implemented** (decided 2026-10-03): all three phases.

## 1. Decision

One page of numbers about the home, `ev stats` and a tab of its own in `ev ui` (`9`). Facts
only, each computed from the records the moment it is asked; nothing is stored, nothing is
scored, nothing reorders a plan. No report per room or per category on pages of their own (the
person asked for one page, not Under My Roof's reports); a line that names a record or a place
points at it, so the tab opens it on the right like any other list.

## 2. Why

The inventory had a week of work in it and no place that said how big it is, what it is worth
as far as ev knows, how far the counting has come, or where the purchases stand. The agent
answered such questions with SQL against a copy. Other inventory apps lead with the same few
numbers (items, quantity, value, places, value by place, recent activity); ev has more to say,
since it knows tours, portions, purchases and coverages.

## 3. What is shown

Each section is an object in `ev stats`' JSON, and a heading in the text and the tab.

| Section | What |
|---|---|
| `overview` | records of things, their units (the sum of counts), holders by kind (rooms, furniture, containers), things kept in several places, records with a photo, documents |
| `value` | what the things still here cost, as far as linked purchases say (each line's share by the units linked), per currency and in today's money when the index is there; how many things that covers; the five that cost most; the latest values recorded (valuations) and their sum |
| `rooms` | per room: records of things, units, holders, what they cost (as above) |
| `tour` | places counted, being counted, not counted, changed since (`ev progress`), and the share of things in counted places |
| `purchases` | lines, linked, dismissed, open durable ones; per year: lines and amount; the five shops with most lines and their amount |
| `activity` | the last 30 days: records added, moves, things gone by how they left, photos; the busiest day |
| `holders` | boxes known to be empty, not known, with a fill and its average, full ones (fill ≥ 90), the five that hold most records |
| `coverage` | active coverages, those ending within 90 days |
| `tags` | the most used tags and how many records carry each (top 10) |
| `oldest` | the five things bought longest ago, by their linked purchase |

Money is summed per currency as given; a sum in today's money is shown only for what the
index and the rates cached can convert, with how many lines that is ("today's money, 12 of 13
lines").

## 4. Phases

| # | What | Done when |
|---|---|---|
| 1 | `Inventory::stats()`, `ev stats` (JSON and text) | the sections above from a test house |
| 2 | `ev ui` tab 9 "Statistics": a heading per section, collapsible, lines that name a record open it | a UI test reads the headings and opens a line |
| 3 | README, REFERENCE, skill (answer "how many / how much / how far" from `ev stats`) | docs test green |

## 5. Not now

Charts; a range of dates to choose; statistics per room or per tag on pages of their own;
exporting a report.
