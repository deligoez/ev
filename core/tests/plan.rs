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

#[test]
fn a_review_covers_everything_below_and_notices_later_changes() {
    let (_d, mut inv) = setup();
    inv.review("K1-01", "toured", None).unwrap();
    let p = inv.progress().unwrap();
    assert_eq!(status_of(&p, "Alt"), "toured");
    assert_eq!(status_of(&p, "Üst"), "toured");
    assert_eq!(p["changed_since_tour"], 0);

    // A second later a new thing lands in the toured drawer.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    add(&mut inv, "Pul", "item", Some("K1-01-A"), None);
    let p = inv.progress().unwrap();
    assert_eq!(p["changed_since_tour"], 1, "{p}");

    inv.review("Karton kutu", "kept", Some("leave it")).unwrap();
    assert_eq!(status_of(&inv.progress().unwrap(), "Karton kutu"), "kept");
    inv.review("Karton kutu", "raw", None).unwrap();
    assert_eq!(status_of(&inv.progress().unwrap(), "Karton kutu"), "raw");
    assert_eq!(inv.review("K1-01-U", "done", None).unwrap_err().code(), 2);
}

#[test]
fn observations_show_on_the_place_and_can_be_removed() {
    let (_d, mut inv) = setup();
    let v = inv
        .observe("K1-01-A", "screws loose in a bag", None)
        .unwrap();
    let obs = v["observations"].as_array().unwrap();
    assert_eq!(obs.len(), 1);
    assert_eq!(obs[0]["text"], "screws loose in a bag");
    assert_eq!(
        inv.observe("K1-01-A", "x", Some(1)).unwrap_err().code(),
        3,
        "no photo 1"
    );
    let id = obs[0]["id"].as_i64().unwrap();
    let v = inv.unobserve(id).unwrap();
    assert!(v["observations"].as_array().unwrap().is_empty());
}

