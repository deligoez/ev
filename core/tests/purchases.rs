//! Purchases (purchases spec §3.2): lines of what was bought, linked to things on the person's
//! word; imported from an adapter's NDJSON or entered by hand.

use ev_core::{Inventory, NewNode};
use serde_json::{Value, json};

fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent) in [
        ("Ev", "home", None),
        ("Oda", "room", Some("Ev")),
        ("Matkap", "item", Some("Oda")),
        ("Kart", "item", Some("Oda")),
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

/// Two durable lines of one order, a consumable and a cancelled line, and the order's invoice;
/// a second invoice for an order whose only line is a consumable.
fn export(dir: &tempfile::TempDir, drill_paid: &str) -> String {
    let inv1 = dir.path().join("inv1.pdf");
    let inv2 = dir.path().join("inv2.pdf");
    std::fs::write(&inv1, "%PDF 1").unwrap();
    std::fs::write(&inv2, "%PDF 2").unwrap();
    [
        json!({"type": "purchase", "source": "shop", "key": "o1:a", "shop": "Shop",
               "order": "o1", "sku": "SKU-A", "name": "Bosch GSB 13 RE Darbeli Matkap",
               "ordered_at": "2024-05-01", "delivered_at": "2024-05-03", "qty": 1,
               "paid": drill_paid, "currency": "try"}),
        json!({"type": "purchase", "source": "shop", "key": "o1:b", "shop": "Shop",
               "order": "o1", "sku": "SKU-B", "name": "Kingston 128 GB microSD",
               "qty": 2, "paid": "800.00", "currency": "TRY"}),
        json!({"type": "purchase", "source": "shop", "key": "o2:c", "name": "Kedi maması",
               "bucket": "consumable"}),
        json!({"type": "purchase", "source": "shop", "key": "o3:d", "name": "Vazgeçilen",
               "status": "cancelled"}),
        json!({"type": "document", "source": "shop", "kind": "invoice",
               "file": inv1.to_str().unwrap(), "purchases": ["o1:a", "o1:b"]}),
        json!({"type": "document", "source": "shop", "kind": "invoice",
               "file": inv2.to_str().unwrap(), "purchases": ["o2:c"]}),
    ]
    .iter()
    .map(Value::to_string)
    .collect::<Vec<_>>()
    .join("\n")
}

fn id_of(inv: &Inventory, name: &str) -> i64 {
    inv.buy_list(false, None, None, None).unwrap()["purchases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"].as_str().unwrap().contains(name))
        .unwrap()["id"]
        .as_i64()
        .unwrap()
}

#[test]
fn the_list_counts_what_came_with_a_line_and_show_has_it() {
    let (d, mut inv) = setup();
    inv.buy_import(&export(&d, "1999.00")).unwrap();
    let drill = id_of(&inv, "Matkap");
    let list = inv.buy_list(false, None, None, None).unwrap();
    let row = list["purchases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == drill)
        .unwrap();
    assert_eq!(row["documents"], 1);
    assert_eq!(row["attachments"], json!({}));
    assert_eq!(row["source_key"], "o1:a");
    assert!(
        row.get("raw").is_none() && row.get("imported_at").is_none(),
        "{row}"
    );
    let shown = &inv.buy_show(drill).unwrap()["purchase"];
    assert_eq!(shown["documents"].as_array().unwrap().len(), 1);
    assert!(shown.get("imported_at").is_some(), "{shown}");
}

#[test]
fn importing_twice_changes_nothing_and_skips_consumables_and_cancelled_lines() {
    let (d, mut inv) = setup();
    let first = inv.buy_import(&export(&d, "1999.00")).unwrap();
    assert_eq!(
        first["imported"],
        json!({"new": 2, "updated": 0, "unchanged": 0, "skipped": 2,
               "document_links": 2, "documents_skipped": 1, "attachments": 0,
               "attachments_skipped": 0, "joined": 0})
    );
    let again = inv.buy_import(&export(&d, "1999.00")).unwrap();
    assert_eq!(again["imported"]["new"], 0);
    assert_eq!(again["imported"]["unchanged"], 2);
    assert_eq!(again["imported"]["document_links"], 0);
    // Only the invoice of an imported line is stored.
    assert_eq!(
        inv.doc_list(None).unwrap()["documents"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let p = &inv.buy_show(id_of(&inv, "Bosch")).unwrap()["purchase"];
    assert_eq!(p["paid"], "1999.00");
    assert_eq!(p["currency"], "TRY");
    assert_eq!(p["documents"][0]["kind"], "invoice");
}

#[test]
fn a_link_takes_part_of_a_line_and_the_thing_reaches_the_invoice_through_it() {
    let (d, mut inv) = setup();
    inv.buy_import(&export(&d, "1999.00")).unwrap();
    let cards = id_of(&inv, "Kingston");
    let p = &inv.buy_link(cards, "Kart", Some(1)).unwrap()["purchase"];
    assert_eq!(p["open_qty"], 1);
    assert!(inv.buy_link(cards, "Matkap", Some(2)).is_err());
    let show = inv.show("Kart", false).unwrap();
    assert_eq!(show["purchases"][0]["linked_qty"], 1);
    assert_eq!(show["documents"][0]["via_purchase"], cards);
    let history: Vec<String> = inv.history("Kart").unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["type"].as_str().unwrap().to_string())
        .collect();
    assert!(history.contains(&"purchase_linked".to_string()));
    // Linking again with no quantity takes all that is left of the line.
    let p = &inv.buy_link(cards, "Kart", None).unwrap()["purchase"];
    assert_eq!(p["open_qty"], 0);
    assert_eq!(p["linked"][0]["qty"], 2);
}

#[test]
fn a_pack_spreads_one_bought_line_over_several_things() {
    let (_d, mut inv) = setup();
    let cells = inv
        .buy_add(
            &json!({"name": "AA cells, 8-pack", "qty": 1, "paid": "160.00", "currency": "TRY"}),
            None,
        )
        .unwrap()["purchase"]["id"]
        .as_i64()
        .unwrap();
    // A line of one pack links once, all of it.
    assert_eq!(inv.buy_show(cells).unwrap()["purchase"]["open_qty"], 1);
    let p = &inv.buy_pack(cells, 8).unwrap()["purchase"];
    assert_eq!(p["units"], 8);
    assert_eq!(p["open_qty"], 8);
    inv.buy_link(cells, "Matkap", Some(3)).unwrap();
    let p = &inv.buy_link(cells, "Kart", Some(5)).unwrap()["purchase"];
    assert_eq!(p["open_qty"], 0);
    assert_eq!(p["linked"].as_array().unwrap().len(), 2);
    // Each thing sees only its own units of the line.
    let show = inv.show("Kart", false).unwrap();
    assert_eq!(show["purchases"][0]["linked_qty"], 5);
}

#[test]
fn a_pack_linked_without_a_count_gives_each_thing_its_own_units() {
    let (_d, mut inv) = setup();
    let set = inv
        .buy_add(
            &json!({"name": "Set: body, lens, two caps", "qty": 1}),
            None,
        )
        .unwrap()["purchase"]["id"]
        .as_i64()
        .unwrap();
    inv.buy_pack(set, 4).unwrap();
    // One thing is one unit: the next things still find units left.
    let p = &inv.buy_link(set, "Matkap", None).unwrap()["purchase"];
    assert_eq!(p["open_qty"], 3, "{p}");
    inv.edit("Kart", &["qty=2".into()]).unwrap();
    let p = &inv.buy_link(set, "Kart", None).unwrap()["purchase"];
    assert_eq!(p["open_qty"], 1, "a thing of two takes two");
}

#[test]
fn a_pack_cannot_shrink_below_what_is_already_linked() {
    let (_d, mut inv) = setup();
    let cells = inv
        .buy_add(
            &json!({"name": "AA cells, 4-pack", "pack": 4}),
            Some("Kart"),
        )
        .unwrap()["purchase"]["id"]
        .as_i64()
        .unwrap();
    // A pack given when the line is added links all its units at once.
    assert_eq!(
        inv.buy_show(cells).unwrap()["purchase"]["linked"][0]["qty"],
        4
    );
    assert!(inv.buy_pack(cells, 2).is_err());
    assert!(inv.buy_pack(cells, 0).is_err());
    assert_eq!(inv.buy_pack(cells, 6).unwrap()["purchase"]["open_qty"], 2);
}

#[test]
fn a_reimport_with_a_new_price_updates_the_line_and_keeps_its_links() {
    let (d, mut inv) = setup();
    inv.buy_import(&export(&d, "1999.00")).unwrap();
    let drill = id_of(&inv, "Bosch");
    inv.buy_link(drill, "Matkap", None).unwrap();
    let v = inv.buy_import(&export(&d, "1,999.50")).unwrap();
    assert_eq!(v["imported"]["updated"], 1);
    let p = &inv.buy_show(drill).unwrap()["purchase"];
    assert_eq!(p["paid"], "1999.50");
    assert_eq!(p["linked"][0]["node"]["name"], "Matkap");
}

#[test]
fn a_dismissed_line_leaves_the_open_list_until_it_is_cleared() {
    let (d, mut inv) = setup();
    inv.buy_import(&export(&d, "1999.00")).unwrap();
    let drill = id_of(&inv, "Bosch");
    assert!(inv.buy_dismiss(drill, Some("lost-it"), None).is_err());
    inv.buy_dismiss(drill, Some("given"), Some("kardeşe verildi"))
        .unwrap();
    let open = inv.buy_list(true, None, None, None).unwrap();
    assert_eq!(open["purchases"].as_array().unwrap().len(), 1);
    assert!(inv.buy_link(drill, "Matkap", None).is_err());
    inv.buy_dismiss(drill, None, None).unwrap();
    assert_eq!(
        inv.buy_list(true, None, None, None).unwrap()["purchases"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn a_purchase_entered_by_hand_is_linked_at_once() {
    let (_d, mut inv) = setup();
    let v = inv
        .buy_add(
            &json!({"name": "Matkap", "shop": "Hırdavatçı", "ordered_at": "2020-03-01",
                    "paid": "350", "currency": "TRY", "qty": 1}),
            Some("Matkap"),
        )
        .unwrap();
    let p = &v["purchase"];
    assert_eq!(p["source"], "manual");
    assert_eq!(p["paid"], "350.00");
    assert_eq!(p["open_qty"], 0);
    assert!(
        inv.buy_add(&json!({"name": "x", "paid": "1.234"}), None)
            .is_err()
    );
}

#[test]
fn lines_are_found_by_words_of_their_name_shop_or_code() {
    let (d, mut inv) = setup();
    inv.buy_import(&export(&d, "1999.00")).unwrap();
    let names = |q: &str| -> Vec<String> {
        inv.buy_list_matching(false, None, None, None, Some(q))
            .unwrap()["purchases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap().to_string())
            .collect()
    };
    // Every word must be there, in any order and case.
    assert_eq!(names("matkap BOSCH"), ["Bosch GSB 13 RE Darbeli Matkap"]);
    assert_eq!(names("sku-b"), ["Kingston 128 GB microSD"]);
    assert!(names("matkap kingston").is_empty());
}

#[test]
fn the_counts_by_bucket_are_the_lengths_of_the_unfiltered_lists() {
    let (d, mut inv) = setup();
    inv.buy_import(&export(&d, "1999.00")).unwrap();
    let counts = inv.buy_counts().unwrap();
    for bucket in ev_core::BUCKETS {
        let listed = inv.buy_list(false, Some(bucket), None, None).unwrap()["purchases"]
            .as_array()
            .unwrap()
            .len() as u64;
        assert_eq!(counts[bucket].as_u64().unwrap_or(0), listed, "{bucket}");
    }
}
