Draft for the next release.

## New

- **A photo can be marked out of date** (`ev photo stale <ref> [--why]`). A drawer emptied
  before it was recorded kept its old photo as current: nothing changed in the records after
  it, so the photo-needed list never showed it, and the only trace was an observation or a photo
  note in prose. The mark puts the place on `photos` in `ev todo` (`photo_reason: "marked"`)
  until a newer photo is attached or `ev photo current` takes it back.

## Fixed

- **An emptied place whose photo still showed what left was never listed for a new photo.**
  The photo-needed list skipped every empty place, so a drawer emptied in the records kept a
  photo full of things. An empty place is now listed when it has a photo older than its last
  change; one never photographed still is not.
