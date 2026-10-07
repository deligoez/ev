//! Vehicles (spec/vehicles-homes.md): at the top beside the homes, a holder with a thing's life.

use ev_core::{Inventory, NewNode};
use serde_json::{Value, json};
use tempfile::TempDir;

fn add(inv: &mut Inventory, new: NewNode) -> Result<i64, ev_core::Error> {
    Ok(inv.add(new)?["node"]["id"].as_i64().unwrap())
}

fn node(name: &str, kind: &str, parent: Option<&str>) -> NewNode {
    NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: parent.map(Into::into),
        ..Default::default()
    }
}

/// A home with a shelf, and a car with a glovebox.
fn setup() -> (TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    add(&mut inv, node("Ev", "home", None)).unwrap();
    add(&mut inv, node("Oda", "room", Some("Ev"))).unwrap();
    add(
        &mut inv,
        NewNode {
            code: Some("R-1".into()),
            ..node("Raf", "container", Some("Oda"))
        },
    )
    .unwrap();
    add(
        &mut inv,
        NewNode {
            code: Some("34 ABC 123".into()),
            make: Some("Marka".into()),
            model: Some("Model 1.6 2020".into()),
            ..node("Aile arabası", "vehicle", None)
        },
    )
    .unwrap();
    add(&mut inv, node("Torpido", "container", Some("34 ABC 123"))).unwrap();
    (dir, inv)
}

fn id_of(e: ev_core::Error) -> Option<String> {
    e.id().map(str::to_string)
}

#[test]
fn a_vehicle_stands_at_the_top_and_is_never_inside_anything_nor_lost() {
    let (_d, mut inv) = setup();
    let inside = add(&mut inv, node("İkinci araba", "vehicle", Some("Oda"))).unwrap_err();
    assert_eq!(id_of(inside).as_deref(), Some("vehicle_inside"));
    let moved = inv.move_to("34 ABC 123", "Oda", false).unwrap_err();
    assert_eq!(id_of(moved).as_deref(), Some("vehicle_inside"));
    let lost = inv.mark_lost_qty("34 ABC 123", None).unwrap_err();
    assert_eq!(id_of(lost).as_deref(), Some("vehicle_cannot_be_lost"));
    // The tree: the homes, then the vehicles beside them.
    let tree = inv.tree(None, None).unwrap();
    let roots: Vec<(&str, &str)> = tree["tree"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| (t["name"].as_str().unwrap(), t["kind"].as_str().unwrap()))
        .collect();
    assert_eq!(roots, [("Ev", "home"), ("Aile arabası", "vehicle")]);
}

#[test]
fn a_vehicles_plate_needs_no_label_and_a_past_vehicle_keeps_it() {
    let (_d, mut inv) = setup();
    let car = inv.show("34 ABC 123", false).unwrap();
    assert!(car["marks"]["label"].is_null(), "{}", car["marks"]);
    // A box's new code still asks for a label.
    let shelf = inv.show("R-1", false).unwrap();
    assert_eq!(shelf["marks"]["label"]["value"], "needed");
    // A car that was sold is recorded with the last plate it carried.
    add(
        &mut inv,
        NewNode {
            code: Some("06 XY 99".into()),
            gone: Some("sell".into()),
            came: Some("2018-03".into()),
            at: Some("2026-04-08".into()),
            ..node("Eski araba", "vehicle", None)
        },
    )
    .unwrap();
    let juke = inv.show("06 XY 99", true).unwrap();
    assert_eq!(juke["node"]["state"], "gone");
    assert!(juke["marks"]["label"].is_null());
}

#[test]
fn placement_keeps_the_home_and_a_vehicle_apart() {
    let (_d, mut inv) = setup();
    add(&mut inv, node("Yangın söndürücü", "item", Some("Torpido"))).unwrap();
    let names = |v: &Value| -> Vec<String> {
        v["containers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap().to_string())
            .collect()
    };
    // Asked for the home, the car's glovebox is never offered, though its words match.
    let home = inv.suggest("yangın söndürücü", None).unwrap();
    assert!(!names(&home).contains(&"Torpido".to_string()), "{home}");
    assert!(names(&home).contains(&"Raf".to_string()), "{home}");
    // Asked about a thing in the car, only the car's places are weighed.
    let car = inv
        .suggest_with("yangın söndürücü", None, Some("Yangın söndürücü"))
        .unwrap();
    assert!(!names(&car).contains(&"Raf".to_string()), "{car}");
}

#[test]
fn a_vehicles_price_is_counted_apart_from_the_homes_things() {
    let (_d, mut inv) = setup();
    add(&mut inv, node("Matkap", "item", Some("R-1"))).unwrap();
    let line = |name: &str, paid: &str| json!({ "name": name, "paid": paid, "currency": "TRY" });
    inv.buy_add(&line("Araba", "2000000"), Some("34 ABC 123"))
        .unwrap();
    inv.buy_add(&line("Matkap", "1999"), Some("Matkap"))
        .unwrap();
    let stats = inv.stats().unwrap();
    // The home's value is its things'; the car stands apart, with its own price.
    assert_eq!(
        stats["value"]["cost"]["TRY"], "1999.00",
        "{}",
        stats["value"]
    );
    assert_eq!(stats["value"]["dearest"].as_array().unwrap().len(), 1);
    let vehicles = stats["vehicles"].as_array().unwrap();
    assert_eq!(vehicles.len(), 1);
    assert_eq!(vehicles[0]["code"], "34 ABC 123");
    assert_eq!(vehicles[0]["cost"]["TRY"], "2000000.00");
}

#[test]
fn units_directly_in_a_vehicle_are_in_use_and_those_in_its_glovebox_spare() {
    let (_d, mut inv) = setup();
    let shelf = add(
        &mut inv,
        NewNode {
            qty: Some(6),
            ..node("H7 ampul", "item", Some("R-1"))
        },
    )
    .unwrap();
    let shelf = format!("#{shelf}");
    // Two fitted in the car, two kept in its glovebox as spares.
    inv.move_qty(&shelf, "34 ABC 123", false, Some(2)).unwrap();
    let v = inv.move_qty(&shelf, "Torpido", false, Some(2)).unwrap();
    let thing = &v["thing"];
    assert_eq!(
        (&thing["total"], &thing["in_use"], &thing["spare"]),
        (&6.into(), &2.into(), &4.into()),
        "{thing}"
    );
}

#[test]
fn a_vehicle_is_toured_through_its_compartments_and_is_ours_in_its_years() {
    let (_d, mut inv) = setup();
    let places: Vec<String> = inv.progress_in(None).unwrap()["places"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_string())
        .collect();
    assert!(places.contains(&"Torpido".to_string()), "{places:?}");
    assert!(!places.contains(&"Aile arabası".to_string()), "{places:?}");
    // A car sold is among what was ours in the years it was here.
    add(
        &mut inv,
        NewNode {
            gone: Some("sell".into()),
            came: Some("2018-03".into()),
            at: Some("2026-04-08".into()),
            ..node("Eski araba", "vehicle", None)
        },
    )
    .unwrap();
    let year = |y: i32| -> Vec<String> {
        inv.past_year(y).unwrap()["owned"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert!(year(2020).contains(&"Eski araba".to_string()));
    assert!(!year(2017).contains(&"Eski araba".to_string()));
}

#[test]
fn a_vehicle_sold_takes_its_compartments_once_they_are_empty() {
    let (_d, mut inv) = setup();
    add(&mut inv, node("Yangın söndürücü", "item", Some("Torpido"))).unwrap();
    let e = inv
        .gone("34 ABC 123", Some(ev_core::Disposition::Sell))
        .unwrap_err();
    assert_eq!(id_of(e).as_deref(), Some("leaving_still_holds"));
    // The extinguisher goes to the home; the car is sold with its glovebox.
    inv.move_to("Yangın söndürücü", "R-1", false).unwrap();
    let sold = inv
        .gone("34 ABC 123", Some(ev_core::Disposition::Sell))
        .unwrap();
    assert_eq!(sold["node"]["state"], "gone");
    let glovebox = inv.show("Torpido", true).unwrap();
    assert_eq!(glovebox["node"]["state"], "gone");
    assert_eq!(glovebox["node"]["disposition"], "sell");
}
