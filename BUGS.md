# Bugs and rough edges

Collected while using `ev`; fixed in batches, not one release per bug. An entry names what
happened, where, and what was expected. Fixed entries move to the release notes of the version
that fixes them.

## Open

- **Point at a thing in the open `ev ui` (idea).** The person asked "which one do you mean?"
  about a tool the agent had named from a photo. Expected: `ev focus <ref> [--photo n]` writes
  a focus request (e.g. a `settings` row); a running `ev ui` already re-reads on
  `data_version`, so it jumps to the node in the tree and shows that photo full screen — the
  crop the agent cut makes it unambiguous. Clears itself once shown.

- **No verb for a record that never existed.** A tool recorded from the first photos turned
  out to be a misreading (the person: "başka pense yok"). The only way out was
  `ev gone --as trash --why …`, which reports it as thrown away and shows it in the trash
  history. Expected: `ev gone <ref> --as mistake` (or `ev remove --why`), kept in history but
  not counted as anything that left the home. The same happens with a duplicate record (a
  toothbrush recorded twice, once as "tel fırça"): `gone --as trash` was the only way to close
  the copy.
