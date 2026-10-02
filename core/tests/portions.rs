//! One thing kept in several places (spec/portions.md): portions that split off, join where
//! they meet, share what the thing is, and add up to the thing.

use ev_core::{Inventory, NewNode};
use serde_json::Value;

/// A drawer with 20 rechargeable cells, a flashlight and a toy beside it.
fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent, code, qty) in [
        ("Ev", "home", None, None, None),
        ("Oda", "room", Some("Ev"), None, None),
        ("Çekmece", "container", Some("Oda"), Some("D1"), None),
        ("El feneri", "item", Some("Oda"), None, None),
        ("Oyuncak", "item", Some("Oda"), None, None),
        ("Eneloop AA", "item", Some("D1"), None, Some(20)),
    ] {
        inv.add(NewNode {
            name: name.into(),
            kind: kind.into(),
            parent: parent.map(Into::into),
            code: code.map(Into::into),
            qty,
            ..Default::default()
        })
        .unwrap();
    }
    (dir, inv)
}

fn id(v: &Value) -> String {
    v["node"]["id"].to_string()
}

#[test]
fn some_units_move_as_a_portion_and_join_the_rest_when_they_come_back() {
    let (_d, mut inv) = setup();
    let lamp = inv
        .move_qty("Eneloop AA", "El feneri", false, Some(2))
        .unwrap();
    let toy = inv.move_qty("#6", "Oyuncak", false, Some(2)).unwrap();
    assert_eq!(toy["node"]["qty"], 2);
    let thing = &toy["thing"];
    assert_eq!(
        (
            &thing["total"],
            &thing["places"],
            &thing["in_use"],
            &thing["spare"]
        ),
        (&20.into(), &3.into(), &4.into(), &16.into())
    );
    // The flashlight's two go back to the drawer: they join the 16 there.
    let back = inv.move_to(&id(&lamp), "D1", false).unwrap();
    assert_eq!(id(&back), "6");
    assert_eq!(back["node"]["qty"], 18);
    assert_eq!(back["thing"]["places"], 2);
    assert_eq!(back["thing"]["total"], 20);
    let gone = inv.show(&id(&lamp), true).unwrap();
    assert_eq!(gone["node"]["disposition"], "merged");
}

#[test]
fn what_the_thing_is_set_on_one_portion_is_set_on_all_and_a_kind_is_refused() {
    let (_d, mut inv) = setup();
    let lamp = inv
        .move_qty("Eneloop AA", "El feneri", false, Some(2))
        .unwrap();
    inv.edit(
        &id(&lamp),
        &[
            "model=BK-3MCCE".into(),
            "tags=+pil".into(),
            "note=şarjlı".into(),
        ],
    )
    .unwrap();
    let drawer = inv.show("#6", false).unwrap();
    assert_eq!(drawer["node"]["model"], "BK-3MCCE");
    assert_eq!(drawer["node"]["tags"], serde_json::json!(["pil"]));
    // A note is the portion's own.
    assert!(drawer["node"]["note"].is_null(), "{drawer}");
    let e = inv
        .edit(&id(&lamp), &["kind=container".into()])
        .unwrap_err();
    assert_eq!(e.code(), 5);
}

#[test]
fn a_planned_move_of_some_sets_them_apart_and_done_joins_them_on_arrival() {
    let (_d, mut inv) = setup();
    inv.move_qty("Eneloop AA", "El feneri", false, Some(2))
        .unwrap();
    // 3 of the drawer's 18 are to go to the flashlight: set apart in the drawer for now.
    let planned = inv.move_qty("#6", "El feneri", true, Some(3)).unwrap();
    assert_eq!(planned["node"]["qty"], 3);
    assert_eq!(planned["node"]["path_text"], "Ev › Oda › D1 › Eneloop AA");
    assert_eq!(planned["pending"]["name"], "El feneri");
    assert_eq!(inv.show("#6", false).unwrap()["node"]["qty"], 15);
    // Done: they join the two already in the flashlight.
    let done = inv.done(&id(&planned)).unwrap();
    assert_eq!(id(&done), "7");
    assert_eq!(done["node"]["qty"], 5);
    assert_eq!(done["thing"]["total"], 20);
}
