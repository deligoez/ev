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
