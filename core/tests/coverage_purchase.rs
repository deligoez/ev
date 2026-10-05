//! An extended warranty bought as a purchase line of its own: the line is the coverage's, which
//! settles it.

use ev_core::{Inventory, NewCoverage, NewNode};

#[test]
fn a_warranty_bought_as_its_own_line_settles_it_and_takes_its_price() {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent) in [
        ("Ev", "home", None),
        ("Mutfak", "room", Some("Ev")),
        ("Buzdolabı", "item", Some("Mutfak")),
    ] {
        inv.add(NewNode {
            name: name.into(),
            kind: kind.into(),
            parent: parent.map(Into::into),
            ..Default::default()
        })
        .unwrap();
    }
    let line = inv
        .buy_add(
            &serde_json::json!({"name": "3 Yıl Uzatılmış Garanti", "qty": 1, "paid": "4390.00"}),
            None,
        )
        .unwrap()["purchase"]["id"]
        .as_i64()
        .unwrap();
    let open = |inv: &Inventory| {
        inv.buy_list(true, None, None, None).unwrap()["purchases"]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!(open(&inv), 1);
    let c = inv
        .cover_add(
            &["Buzdolabı".into()],
            &NewCoverage {
                kind: "extended".into(),
                term: Some("3y".into()),
                ..Default::default()
            },
        )
        .unwrap()["coverage"]["id"]
        .as_i64()
        .unwrap();
    let v = inv.cover_purchase(c, Some(line)).unwrap();
    assert_eq!(v["coverage"]["purchase"]["id"], line);
    assert_eq!(v["coverage"]["premium"], "4390.00");
    assert_eq!(open(&inv), 0);
    assert_eq!(inv.todo().unwrap()["counts"]["purchases"], 0);
    // Taken back, the line waits again.
    inv.cover_purchase(c, None).unwrap();
    assert_eq!(open(&inv), 1);
}
