//! Spec §17: the tidy-up plan — units, reviews, observations, tasks, `next`, goal.

use ev_core::{Error, Inventory, NewNode};
use serde_json::Value;
use tempfile::TempDir;

fn add(
    inv: &mut Inventory,
    name: &str,
    kind: &str,
    parent: Option<&str>,
    code: Option<&str>,
) -> i64 {
    let v = inv
        .add(NewNode {
            name: name.into(),
            kind: kind.into(),
            parent: parent.map(Into::into),
            code: code.map(Into::into),
            ..Default::default()
        })
        .unwrap();
    v["node"]["id"].as_i64().unwrap()
}

/// A Kallax with two labelled drawers (one holding an unlabelled box) and a loose box in the
/// room: three places to go through.
fn setup() -> (TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    add(&mut inv, "Ev", "home", None, None);
    add(&mut inv, "Oda", "room", Some("Ev"), None);
    add(&mut inv, "Kallax", "furniture", Some("Oda"), Some("K1"));
    add(&mut inv, "Bölme", "container", Some("K1"), Some("K1-01"));
    add(&mut inv, "Üst", "container", Some("K1-01"), Some("K1-01-U"));
    add(&mut inv, "Alt", "container", Some("K1-01"), Some("K1-01-A"));
    add(&mut inv, "Vida kutusu", "container", Some("K1-01-A"), None);
    add(&mut inv, "Vida", "item", Some("Vida kutusu"), None);
    add(&mut inv, "Karton kutu", "container", Some("Oda"), None);
    (dir, inv)
}

fn status_of(progress: &Value, name: &str) -> String {
    progress["places"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("{name} is not a unit: {progress}"))["review"]["status"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn units_are_the_innermost_labelled_places_and_loose_holders() {
    let (_d, inv) = setup();
    let p = inv.progress().unwrap();
    let names: Vec<&str> = p["places"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Üst", "Alt", "Karton kutu"], "{p}");
    assert_eq!(p["raw"], 3);
}

