//! Errors with ids (spec/error-ids.md): what an agent reads in the JSON of an error.

use ev_core::{Inventory, NewNode};

#[test]
fn a_batch_lines_error_says_its_line_apart_from_its_sentence() {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    let node = |name: &str, kind: &str, parent: Option<&str>| NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: parent.map(Into::into),
        ..Default::default()
    };
    let e = inv
        .add_batch(vec![
            node("Ev", "home", None),
            node("Kutu", "container", Some("Salon")),
        ])
        .unwrap_err();
    assert_eq!(e.code(), 3);
    assert_eq!(e.id(), Some("no_record_matches"));
    assert_eq!(
        e.to_string(),
        "line 2: no node matches `Salon`; search with `ev find` and retry with an id"
    );
    let j = e.to_json();
    assert_eq!(j["error"]["values"]["ref"], "Salon");
    assert_eq!(j["error"]["at"], serde_json::json!([{ "line": 2 }]));
}
