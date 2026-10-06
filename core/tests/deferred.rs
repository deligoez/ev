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

fn line(inv: &mut Inventory, name: &str) -> i64 {
    inv.buy_add(&serde_json::json!({"name": name, "qty": 1}), None)
        .unwrap()["purchase"]["id"]
        .as_i64()
        .unwrap()
}

#[test]
fn a_lines_bucket_is_said_by_hand_but_not_for_a_linked_line() {
    let (_d, mut inv) = setup();
    let l = line(&mut inv, "Üyelik");
    let v = inv.buy_bucket(l, "digital").unwrap();
    assert_eq!(v["purchase"]["bucket"], "digital");
    assert_eq!(inv.buy_bucket(l, "digital").unwrap_err().code(), 5);
    assert_eq!(inv.buy_bucket(l, "consumable").unwrap_err().code(), 2);
    let linked = line(&mut inv, "Telefon");
    inv.buy_link(linked, "Telefon", None).unwrap();
    assert_eq!(inv.buy_bucket(linked, "service").unwrap_err().code(), 5);
}

#[test]
fn an_import_that_does_not_say_a_lines_bucket_keeps_the_one_set_by_hand() {
    let (_d, mut inv) = setup();
    let line = r#"{"source":"mail","key":"r1","name":"Üyelik","qty":1}"#;
    inv.buy_import(line).unwrap();
    let id = inv.buy_list(false, None, None, None).unwrap()["purchases"][0]["id"]
        .as_i64()
        .unwrap();
    inv.buy_bucket(id, "digital").unwrap();
    inv.buy_import(line).unwrap();
    assert_eq!(inv.buy_show(id).unwrap()["purchase"]["bucket"], "digital");
    let said = r#"{"source":"mail","key":"r1","name":"Üyelik","qty":1,"bucket":"service"}"#;
    inv.buy_import(said).unwrap();
    assert_eq!(inv.buy_show(id).unwrap()["purchase"]["bucket"], "service");
}
