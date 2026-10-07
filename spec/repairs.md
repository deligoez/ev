# Repairs and maintenance (upkeep)

## Decision (2026-10-08)

What was done to a thing, and when it is due again: a car's service and inspection, a boiler's
yearly check, a phone's new battery, a washing machine's repair. Decided with the person when
vehicles came (spec/vehicles-homes.md): **ev keeps what was done to the thing, ak keeps what it
cost.** `ev broken` / `ev fixed` say that a thing does not work; upkeep says what was done.

- **One record per piece of work**, in a table of its own (`upkeep`, schema 36):

  | Field | |
  |---|---|
  | `node_id` | the thing (a vehicle, an appliance, a home) |
  | `kind` | `repair` (something broken was fixed), `service` (routine care: an oil change, a boiler check), `inspection` (an official one: a car's periodic inspection) |
  | `at` | when, a date or a partial date (`2024`, `2024-06`) |
  | `km` | the odometer then, for a vehicle (optional) |
  | `work` | what was done, in the person's words |
  | `by` | who did it: a shop, a technician, "ourselves" (text) |
  | `next_at`, `next_km` | when it is due again, by date and/or odometer (optional) |
  | `doc_id` | the invoice or service form, a document of `ev doc` (optional) |
  | `note` | free text |

- **Commands:**
  - `ev upkeep add <ref> --kind repair|service|inspection --work "<what>" [--at <date>] [--km n]
    [--by "<who>"] [--next <date>] [--next-km n] [--doc <id>] [--note t]`; `--at` is today
    when left out.
  - `ev upkeep list [<ref>] [--due]`: a thing's upkeep, newest first; without a reference,
    every thing's; `--due` only what is due within 60 days or past due, by date or by
    odometer.
  - `ev upkeep edit <id> field=value…` corrects one (`at`, `km`, `work`, `by`, `next_at`,
    `next_km`, `doc`, `note`, `kind`); `ev upkeep remove <id>` drops one recorded by mistake.
  - `ev fixed <ref> --work "<what was done>" [--by …]` closes the `broken` mark and records the
    repair in one step (a `repair` dated today).
- **Due by odometer:** a vehicle's last known odometer is the largest `km` of its upkeep;
  `next_km` minus that is how far is left. Nothing guesses how far a car is driven a month.
- **Where it shows:** `ev show` (`upkeep`, newest first, up to 10); `ev todo` (`upkeep_due`: due
  within 60 days or past, by date, or within 1,000 km by odometer); `ev ui` (the thing's
  summary, a section "Upkeep"); `ev history`
  (`upkeep_added`, `upkeep_edited`, `upkeep_removed` events on the thing).
- **The cost is ak's.** ak tags a payment with the thing's `#id` (spec/vehicles-homes.md); a
  money panel reads both (spec of step "cost of ownership"). ev stores no amount here.

## Why

A car's service history and a boiler's next check are the questions people ask a home
inventory after "where is it"; they were notes so far, which no reminder reads. The split with
ak keeps one owner per fact: the thing and its work here, the money there.

## Not now

- Recurring schedules ("every 15,000 km or every year"): `next_at` / `next_km` are said each
  time, as a service form says them.
- Parts used in a repair as records of their own (a new battery as a thing): `ev add` it if it
  matters.
- Consumables (oil, filters) as stock.
- The sidebar's UPKEEP section in `ev ui` ("Due", "All"; spec/ui-sidebar.md phase 5): the
  thing's summary and `ev todo` carry upkeep until a list of it is asked for.
