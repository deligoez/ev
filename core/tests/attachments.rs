//! What an adapter hangs on a purchase line (purchases spec §6), and one purchase seen by two
//! sources joined by `same_as`.

use ev_core::{Inventory, NewNode};
use serde_json::{Value, json};

fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent) in [
        ("Ev", "home", None),
        ("Oda", "room", Some("Ev")),
        ("Televizyon", "item", Some("Oda")),
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

fn lines(v: &[Value]) -> String {
    v.iter().map(|l| format!("{l}\n")).collect()
}

/// A shop's order of two lines.
fn shop() -> Vec<Value> {
    vec![
        json!({"type": "purchase", "source": "shop", "key": "o1:a", "order": "404-1234567",
               "sku": "B0TVTVTV01", "name": "OLED TV 55", "ordered_at": "2022-02-08",
               "paid": "15000.00", "currency": "TRY"}),
        json!({"type": "purchase", "source": "shop", "key": "o1:b", "order": "404-1234567",
               "sku": "B0WALLMNT2", "name": "Duvar askı aparatı", "ordered_at": "2022-02-08",
               "paid": "300", "currency": "TRY"}),
    ]
}

/// The same TV recorded in another app: its order page, product page, a value and a warranty.
fn other_app() -> Vec<Value> {
    vec![
        json!({"type": "purchase", "source": "umr", "key": "item-1", "name": "OLED TV 55 inç",
               "order_url": "https://shop.example/orders?orderID=404-1234567",
               "product_url": "https://shop.example/dp/B0TVTVTV01", "paid": "15000.00"}),
        json!({"type": "valuation", "source": "umr", "purchase": "item-1", "amount": "27000",
               "at": "2025-08-01", "approximate": true, "from": "umr"}),
        json!({"type": "coverage", "source": "umr", "purchase": "item-1",
               "kind": "manufacturer", "term": "2y", "issuer": "LG"}),
        json!({"type": "link", "source": "umr", "purchase": "item-1",
               "url": "https://www.lg.example/oled55c1", "kind": "info"}),
    ]
}

fn id_of(inv: &Inventory, source_key: &str) -> i64 {
    inv.buy_list(false, None, None, None).unwrap()["purchases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["source_key"] == source_key)
        .unwrap()["id"]
        .as_i64()
        .unwrap()
}

#[test]
fn an_order_page_joins_the_shop_line_narrowed_by_the_product_key() {
    let (_d, mut inv) = setup();
    inv.buy_import(&lines(&shop())).unwrap();
    let v = inv.buy_import(&lines(&other_app())).unwrap();
    assert_eq!(v["imported"]["joined"], 1);
    assert_eq!(v["imported"]["attachments"], 3);
    let tv = id_of(&inv, "o1:a");
    let umr = &inv.buy_show(id_of(&inv, "item-1")).unwrap()["purchase"];
    assert_eq!(umr["same_as"], tv);
    assert_eq!(umr["open_qty"], 0);
    assert_eq!(
        inv.buy_show(tv).unwrap()["purchase"]["joined"],
        json!([umr["id"]])
    );
    // Imported again, nothing new and nothing joined twice.
    let again = inv.buy_import(&lines(&other_app())).unwrap();
    assert_eq!(again["imported"]["attachments"], 0);
    assert_eq!(again["imported"]["joined"], 0);
}

#[test]
fn the_join_holds_whichever_source_comes_first() {
    let (_d, mut inv) = setup();
    inv.buy_import(&lines(&other_app())).unwrap();
    let v = inv.buy_import(&lines(&shop())).unwrap();
    assert_eq!(v["imported"]["joined"], 1);
    let umr = &inv.buy_show(id_of(&inv, "item-1")).unwrap()["purchase"];
    assert_eq!(umr["same_as"], id_of(&inv, "o1:a"));
}

#[test]
fn without_a_product_key_the_name_picks_the_line_only_when_it_is_clear() {
    let (_d, mut inv) = setup();
    inv.buy_import(&lines(&shop())).unwrap();
    let order = "https://shop.example/orders?orderID=404-1234567";
    let v = inv
        .buy_import(&lines(&[
            json!({"type": "purchase", "source": "umr", "key": "item-2",
                   "name": "Duvar askı aparatı TV için", "order_url": order}),
            json!({"type": "purchase", "source": "umr", "key": "item-3",
                   "name": "Kablo", "order_url": order}),
        ]))
        .unwrap();
    assert_eq!(v["imported"]["joined"], 1);
    let wall = &inv.buy_show(id_of(&inv, "item-2")).unwrap()["purchase"];
    assert_eq!(wall["same_as"], id_of(&inv, "o1:b"));
    let cable = &inv.buy_show(id_of(&inv, "item-3")).unwrap()["purchase"];
    assert!(cable["same_as"].is_null());
}

#[test]
fn bringing_makes_the_attachments_the_things_own_once() {
    let (_d, mut inv) = setup();
    inv.buy_import(&lines(&shop())).unwrap();
    inv.buy_import(&lines(&other_app())).unwrap();
    let tv = id_of(&inv, "o1:a");
    assert!(
        inv.buy_bring(tv, "Televizyon", &[]).is_err(),
        "not linked yet"
    );
    let linked = inv.buy_link(tv, "Televizyon", None).unwrap();
    assert_eq!(
        linked["purchase"]["attachments"].as_array().unwrap().len(),
        3
    );
    let v = inv.buy_bring(tv, "Televizyon", &[]).unwrap();
    assert_eq!(v["brought"].as_array().unwrap().len(), 3);
    assert_eq!(v["valuations"][0]["amount"], "27000.00");
    assert_eq!(v["valuations"][0]["approximate"], true);
    assert_eq!(v["coverages"][0]["kind"], "manufacturer");
    assert_eq!(v["links"][0]["url"], "https://www.lg.example/oled55c1");
    let twice = inv.buy_bring(tv, "Televizyon", &[]).unwrap();
    assert_eq!(twice["brought"], json!([]));
}

#[test]
fn a_shops_product_image_is_brought_as_a_document_never_as_the_things_photo() {
    let (dir, mut inv) = setup();
    let picture = dir.path().join("B0TVTVTV01-01.jpg");
    image::RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3]))
        .save(&picture)
        .unwrap();
    let mut l = shop();
    l.push(
        json!({"type": "image", "source": "shop", "purchase": "o1:a",
                  "file": picture.to_string_lossy()}),
    );
    inv.buy_import(&lines(&l)).unwrap();
    let tv = id_of(&inv, "o1:a");
    inv.buy_link(tv, "Televizyon", None).unwrap();
    let v = inv.buy_bring(tv, "Televizyon", &[]).unwrap();
    assert_eq!(v["brought"].as_array().unwrap().len(), 1);
    assert_eq!(v["documents"][0]["kind"], "image");
    assert!(v["node"]["photos"].as_array().unwrap().is_empty(), "{v}");
}
