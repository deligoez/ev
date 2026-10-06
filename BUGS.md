# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **`ev find` does not fold a code's `-` and `_` as `ev show` does.** A box coded `X5_13` is
  found by `ev find x5_13` and `ev show X5-13`, but `ev find X5-13` answers nothing, and
  `ev find "X5-" --kind container` leaves out `X5_12` and `X5_13`. The agent listed a series
  that way and proposed a code already in use as the next free one. Expected: `find` compares
  codes as references do (`-` and `_` one, leading zeros of a number ignored; spec/codes.md),
  and the next free code of a series (`code=X5-*`) counts `X5_12` as taken. (Reported by the
  inventory agent.)
- **Where a marked-photo series ends is a guess.** A new drawer's photo batch joined the series
  still open from the previous drawer, so its frames were numbered 16–32 while its pictures were
  f16–f34: frame 16 and f16 named different things, and the person noticed only afterwards. The
  agent must not close a series on its own (`ev focus --clear` only on the person's word), and
  closing one by mistake loses context, but nothing tells the agent that a new batch has begun
  ("done" on a place does not mean the next photos are about another one). Expected: a series
  knows which place (or batch) it is about (`ev focus --file … --for <place>`, or a series
  title); when a photo for another place joins an open series, ev answers with a hint ("the
  series is about A; this photo is about B — close it?") so the agent asks the person in one
  line; and a new batch's frames are numbered from 1. ev still closes nothing by itself.
  (Reported by the inventory agent.)
- **A root the inventory never inflects is still cut to a shorter written word.** `kapı` →
  `kap`, `veri` → `ver`, `mini` → `min`, `yani` → `yan`, `boya` → `boy`, `powerline` →
  `power`: the word reads as that word plus a valid ending (`kap`+`ı`), and no other written
  form of it shows it is a root. The cases the inventory does show (`altın`, `ünite`, `pense`,
  `cıvata`) and those an ending cannot follow (`varta`, `sabun`, `güneş`, `türkiye`) were fixed
  in v0.18.0. Expected: a root stays whole. The rest needs a dictionary and waits for
  Çözgü's embeddable core. Measure with `tools/measure/stems.py` (1,736/1,919 after the fix).
- **Cell frames drift on tall boxes.** On a drawer photo with its grid corners kept, the frames
  drawn for 1x2 boxes sat a little low on the top edge (perspective). Minor. (Reported by the
  inventory agent.)
