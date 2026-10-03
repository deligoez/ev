Draft for the next release.

## Bringing what came with a purchase says what it did

`ev buy bring` used to answer with the thing alone: when it brought nothing (everything already
brought, or an `--only` id the line does not carry) it said nothing either, and the agent had to
guess. It now answers with what it brought, by type (`brought_types`), and what it left and why
(`skipped`: already brought, and to which thing, or of a type not asked for); the text output
puts that above the thing. An `--only` id the line does not carry is refused instead of being
skipped.

Two additions make the product pictures easy to bring without scripting around ev:

- `--type image` (or `link`, `valuation`, `coverage`, comma-separated) brings one kind only, and
  `--only` takes a comma-separated list.
- `ev buy bring --all [--type image]` is the back-fill: every linked line's attachments not
  brought yet go to the thing it is linked to. A line linked to several things, or to a thing
  that is gone, is left and named, for the person to decide.

Pictures are never brought on their own: the agent brings them (on the person's word, or by a
rule the person set) and tells the person what it brought.
