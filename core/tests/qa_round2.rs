//! What the second QA round of the past-belongings release found, each held by a test.

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

fn id(inv: &Inventory, name: &str) -> String {
    format!(
        "#{}",
        inv.show(name, true).unwrap()["node"]["id"]
            .as_i64()
            .unwrap()
    )
}

#[test]
fn a_thing_taken_back_keeps_no_departure_and_its_swap_is_undone() {
    let (_d, mut inv) = setup();
    inv.gone_left(
        "Telefon",
        Some(Disposition::Give),
        None,
        false,
        None,
        None,
        None,
    )
    .unwrap();
    let phone = id(&inv, "Telefon");
    inv.traded(&phone, Some("Tablet")).unwrap();
    inv.correct_gone(&phone, "it is still here").unwrap();
    let tablet = inv.show("Tablet", false).unwrap();
    assert!(
        tablet["traded_from"].as_array().is_none_or(Vec::is_empty),
        "{tablet}"
    );
}

#[test]
fn a_sale_before_the_thing_came_or_a_line_bought_after_it_left_is_refused() {
    let (_d, mut inv) = setup();
    inv.add(NewNode {
        name: "Konsol".into(),
        gone: Some("sell".into()),
        at: Some("2019".into()),
        came: Some("2016".into()),
        ..Default::default()
    })
    .unwrap();
    let r = id(&inv, "Konsol");
    let early = inv.sold(&r, "100", None, Some("2015"), None, None);
    assert_eq!(early.unwrap_err().code(), 2);
    let line = inv
        .buy_add(
            &serde_json::json!({"name": "Konsol", "qty": 1, "ordered_at": "2020-03-01"}),
            None,
        )
        .unwrap()["purchase"]["id"]
        .as_i64()
        .unwrap();
    assert_eq!(inv.buy_link(line, &r, None).unwrap_err().code(), 5);
}

#[test]
fn a_coverage_s_line_is_checked_first_and_its_currency_follows_the_line() {
    let (_d, mut inv) = setup();
    assert_eq!(inv.cover_show(99).unwrap_err().code(), 3);
    let line = |inv: &mut Inventory, paid: &str, currency: &str| {
        inv.buy_add(
            &serde_json::json!({"name": "Garanti", "qty": 1, "paid": paid, "currency": currency}),
            None,
        )
        .unwrap()["purchase"]["id"]
            .as_i64()
            .unwrap()
    };
    let (try_line, eur_line) = (
        line(&mut inv, "400.00", "TRY"),
        line(&mut inv, "30.00", "EUR"),
    );
    inv.buy_dismiss(try_line, Some("given"), None).unwrap();
    assert_eq!(inv.cover_line_check(try_line, None).unwrap_err().code(), 5);
    let c = inv
        .cover_add(
            &["Telefon".into()],
            &NewCoverage {
                kind: "extended".into(),
                term: Some("2y".into()),
                ..Default::default()
            },
        )
        .unwrap()["coverage"]["id"]
        .as_i64()
        .unwrap();
    let v = inv.cover_purchase(c, Some(eur_line)).unwrap();
    assert_eq!(v["coverage"]["premium"], "30.00");
    assert_eq!(v["coverage"]["currency"], "EUR");
}

#[test]
fn the_dearest_things_cost_is_never_added_across_currencies() {
    let (_d, mut inv) = setup();
    for (paid, currency) in [("299.00", "EUR"), ("6.00", "USD")] {
        inv.buy_add(
            &serde_json::json!({"name": "Telefon", "qty": 1, "paid": paid, "currency": currency}),
            Some("Telefon"),
        )
        .unwrap();
    }
    let v = inv.stats().unwrap();
    let cost = &v["value"]["dearest"][0]["cost"];
    assert_eq!(cost["EUR"], "299.00", "{v}");
    assert_eq!(cost["USD"], "6.00", "{v}");
}

#[test]
fn a_dropped_task_is_not_done_and_an_open_one_is_not_reopened() {
    let (_d, mut inv) = setup();
    let t = inv.task_add("Say", "neden", &["Oda".into()], None).unwrap()["id"]
        .as_i64()
        .unwrap();
    assert_eq!(inv.task_set(t, "open", None).unwrap_err().code(), 5);
    inv.task_set(t, "dropped", None).unwrap();
    assert_eq!(inv.task_set(t, "done", None).unwrap_err().code(), 5);
}
