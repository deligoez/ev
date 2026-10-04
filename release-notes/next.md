Draft for the next release.

## The numbered photo goes to the screen on its own

An agent cut three batches of photos and a drawer, checked every frame itself, and never sent
a single numbered photo to the person's `ev ui`: showing was opt-in (`--show`), and nothing
caught that it was skipped. `ev photo cut`, its `--preview` and `ev photo mark` now send what
they drew to a running `ev ui` by default; `--no-show` keeps it off the screen (a script, or a
picture the person should keep looking at). `--show` is still accepted, and `--show <note>` on
`photo mark` still titles it.

## A longer value is not shown as an addition

0.24.0 printed a line added to a note as `note: + <the line>`, but read any value that began
with the old one as an addition: a theme written out longer showed as `theme: +  ve bağlantı
parçaları`. Only a value that goes on with a new line, as `field=+text` adds it, is shown as an
addition now; any other change is `old → new`.

## Purchase lines found by their words

"Is there a purchase of this pen?" had no answer without recording the pen first and asking
`ev buy for` about it, or a script over `ev buy list`. `ev buy list --query "<words>"` keeps the
lines with every word in their name, shop, brand, product code or order number, compared
folded; with the other filters it narrows them further.

## Several records move in one call

Emptying a box before it was thrown out took one `ev move` per thing inside. `ev move <a> <b>
<c> --to <place>` (or `--plan`) now moves them together, all or none: one that is refused (it is
already there) leaves every one where it was. In `ev edit --stdin`, a list of tags,
`"tags": ["vida", "-m3"]`, adds each item that does not say `+` or `-` itself; it was refused.

## A reason when setting a thing aside

`ev gone --why` kept what the person said about a thing leaving, but `ev dispose` refused
`--why`, though the skill names both together. `ev dispose <x> --as give --why "<text>"` now adds
the reason to the thing's note, as `gone --why` does.

## An empty bring says why

`ev buy bring --all --type image` after linking three old lines printed only "Nothing brought",
though the skill says a bring never stays silent. It now says what it looked at: how many linked
lines it checked, how many carry that type at all, and how many of those were brought already
(`checked: {lines, carrying, already}`).

## A box called empty can be taken back

A box marked empty turned out full of odds and ends nobody had counted; `ev review <box> --as
raw` cleared its tour, but `ev find --empty` and `ev suggest` still offered it as empty, since an
`empty` once said was kept for good. Setting a box back to `raw` now forgets what made it known
empty up to then, so it lists among the boxes never counted until it is toured again.

## A code is the same however its label is printed

The household's label maker cannot print a hyphen with its cassette, so new labels come out with
`_` while the old ones have `-`, and a new box of the `S5` series was printed `S05_12`. Codes now
compare with `-` and `_` as one and a number's leading zeros ignored (spec/codes.md): `S5_11`
finds `S5-11`, a second `S5_12` is refused where `S5-12` is in use, and `code=S5_*` continues the
series after `S05_12`. Every record keeps its code as printed. **Schema 32**: the folded form of
every code is worked out again when this version first opens an inventory of 31; nothing else
changes.

## A thing that waits for another

Glue sticks were parked until the glue gun, recorded lost, turned up; the only link was a line
in the gun's note, read by nothing. `ev edit <thing> waits_for=<other>` records it
(spec/waits-for.md): `ev show` names the wait both ways, `ev todo` shows what each parked thing
waits for, and `ev found <other>` lists what waited for it so the person can say where those go
now. A move of the waiting thing ends the wait. **Schema 33** adds `nodes.waits_for` when this
version first opens an inventory of 32.
