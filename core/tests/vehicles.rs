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
