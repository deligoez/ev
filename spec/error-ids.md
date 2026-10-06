# Errors with ids, in the reader's language

## Decision

An error of ev is read by two readers: the agent, through JSON, and the person, through the
text output when they run ev themselves. Today every message is an English sentence built where
the error is raised (about 370 places: 187 usage, 108 refusals, 41 not found, 6 ambiguous, 29
internal). The person reads Turkish everywhere else in ev's text output; the errors are the one
place left in English. Decided with the person on 2026-10-06, for ev and for ak alike (ak's
spec, phase 1.2): **every error carries a stable id and its values, and the text output
translates the sentence by that id.** Never by matching the English text: it carries values
and changes.

- **An error is an id, values and a context.** `id` is stable snake_case (`not_found_ref`,
  `move_into_itself`). `values` are named (`{"ref": "S5-13"}`). `at` is the context, structured
  instead of a prefix glued onto the message: `[{"line": 2}]`, `[{"picture": "f3"}]`; it is
  rendered `line 2: …`, `f3: …`.
- **The English sentence is a template in core**, one table `id → "no record matches {ref}"`
  with named placeholders, rendered when the error is shown. JSON keeps it as `message`, so an
  agent reading `message` loses nothing.
- **JSON:** `{"error": {"code", "kind", "id", "message", "values", "at", "candidates",
  "details"}}`, each as today when present. `code` and `kind` do not change.
- **The text output** looks the id up in the CLI's Turkish table (`cli/src/i18n.rs`, a second
  table keyed by id, same named placeholders) and falls back to the English template. A value
  that is a word of a closed set (a kind, a state, a bucket, a field name) goes through the word
  tables the CLI already has; any other value is shown as given.
- **The same field names as ak** (`id`, `values`, `at`), so both tools' errors read alike to an
  agent that uses both.
- **Kept stable:** an id, once released, is not renamed (agents and tests match on it); a change
  of one is said in the release notes. Its English sentence may be reworded.
- **What stays English:** internal errors (exit 1: a bug, read by the developer), and the
  argument parser's own messages (clap: a mistyped flag), until they are asked for.

## Data model

`core/src/error.rs`: the variants keep their exit codes, and the message becomes a value of its
own:

```rust
pub struct Message {
    pub id: Option<&'static str>,   // None: a sentence not yet given an id
    pub text: String,               // the English sentence, rendered
    pub values: serde_json::Map<String, Value>,
    pub at: Vec<Value>,
}
```

A constructor per kind takes the id and the values (`Error::usage("bucket_unknown",
json!({"bucket": b}))`) and renders the English from `ERRORS` (`core/src/errors.rs`). `prefixed`
and `at_line` push onto `at` instead of rewriting the text. A sentence without an id keeps
working during the move (`id: None`) and is shown as it is in both languages.

## Tests

- Every id used in core has one English template; no id twice; every template's placeholders
  are given by its call site (a missing value fails a test, never prints `{ref}`).
- Every id has a Turkish template with the same placeholder names (the CLI's i18n test).
- A ratchet: the number of errors raised without an id may only go down; the test holds the
  current number and fails when it grows, and is lowered with every phase.

## Phases

1. **The shape.** `Message`, the constructors, `ERRORS`, the JSON fields, `at` for `at_line` and
   `prefixed`, the Turkish lookup with its fallback, the tests and the ratchet. The errors every
   command meets first get ids: a reference not found, ambiguous, the wrong kind of record.
2. **Refusals (exit 5)**, module by module, most used first: moves and places, gone and past,
   purchases, photos, plan and tasks, the rest.
3. **Usage errors (exit 2)**, module by module the same way.
4. **The rest:** not found outside references, the schema refusal; the ratchet reaches zero for
   everything but internal errors.

Each phase updates REFERENCE (the error payload), the skill (an agent may match on `id`), and
the release notes.

## Not now

- Translating clap's parse errors.
- Errors of the MCP transport itself (they are the protocol's).
- A language other than English and Turkish.
