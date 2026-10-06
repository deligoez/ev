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
        inv.buy_bring(tv, "Televizyon", &[], &[]).is_err(),
        "not linked yet"
    );
    let linked = inv.buy_link(tv, "Televizyon", None).unwrap();
    assert_eq!(
        linked["purchase"]["attachments"].as_array().unwrap().len(),
        3
    );
    let v = inv.buy_bring(tv, "Televizyon", &[], &[]).unwrap();
    assert_eq!(v["brought"].as_array().unwrap().len(), 3);
    let s = inv.show("Televizyon", false).unwrap();
    assert_eq!(s["valuations"][0]["amount"], "27000.00");
    assert_eq!(s["valuations"][0]["approximate"], true);
    assert_eq!(s["coverages"][0]["kind"], "manufacturer");
    assert_eq!(s["links"][0]["url"], "https://www.lg.example/oled55c1");
    let twice = inv.buy_bring(tv, "Televizyon", &[], &[]).unwrap();
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
    let v = inv.buy_bring(tv, "Televizyon", &[], &[]).unwrap();
    assert_eq!(v["brought"].as_array().unwrap().len(), 1);
    let s = inv.show("Televizyon", false).unwrap();
    assert_eq!(s["documents"][0]["kind"], "image");
    assert!(s["node"]["photos"].as_array().unwrap().is_empty(), "{s}");
}

#[test]
fn unlinking_takes_back_the_pictures_and_pages_a_wrong_link_brought() {
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
    inv.buy_import(&lines(&other_app())).unwrap();
    let tv = id_of(&inv, "o1:a");
    inv.buy_link(tv, "Televizyon", None).unwrap();
    inv.buy_bring(tv, "Televizyon", &[], &[]).unwrap();
    let v = inv.buy_unlink(tv, "Televizyon").unwrap();
    let types = |k: &str| -> Vec<String> {
        let mut t: Vec<String> = v[k]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["type"].as_str().unwrap().to_string())
            .collect();
        t.sort();
        t
    };
    assert_eq!(types("taken_back"), ["image", "link"]);
    assert_eq!(types("left"), ["coverage", "valuation"]);
    let s = inv.show("Televizyon", false).unwrap();
    assert!(
        s["documents"].is_null() || s["documents"] == json!([]),
        "{s}"
    );
    assert!(s["links"].is_null() || s["links"] == json!([]), "{s}");
    assert_eq!(s["valuations"][0]["amount"], "27000.00");
    // Linked again, what it carries can be brought again.
    inv.buy_link(tv, "Televizyon", None).unwrap();
    let again = inv.buy_bring(tv, "Televizyon", &[], &[]).unwrap();
    assert!(!again["brought"].as_array().unwrap().is_empty(), "{again}");
}

#[test]
fn bringing_by_type_brings_only_that_type_and_names_what_it_left() {
    let (_d, mut inv) = setup();
    inv.buy_import(&lines(&shop())).unwrap();
    inv.buy_import(&lines(&other_app())).unwrap();
    let tv = id_of(&inv, "o1:a");
    inv.buy_link(tv, "Televizyon", None).unwrap();
    let v = inv
        .buy_bring(tv, "Televizyon", &[], &["link".into()])
        .unwrap();
    assert_eq!(v["brought_types"], json!({"link": 1}));
    let s = inv.show("Televizyon", false).unwrap();
    assert_eq!(s["links"].as_array().unwrap().len(), 1);
    assert!(s["valuations"].as_array().unwrap().is_empty(), "{s}");
    let left: Vec<&str> = v["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["why"].as_str().unwrap())
        .collect();
    assert_eq!(left, ["type", "type"]);
    assert!(
        inv.buy_bring(tv, "Televizyon", &[], &["photo".into()])
            .is_err(),
        "an unknown type is refused"
    );
}

#[test]
fn an_attachment_the_line_does_not_carry_is_refused_not_skipped() {
    let (_d, mut inv) = setup();
    inv.buy_import(&lines(&shop())).unwrap();
    inv.buy_import(&lines(&other_app())).unwrap();
    let tv = id_of(&inv, "o1:a");
    inv.buy_link(tv, "Televizyon", None).unwrap();
    let e = inv
        .buy_bring(tv, "Televizyon", &[9999], &[])
        .unwrap_err()
        .to_string();
    assert!(e.contains("#9999"), "{e}");
}

#[test]
fn bringing_again_says_each_attachment_is_already_brought_and_where() {
    let (_d, mut inv) = setup();
    inv.buy_import(&lines(&shop())).unwrap();
    inv.buy_import(&lines(&other_app())).unwrap();
    let tv = id_of(&inv, "o1:a");
    inv.buy_link(tv, "Televizyon", None).unwrap();
    let first = inv.buy_bring(tv, "Televizyon", &[], &[]).unwrap();
    let node = first["node"]["id"].clone();
    let again = inv.buy_bring(tv, "Televizyon", &[], &[]).unwrap();
    assert_eq!(again["brought_types"], json!({}));
    let skipped = again["skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 3);
    assert!(
        skipped
            .iter()
            .all(|s| s["why"] == "brought" && s["to"] == node),
        "{skipped:?}"
    );
}

#[test]
fn bringing_all_brings_each_line_to_its_thing_and_names_a_gone_one() {
    let (_d, mut inv) = setup();
    inv.add(NewNode {
        name: "Aparat".into(),
        kind: "item".into(),
        parent: Some("Oda".into()),
        ..Default::default()
    })
    .unwrap();
    let mut l = shop();
    l.push(json!({"type": "link", "source": "shop", "purchase": "o1:b",
                  "url": "https://shop.example/mount"}));
    inv.buy_import(&lines(&l)).unwrap();
    inv.buy_import(&lines(&other_app())).unwrap();
    let (tv, mount) = (id_of(&inv, "o1:a"), id_of(&inv, "o1:b"));
    inv.buy_link(tv, "Televizyon", None).unwrap();
    inv.buy_link(mount, "Aparat", None).unwrap();
    inv.gone("Aparat", Some(ev_core::Disposition::Trash))
        .unwrap();
    let v = inv.buy_bring_all(&["link".into()]).unwrap();
    assert_eq!(v["brought_types"], json!({"link": 1}));
    assert_eq!(v["brought_from"][0]["purchase"], tv);
    assert_eq!(v["left"][0]["purchase"], mount);
    assert_eq!(v["left"][0]["why"], "gone");
    let again = inv.buy_bring_all(&[]).unwrap();
    assert_eq!(
        again["brought_types"],
        json!({"valuation": 1, "coverage": 1})
    );
}

#[test]
fn importing_the_same_lines_again_leaves_the_database_file_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ev.db");
    let all: Vec<Value> = shop().into_iter().chain(other_app()).collect();
    let import = || {
        let mut inv = Inventory::open(&db).unwrap();
        inv.buy_import(&lines(&all)).unwrap();
        drop(inv);
        std::fs::read(&db).unwrap()
    };
    let first = import();
    assert_eq!(
        import(),
        first,
        "a re-import that changes nothing writes nothing"
    );
}

#[test]
fn what_an_unlink_leaves_comes_with_a_command_that_runs_as_it_is() {
    let (_d, mut inv) = setup();
    inv.buy_import(&lines(&shop())).unwrap();
    inv.buy_import(&lines(&other_app())).unwrap();
    let tv = id_of(&inv, "o1:a");
    inv.buy_link(tv, "Televizyon", None).unwrap();
    inv.buy_bring(tv, "Televizyon", &[], &[]).unwrap();
    let v = inv.buy_unlink(tv, "Televizyon").unwrap();
    for left in v["left"].as_array().unwrap() {
        let how = left["how"].as_str().unwrap();
        assert!(!how.contains('<'), "{how}");
        assert!(how.starts_with("ev "), "{how}");
    }
}
