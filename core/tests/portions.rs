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

#[test]
fn records_made_separately_join_into_one_thing_and_a_different_model_is_refused() {
    let (_d, mut inv) = setup();
    let add = |inv: &mut Inventory, parent: &str, qty: i64, model: Option<&str>| {
        inv.add(NewNode {
            name: "Eneloop AA pil".into(),
            kind: "item".into(),
            parent: Some(parent.into()),
            qty: Some(qty),
            model: model.map(Into::into),
            tags: vec!["şarjlı".into()],
            ..Default::default()
        })
        .unwrap()
    };
    let toy = add(&mut inv, "Oyuncak", 4, Some("BK-3MCCE"));
    let other = add(&mut inv, "El feneri", 2, Some("BK-3HCDE"));
    let e = inv
        .join(&["Eneloop AA".into(), id(&toy), id(&other)])
        .unwrap_err();
    assert_eq!(e.code(), 5);
    assert!(inv.show("Eneloop AA", false).unwrap()["thing"].is_null());
    // Without the other model: the toy's 4 become a portion, the drawer's name and the toy's
    // model and tags hold for both; 4 more recorded apart in the drawer join the 20 there.
    let drawer_copy = add(&mut inv, "D1", 4, None);
    let v = inv
        .join(&["Eneloop AA".into(), id(&toy), id(&drawer_copy)])
        .unwrap();
    assert_eq!(id(&v), "6");
    assert_eq!(v["node"]["qty"], 24);
    assert_eq!(v["node"]["model"], "BK-3MCCE");
    assert_eq!(v["node"]["tags"], serde_json::json!(["şarjlı"]));
    assert_eq!(v["thing"]["total"], 28);
    assert_eq!(v["thing"]["places"], 2);
    let toy = inv.show(&id(&toy), false).unwrap();
    assert_eq!(toy["node"]["name"], "Eneloop AA");
    // The toy's portion goes its own way again.
    let alone = inv.unjoin(&id(&toy)).unwrap();
    assert!(alone["thing"].is_null());
    assert!(inv.show("#6", false).unwrap()["thing"].is_null());
}

#[test]
fn audit_leaves_a_spread_thing_alone_and_hints_join_for_the_same_name_recorded_twice() {
    let (_d, mut inv) = setup();
    inv.move_qty("Eneloop AA", "El feneri", false, Some(2))
        .unwrap();
    let rows = |inv: &Inventory| -> Vec<Value> {
        inv.audit().unwrap()["spread"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["word"] == "eneloop")
            .cloned()
            .collect()
    };
    assert!(rows(&inv).is_empty(), "{:?}", rows(&inv));
    // The same name recorded apart in the toy: one thing recorded twice?
    inv.add(NewNode {
        name: "Eneloop AA".into(),
        kind: "item".into(),
        parent: Some("Oyuncak".into()),
        ..Default::default()
    })
    .unwrap();
    // Alike in two places again, and one hint for the name, each record of it named.
    assert_eq!(rows(&inv).len(), 1, "{:?}", rows(&inv));
    let same = inv.audit().unwrap()["same_name"].clone();
    assert_eq!(same.as_array().unwrap().len(), 1, "{same}");
    assert_eq!(same[0]["name"], "Eneloop AA");
    assert_eq!(same[0]["nodes"].as_array().unwrap().len(), 3);
}

#[test]
fn some_units_leave_are_lent_or_go_missing_while_the_rest_stay() {
    let (_d, mut inv) = setup();
    // 2 of the 20 are dead: they leave, 18 stay.
    let gone = inv
        .gone_qty(
            "Eneloop AA",
            Some(ev_core::Disposition::Trash),
            None,
            false,
            Some(2),
        )
        .unwrap();
    assert_eq!(gone["node"]["qty"], 2);
    assert_eq!(gone["node"]["state"], "gone");
    assert_eq!(inv.show("#6", false).unwrap()["node"]["qty"], 18);
    // 3 lent to a neighbour come back and join the rest.
    let lent = inv.lend_qty("#6", "Komşu", Some(3)).unwrap();
    assert_eq!(inv.show("#6", false).unwrap()["node"]["qty"], 15);
    let back = inv.back(&id(&lent)).unwrap();
    assert_eq!(id(&back), "6");
    assert_eq!(back["node"]["qty"], 18);
    // 1 goes missing, then turns up where it was.
    let lost = inv.mark_lost_qty("#6", Some(1)).unwrap();
    assert_eq!(lost["node"]["lost"], true);
    let shown = inv.show("#6", false).unwrap();
    assert_eq!(shown["thing"]["total"], 17);
    assert_eq!(shown["thing"]["lost"], 1);
    let found = inv.found(&id(&lost)).unwrap();
    assert_eq!(id(&found), "6");
    assert_eq!(found["node"]["qty"], 18);
    // A verb that refuses after the split leaves nothing split: gone needs --as here.
    assert_eq!(
        inv.gone_qty("#6", None, None, false, Some(2))
            .unwrap_err()
            .code(),
        5
    );
    let shown = inv.show("#6", false).unwrap();
    assert_eq!(shown["node"]["qty"], 18);
    // One place left, and the account of the rest: 2 thrown out.
    assert_eq!(shown["thing"]["places"], 1, "{shown}");
    assert_eq!(shown["thing"]["gone"], serde_json::json!({ "trash": 2 }));
}

#[test]
fn a_purchase_linked_on_one_portion_is_the_things_and_accounts_for_its_units() {
    let (_d, mut inv) = setup();
    inv.buy_add(
        &serde_json::json!({
            "name": "Eneloop AA 20'li", "shop": "Dükkan", "ordered_at": "2026-01-05",
            "paid": "1999", "currency": "TRY", "qty": 1, "pack": 20,
        }),
        Some("Eneloop AA"),
    )
    .unwrap();
    let lamp = inv
        .move_qty("Eneloop AA", "El feneri", false, Some(2))
        .unwrap();
    // Linked on the drawer's record, it is the flashlight's too, marked as on the drawer's.
    assert_eq!(lamp["purchases"][0]["on"], 6);
    assert_eq!(lamp["thing"]["bought"], 20);
    assert_eq!(lamp["thing"]["unaccounted"], 0);
    // 1 of the flashlight's is used up: gone, so still accounted for.
    inv.gone_qty(
        &id(&lamp),
        Some(ev_core::Disposition::Used),
        None,
        false,
        Some(1),
    )
    .unwrap();
    let drawer = inv.show("#6", false).unwrap();
    assert_eq!(drawer["thing"]["gone"], serde_json::json!({ "used": 1 }));
    assert_eq!(drawer["thing"]["unaccounted"], 0);
    // 4 more found than were bought: more here than the purchases say.
    let more = inv
        .add(NewNode {
            of: Some("#6".into()),
            parent: Some("Oyuncak".into()),
            qty: Some(4),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(more["thing"]["unaccounted"], -4);
}

#[test]
fn a_thing_its_purchases_account_for_asks_nothing_and_more_than_bought_asks_again() {
    let (_d, mut inv) = setup();
    inv.edit("Eneloop AA", &["model=BK-3MCCE".into()]).unwrap();
    let line = |name: &str, pack: i64| {
        serde_json::json!({
            "name": name, "shop": "Dükkan", "ordered_at": "2026-01-05", "paid": "1999",
            "currency": "TRY", "qty": 1, "pack": pack,
        })
    };
    inv.buy_add(&line("Eneloop AA BK-3MCCE 20'li", 20), Some("Eneloop AA"))
        .unwrap();
    inv.buy_add(&line("Eneloop AA BK-3MCCE 4'lü", 4), None)
        .unwrap();
    // The flashlight's 2 are the 20-pack's, linked on the drawer's record.
    let lamp = inv
        .move_qty("Eneloop AA", "El feneri", false, Some(2))
        .unwrap();
    let ranked = inv.buy_for(&id(&lamp)).unwrap()["candidates"].clone();
    let twenty = ranked
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["purchase"]["name"] == "Eneloop AA BK-3MCCE 20'li")
        .cloned()
        .unwrap();
    assert_eq!(twenty["linked"], true);
    // 2 used up, 2 more found: still 20 here, all bought; nothing to ask.
    inv.gone_qty(
        &id(&lamp),
        Some(ev_core::Disposition::Used),
        None,
        false,
        Some(2),
    )
    .unwrap();
    let v = inv
        .add(NewNode {
            of: Some("#6".into()),
            parent: Some("Oyuncak".into()),
            qty: Some(2),
            ..Default::default()
        })
        .unwrap();
    assert!(v.get("purchase_candidates").is_none(), "{v}");
    // 4 more than were bought: which purchase were they?
    let v = inv
        .add(NewNode {
            of: Some("#6".into()),
            parent: Some("Oyuncak".into()),
            qty: Some(4),
            ..Default::default()
        })
        .unwrap();
    let offered = v["purchase_candidates"].as_array().expect("offered");
    assert_eq!(offered[0]["purchase"]["name"], "Eneloop AA BK-3MCCE 4'lü");
    assert!(offered.iter().all(|c| c["linked"] != true));
}

#[test]
fn the_same_one_put_in_for_a_used_up_one_is_more_of_that_thing() {
    let (_d, mut inv) = setup();
    inv.gone("El feneri", Some(ev_core::Disposition::Used))
        .unwrap();
    let new = inv
        .add(NewNode {
            of: Some("El feneri".into()),
            parent: Some("Oda".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(new["node"]["name"], "El feneri");
    assert_eq!(new["thing"]["total"], 1);
    assert_eq!(new["thing"]["gone"], serde_json::json!({"used": 1}));
}
