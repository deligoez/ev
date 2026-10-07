//! Coverage (purchases spec §3.6, §3.7): warranties and insurance with a computed status,
//! proposals from linked purchases, and decisions not to track.

use chrono::{Days, Local, Months};
use ev_core::{Inventory, NewCoverage, NewNode};
use serde_json::{Value, json};

fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent) in [
        ("Ev", "home", None),
        ("Oda", "room", Some("Ev")),
        ("Vida kutusu", "container", Some("Oda")),
        ("Vida", "item", Some("Vida kutusu")),
        ("Matkap", "item", Some("Oda")),
        ("Telefon", "item", Some("Oda")),
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

/// A local day, as ev's own "today" is: a UTC one is the day before between midnight and the
/// UTC offset.
fn day(back_months: u32, back_days: u64) -> String {
    Local::now()
        .date_naive()
        .checked_sub_months(Months::new(back_months))
        .unwrap()
        .checked_sub_days(Days::new(back_days))
        .unwrap()
        .to_string()
}

/// A durable purchase delivered on `delivered`, paid `paid` TRY, linked to `node`.
fn bought(inv: &mut Inventory, node: &str, delivered: &str, paid: &str) {
    let v = inv
        .buy_add(
            &json!({"name": node, "shop": "Shop", "ordered_at": delivered, "paid": paid,
                    "currency": "TRY"}),
            Some(node),
        )
        .unwrap();
    assert_eq!(v["purchase"]["open_qty"], 0);
}

fn cover(kind: &str, term: Option<&str>) -> NewCoverage {
    NewCoverage {
        kind: kind.into(),
        term: term.map(Into::into),
        ..Default::default()
    }
}

fn coverages(inv: &Inventory, r: &str) -> Value {
    inv.show(r, false).unwrap()["coverages"].clone()
}

#[test]
fn a_linked_purchase_proposes_two_years_from_delivery_until_one_is_recorded() {
    let (_d, mut inv) = setup();
    let delivered = day(23, 0);
    bought(&mut inv, "Matkap", &delivered, "1999");
    let p = inv.show("Matkap", false).unwrap()["coverage_proposal"].clone();
    assert_eq!(p["kind"], "statutory");
    assert_eq!(p["start"], delivered);
    inv.cover_add(&["Matkap".into()], &cover("statutory", Some("2y")))
        .unwrap();
    let s = inv.show("Matkap", false).unwrap();
    assert!(s["coverage_proposal"].is_null());
    // Twenty-three months in, a two-year warranty ends within the 60-day window.
    assert_eq!(s["coverages"][0]["status"], "ending");
    assert_eq!(s["coverages"][0]["start"], delivered);
    let todo = inv.todo().unwrap();
    assert_eq!(todo["counts"]["coverage_ending"], 1);
}

#[test]
fn time_in_repair_is_added_to_a_manufacturer_warranty() {
    let (_d, mut inv) = setup();
    bought(&mut inv, "Matkap", &day(25, 0), "1999");
    inv.cover_add(&["Matkap".into()], &cover("manufacturer", Some("2y")))
        .unwrap();
    assert_eq!(coverages(&inv, "Matkap")[0]["status"], "ended");
    // Broken ninety days ago and still in repair: those days are added, so it runs again.
    inv.broken("Matkap", Some("motor"), false).unwrap();
    let db = rusqlite::Connection::open(_d.path().join("ev.db")).unwrap();
    db.execute(
        "UPDATE events SET at = ?1 WHERE type = 'broken'",
        [format!("{}T09:00:00Z", day(0, 90))],
    )
    .unwrap();
    let c = coverages(&inv, "Matkap")[0].clone();
    assert_eq!(c["repair_days"], 90, "{c}");
    // Ended a month ago; ninety days later it ends in about two months.
    assert_eq!(c["status"], "ending", "{c}");
    // A statutory one counts it too; a store warranty does not.
    inv.cover_add(&["Matkap".into()], &cover("store", Some("2y")))
        .unwrap();
    assert!(coverages(&inv, "Matkap")[1].get("repair_days").is_none());
}

#[test]
fn an_extended_warranty_starts_when_the_manufacturers_ends() {
    let (_d, mut inv) = setup();
    let delivered = day(30, 0);
    bought(&mut inv, "Matkap", &delivered, "1999");
    let m = inv
        .cover_add(&["Matkap".into()], &cover("manufacturer", Some("2y")))
        .unwrap()["coverage"]["id"]
        .as_i64()
        .unwrap();
    let e = inv
        .cover_add(
            &["Matkap".into()],
            &NewCoverage {
                from: Some(format!("after:{m}")),
                ..cover("extended", Some("1y"))
            },
        )
        .unwrap();
    let c = &e["coverage"];
    assert_eq!(c["start"], coverages(&inv, "Matkap")[0]["end"]);
    assert_eq!(c["status"], "active");
}

#[test]
fn an_insurance_needs_an_end_and_can_cover_several_things() {
    let (_d, mut inv) = setup();
    let refs = ["Matkap".to_string(), "Telefon".to_string()];
    assert!(inv.cover_add(&refs, &cover("insurance", None)).is_err());
    assert!(
        inv.cover_add(&refs, &cover("warranty", Some("2y")))
            .is_err()
    );
    let v = inv
        .cover_add(
            &refs,
            &NewCoverage {
                ends: Some(day(0, 0).replace(&day(0, 0)[..4], "2099")),
                issuer: Some("Sigortacı".into()),
                number: Some("POL-1".into()),
                premium: Some("1.200,00".into()),
                scope: Some("ekran kırılması, hırsızlık".into()),
                ..cover("insurance", None)
            },
        )
        .unwrap();
    assert_eq!(v["coverage"]["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(v["coverage"]["premium"], "1200.00");
    assert_eq!(v["coverage"]["status"], "active");
    assert_eq!(coverages(&inv, "Telefon")[0]["kind"], "insurance");
}

#[test]
fn a_decision_on_a_holder_silences_its_contents_until_data_is_entered() {
    let (_d, mut inv) = setup();
    bought(&mut inv, "Vida", &day(1, 0), "5000");
    bought(&mut inv, "Matkap", &day(1, 0), "1999");
    assert_eq!(inv.todo().unwrap()["counts"]["coverage"], 2);
    inv.track("Vida kutusu", "coverage", "no", Some("vidalar"))
        .unwrap();
    let s = inv.show("Vida", false).unwrap();
    assert_eq!(s["tracking"]["coverage"]["decision"], "no");
    assert!(s["coverage_proposal"].is_null());
    assert_eq!(inv.todo().unwrap()["counts"]["coverage"], 1);
    inv.track("Matkap", "coverage", "later", None).unwrap();
    assert_eq!(inv.todo().unwrap()["counts"]["coverage"], 0);
    // Recording a coverage is entering data: the thing's own decision goes.
    inv.cover_add(&["Matkap".into()], &cover("manufacturer", Some("2y")))
        .unwrap();
    assert!(inv.show("Matkap", false).unwrap()["tracking"]["coverage"].is_null());
    assert!(inv.track("Matkap", "color", "no", None).is_err());
}

#[test]
fn the_threshold_is_an_inventory_setting() {
    let (_d, mut inv) = setup();
    bought(&mut inv, "Matkap", &day(1, 0), "800");
    assert_eq!(inv.todo().unwrap()["counts"]["coverage"], 0);
    inv.inventory_settings(Some("valuable_threshold"), Some("500"))
        .unwrap();
    assert_eq!(inv.todo().unwrap()["counts"]["coverage"], 1);
    let s = inv.inventory_settings(None, None).unwrap();
    assert_eq!(
        s["inventory_settings"]["valuable_threshold"]["value"],
        "500"
    );
    assert_eq!(s["inventory_settings"]["home_currency"]["default"], true);
    assert!(
        inv.inventory_settings(Some("home_currency"), Some("lira"))
            .is_err()
    );
}

/// A durable purchase from `shop`, paid in `currency`, delivered `delivered`, linked to `node`.
fn bought_from(inv: &mut Inventory, node: &str, shop: &str, currency: &str, delivered: &str) {
    inv.buy_add(
        &json!({"name": node, "shop": shop, "ordered_at": delivered, "paid": "100",
                "currency": currency}),
        Some(node),
    )
    .unwrap();
}

fn proposal(inv: &Inventory, r: &str) -> Value {
    inv.show(r, false).unwrap()["coverage_proposal"].clone()
}

#[test]
fn no_statutory_warranty_is_proposed_once_it_has_ended_or_for_a_line_bought_abroad() {
    let (_d, mut inv) = setup();
    // Two years and a month ago: the two years have passed, so there is nothing to ask.
    bought(&mut inv, "Matkap", &day(25, 0), "1999");
    assert!(proposal(&inv, "Matkap").is_null());
    // A foreign marketplace, even charging in the home currency.
    bought_from(&mut inv, "Telefon", "AliExpress", "TRY", &day(3, 0));
    assert!(proposal(&inv, "Telefon").is_null());
    // Another currency than the home one.
    bought_from(&mut inv, "Vida kutusu", "Shop", "EUR", &day(3, 0));
    assert!(proposal(&inv, "Vida kutusu").is_null());
    // A line from a shop at home proposes, from its own delivery.
    bought_from(&mut inv, "Vida", "Shop", "TRY", &day(3, 0));
    assert_eq!(proposal(&inv, "Vida")["start"], day(3, 0));

    // At home in Germany, a euro line and Amazon.de are at home; AliExpress is still abroad.
    inv.inventory_settings(Some("home_country"), Some("DE"))
        .unwrap();
    inv.inventory_settings(Some("home_currency"), Some("EUR"))
        .unwrap();
    assert_eq!(proposal(&inv, "Vida kutusu")["kind"], "statutory");
    assert!(proposal(&inv, "Telefon").is_null());
    bought_from(&mut inv, "Telefon", "Amazon.de", "EUR", &day(2, 0));
    assert_eq!(proposal(&inv, "Telefon")["start"], day(2, 0));
}

#[test]
fn a_coverage_on_the_wrong_thing_moves_to_the_right_one_and_keeps_its_id() {
    let (_d, mut inv) = setup();
    let id = inv
        .cover_add(
            &["Matkap".into()],
            &NewCoverage {
                ends: Some("2030-01-01".into()),
                ..cover("insurance", None)
            },
        )
        .unwrap()["coverage"]["id"]
        .as_i64()
        .unwrap();
    // It insures the phone, not the drill: moved, with its number corrected.
    let v = inv
        .cover_edit(
            id,
            &["Telefon".into()],
            &["Matkap".into()],
            &["number=P-123".into(), "premium=500".into()],
        )
        .unwrap();
    let c = &v["coverage"];
    assert_eq!(c["id"], id);
    assert_eq!(c["number"], "P-123");
    assert_eq!(c["premium"], "500.00");
    assert!(coverages(&inv, "Matkap").as_array().unwrap().is_empty());
    assert_eq!(coverages(&inv, "Telefon")[0]["id"], id);
    let e = |r: ev_core::Result<Value>| r.unwrap_err().id().map(str::to_string);
    // Never on nothing; never without an end; no field it does not have.
    assert_eq!(
        e(inv.cover_edit(id, &[], &["Telefon".into()], &[])).as_deref(),
        Some("coverage_on_nothing")
    );
    assert_eq!(
        e(inv.cover_edit(id, &[], &[], &["ends=".into()])).as_deref(),
        Some("coverage_needs_term")
    );
    assert_eq!(
        e(inv.cover_edit(id, &[], &[], &["colour=red".into()])).as_deref(),
        Some("coverage_edit_field_unknown")
    );
}
