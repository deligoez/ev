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

#[test]
fn a_source_and_a_key_find_a_payments_line_and_its_items() {
    let (_d, mut inv) = setup();
    let line = |source: &str, key: &str| {
        json!({"type": "purchase", "source": source, "key": key, "name": format!("{source} {key}"),
               "qty": 1, "paid": "1999", "currency": "TRY", "bucket": "durable"})
        .to_string()
    };
    let ndjson = [
        line("ak", "41"),
        line("ak", "42.1"),
        line("ak", "42.2"),
        line("ak", "420"),
        line("shop", "42"),
    ]
    .join("\n");
    inv.buy_import(&ndjson).unwrap();
    let keys = |source: Option<&str>, key: Option<&str>| -> Vec<String> {
        let mut k: Vec<String> = inv
            .buy_list_where(&ev_core::BuyFilter {
                source,
                key,
                ..Default::default()
            })
            .unwrap()["purchases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                format!(
                    "{}:{}",
                    p["source"].as_str().unwrap(),
                    p["source_key"].as_str().unwrap()
                )
            })
            .collect();
        k.sort();
        k
    };
    assert_eq!(keys(Some("ak"), None).len(), 4);
    // A payment's key finds its items, not another payment that starts with the same digits.
    assert_eq!(keys(Some("ak"), Some("42")), ["ak:42.1", "ak:42.2"]);
    assert_eq!(keys(Some("ak"), Some("42.2")), ["ak:42.2"]);
    assert_eq!(keys(Some("ak"), Some("41")), ["ak:41"]);
    // The key alone spans the sources; the source is matched exactly.
    assert_eq!(keys(None, Some("42")), ["ak:42.1", "ak:42.2", "shop:42"]);
    assert!(keys(Some("a"), None).is_empty());
}

#[test]
fn a_count_sent_as_text_is_read_and_one_that_is_no_count_is_refused() {
    let (_d, mut inv) = setup();
    let line = |qty: Value| {
        json!({"type": "purchase", "source": "ak", "key": "7.1", "name": "Tencere", "qty": qty})
            .to_string()
    };
    // A receipt's count as text (ak sends it so) is the count, not 1.
    inv.buy_import(&line(json!("2"))).unwrap();
    let qty = |inv: &Inventory| {
        inv.buy_list(false, None, None, None).unwrap()["purchases"][0]["qty"].clone()
    };
    assert_eq!(qty(&inv), 2);
    inv.buy_import(&line(json!(3))).unwrap();
    assert_eq!(qty(&inv), 3);
    // A weight or a word is no count: refused, never read as 1.
    let e = inv.buy_import(&line(json!("1.5"))).unwrap_err();
    assert_eq!(e.code(), 2);
    assert_eq!(e.id(), Some("purchase_qty_not_a_count"));
    assert_eq!(qty(&inv), 3);
}

/// Two lines as `ak export --for ev` writes them (spec/ak.md): an item of a receipt and a whole
/// order, keys sorted, no field without a value.
const AK_EXPORT: &str = r#"{"billed_to":"Kart","bucket":"durable","category":"home","currency":"TRY","key":"1.2","name":"Tencere","ordered_at":"2026-10-01","paid":"1190.00","qty":"2","raw":"ak:1.2","shop":"Örnek Market","source":"ak","status":"delivered"}
{"billed_to":"Kart","bucket":"durable","currency":"TRY","key":"2","name":"Bulaşık makinesi","order":"A-100","ordered_at":"2026-09-20","paid":"19999.00","raw":"ak:2","shop":"Örnek Elektronik","source":"ak","status":"delivered"}"#;

#[test]
fn aks_export_imports_as_it_is_written_and_again_changes_nothing() {
    let (_d, mut inv) = setup();
    let v = inv.buy_import(AK_EXPORT).unwrap();
    assert_eq!(v["imported"]["new"], 2, "{v}");
    assert!(v["imported"]["unknown_fields"].is_null(), "{v}");
    let line = |key: &str| {
        inv.buy_list_where(&ev_core::BuyFilter {
            source: Some("ak"),
            key: Some(key),
            ..Default::default()
        })
        .unwrap()["purchases"][0]
            .clone()
    };
    let pot = line("1.2");
    assert_eq!(pot["name"], "Tencere");
    assert_eq!(pot["qty"], 2);
    assert_eq!(pot["shop"], "Örnek Market");
    assert_eq!(pot["billed_to"], "Kart");
    assert_eq!(pot["ordered_at"], "2026-10-01");
    let washer = line("2");
    assert_eq!(washer["order_no"], "A-100");
    assert_eq!(washer["qty"], 1);
    assert_eq!(washer["bucket"], "durable");
    let again = inv.buy_import(AK_EXPORT).unwrap();
    assert_eq!(again["imported"]["unchanged"], 2, "{again}");
}

/// A shop's lines: a one-line order and an order of two lines; then ak's lines of them.
fn shop_then_ak(inv: &mut Inventory) -> Value {
    let line = |source: &str, key: &str, order: &str, name: &str, paid: &str| {
        json!({"type": "purchase", "source": source, "key": key, "shop": "Shop", "order": order,
               "name": name, "paid": paid, "currency": "TRY"})
        .to_string()
    };
    inv.buy_import(
        &[
            line("shop", "a", "ORD-100001", "Bulaşık makinesi", "19999"),
            line("shop", "b", "ORD-200002", "Bosch matkap", "1999"),
            line("shop", "c", "ORD-200002", "Uç seti", "500"),
            line("shop", "d", "ORD-300003", "HDMI kablo", "100"),
            line("shop", "e", "ORD-300003", "USB kablo", "120"),
        ]
        .join("\n"),
    )
    .unwrap();
    inv.buy_import(
        &[
            // The whole one-line order; an item of the two-line one, told by its price.
            line("ak", "1", "ORD-100001", "Makine", "19999"),
            line("ak", "2.2", "ORD-200002", "Set", "500"),
            // The whole order against its two lines, priced as neither: none can be told.
            line("ak", "3", "ORD-300003", "Kablo", "90"),
            // No line of this order in ev: a purchase of its own.
            line("ak", "4", "ORD-400004", "Tencere", "1190"),
        ]
        .join("\n"),
    )
    .unwrap()
}

#[test]
fn aks_lines_join_the_lines_of_their_order_and_the_untold_ones_are_listed() {
    let (_d, mut inv) = setup();
    let v = shop_then_ak(&mut inv);
    assert_eq!(v["imported"]["joined"], 2, "{v}");
    let unjoined = v["imported"]["unjoined"].as_array().unwrap();
    assert_eq!(unjoined.len(), 1, "{v}");
    assert_eq!(unjoined[0]["key"], "3");
    assert_eq!(unjoined[0]["order"], "ORD-300003");
    assert_eq!(unjoined[0]["candidates"].as_array().unwrap().len(), 2);
    let ak = |key: &str| {
        inv.buy_list_where(&ev_core::BuyFilter {
            source: Some("ak"),
            key: Some(key),
            ..Default::default()
        })
        .unwrap()["purchases"][0]
            .clone()
    };
    assert_eq!(ak("1")["same_as"], id_of(&inv, "Bulaşık"));
    assert_eq!(ak("2.2")["same_as"], id_of(&inv, "Uç seti"));
    assert_eq!(ak("1")["open_qty"], 0);
    assert!(ak("3")["same_as"].is_null());
    assert!(ak("4")["same_as"].is_null());
    assert_eq!(ak("4")["open_qty"], 1);
    // The shop's own lines are not joined to each other, though they share an order.
    let cables = inv
        .buy_list_matching(false, None, None, None, Some("Kablo"))
        .unwrap();
    assert!(
        cables["purchases"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["same_as"].is_null())
    );
}

#[test]
fn a_joined_line_names_the_line_it_joins_and_what_that_is_linked_to() {
    let (_d, mut inv) = setup();
    shop_then_ak(&mut inv);
    let drill = id_of(&inv, "Uç seti");
    inv.buy_link(drill, "Matkap", None).unwrap();
    let row = inv
        .buy_list_where(&ev_core::BuyFilter {
            source: Some("ak"),
            key: Some("2"),
            ..Default::default()
        })
        .unwrap()["purchases"][0]
        .clone();
    // ak's key leads to the thing in one call.
    let to = &row["joined_to"];
    assert_eq!(to["id"], drill);
    assert_eq!(to["source"], "shop");
    assert_eq!(to["source_key"], "c");
    assert_eq!(to["linked"][0]["node"]["name"], "Matkap");
    // A line joined to nothing has no such field.
    let own = inv
        .buy_list_where(&ev_core::BuyFilter {
            key: Some("4"),
            ..Default::default()
        })
        .unwrap()["purchases"][0]
        .clone();
    assert!(own.get("joined_to").is_none());
}

#[test]
fn a_line_the_import_could_not_tell_is_joined_by_hand_and_taken_back() {
    let (_d, mut inv) = setup();
    let v = shop_then_ak(&mut inv);
    let line = v["imported"]["unjoined"][0]["id"].as_i64().unwrap();
    let into = v["imported"]["unjoined"][0]["candidates"][0]
        .as_i64()
        .unwrap();
    // The person says which line it is: settled through it.
    let j = inv.buy_join(line, Some(into)).unwrap();
    assert_eq!(j["purchase"]["same_as"], into);
    assert_eq!(j["purchase"]["open_qty"], 0);
    // An import again leaves it so, and lists it no more.
    let again = inv.buy_import(r#"{"source":"ak","key":"3","name":"Kablo","order":"ORD-300003","paid":"90","currency":"TRY"}"#).unwrap();
    assert!(again["imported"]["unjoined"].is_null(), "{again}");
    // Refused: to itself, to a line that joins another, a line linked to a thing.
    let id = |e: ev_core::Error| e.id().map(str::to_string);
    assert_eq!(
        id(inv.buy_join(into, Some(into)).unwrap_err()).as_deref(),
        Some("purchase_join_itself")
    );
    let other = v["imported"]["unjoined"][0]["candidates"][1]
        .as_i64()
        .unwrap();
    assert_eq!(
        id(inv.buy_join(other, Some(line)).unwrap_err()).as_deref(),
        Some("purchase_join_to_joined")
    );
    inv.buy_link(other, "Kart", None).unwrap();
    assert_eq!(
        id(inv.buy_join(other, Some(into)).unwrap_err()).as_deref(),
        Some("purchase_join_linked")
    );
    // Taken back: open again.
    let back = inv.buy_join(line, None).unwrap();
    assert!(back["purchase"]["same_as"].is_null());
    assert_eq!(back["purchase"]["open_qty"], 1);
}

#[test]
fn a_price_told_join_wins_a_guess_and_no_line_is_joined_twice() {
    let (_d, mut inv) = setup();
    let line = |source: &str, key: &str, order: &str, name: &str, paid: &str| {
        json!({"type": "purchase", "source": source, "key": key, "order": order,
               "name": name, "paid": paid, "currency": "TRY"})
        .to_string()
    };
    inv.buy_import(
        &[
            line("shop", "s1", "ORD-500005", "Marka cam spreyi 750 ml", "150"),
            line("shop", "s2", "ORD-500005", "Dijital tartı", "200"),
            line("shop", "k1", "ORD-600006", "Bir kitap", "250"),
            line("shop", "m1", "ORD-700007", "Hafıza kartı 128GB", "500"),
            line("shop", "m2", "ORD-700007", "Hafıza kartı 128GB", "500"),
            line("shop", "w1", "ORD-800008", "Araç kamerası", "12000"),
        ]
        .join("\n"),
    )
    .unwrap();
    let v = inv
        .buy_import(
            &[
                // Its name is close to the spray's, its price is not: no guess takes the spray,
                // which the line after it is told by price.
                line("ak", "5.1", "ORD-500005", "Marka IPA spreyi 750 ml", "300"),
                line("ak", "5.2", "ORD-500005", "Marka cam spreyi 750 ml", "150"),
                // The book is the order's only line ev has; the cake is not it.
                line("ak", "6.1", "ORD-600006", "Bir kitap", "250"),
                line("ak", "6.2", "ORD-600006", "Kek", "260"),
                // Two alike: one each.
                line("ak", "7.1", "ORD-700007", "Hafıza kartı 128GB", "500"),
                line("ak", "7.2", "ORD-700007", "Hafıza kartı 128GB", "500"),
                // A whole payment of a one-line order, shipping included.
                line("ak", "8", "ORD-800008", "Amazon", "12090"),
            ]
            .join("\n"),
        )
        .unwrap();
    let same_as = |key: &str| {
        inv.buy_list_where(&ev_core::BuyFilter {
            source: Some("ak"),
            key: Some(key),
            ..Default::default()
        })
        .unwrap()["purchases"][0]["same_as"]
            .as_i64()
    };
    // The shop's line by its key: the ak lines carry the same names.
    let shop = |key: &str| {
        inv.buy_list_where(&ev_core::BuyFilter {
            source: Some("shop"),
            key: Some(key),
            ..Default::default()
        })
        .unwrap()["purchases"][0]["id"]
            .as_i64()
    };
    assert_eq!(same_as("5.2"), shop("s1"));
    assert_eq!(same_as("5.1"), None);
    assert_eq!(same_as("6.1"), shop("k1"));
    assert_eq!(same_as("6.2"), None);
    let (a, b) = (same_as("7.1").unwrap(), same_as("7.2").unwrap());
    assert_ne!(a, b);
    assert_eq!(same_as("8"), shop("w1"));
    let keys: Vec<&str> = v["imported"]["unjoined"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["5.1", "6.2"], "{v}");
}

#[test]
fn a_shops_lines_never_join_an_ak_line_that_waits_unjoined() {
    let (_d, mut inv) = setup();
    // A shop's two lines whose order page names the order number: the way a second source's
    // line was always joined to them.
    let shop = |key: &str, name: &str, paid: &str| {
        json!({"type": "purchase", "source": "shop", "key": key, "order": "ORD-900009",
               "order_url": "https://shop.example/orders?id=ORD-900009", "name": name,
               "paid": paid, "currency": "TRY"})
        .to_string()
    };
    inv.buy_import(
        &[
            shop("p1", "Kapı stoperi", "100"),
            shop("p2", "Yapboz", "150"),
        ]
        .join("\n"),
    )
    .unwrap();
    // ak's whole payment of that order, told by neither line: it waits unjoined.
    let ak = r#"{"source":"ak","key":"9","name":"Kapı stoperi, Yapboz","order":"ORD-900009","paid":"260","currency":"TRY"}"#;
    let first = inv.buy_import(ak).unwrap();
    let again = inv.buy_import(ak).unwrap();
    for v in [&first, &again] {
        assert_eq!(v["imported"]["unjoined"][0]["key"], "9", "{v}");
    }
    // The shop's lines are not turned into joins of ak's line: they stay the ones to link.
    let rows = inv
        .buy_list_matching(false, None, None, None, None)
        .unwrap();
    for p in rows["purchases"].as_array().unwrap() {
        assert!(p["same_as"].is_null(), "{p}");
    }
}

#[test]
fn dismissed_lines_are_listed_of_any_reason_or_of_one() {
    let (d, mut inv) = setup();
    inv.buy_import(&export(&d, "1999.00")).unwrap();
    let drill = id_of(&inv, "Bosch");
    let card = id_of(&inv, "Kingston");
    inv.buy_dismiss(drill, Some("elsewhere"), Some("a bill, kept elsewhere"))
        .unwrap();
    inv.buy_dismiss(card, Some("given"), None).unwrap();
    let ids = |dismissed: Option<Option<&str>>| -> Vec<i64> {
        let mut v: Vec<i64> = inv
            .buy_list_where(&ev_core::BuyFilter {
                dismissed,
                ..Default::default()
            })
            .unwrap()["purchases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["id"].as_i64().unwrap())
            .collect();
        v.sort_unstable();
        v
    };
    let mut both = vec![drill, card];
    both.sort_unstable();
    assert_eq!(ids(Some(None)), both);
    assert_eq!(ids(Some(Some("elsewhere"))), [drill]);
    assert!(ids(Some(Some("consumed"))).is_empty());
    let e = inv
        .buy_list_where(&ev_core::BuyFilter {
            dismissed: Some(Some("bogus")),
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(e.id(), Some("purchase_reason_unknown"));
}

#[test]
fn a_line_entered_by_hand_is_corrected_and_one_from_a_source_is_not() {
    let (d, mut inv) = setup();
    let hand = inv
        .buy_add(
            &json!({ "name": "Araba", "paid": "100000" }),
            Some("Matkap"),
        )
        .unwrap()["purchase"]["id"]
        .as_i64()
        .unwrap();
    let fields = |f: &[&str]| f.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let v = inv
        .buy_edit(
            hand,
            &fields(&[
                "date=2018-03",
                "paid=90000",
                "shop=Bayi",
                "order=",
                "name=Eski araba",
            ]),
        )
        .unwrap();
    let p = &v["purchase"];
    assert_eq!(p["ordered_at"], "2018-03");
    assert_eq!(p["paid"], "90000.00");
    assert_eq!(p["shop"], "Bayi");
    assert_eq!(p["name"], "Eski araba");
    // Still linked to what it bought.
    assert_eq!(p["linked"][0]["node"]["name"], "Matkap");
    let id = |e: ev_core::Error| e.id().map(str::to_string);
    // No fewer units than are linked; no field it does not have; no day still to come.
    let below = inv.buy_edit(hand, &fields(&["qty=0"])).unwrap_err();
    assert_eq!(id(below).as_deref(), Some("qty_below_one"));
    let unknown = inv.buy_edit(hand, &fields(&["colour=red"])).unwrap_err();
    assert_eq!(id(unknown).as_deref(), Some("purchase_edit_field_unknown"));
    let future = inv.buy_edit(hand, &fields(&["date=2099-01"])).unwrap_err();
    assert_eq!(id(future).as_deref(), Some("date_still_to_come"));
    // A line from a shop's export is the shop's: corrected there.
    inv.buy_import(&export(&d, "1999.00")).unwrap();
    let shop = id_of(&inv, "Bosch");
    let e = inv.buy_edit(shop, &fields(&["paid=1"])).unwrap_err();
    assert_eq!(id(e).as_deref(), Some("purchase_edit_not_manual"));
}

#[test]
fn an_ak_payment_about_a_thing_is_tied_to_it_and_counts_in_what_it_costs() {
    let (_d, mut inv) = setup();
    let drill = inv.resolve("Matkap", false).unwrap();
    let lines = [
        json!({"source": "ak", "key": "3", "name": "Örnek Servis #3", "paid": "350.00",
               "currency": "TRY", "bucket": "service", "thing": drill, "period": "2026-01",
               "status": "delivered", "shop": "Örnek Servis", "ordered_at": "2026-01-10"}),
        // A refund comes as a negative amount, and an id ev does not know is said, not failed.
        json!({"source": "ak", "key": "4", "name": "İade #4", "paid": "-50.00",
               "currency": "TRY", "bucket": "service", "thing": drill,
               "ordered_at": "2026-02-01"}),
        json!({"source": "ak", "key": "5", "name": "Başka #5", "paid": "10.00",
               "currency": "TRY", "bucket": "service", "thing": 99999,
               "ordered_at": "2026-02-02"}),
    ]
    .iter()
    .map(Value::to_string)
    .collect::<Vec<_>>()
    .join("\n");
    let v = inv.buy_import(&lines).unwrap();
    assert_eq!(
        v["imported"]["things_unknown"],
        json!([{"key": "5", "thing": 99999}])
    );
    let line = inv
        .buy_list_where(&ev_core::BuyFilter {
            source: Some("ak"),
            key: Some("3"),
            ..Default::default()
        })
        .unwrap();
    let three = line["purchases"][0].clone();
    assert_eq!(three["about"]["id"], drill);
    assert_eq!(three["period"], "2026-01");
    assert_eq!(three["linked"], json!([]));
    let shown = inv.show("Matkap", false).unwrap();
    assert_eq!(shown["cost"]["TRY"]["upkeep"], "300.00");
}

#[test]
fn a_service_line_is_tied_by_hand_and_a_thing_own_purchase_is_linked_instead() {
    let (_d, mut inv) = setup();
    let line = |key: &str, bucket: &str| {
        json!({"source": "shop", "key": key, "name": format!("Satır {key}"), "paid": "100.00",
               "currency": "TRY", "bucket": bucket, "ordered_at": "2025-03-01"})
        .to_string()
    };
    inv.buy_import(&[line("s1", "service"), line("d1", "durable")].join("\n"))
        .unwrap();
    let id = |key: &str| -> i64 {
        inv.buy_list_where(&ev_core::BuyFilter {
            key: Some(key),
            ..Default::default()
        })
        .unwrap()["purchases"][0]["id"]
            .as_i64()
            .unwrap()
    };
    let (service, durable) = (id("s1"), id("d1"));
    let v = inv.buy_about(service, Some("Matkap"), false).unwrap();
    assert_eq!(v["purchase"]["about"]["name"], "Matkap");
    // A thing's own purchase is linked, not tied.
    let e = inv.buy_about(durable, Some("Matkap"), false).unwrap_err();
    assert_eq!(e.id(), Some("purchase_about_needs_service"));
    let v = inv.buy_about(service, None, true).unwrap();
    assert!(v["purchase"]["about"].is_null());
}

#[test]
fn lines_are_narrowed_by_month_and_currency_and_sorted_dearest_first() {
    let (_d, mut inv) = setup();
    let line = |key: &str, paid: &str, currency: &str, at: &str| {
        json!({"source": "shop", "key": key, "name": format!("Satır {key}"), "paid": paid,
               "currency": currency, "ordered_at": at})
        .to_string()
    };
    inv.buy_import(
        &[
            line("a", "100.00", "TRY", "2025-03-02"),
            line("b", "900.00", "TRY", "2025-03-20"),
            line("c", "50.00", "USD", "2025-03-05"),
            line("d", "500.00", "TRY", "2025-04-01"),
        ]
        .join("\n"),
    )
    .unwrap();
    let keys = |f: &ev_core::BuyFilter| -> Vec<String> {
        inv.buy_list_where(f).unwrap()["purchases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["source_key"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        keys(&ev_core::BuyFilter {
            month: Some("2025-03"),
            currency: Some("TRY"),
            by_paid: true,
            ..Default::default()
        }),
        ["b", "a"]
    );
    assert_eq!(
        keys(&ev_core::BuyFilter {
            currency: Some("usd"),
            ..Default::default()
        }),
        ["c"]
    );
}

#[test]
fn a_line_names_the_records_it_could_be_best_first() {
    let (_d, mut inv) = setup();
    inv.buy_import(
        &json!({"source": "shop", "key": "m1", "name": "Bosch GSB 13 RE Darbeli Matkap",
                "paid": "1999.00", "currency": "TRY", "ordered_at": "2024-05-01"})
        .to_string(),
    )
    .unwrap();
    let id = inv
        .buy_list_where(&ev_core::BuyFilter {
            key: Some("m1"),
            ..Default::default()
        })
        .unwrap()["purchases"][0]["id"]
        .as_i64()
        .unwrap();
    let v = inv.buy_things(id).unwrap();
    assert_eq!(v["candidates"][0]["node"]["name"], "Matkap", "{v}");
    assert!(
        v["candidates"][0]["why"]
            .as_array()
            .is_some_and(|w| !w.is_empty())
    );
    // Linked, the line is settled and names nothing more.
    inv.buy_link(id, "Matkap", None).unwrap();
    assert_eq!(inv.buy_things(id).unwrap()["candidates"], json!([]));
}

#[test]
fn one_purchase_seen_twice_is_listed_as_a_possible_duplicate() {
    let (_d, mut inv) = setup();
    let line = |source: &str, key: &str, at: &str| {
        json!({"source": source, "key": key, "name": "USB kablo", "paid": "99.90",
               "currency": "TRY", "ordered_at": at})
        .to_string()
    };
    inv.buy_import(
        &[
            line("shop", "a", "2025-03-01"),
            line("mail", "b", "2025-03-03"),
            // The same cable a month later is a second purchase.
            line("shop", "c", "2025-04-10"),
        ]
        .join("\n"),
    )
    .unwrap();
    let v = inv.buy_duplicates().unwrap();
    let groups = v["duplicates"].as_array().unwrap();
    assert_eq!(groups.len(), 1, "{v}");
    let keys: Vec<&str> = groups[0]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["source_key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["a", "b"]);
}

#[test]
fn open_lines_are_grouped_by_the_place_of_the_record_each_could_be() {
    let (_d, mut inv) = setup();
    inv.buy_import(
        &[
            json!({"source": "shop", "key": "m1", "name": "Bosch GSB 13 RE Darbeli Matkap",
                   "paid": "1999.00", "currency": "TRY", "ordered_at": "2024-05-01"}),
            json!({"source": "shop", "key": "z1", "name": "Bahçe hortumu 20 m",
                   "paid": "300.00", "currency": "TRY", "ordered_at": "2024-06-01"}),
        ]
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n"),
    )
    .unwrap();
    // Its model read off the label: what makes a line sure enough to be offered.
    inv.edit("Matkap", &["make=Bosch".into(), "model=GSB 13 RE".into()])
        .unwrap();
    let v = inv.buy_by_place().unwrap();
    let groups = v["by_place"].as_array().unwrap();
    // The drill under the room it lies in; the hose, which nothing recorded could be, last.
    assert_eq!(groups[0]["place"]["name"], "Oda", "{v}");
    assert_eq!(groups[0]["lines"][0]["candidate"]["node"]["name"], "Matkap");
    let last = groups.last().unwrap();
    assert!(last["place"].is_null());
    assert_eq!(last["lines"][0]["purchase"]["source_key"], "z1");
}
