# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **`regroup` can propose a thing as its own better place.** An item that holds things is a
  holder too, and its own document (its name) is not skipped when it is scored: "LiPo pil
  kutusu + Seeed Li-Po Rider Pro seti" in K4x4-02-Ü comes out best in itself, and so do the
  DVD writer, the HDMI switch set, the Wavlink dock and two Libraton sets (6 items on the live
  inventory). Expected: the item's own subtree is never a candidate, as `suggest --for` already
  does.
- **One noun gets two keys depending on its ending.** On the live inventory `bağı` keys to
  `bak` (the hardened candidate wins) while `bağları` stays `bagları`, so "kablo bağı" and
  "kablo bağları" do not meet. Other over-cuts found by the Çözgü comparison: `altın` → `alt`
  (meets `altında`), `açılı` → `açı`; under-cut: `bacaklarında` stays `bacakları`. Expected:
  one key per noun, and a word that is itself a root (altın, bağ) is not cut to another root.
- **Colour words decide placement on their own.** "Sıcak hava tabancası (yeşil)" and "Yeşil
  polar kumaş" go to the green LED box, "Şerit metre (kırmızı)" to the red LED box, because the
  colour is the only word they share with anything. Lowering colour weight fixes them but sends
  yellow and green LEDs to the red LED box; a colour must count with the noun it describes
  ("yeşil LED"), not alone.
- **Noun compounds are split into unrelated words.** "Hesap makinesi" and "etiket makinesi"
  match "makine vidaları", "kablo bağı" matches cable drawers, "lehim pompası" and "su pompası
  pensesi" match "pompa uçları". A two-word term for known compounds fixed 8 such cases in a
  trial on a copy. Most modern compounds (hesap makinesi, kablo bağı, şarj cihazı) are in no
  dictionary, so the trial that works needs no list: pair a bare noun with a following noun
  that carries only the 3rd-person possessive (-sI/-I, soft stems included), pair a colour
  with the noun after it, and weigh a lone colour 0.3. On the copy this kept the LED boxes
  right and fixed kablo bağı, kontrol kalemi, ahşap vidası, priz kapağı and the colour-only
  matches (heat gun, kumaş, şerit metre); new misses were "güç adaptörü" / "şarj cihazı"
  pulled toward sets that hold one.
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
