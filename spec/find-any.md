# Finding several kinds of thing at once

## Decision (2026-10-08)

Other agents now ask the inventory questions like "every Raspberry Pi, ESP board and sensor we
have" (ak, a project agent). `ev find` matches every word of its text, so the inventory agent
ran about 35 finds, one per word, walked drawers with `ev tree`, and merged and de-duplicated
the answers by hand (its report). It also had no way to say which of the results were counted
and which were only guessed from a place never gone through.

- **`ev find --any <text>…`** runs each text as its own `ev find` (every word of one text, as
  today) and answers one list: each record once, in the order first found, with `matched`, the
  texts that found it. `--tag`, `--kind`, `--include-gone` apply to every text. `per_text`
  counts the hits of each, so a text that found nothing is visible (`{"esp32": 0}`).
- **Several words without `--any` stay one text.** `ev find pasif buzzer` (unquoted) now reads
  as `ev find "pasif buzzer"` instead of refusing the second word: every word, as before.
- **Every result says how far its place was counted:** `place_count`, the count state of the
  unit it is in (`raw`, `counting`, `toured`, `kept`; absent above the units or for a lost
  thing), the same states `ev progress` shows. A thing in a `toured` place was counted; one in
  a `raw` place is what the inventory guessed before anyone looked. The text output marks a
  result whose place is not yet counted.
- **MCP:** the `find` tool takes `any` (a list of texts) as well as `text`.
- **A short word is a word's start** (decided with the inventory agent's cases, same day): a
  query word of three letters or fewer meets only the start of a word, never its middle, and a
  Turkish stem of two letters or fewer is not tried. `ir` found 365 records of one household
  (every `bir`), now the 13 that say IR; `ble` found an ink cartridge (`Mixable`) and a
  cabinet lock, now nothing but BLE. A longer word is still found inside words. A family named
  by several words (ekran, LCD, OLED, display) is a synonym group (`ev synonym add`), not a
  ranking rule.

## Why

Friction is a missing verb: an agent merging thirty-five answers by hand is ev lacking a
command. And a list handed to another agent should say which lines are facts and which are
guesses, since that agent cannot tell.

## Not now

- Categories (electronics › sensor › presence) to find a whole kind without naming its words:
  a decision of its own (tags may be enough).
