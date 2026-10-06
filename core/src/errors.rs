//! The English sentence of every error with an id (spec/error-ids.md): one table, named
//! placeholders filled from the error's values when it is shown. The CLI keeps the Turkish
//! sentences by the same ids.

/// `(id, English template)`. An id, once released, is not renamed: agents and tests match on
/// it. The sentence may be reworded.
pub const ERRORS: &[(&str, &str)] = &[
    // References (phase 1).
    (
        "no_record_matches",
        "no node matches `{ref}`; search with `ev find` and retry with an id",
    ),
    ("no_record_with_id", "no node with id {id}"),
    (
        "ref_matches_several",
        "`{ref}` matches {count} nodes; retry with an id",
    ),
    (
        "record_gone",
        "node {id} is gone; `ev show {id} --include-gone` or `ev history {id}` still find it",
    ),
    (
        "record_joined",
        "node {id} joined #{into}; it is counted there now",
    ),
];

/// The English template of an id.
pub fn template(id: &str) -> Option<&'static str> {
    ERRORS.iter().find(|(i, _)| *i == id).map(|(_, t)| *t)
}

/// The names a template fills: `{ref}` gives `ref`.
pub fn placeholders(template: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let Some(len) = rest[start + 1..].find('}') else {
            break;
        };
        let name = &rest[start + 1..start + 1 + len];
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            out.push(name);
        }
        rest = &rest[start + 1 + len..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{ERRORS, placeholders, template};

    /// Every Rust file of core but the two that define errors.
    fn sources() -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut dirs = vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs")
                    && !path.ends_with("error.rs")
                    && !path.ends_with("errors.rs")
                {
                    let text = std::fs::read_to_string(&path).unwrap();
                    out.push((path.display().to_string(), text));
                }
            }
        }
        out
    }

    #[test]
    fn every_id_is_written_once_in_snake_case() {
        let mut ids: Vec<&str> = ERRORS.iter().map(|(i, _)| *i).collect();
        assert!(
            ids.iter()
                .all(|i| i.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        );
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "an id is written twice");
    }

    #[test]
    fn every_id_raised_has_a_sentence() {
        for (path, text) in sources() {
            for part in text.split("Error::said(").skip(1) {
                let Some(id) = part.split('"').nth(1) else {
                    continue;
                };
                assert!(template(id).is_some(), "{path}: `{id}` has no sentence");
            }
        }
    }

    #[test]
    fn a_template_names_its_values_plainly() {
        for (id, t) in ERRORS {
            assert!(!placeholders(t).is_empty() || !t.contains('{'), "{id}: {t}");
        }
    }

    /// Errors raised as a bare sentence, without an id, may only become fewer
    /// (spec/error-ids.md); lower the number as they are given ids.
    #[test]
    fn errors_without_an_id_only_become_fewer() {
        const LEFT: usize = 321;
        let n: usize = sources()
            .iter()
            .map(|(_, t)| {
                [
                    "Error::Usage(",
                    "Error::NotFound(",
                    "Error::Ambiguous {",
                    "refused(",
                ]
                .iter()
                .map(|p| t.matches(p).count())
                .sum::<usize>()
            })
            .sum();
        assert!(n <= LEFT, "{n} errors without an id, more than {LEFT}");
    }
}
