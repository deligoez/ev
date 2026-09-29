# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **One noun gets two keys depending on its ending.** On the live inventory `bağı` keys to
  `bak` (the hardened candidate wins) while `bağları` stays `bagları`, so "kablo bağı" and
  "kablo bağları" do not meet. Other over-cuts found by the Çözgü comparison: `altın` → `alt`
  (meets `altında`), `açılı` → `açı`; under-cut: `bacaklarında` stays `bacakları`. Expected:
  one key per noun, and a word that is itself a root (altın, bağ) is not cut to another root.
  2026-09-29: rule changes alone do not fix this. Scored against Çözgü's 309 judged
  differences, preferring the soft stem broke dolap/çubuk/eşik and fixed none, and taking the
  shared stem when shorter fixed 3 and broke 6 (kendi→kend, yeri→yer); only a dictionary tells
  bağ from bak. Waiting for Çözgü's embeddable core.
- **A thing alone of its kind in its holder is flagged on any rare word.** Leave-one-out
  scoring takes the thing's own words out of its holder, so when nothing else there shares a
  word with it, its home scores 0 and a single rare word elsewhere wins: the heat gun in
  K4x2-07 (next to other Bosch tools, but no other "tabanca") is flagged for the MQ gas box on
  "hava"; the "Dur Yolcu" dagger in K4x4-05-Ü for the labelling drawer. A home's theme and
  kind (tools, keepsakes) should count for it even when no other thing repeats its words.
- **Measuring placement has no answer key.** `regroup`'s "best where they are" treats the
  current layout as correct, but the layout has known misplacements, so a better scorer can
  show no gain. A reviewed list of where each flagged thing belongs is needed to measure.
  2026-09-29: the 70 things `regroup` flagged were reviewed (16 moves, 4 undecided, the rest
  belong where they are). Against that key every variant tried (no change, colour weight,
  all word pairs, compound list, possessive-compound rule) scores 264–267 of 403: the word
  handling changes which few cases are right, not how many. The remaining ~137 misses are
  holders without a theme or with a shared one (the four book shelves, K4x4-16-A, S5-06, the
  maker desk) and things whose words say nothing about their kind. That is data (themes, a
  genre or category tag), not matching; the key was built from the current scorer's own
  flags, so it favours it slightly.
- **`ev ui`: the details pane cannot scroll, so long contents are cut off.** K4x4-07-A lists
  "İçindekiler (24)" and shows 13 on a 150×44 terminal (fewer with its photo panel); the rest
  cannot be reached from the pane. Expected: the details scroll (mouse wheel over the pane,
  and keys), or the contents list is the part that scrolls.
- **`ev ui` does not show `size` or `room`.** v0.10.0 added a box's size and the room it has
  (yes / little / none, stale when the contents changed after the fill), but the details pane
  still shows only the raw fill percentage, and a stale fill looks as trustworthy as a fresh
  one.
- **`ev ui` shows `updated` as a raw UTC timestamp** (`2026-09-29T09:19:19Z`), three hours off
  the local clock; it should be local time, and relative when recent.
