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
    // Refusals of the records and their places (phase 2).
    (
        "split_holds_things",
        "{node} holds {inside} thing(s); split what is inside, move it out first, or take empty units off it with --take",
    ),
    (
        "split_takes_all",
        "the parts take {take} of the {had} of {node}; nothing would be left on it: keep one part as the original with --rename and --qty instead",
    ),
    (
        "already_candidate",
        "{node} is already a candidate; use `ev gone` or `ev restore`",
    ),
    (
        "restore_gone_by_correction",
        "node {id} is gone; `ev restore {id} --correction \"why\"` undoes it",
    ),
    ("not_a_candidate", "{node} is not a candidate"),
    (
        "gone_needs_how",
        "{node} is active; say how it left with --as trash|give|sell|trade|used|digitize|left|stolen|unknown",
    ),
    ("already_lost", "{node} is already lost"),
    ("home_cannot_be_lost", "a home cannot be lost"),
    ("not_lost", "{node} is not lost"),
    (
        "never_seen",
        "{node} was never seen anywhere; say where it turned up with `ev found <ref> --in <place>`",
    ),
    ("no_pending_move", "{node} has no pending move"),
    (
        "already_there",
        "{node} is already in {place}; a place inside it (a compartment) is a grid cell (`ev grid`, `ev cell`) or a holder of its own",
    ),
    (
        "move_already_pending",
        "{node} already has a pending move; cancel it first",
    ),
    (
        "code_only_digits",
        "code `{code}` is only digits and would read as an id",
    ),
    ("code_in_use", "code `{code}` is already in use"),
    (
        "home_inside_another",
        "a home cannot be placed inside another node",
    ),
    (
        "needs_a_place",
        "a {kind} needs a place: give --in, or --lost if its place is unknown",
    ),
    ("holder_gone", "{node} is gone and cannot hold anything"),
    (
        "room_inside_wrong_kind",
        "a room can only be inside a home or another room, not a {kind}",
    ),
    (
        "into_itself",
        "a node cannot be moved into itself or into something it contains",
    ),
    ("address_only_home", "only a home has an address"),
    (
        "still_holds",
        "{node} still holds {count} active node(s); move or dispose of them first",
    ),
    (
        "digitize_needs_copy",
        "{node} has no copy yet; attach one with `ev photo add` or `ev doc add --for` before it leaves as digitized",
    ),
    ("not_gone", "{node} is not gone"),
    ("left_with", "{node} left with {holder}; restore that first"),
    // Refusals of the purchases.
    (
        "purchase_dismissed",
        "purchase {id} is dismissed as {as}; clear that first",
    ),
    (
        "purchase_not_enough_open",
        "purchase {id} has {open} left to link, not {qty}",
    ),
    (
        "purchase_never_a_thing",
        "purchase {id} is a {bucket} purchase; it is never a thing in the home",
    ),
    (
        "purchase_nothing_open",
        "purchase {id} has nothing left to link",
    ),
    (
        "purchase_already_bucket",
        "purchase {id} is already {bucket}",
    ),
    (
        "purchase_linked_unlink_first",
        "purchase {id} is linked to a thing; `ev buy unlink` it first",
    ),
    (
        "purchase_not_linked_to",
        "purchase {id} is not linked to node {node}",
    ),
    (
        "line_linked_unlink_first",
        "line {id} is linked to #{node}; `ev buy unlink {id} {node}` first",
    ),
    (
        "line_not_declined",
        "line {id} was not declined for #{node}",
    ),
    (
        "left_holds_no_purchase",
        "#{node} left as {how}; it holds no purchase",
    ),
    (
        "bought_after_left",
        "the purchase was bought {bought}, after #{node} left ({left})",
    ),
    // Refusals of the grids.
    (
        "grid_none",
        "this place has no grid; set one with `ev grid <ref> --cols N --rows M`",
    ),
    (
        "cells_outside_grid",
        "{cells} is outside the {cols}×{rows} grid",
    ),
    (
        "grid_too_small",
        "{count} placed box(es) would fall outside a {cols}×{rows} grid",
    ),
    (
        "face_needs_grid",
        "it has no grid; give it one with --cols and --rows",
    ),
    (
        "grid_has_boxes",
        "{count} box(es) are placed in this grid; clear their cells first",
    ),
    ("not_inside_anything", "{node} is not inside anything"),
    (
        "holder_has_no_grid",
        "{holder} has no grid; set one with `ev grid <holder> --cols N --rows M`",
    ),
    (
        "cells_do_not_fit",
        "{cells} does not fit in a {cols}×{rows} grid (columns A–{last_col}, rows 1–{rows})",
    ),
    (
        "cells_shared",
        "{a} ({a_cells}) and {b} ({b_cells}) would share cells",
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
            for start in ["Error::said(", "refuse(\n", "refuse(\""] {
                for part in text.split(start).skip(1) {
                    let id = if start.ends_with('"') {
                        part.split('"').next()
                    } else {
                        part.split('"').nth(1)
                    };
                    let Some(id) = id else { continue };
                    assert!(template(id).is_some(), "{path}: `{id}` has no sentence");
                }
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
        const LEFT: usize = 273;
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
