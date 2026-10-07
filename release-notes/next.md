Draft for the next release.

## `:` runs commands too

The palette that finds any list or record in `ev ui` now also runs the screen's own commands:
type a few letters of "map", "rotate" or "two levels" and `Enter` does what the key would. Each
command shows its key at the right edge, so the palette teaches the keys that the help line has
no room for on a narrow screen. A query starting with `>` finds commands only, and only those
that apply where you are: the tree's commands on the tree, the photo's when a photo is shown,
the filter on a purchase list. Every command is one entry of a single table (its name, its key,
when it applies), and running one presses that key, so the palette and the keys cannot drift
apart. Nothing in it writes to the inventory.

## A crash of `ev ui` leaves the terminal clean

When `ev ui` panicked, ratatui put the screen and the keyboard back, but the mouse reports and
the light/dark reports that `ev ui` had turned on itself stayed on, so the shell went on
printing escape codes at every mouse move. A panic now turns both off before ratatui restores
the rest, the same way a normal exit does.

## A batch of photos, for the agent

The series in `ev ui` is how the person and the agent talk about a batch (`f12`, frame `3`), but
the agent had no view of a batch of its own: to look at sixteen new photos it built a contact
sheet with ImageMagick, and to give each photo of one thing a number it ran one `ev photo mark`
per photo. `ev photo sheet f16..f31` now puts those pictures on one image, four to a row, each
titled with its `f`-number and note (through MCP it comes back as an image), and
`ev photo mark --whole f16..f31` frames each of them whole, numbered on from the series, in one
call. Both read ranges and single pictures (`f2 f5..f8`).

## A split part goes where it belongs in the same step

Touring a place, a record the inventory once guessed in bulk ("many cables") is broken up into
one record per piece, and each piece goes its own way. That took `ev split`, then an `ev move`
or `ev gone` per piece, carrying ids from one answer to the next. A part of `ev split` now says
where it goes: `"USB-C kablo=2@S5-01"` moves it there, and `ev split <ref> --stdin` takes one
JSON line per part with `to`, or with `gone` and `why` to let it leave at once. The split, the
moves and the leavings are one transaction: an unknown place undoes all of it.

## Finding a whole family of things, and whether it was counted

Other agents now ask the inventory questions like "every Raspberry Pi, ESP board and sensor
we have". `ev find` matches every word of its text, so the inventory agent ran dozens of finds
and merged them by hand. `ev find --any "raspberry pi" esp32 sensör` now finds each text on its
own and answers one list, each record once with the texts that found it (`matched`), and how
many each text found (`per_text`, so one that found nothing shows); the MCP `find` tool takes
`any` too. Every result now also says how far the place it is in was counted (`place_count`):
a thing in a toured place was counted, one in a raw place is what the inventory guessed, and
the text output marks it. Several words without `--any` are one text, unquoted.

A query word of three letters or fewer now meets only the start of a word: `ir` no longer
finds every `bir` (in one household's inventory it went from 365 results to the 13 that say
IR), and `ble` no longer finds an ink cartridge called `Mixable`. A synonym is met only as a
whole word or its stem: a group `ekran, lcd, display` no longer brings in every DisplayPort
cable.

## What is only a guess

The inventory agent kept two kinds of uncertainty in prose: things known only from the
person's words before their place was toured (written as observations, so `ev find` missed
them), and facts it assumed ("kesinlik: büyük ihtimalle" in a note, a coverage end taken as one
year from the purchase), which reminders then trusted. `ev guess` marks either: a record
(`ev add … --guess "<who said it>"` adds one as a guess) or some of its fields (`--field came`,
`--field cover:<id>` for a coverage's end), with a note. A guess is shown everywhere it
matters: `ev show`, `ev find` (marked), `ev todo` (a list to check), `ev next` while its place
is open, the answer of `ev review --as toured` (the guesses inside, to settle on the person's
word), and a coverage whose end is a guess says so. `--clear` takes it back, and a field said
again with `ev edit` is no longer a guess. Stored as marks: no schema change.

## A place just emptied says what of it went stale

When the last thing leaves a box, its theme and the observations on it describe what used to be
there, and nothing said which place had just become empty. `ev move`, `ev done` and `ev gone`
now answer `emptied` for such a place, with its theme and observations, and the text output
names the two commands that clear them. ev clears nothing on its own. `ev unobserve` takes
several ids now, and `--on <place>` removes every observation of a place at once, each kept in
the history.

## A family of things is a tag

Asked for every Zigbee device, the inventory agent read drawers one by one and missed a
shelf. ev keeps families as tags rather than a tree of categories (one household already uses
90 tags on a quarter of its records), and gets what was missing to use them that way:
`ev tag <tag> <ref>…` tags several records in one step (`--remove` takes it off), and
`ev find --tag a --tag b` lists records with any of the tags.

## Repairs and maintenance

What was done to a thing, and when it is due again: a car's service and inspection, a boiler's
yearly check, a phone's new battery.
`ev upkeep add <ref> --kind service --work "…"` (or `repair`, `inspection`) records it with
when it was done, the odometer, who did it, when or at what
odometer it is due again, and the invoice from `ev doc`. `ev upkeep list --due` (and `ev todo`)
names what is due within 60 days or 1,000 km, or past, from the newest piece of each kind;
`ev show` and the thing's summary in `ev ui` list what was done, newest first.
`ev fixed <ref> --work "…"` closes a broken mark and records the repair in one step. What it cost stays ak's:
ev records no amount here. Schema 36 adds the `upkeep` table.
