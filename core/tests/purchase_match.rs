//! Which purchase lines could be this thing (purchases spec §5).

use ev_core::{Inventory, NewNode};
use serde_json::{Value, json};

fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent) in [("Ev", "home", None), ("Oda", "room", Some("Ev"))] {
        inv.add(NewNode {
            name: name.into(),
            kind: kind.into(),
            parent: parent.map(Into::into),
            ..Default::default()
        })
        .unwrap();
    }
    let lines = [
        json!({"source": "s", "key": "1", "shop": "Shop", "sku": "P1", "brand": "Mettzchrom",
               "name": "Mettzchrom AG10 LR1130 1.5V Alkaline Düğme Pil 100 Adet"}),
        json!({"source": "s", "key": "2", "shop": "Shop", "sku": "P2",
               "name": "125 Khz T5577 RFID Kart Yazdırılabilir"}),
        json!({"source": "s", "key": "3", "shop": "Shop", "sku": "P3", "brand": "Pro's Kit",
               "name": "Pro's Kit 1PK-052DS Pense"}),
        json!({"source": "s", "key": "4", "shop": "Shop", "sku": "P4", "brand": "Shop",
               "name": "Shop Aylık Premium"}),
    ];
    inv.buy_import(
        &lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    (dir, inv)
}

fn item(inv: &mut Inventory, name: &str, note: Option<&str>) -> Value {
    inv.add(NewNode {
        name: name.into(),
        kind: "item".into(),
        parent: Some("Oda".into()),
        note: note.map(Into::into),
        ..Default::default()
    })
    .unwrap()
}

fn first(inv: &Inventory, r: &str) -> Value {
    inv.buy_for(r).unwrap()["candidates"][0].clone()
}

#[test]
fn shared_model_codes_and_the_brand_put_the_right_line_first() {
    let (_d, mut inv) = setup();
    let v = item(&mut inv, "Mettzchrom LR1130/AG10 düğme piller", None);
    let offered = v["purchase_candidates"].as_array().unwrap();
    assert_eq!(
        offered[0]["purchase"]["name"].as_str().unwrap()[..10],
        *"Mettzchrom"
    );
    let why: Vec<&str> = offered[0]["why"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["why"].as_str().unwrap())
        .collect();
    assert!(
        why.contains(&"code lr1130") && why.contains(&"code ag10"),
        "{why:?}"
    );
    assert!(why.contains(&"brand mettzchrom"), "{why:?}");
}

#[test]
fn a_brand_written_differently_still_counts_and_the_model_field_is_an_exact_key() {
    let (_d, mut inv) = setup();
    item(&mut inv, "Kombine pense, Pro'sKit", None);
    let c = first(&inv, "Kombine pense, Pro'sKit");
    assert_eq!(c["purchase"]["name"], "Pro's Kit 1PK-052DS Pense");
    let before = c["score"].as_f64().unwrap();
    inv.edit("Kombine pense, Pro'sKit", &["model=1PK-052DS".into()])
        .unwrap();
    let after = first(&inv, "Kombine pense, Pro'sKit")["score"]
        .as_f64()
        .unwrap();
    assert!(after >= before + 60.0, "{before} → {after}");
}

#[test]
fn a_number_of_the_same_unit_that_differs_rules_a_line_out() {
    let (_d, mut inv) = setup();
    let v = item(&mut inv, "MIFARE RFID kart, 13,56 MHz", None);
    assert!(v.get("purchase_candidates").is_none(), "{v}");
    // The shared words ("rfid", "kart") are outweighed: the line scores below zero and is not
    // a candidate at all.
    let c = inv.buy_for("MIFARE RFID kart, 13,56 MHz").unwrap()["candidates"].clone();
    assert!(
        c.as_array()
            .unwrap()
            .iter()
            .all(|x| !x["purchase"]["name"].as_str().unwrap().contains("125 Khz")),
        "{c}"
    );
    // Without the frequency the same words make it a candidate.
    item(&mut inv, "MIFARE RFID kart", None);
    assert!(
        first(&inv, "MIFARE RFID kart")["purchase"]["name"]
            .as_str()
            .unwrap()
            .contains("125 Khz")
    );
}

#[test]
fn the_shop_named_as_the_brand_and_a_note_mention_do_not_count() {
    let (_d, mut inv) = setup();
    item(&mut inv, "Kedi tırmalama tahtası", Some("Shop'tan alındı"));
    let c = inv.buy_for("Kedi tırmalama tahtası").unwrap()["candidates"].clone();
    assert!(
        c.as_array()
            .unwrap()
            .iter()
            .all(|x| x["score"].as_f64().unwrap() < 15.0),
        "{c}"
    );
}

#[test]
fn a_linked_or_dismissed_line_is_offered_no_more_except_to_its_own_thing() {
    let (_d, mut inv) = setup();
    item(&mut inv, "Pense A, Pro'sKit", None);
    item(&mut inv, "Pense B, Pro'sKit", None);
    let line = first(&inv, "Pense A, Pro'sKit")["purchase"]["id"]
        .as_i64()
        .unwrap();
    inv.buy_link(line, "Pense A, Pro'sKit", None).unwrap();
    assert_eq!(first(&inv, "Pense A, Pro'sKit")["linked"], true);
    assert!(
        inv.buy_for("Pense B, Pro'sKit").unwrap()["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["purchase"]["id"] != line)
    );
}
