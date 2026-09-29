//! Spec §22: a holder laid out in cells, and boxes placed in them.

use ev_core::{Inventory, NewNode};
use serde_json::Value;
use tempfile::TempDir;

fn add(inv: &mut Inventory, name: &str, kind: &str, parent: Option<&str>, code: Option<&str>) {
    inv.add(NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: parent.map(Into::into),
        code: code.map(Into::into),
        ..Default::default()
    })
    .unwrap();
}

/// A drawer `D` with a tall box and two small ones, none placed yet.
fn setup() -> (TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    add(&mut inv, "Ev", "home", None, None);
    add(&mut inv, "Oda", "room", Some("Ev"), None);
    add(&mut inv, "Çekmece", "container", Some("Oda"), Some("D"));
    add(&mut inv, "Uzun kutu", "container", Some("D"), Some("D-A4"));
    add(&mut inv, "Sıcaklık", "container", Some("D"), Some("D-A3"));
    add(&mut inv, "Giriş", "container", Some("D"), Some("D-B3"));
    (dir, inv)
}

fn pairs(p: &[(&str, &str)]) -> Vec<(String, String)> {
    p.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

fn free(inv: &Inventory) -> Vec<String> {
    inv.grid("D").unwrap()["grid"]["free"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn boxes_are_placed_checked_and_mapped() {
    let (_d, mut inv) = setup();
    // No grid yet: placing is refused and says how to set one.
    assert_eq!(
        inv.cells_set(&pairs(&[("D-A3", "A3")]), false)
            .unwrap_err()
            .code(),
        5
    );
    inv.grid_set("D", 3, 4).unwrap();
    inv.cells_set(
        &pairs(&[("D-A4", "A4-B4"), ("D-A3", "A3"), ("D-B3", "B3")]),
        false,
    )
    .unwrap();
    assert_eq!(free(&inv).len(), 12 - 4);
    let g = &inv.grid("D").unwrap()["grid"];
    let a4: Value = g["map"][3][1].clone();
    assert_eq!(a4, g["map"][3][0], "a box covers every cell of its range");
    assert!(g["map"][0][0].is_null());

    // Outside the grid, and on top of another box, are refused and change nothing.
    assert_eq!(
        inv.cells_set(&pairs(&[("D-A3", "D3")]), false)
            .unwrap_err()
            .code(),
        5
    );
    assert_eq!(
        inv.cells_set(&pairs(&[("D-A3", "B4")]), false)
            .unwrap_err()
            .code(),
        5
    );
    assert_eq!(inv.show("D-A3", false).unwrap()["cells"], "A3");
    // A grid cannot shrink under a placed box.
    assert_eq!(inv.grid_set("D", 3, 3).unwrap_err().code(), 5);
    // Nor be removed while boxes sit in it.
    assert_eq!(inv.grid_clear("D").unwrap_err().code(), 5);
}

#[test]
fn boxes_trade_places_and_codes_in_one_step() {
    let (_d, mut inv) = setup();
    inv.grid_set("D", 3, 4).unwrap();
    inv.cells_set(
        &pairs(&[("D-A4", "A4-B4"), ("D-A3", "A3"), ("D-B3", "B3")]),
        false,
    )
    .unwrap();
    // The tall box goes back a row and the two small ones come forward: one at a time each
    // move would overlap, together they fit, and the codes follow the cells.
    let v = inv
        .cells_set(
            &pairs(&[("Uzun kutu", "A3-B3"), ("Sıcaklık", "A4"), ("Giriş", "B4")]),
            true,
        )
        .unwrap();
    assert_eq!(v["placed"].as_array().unwrap().len(), 3);
    assert_eq!(
        inv.show("Uzun kutu", false).unwrap()["node"]["code"],
        "D-A3"
    );
    assert_eq!(inv.show("Sıcaklık", false).unwrap()["node"]["code"], "D-A4");
    assert_eq!(inv.show("Giriş", false).unwrap()["node"]["code"], "D-B4");
    assert_eq!(inv.show("D-A3", false).unwrap()["cells"], "A3-B3");
    // New codes need their labels printed.
    assert_eq!(
        inv.show("D-B4", false).unwrap()["marks"]["label"]["value"],
        "needed"
    );
}

