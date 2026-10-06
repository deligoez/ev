//! What the third QA round of the past-belongings release found, each held by a test.

use ev_core::{Disposition, Inventory, NewNode};

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

fn id(inv: &Inventory, name: &str) -> String {
    format!(
        "#{}",
        inv.show(name, true).unwrap()["node"]["id"]
            .as_i64()
            .unwrap()
    )
}

fn gone(inv: &mut Inventory, name: &str, how: Disposition, at: Option<&str>) {
    inv.gone_left(name, Some(how), None, false, None, at, None)
        .unwrap();
}

#[test]
fn a_leaving_before_the_purchase_that_brought_it_is_refused() {
    let (_d, mut inv) = setup();
    let line = inv
        .buy_add(
            &serde_json::json!({"name": "Telefon", "qty": 1, "ordered_at": "2025-05-05"}),
            None,
        )
        .unwrap()["purchase"]["id"]
        .as_i64()
        .unwrap();
    inv.buy_link(line, "Telefon", None).unwrap();
    let early = inv.gone_left(
        "Telefon",
        Some(Disposition::Trash),
        None,
        false,
        None,
        Some("2016"),
        None,
    );
    assert_eq!(early.unwrap_err().code(), 2);
    gone(&mut inv, "Telefon", Disposition::Trash, None);
    let phone = id(&inv, "Telefon");
    assert_eq!(
        inv.edit(&phone, &["left=2014".into()])
            .unwrap_err()
            .code(),
        2
    );
}

#[test]
fn a_former_place_given_as_a_bare_id_is_refused() {
    let (_d, mut inv) = setup();
    let bare = inv.gone_left(
        "Telefon",
        Some(Disposition::Give),
        None,
        false,
        None,
        None,
        Some("612"),
    );
    assert_eq!(bare.unwrap_err().code(), 2);
}

#[test]
fn a_sale_said_while_waiting_is_dropped_when_the_thing_is_given_instead() {
    let (_d, mut inv) = setup();
    inv.dispose("Telefon", Disposition::Sell).unwrap();
    let phone = id(&inv, "Telefon");
    inv.sold(&phone, "700", None, None, None, None).unwrap();
    gone(&mut inv, &phone, Disposition::Give, None);
    let v = inv.show(&phone, true).unwrap();
    assert!(v["departure"].is_object(), "{v}");
    assert!(v["departure"]["price"].is_null(), "{v}");
}

#[test]
fn a_sale_dated_before_the_thing_left_keeps_it_in_the_left_inventory_list() {
    let (_d, mut inv) = setup();
    inv.dispose("Telefon", Disposition::Sell).unwrap();
    let phone = id(&inv, "Telefon");
    inv.sold(&phone, "700", None, Some("2026-01"), None, None)
        .unwrap();
    gone(&mut inv, &phone, Disposition::Sell, None);
    let past = inv.past(None, None).unwrap();
    assert_eq!(past["left_inventory"]["past"].as_array().unwrap().len(), 1);
    assert!(
        past["remembered"]["past"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_swap_is_said_of_a_giving_or_a_sale_only_and_once() {
    let (_d, mut inv) = setup();
    gone(&mut inv, "Telefon", Disposition::Trash, None);
    let phone = id(&inv, "Telefon");
    assert_eq!(inv.traded(&phone, None).unwrap_err().code(), 5);
}

#[test]
fn a_swap_already_recorded_is_not_written_again() {
    let (_d, mut inv) = setup();
    gone(&mut inv, "Telefon", Disposition::Trade, None);
    let phone = id(&inv, "Telefon");
    let before = inv.history(&phone).unwrap().to_string().len();
    assert_eq!(inv.traded(&phone, None).unwrap_err().code(), 5);
    assert_eq!(inv.history(&phone).unwrap().to_string().len(), before);
}

#[test]
fn a_swap_for_a_thing_that_had_already_left_or_was_ours_before_is_refused() {
    let (_d, mut inv) = setup();
    inv.add(NewNode {
        name: "Konsol".into(),
        gone: Some("give".into()),
        at: Some("2020".into()),
        ..Default::default()
    })
    .unwrap();
    inv.add(NewNode {
        name: "Saat".into(),
        gone: Some("give".into()),
        at: Some("2018".into()),
        ..Default::default()
    })
    .unwrap();
    let (konsol, saat) = (id(&inv, "Konsol"), id(&inv, "Saat"));
    assert_eq!(inv.traded(&konsol, Some(&saat)).unwrap_err().code(), 2);
    inv.edit("Tablet", &["came=2010".into()]).unwrap();
    assert_eq!(inv.traded(&konsol, Some("Tablet")).unwrap_err().code(), 2);
}

#[test]
fn a_gone_thing_is_traded_by_its_name() {
    let (_d, mut inv) = setup();
    gone(&mut inv, "Telefon", Disposition::Give, None);
    inv.traded("Telefon", Some("Tablet")).unwrap();
}

fn line(inv: &mut Inventory, name: &str, ordered: &str, bucket: &str) -> i64 {
    inv.buy_add(
        &serde_json::json!({"name": name, "qty": 1, "ordered_at": ordered, "bucket": bucket}),
        None,
    )
    .unwrap()["purchase"]["id"]
        .as_i64()
        .unwrap()
}

#[test]
fn a_thing_that_left_is_offered_no_line_bought_after_it_left() {
    let (_d, mut inv) = setup();
    gone(&mut inv, "Telefon", Disposition::Give, Some("2018"));
    let later = line(&mut inv, "Telefon", "2024-05-01", "durable");
    let earlier = line(&mut inv, "Telefon", "2017-05-01", "durable");
    let offered: Vec<i64> = inv.buy_for(&id(&inv, "Telefon")).unwrap()["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["purchase"]["id"].as_i64())
        .collect();
    assert!(offered.contains(&earlier), "{offered:?}");
    assert!(!offered.contains(&later), "{offered:?}");
}

#[test]
fn a_link_of_no_units_or_of_a_service_says_so() {
    let (_d, mut inv) = setup();
    let durable = line(&mut inv, "Telefon", "2024-05-01", "durable");
    let zero = inv.buy_link(durable, "Telefon", Some(0)).unwrap_err();
    assert!(zero.to_string().contains("at least 1"), "{zero}");
    let service = line(&mut inv, "Kurulum", "2024-05-01", "service");
    let never = inv.buy_link(service, "Telefon", None).unwrap_err();
    assert!(never.to_string().contains("service"), "{never}");
}
