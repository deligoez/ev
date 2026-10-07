//! Money over time (purchases spec §3.9): a purchase price in today's home money, from the
//! cached price index and the purchase day's exchange rate.

use ev_core::Inventory;
use serde_json::{Value, json};

fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    (dir, inv)
}

fn buy(inv: &mut Inventory, paid: &str, currency: &str, day: &str) -> i64 {
    let v = inv
        .buy_add(
            &json!({"name": "Kulaklık", "ordered_at": day, "paid": paid, "currency": currency}),
            None,
        )
        .unwrap();
    v["purchase"]["id"].as_i64().unwrap()
}

fn lines(v: &[Value]) -> String {
    v.iter().map(|l| format!("{l}\n")).collect()
}

const INDEX: [(&str, f64); 2] = [("2024-05", 71.67), ("2026-08", 134.76)];

fn index_lines() -> Vec<Value> {
    INDEX
        .iter()
        .map(|(p, v)| json!({"type": "index", "series": "eurostat:TR", "period": p, "value": v}))
        .collect()
}

#[test]
fn a_foreign_purchase_takes_its_day_rate_then_grows_by_the_home_index() {
    let (_d, mut inv) = setup();
    // Bought on a Saturday: the rate is Friday's.
    let id = buy(&mut inv, "120", "EUR", "2024-05-04");
    let mut l = index_lines();
    l.push(
        json!({"type": "rate", "currency": "eur", "home": "TRY", "day": "2024-05-03",
                  "rate": 34.6959}),
    );
    inv.money_import(&lines(&l)).unwrap();
    let t = &inv.buy_show(id).unwrap()["purchase"]["today"];
    assert_eq!(t["in_home_then"], "4163.51");
    assert_eq!(t["amount"], "7828.58");
    assert_eq!(t["index_month"], "2026-08");
    assert_eq!(t["rate_day"], "2024-05-03");
}

#[test]
fn without_the_index_or_the_rate_there_is_no_todays_money() {
    let (_d, mut inv) = setup();
    let home = buy(&mut inv, "500.00", "TRY", "2024-05-16");
    let foreign = buy(&mut inv, "120", "EUR", "2024-05-04");
    assert!(
        inv.buy_show(home).unwrap()["purchase"]
            .get("today")
            .is_none()
    );
    inv.money_import(&lines(&index_lines())).unwrap();
    assert_eq!(
        inv.buy_show(home).unwrap()["purchase"]["today"]["amount"],
        "940.14"
    );
    assert!(
        inv.buy_show(foreign).unwrap()["purchase"]
            .get("today")
            .is_none()
    );
}

#[test]
fn needs_asks_for_the_index_from_the_first_purchase_and_each_missing_rate() {
    let (_d, mut inv) = setup();
    buy(&mut inv, "10", "TRY", "2019-02-10");
    buy(&mut inv, "120", "EUR", "2024-05-04");
    let n = &inv.money_needs().unwrap()["money_needs"];
    assert_eq!(n["from_month"], "2019-02");
    assert_eq!(n["index"], "eurostat:TR");
    assert_eq!(
        n["rates"],
        json!([{"currency": "EUR", "day": "2024-05-04"}])
    );
    // A rate from the week before covers the day.
    inv.money_import(&lines(&[
        json!({"type": "rate", "currency": "EUR", "home": "TRY",
                                     "day": "2024-05-03", "rate": 34.7}),
    ]))
    .unwrap();
    assert_eq!(
        inv.money_needs().unwrap()["money_needs"]["rates"],
        json!([])
    );
}

#[test]
fn an_import_with_one_bad_line_imports_nothing() {
    let (_d, mut inv) = setup();
    let mut l = index_lines();
    l.push(json!({"type": "index", "series": "eurostat:TR", "period": "2024/06", "value": 72}));
    let e = inv.money_import(&lines(&l)).unwrap_err().to_string();
    assert!(e.contains("line 3"), "{e}");
    assert_eq!(inv.money_status().unwrap()["money"]["periods"], 0);
}

#[test]
fn a_purchase_remembered_to_the_month_reads_in_todays_money_as_that_month() {
    let (_d, mut inv) = setup();
    // Bought "in 2024-05", no day said: kept as said, and today's money reads that month.
    let id = buy(&mut inv, "500.00", "TRY", "2024-05");
    inv.money_import(&lines(&index_lines())).unwrap();
    let p = &inv.buy_show(id).unwrap()["purchase"];
    assert_eq!(p["ordered_at"], "2024-05");
    assert_eq!(p["today"]["amount"], "940.14");
    // A month that is none, or one still to come, is refused.
    for bad in ["2024-13", "2099-01"] {
        let e = inv
            .buy_add(&json!({"name": "X", "ordered_at": bad, "paid": "1"}), None)
            .unwrap_err();
        assert_eq!(e.code(), 2, "{bad}");
    }
}
