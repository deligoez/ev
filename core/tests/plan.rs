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

#[test]
fn tasks_keep_an_order_and_one_is_in_progress() {
    let (_d, mut inv) = setup();
    let a = inv
        .task_add("Tour Alt", "bags", &["K1-01-A".into()], None)
        .unwrap();
    let b = inv
        .task_add("Open box", "never opened", &["Karton kutu".into()], Some(1))
        .unwrap();
    let (a, b) = (a["id"].as_i64().unwrap(), b["id"].as_i64().unwrap());
    let order = |inv: &Inventory| -> Vec<i64> {
        inv.task_list(false).unwrap()["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_i64().unwrap())
            .collect()
    };
    assert_eq!(order(&inv), [b, a]);
    inv.task_edit(a, None, None, &[], &[], Some(1)).unwrap();
    assert_eq!(order(&inv), [a, b]);

    inv.task_set(b, "doing", None).unwrap();
    inv.task_set(a, "doing", None).unwrap();
    assert_eq!(inv.task_show(b).unwrap()["status"], "open");

    let done = inv.task_set(a, "done", Some("person said so")).unwrap();
    assert!(done["position"].is_null() && done["closed_at"].is_string());
    assert_eq!(order(&inv), [b]);
    assert_eq!(
        inv.task_list(true).unwrap()["tasks"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(inv.task_add("  ", "why", &[], None).unwrap_err().code(), 2);
    assert!(matches!(inv.task_show(99).unwrap_err(), Error::NotFound(_)));
}

#[test]
fn next_brings_the_task_its_places_what_arrives_and_the_gaps() {
    let (_d, mut inv) = setup();
    add(&mut inv, "Alyan", "item", Some("Karton kutu"), None);
    inv.move_to("Alyan", "K1-01-U", true).unwrap();
    inv.task_add(
        "Tour Üst",
        "the hex keys go there",
        &["K1-01-U".into()],
        None,
    )
    .unwrap();
    let v = inv.next().unwrap();
    assert_eq!(v["task"]["title"], "Tour Üst");
    let place = &v["task"]["places"][0];
    assert_eq!(place["node"]["code"], "K1-01-U");
    assert_eq!(place["arriving"][0]["name"], "Alyan");
    let gaps: Vec<&str> = v["unplanned"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(gaps, ["Alt", "Karton kutu"]);

    // A household that only wants records gets no tidy-up gaps.
    inv.goal(Some("track")).unwrap();
    let v = inv.next().unwrap();
    assert_eq!(v["goal"], "track");
    assert!(v["unplanned"].as_array().unwrap().is_empty());
    assert_eq!(inv.goal(Some("tidy")).unwrap_err().code(), 2);
}
