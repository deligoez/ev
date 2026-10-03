//! `ev stats` (spec/stats.md): the numbers of a small house.

use ev_core::{Disposition, Inventory, NewNode};
use serde_json::json;

fn node(name: &str, kind: &str, parent: Option<&str>, qty: Option<i64>) -> NewNode {
    NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: parent.map(Into::into),
        qty,
        ..Default::default()
    }
}

#[test]
fn stats_count_the_house_what_it_cost_and_where_it_stands() {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for n in [
        node("Ev", "home", None, None),
        node("Oda", "room", Some("Ev"), None),
        node("Mutfak", "room", Some("Ev"), None),
        node("Kutu", "container", Some("Oda"), None),
        node("Boş kutu", "container", Some("Oda"), None),
        node("Matkap", "item", Some("Kutu"), None),
        node("AA pil", "item", Some("Kutu"), Some(8)),
        node("Eski kablo", "item", Some("Oda"), None),
    ] {
        inv.add(n).unwrap();
    }
    inv.edit("Matkap", &["tags=+alet".into()]).unwrap();
    inv.gone("Eski kablo", Some(Disposition::Trash)).unwrap();
    inv.mark_empty(&["Boş kutu".into()], None).unwrap();
    inv.buy_add(
        &json!({"name": "Darbeli matkap", "shop": "Hırdavatçı", "ordered_at": "2024-05-03",
                "paid": "1999", "currency": "TRY"}),
        Some("Matkap"),
    )
    .unwrap();
    inv.buy_add(
        &json!({"name": "Pil", "shop": "Hırdavatçı", "ordered_at": "2023-01-02",
                "paid": "100", "currency": "TRY"}),
        None,
    )
    .unwrap();

    let v = inv.stats().unwrap();
    assert_eq!(v["overview"]["records"], 2, "the gone cable is not counted");
    assert_eq!(v["overview"]["units"], 9);
    assert_eq!(v["overview"]["rooms"], 2);
    assert_eq!(v["overview"]["containers"], 2);
    assert_eq!(v["value"]["cost"], json!({"TRY": "1999.00"}));
    assert_eq!(v["value"]["things_with_cost"], 1);
    assert_eq!(v["value"]["dearest"][0]["name"], "Matkap");
    assert_eq!(v["rooms"][0]["name"], "Oda");
    assert_eq!(v["rooms"][0]["records"], 2);
    assert_eq!(v["rooms"][0]["cost"], json!({"TRY": "1999.00"}));
    assert_eq!(v["purchases"]["lines"], 2);
    assert_eq!(v["purchases"]["linked"], 1);
    assert_eq!(v["purchases"]["years"][0]["year"], "2024");
    assert_eq!(v["purchases"]["shops"][0]["lines"], 2);
    assert_eq!(v["activity"]["gone"], json!({"trash": 1}));
    assert_eq!(v["holders"]["empty"], 1, "the box called empty");
    assert_eq!(v["tags"], json!([{"tag": "alet", "records": 1}]));
    assert_eq!(v["oldest"][0]["bought"], "2024-05-03");
}
