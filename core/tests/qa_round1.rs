//! What the first QA round of the past-belongings release found, each held by a test.

use ev_core::{Disposition, Inventory, NewCoverage, NewNode};

fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent) in [
        ("Ev", "home", None),
        ("Oda", "room", Some("Ev")),
        ("Telefon", "item", Some("Oda")),
        ("Tablet", "item", Some("Oda")),
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

fn id(inv: &Inventory, name: &str) -> i64 {
    inv.show(name, true).unwrap()["node"]["id"]
        .as_i64()
        .unwrap()
}

#[test]
fn things_set_aside_to_trade_or_return_are_in_the_piles() {
    let (_d, mut inv) = setup();
    inv.dispose("Telefon", Disposition::Trade).unwrap();
    inv.dispose("Tablet", Disposition::Return).unwrap();
    let v = inv.disposals(None).unwrap();
    assert_eq!(v["disposals"]["trade"][0]["name"], "Telefon", "{v}");
    assert_eq!(v["disposals"]["return"][0]["name"], "Tablet", "{v}");
    assert_eq!(inv.todo().unwrap()["counts"]["disposals"], 2);
}

#[test]
fn a_sale_said_again_keeps_its_currency_and_a_trade_drops_the_price() {
    let (_d, mut inv) = setup();
    inv.gone_left(
        "Telefon",
        Some(Disposition::Sell),
        None,
        false,
        None,
        None,
        None,
    )
    .unwrap();
    let r = format!("#{}", id(&inv, "Telefon"));
    inv.sold(&r, "1500", Some("EUR"), None, None, None).unwrap();
    let v = inv.sold(&r, "1400", None, None, None, None).unwrap();
    assert_eq!(v["departure"]["currency"], "EUR");
    // It was a swap after all: what it brought is no longer a price.
    let v = inv.traded(&r, Some("Tablet")).unwrap();
    assert!(v["departure"]["price"].is_null(), "{v}");
}

#[test]
fn a_date_still_to_come_or_a_coming_after_the_leaving_is_refused() {
    let (_d, mut inv) = setup();
    let future = inv.add(NewNode {
        name: "Konsol".into(),
        gone: Some("sell".into()),
        at: Some("2099".into()),
        ..Default::default()
    });
    assert_eq!(future.unwrap_err().code(), 2);
    let backwards = inv.add(NewNode {
        name: "Konsol".into(),
        gone: Some("sell".into()),
        at: Some("2018".into()),
        came: Some("2020".into()),
        ..Default::default()
    });
    assert_eq!(backwards.unwrap_err().code(), 2);
    assert_eq!(inv.past_year(2099).unwrap_err().code(), 2);
}

#[test]
fn a_past_thing_taken_back_has_a_place_not_known() {
    let (_d, mut inv) = setup();
    inv.add(NewNode {
        name: "Konsol".into(),
        gone: Some("left".into()),
        ..Default::default()
    })
    .unwrap();
    let r = format!("#{}", id(&inv, "Konsol"));
    let v = inv.correct_gone(&r, "it is still here").unwrap();
    assert_eq!(v["node"]["state"], "active");
    assert_eq!(v["node"]["lost"], true, "{v}");
}

#[test]
fn a_part_sold_from_a_listing_keeps_where_it_was_listed() {
    let (_d, mut inv) = setup();
    inv.edit("Telefon", &["qty=3".into()]).unwrap();
    inv.dispose("Telefon", Disposition::Sell).unwrap();
    inv.sale("Telefon", Some("listed"), Some(500), Some("Pazar"), None)
        .unwrap();
    let v = inv
        .gone_left("Telefon", None, None, false, Some(1), None, None)
        .unwrap();
    assert_eq!(v["departure"]["via"], "Pazar", "{v}");
    // The rest is still listed.
    let rest = inv.show("Telefon", false).unwrap();
    assert!(!rest["marks"]["sale"].is_null(), "{rest}");
}

#[test]
fn a_line_linked_to_a_thing_or_another_coverage_is_not_a_coverage_s() {
    let (_d, mut inv) = setup();
    let line = |inv: &mut Inventory, name: &str, for_ref: Option<&str>| {
        inv.buy_add(
            &serde_json::json!({"name": name, "qty": 1, "paid": "300.00"}),
            for_ref,
        )
        .unwrap()["purchase"]["id"]
            .as_i64()
            .unwrap()
    };
    let linked = line(&mut inv, "Telefon", Some("Telefon"));
    let warranty = line(&mut inv, "Uzatılmış garanti", None);
    let cover = |inv: &mut Inventory| {
        inv.cover_add(
            &["Telefon".into()],
            &NewCoverage {
                kind: "extended".into(),
                term: Some("2y".into()),
                ..Default::default()
            },
        )
        .unwrap()["coverage"]["id"]
            .as_i64()
            .unwrap()
    };
    let (a, b) = (cover(&mut inv), cover(&mut inv));
    assert_eq!(inv.cover_purchase(a, Some(linked)).unwrap_err().code(), 5);
    inv.cover_purchase(a, Some(warranty)).unwrap();
    assert_eq!(inv.cover_purchase(b, Some(warranty)).unwrap_err().code(), 5);
    // Taken back, the price it brought goes too.
    let v = inv.cover_purchase(a, None).unwrap();
    assert!(v["coverage"]["premium"].is_null(), "{v}");
}
