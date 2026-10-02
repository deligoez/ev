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

#[test]
fn more_than_there_is_a_box_and_a_holder_are_not_split_and_nothing_changes() {
    let (_d, mut inv) = setup();
    assert_eq!(
        inv.move_qty("Eneloop AA", "Oyuncak", false, Some(21))
            .unwrap_err()
            .code(),
        5
    );
    assert_eq!(
        inv.move_qty("Eneloop AA", "Oyuncak", false, Some(0))
            .unwrap_err()
            .code(),
        2
    );
    // A box is not a thing kept in several places.
    inv.edit("D1", &["qty=2".into()]).unwrap();
    assert_eq!(
        inv.move_qty("D1", "Ev", false, Some(1)).unwrap_err().code(),
        5
    );
    // A device with cells in it: which of its units would hold them?
    inv.edit("El feneri", &["qty=2".into()]).unwrap();
    inv.move_to("Eneloop AA", "El feneri", false).unwrap();
    assert_eq!(
        inv.move_qty("El feneri", "Ev", false, Some(1))
            .unwrap_err()
            .code(),
        5
    );
    assert_eq!(inv.show("El feneri", false).unwrap()["node"]["qty"], 2);
    assert!(inv.show("Eneloop AA", false).unwrap()["thing"].is_null());
}

#[test]
fn more_of_a_thing_takes_what_it_is_and_joins_a_portion_already_there() {
    let (_d, mut inv) = setup();
    inv.edit("Eneloop AA", &["make=Panasonic".into(), "tags=+pil".into()])
        .unwrap();
    // 4 more turn up in the toy: a new portion, the thing's identity.
    let toy = inv
        .add(NewNode {
            of: Some("Eneloop AA".into()),
            parent: Some("Oyuncak".into()),
            qty: Some(4),
            note: Some("oyuncağın içinden çıktı".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(toy["node"]["name"], "Eneloop AA");
    assert_eq!(toy["node"]["make"], "Panasonic");
    assert_eq!(toy["node"]["tags"], serde_json::json!(["pil"]));
    assert_eq!(toy["node"]["note"], "oyuncağın içinden çıktı");
    assert_eq!(toy["thing"]["total"], 24);
    // 2 more in the drawer join the 20 there.
    let drawer = inv
        .add(NewNode {
            of: Some(id(&toy)),
            parent: Some("D1".into()),
            qty: Some(2),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(id(&drawer), "6");
    assert_eq!(drawer["node"]["qty"], 22);
    assert_eq!(drawer["thing"]["total"], 26);
    assert_eq!(drawer["thing"]["places"], 2);
}
