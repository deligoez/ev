//! The rough edges deferred from the past-belongings release, each held by a test.

use ev_core::{Inventory, NewNode};

fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent) in [
        ("Ev", "home", None),
        ("Oda", "room", Some("Ev")),
        ("Kutu", "container", Some("Oda")),
        ("Telefon", "item", Some("Oda")),
        ("Tablet", "item", Some("Oda")),
    ] {
        inv.add(NewNode {
            name: name.into(),
            kind: kind.into(),
            parent: parent.map(Into::into),
            ..Default::default()
        })
        .unwrap();
    }
    (dir, inv)
}

#[test]
fn several_planned_moves_are_made_at_once_or_none_is() {
    let (_d, mut inv) = setup();
    inv.move_to("Telefon", "Kutu", true).unwrap();
    inv.move_to("Tablet", "Kutu", true).unwrap();
    // One without a plan refuses the lot.
    let refused = inv.done_many(&["Telefon".into(), "Kutu".into()]);
    assert_eq!(refused.unwrap_err().code(), 5);
    assert!(!inv.show("Telefon", false).unwrap()["node"]["pending_to"].is_null());
    let v = inv
        .done_many(&["Telefon".into(), "Tablet".into()])
        .unwrap();
    assert_eq!(v["done"].as_array().unwrap().len(), 2, "{v}");
    assert!(inv.pending().unwrap()["pending"].as_array().unwrap().is_empty());
}

#[test]
fn several_planned_moves_are_dropped_at_once() {
    let (_d, mut inv) = setup();
    inv.move_to("Telefon", "Kutu", true).unwrap();
    inv.move_to("Tablet", "Kutu", true).unwrap();
    let v = inv
        .cancel_many(&["Telefon".into(), "Tablet".into()])
        .unwrap();
    assert_eq!(v["cancelled"].as_array().unwrap().len(), 2, "{v}");
    assert!(inv.pending().unwrap()["pending"].as_array().unwrap().is_empty());
}
