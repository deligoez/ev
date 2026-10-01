//! Coverage (purchases spec §3.6, §3.7): warranties and insurance with a computed status,
//! proposals from linked purchases, and decisions not to track.

use chrono::{Days, Months, Utc};
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

fn day(back_months: u32, back_days: u64) -> String {
    Utc::now()
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
    bought(&mut inv, "Matkap", &delivered, "2479");
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
    bought(&mut inv, "Matkap", &day(25, 0), "2479");
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
    bought(&mut inv, "Matkap", &delivered, "2479");
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
    bought(&mut inv, "Matkap", &day(1, 0), "2479");
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
