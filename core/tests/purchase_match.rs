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

#[test]
fn a_part_split_off_is_offered_its_purchase_too() {
    let (_d, mut inv) = setup();
    item(&mut inv, "Karışık el aletleri", None);
    let v = inv
        .split(
            "Karışık el aletleri",
            &[("Pense, Pro'sKit 1PK-052DS".into(), Some(1))],
            None,
            None,
        )
        .unwrap();
    let offered = v["into"][0]["purchase_candidates"].as_array().unwrap();
    assert_eq!(offered[0]["purchase"]["name"], "Pro's Kit 1PK-052DS Pense");
}

/// Puts a node of `kind` in `parent`.
fn put(inv: &mut Inventory, name: &str, kind: &str, parent: &str) {
    inv.add(NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: Some(parent.into()),
        ..Default::default()
    })
    .unwrap();
}

#[test]
fn the_back_fill_offers_one_line_for_each_unlinked_thing_in_a_toured_place_best_first() {
    let (_d, mut inv) = setup();
    put(&mut inv, "Çekmece", "container", "Oda");
    put(&mut inv, "Mettzchrom LR1130 düğme pil", "item", "Çekmece");
    put(&mut inv, "Kombine pense, Pro'sKit", "item", "Çekmece");
    put(&mut inv, "Kâğıt", "item", "Çekmece");
    // A box with a review of its own: what is in it stands under that, not the drawer's.
    put(&mut inv, "Kart kutusu", "container", "Çekmece");
    put(&mut inv, "RFID kart", "item", "Kart kutusu");
    // Not toured: the room itself.
    put(&mut inv, "Pense 1PK-052DS", "item", "Oda");
    // Already bought: a thing linked to a line is not asked about again.
    put(&mut inv, "Yedek LR1130 pil", "item", "Çekmece");
    inv.buy_add(
        &json!({"name": "LR1130 pil", "shop": "Other"}),
        Some("Yedek LR1130 pil"),
    )
    .unwrap();
    inv.review("Kart kutusu", "counting", None).unwrap();
    inv.photo_current("Çekmece").unwrap();
    inv.review("Çekmece", "toured", None).unwrap();

    let v = inv.buy_backfill().unwrap();
    // The pill, the pliers, the paper and the box itself; not the card in the counted box,
    // the pliers in the room, or the linked spare.
    assert_eq!(v["toured_things"], 4, "{v}");
    let names: Vec<(&str, &str)> = v["backfill"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["node"]["name"].as_str().unwrap(),
                b["candidate"]["purchase"]["name"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        names,
        [
            (
                "Mettzchrom LR1130 düğme pil",
                "Mettzchrom AG10 LR1130 1.5V Alkaline Düğme Pil 100 Adet"
            ),
            ("Kombine pense, Pro'sKit", "Pro's Kit 1PK-052DS Pense"),
        ],
        "{v}"
    );
    let scores: Vec<f64> = v["backfill"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["candidate"]["score"].as_f64().unwrap())
        .collect();
    assert!(scores[0] >= scores[1] && scores[1] > 15.0, "{scores:?}");

    // The box's own tour brings its card up; the same line stays offered to each thing.
    inv.photo_current("Kart kutusu").unwrap();
    inv.review("Kart kutusu", "toured", None).unwrap();
    let v = inv.buy_backfill().unwrap();
    assert_eq!(v["toured_things"], 5, "{v}");
}

/// Lines that share only what a thing is for, made of or used with.
fn import(inv: &mut Inventory, lines: &[Value]) {
    inv.buy_import(
        &lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
}

#[test]
fn words_shared_only_with_what_it_is_for_are_never_offered() {
    let (_d, mut inv) = setup();
    import(
        &mut inv,
        &[json!({"source": "t", "key": "1", "shop": "Shop",
                 "name": "Akım Korumalı Uzatma Priz, USB Girişli, Korumalı Kablo"})],
    );
    let v = item(
        &mut inv,
        "TP4056 Li-ion şarj modülü, USB girişli, korumalı",
        None,
    );
    assert!(v.get("purchase_candidates").is_none(), "{v}");
}

#[test]
fn a_code_in_the_note_is_not_what_the_thing_is() {
    let (_d, mut inv) = setup();
    import(
        &mut inv,
        &[json!({"source": "t", "key": "1", "shop": "Shop",
                 "name": "M5Stack StickC Plus2 ESP32 IoT kit"})],
    );
    let v = item(
        &mut inv,
        "MQ-2 gaz sensörü modülü",
        Some("ESP32 ile kullanılıyor"),
    );
    assert!(v.get("purchase_candidates").is_none(), "{v}");
}

#[test]
fn a_brand_alone_does_not_offer_another_product_of_it() {
    let (_d, mut inv) = setup();
    let v = item(&mut inv, "Otomatik kablo sıyırıcı, Pro'sKit CP-367A", None);
    assert!(v.get("purchase_candidates").is_none(), "{v}");
    // Its own kind of thing from the brand still comes first, with the brand counted in full.
    item(&mut inv, "Pense, Pro'sKit (yeşil saplı)", None);
    let c = first(&inv, "Pense, Pro'sKit (yeşil saplı)");
    assert_eq!(c["purchase"]["name"], "Pro's Kit 1PK-052DS Pense");
    assert_eq!(c["why"][0]["points"], 12.0, "{c}");
}

#[test]
fn an_ampere_is_read_only_where_it_is_written_as_one() {
    let (_d, mut inv) = setup();
    import(
        &mut inv,
        &[json!({"source": "t", "key": "1", "shop": "Shop",
                 "name": "Raspberry Pi resmi güç kaynağı USB-C 5.1V 3A"})],
    );
    let v = item(
        &mut inv,
        "Raspberry Pi resmi micro-USB güç kaynağı (5,1 V 2,5 A)",
        Some("Pi 3 A+ gibi micro-USB girişli kartlar için"),
    );
    assert!(v.get("purchase_candidates").is_none(), "{v}");
}

#[test]
fn a_declined_line_is_offered_to_other_things_but_never_again_to_that_one() {
    let (_d, mut inv) = setup();
    item(&mut inv, "Mettzchrom LR1130 düğme pil", None);
    item(&mut inv, "Mettzchrom AG10 düğme pil", None);
    let line = first(&inv, "Mettzchrom LR1130 düğme pil")["purchase"]["id"]
        .as_i64()
        .unwrap();
    let v = inv
        .buy_decline(
            line,
            "Mettzchrom LR1130 düğme pil",
            Some("başka paket"),
            false,
        )
        .unwrap();
    assert_eq!(v["purchase"]["declined"][0]["why"], "başka paket");
    assert_eq!(v["purchase"]["open_qty"], 1);
    assert!(
        inv.buy_for("Mettzchrom LR1130 düğme pil").unwrap()["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["purchase"]["id"] != line)
    );
    assert_eq!(
        first(&inv, "Mettzchrom AG10 düğme pil")["purchase"]["id"],
        line
    );
    inv.buy_decline(line, "Mettzchrom LR1130 düğme pil", None, true)
        .unwrap();
    assert_eq!(
        first(&inv, "Mettzchrom LR1130 düğme pil")["purchase"]["id"],
        line
    );
}

#[test]
fn a_make_and_model_learned_later_offers_the_purchases_as_add_does() {
    let (_d, mut inv) = setup();
    let v = item(&mut inv, "Pense", None);
    assert!(v.get("purchase_candidates").is_none(), "{v}");
    let v = inv
        .edit("Pense", &["make=Pro'sKit".into(), "model=1PK-052DS".into()])
        .unwrap();
    let offered = v["purchase_candidates"].as_array().expect("offered");
    assert_eq!(offered[0]["purchase"]["name"], "Pro's Kit 1PK-052DS Pense");
    // Any other field is no reason to ask again.
    let v = inv.edit("Pense", &["note=masada".into()]).unwrap();
    assert!(v.get("purchase_candidates").is_none(), "{v}");
}
