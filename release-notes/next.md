Draft for the next release.

## New

- **Facets keep kinds of things apart when placing.** `ev facet add modül --words "modül,
  kart"` defines a kind; tagging a holder with its name puts it (and what is inside it) in that
  facet. A thing's facet comes from its own tag, else from a facet word in its name in any form
  (`modülü`, `modülleri`), else from where it is. `ev suggest` never ranks a holder of another
  facet (they are listed apart as `other_facet`) and `ev regroup` never proposes one: a buzzer
  module is no longer sent to the bare-buzzer box. `ev facet list|remove`. Schema version 10.
- **`ev tree` reads as a layout**: each node carries its theme, fill, size and tags.

## Fixed

- **`ev ui` tree rows now show fill bars.** They read the fill from `ev tree`, which did not
  carry it, so the bars added in v0.11.0 never appeared.
