Draft for the next release.

## A tour goes out of date only when the contents change

`ev progress` (and `ev next`) mark a toured place "changed since" when it may need counting
again. The mark compared the tour with the latest edit anywhere below the place, so re-coding a
box, linking a document or a purchase, or editing a note made a place read as changed: in one
household 47 of 49 toured places did, and the mark no longer said where to go back. It now
follows only what changes what a place holds — something added, moved in or out, found, lost or
gone — the same rule a photo goes out of date by.

## Numbers on one page: `ev stats`

The inventory had no place that said how big it is, what it cost as far as ev knows, how far
the counting has come or where the purchases stand; the agent answered such questions with SQL
against a copy. `ev stats` gives them on one page, computed when asked and never scored: how
much there is (records, units, rooms, furniture, boxes, things in several places, photos,
documents); what the things still here cost by their linked purchases, per currency and in
today's money, with how many records that covers and the five that cost most; the same per
room; the counting (places counted, being counted, not counted, changed since, and the share of
things in counted places); the purchases (linked, settled, still open, per year, the five
shops with most lines); the last 30 days (added, moved, gone by how, photos, the busiest day);
the boxes (known to be empty, never counted, fill, the fullest); coverages; the tags most used;
and the things bought longest ago. `ev ui` shows the same on a ninth tab, Statistics, one
collapsible section per heading, where a line that names a record opens it.
