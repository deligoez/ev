//! Spec §30: a parking place — things put in it wait for their final place.

use ev_core::{Inventory, NewNode};
use serde_json::Value;

fn add(inv: &mut Inventory, name: &str, kind: &str, parent: &str, theme: Option<&str>) -> i64 {
    let v = inv
        .add(NewNode {
            name: name.into(),
            kind: kind.into(),
            parent: Some(parent.into()),
            theme: theme.map(Into::into),
            ..Default::default()
        })
        .unwrap();
    v["node"]["id"].as_i64().unwrap()
}

/// A drawer themed for keys that is only where keys wait (a box of them inside it), and a proper
/// key cabinet elsewhere.
fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    inv.add(NewNode {
        name: "Ev".into(),
        kind: "home".into(),
        ..Default::default()
    })
    .unwrap();
    add(&mut inv, "Oda", "room", "Ev", None);
    add(
        &mut inv,
        "Çekmece",
        "container",
        "Oda",
        Some("Anahtarlar ve kumandalar"),
    );
    add(&mut inv, "Kutu", "container", "Çekmece", Some("Anahtarlar"));
    add(&mut inv, "Kale anahtar", "item", "Çekmece", None);
    add(&mut inv, "Kapı anahtarı", "item", "Kutu", None);
    add(
        &mut inv,
        "Dolap",
        "container",
        "Oda",
        Some("Anahtar dolabı, anahtarlar"),
    );
    add(&mut inv, "Yedek anahtar", "item", "Dolap", None);
    (dir, inv)
}

fn names(list: &Value, key: &str) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|x| x[key]["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn a_parking_place_and_what_is_inside_it_are_not_offered_as_a_final_place() {
    let (_d, mut inv) = setup();
    let before = inv.suggest("anahtar", None).unwrap();
    assert!(names(&before["similar"], "container").contains(&"Çekmece".to_string()));
    inv.edit("Çekmece", &["temporary=true".into()]).unwrap();
    let v = inv.suggest("anahtar", None).unwrap();
    let similar = names(&v["similar"], "container");
    // The drawer and the box inside it are where keys wait, not where they belong.
    assert!(!similar.contains(&"Çekmece".to_string()), "{similar:?}");
    assert!(!similar.contains(&"Kutu".to_string()), "{similar:?}");
    assert_eq!(similar.first().map(String::as_str), Some("Dolap"));
    let parking = names(&v["parking"], "container");
    assert!(parking.contains(&"Çekmece".to_string()) && parking.contains(&"Kutu".to_string()));
    // Each offered place says whether it has been gone through, and a reviewed room covers
    // the places in it.
    assert_eq!(v["similar"][0]["container"]["review"], Value::Null);
    let oda = inv.show("Oda", false).unwrap()["node"]["id"].clone();
    inv.review("Oda", "kept", None).unwrap();
    let v = inv.suggest("anahtar", None).unwrap();
    assert_eq!(v["similar"][0]["container"]["review"]["status"], "kept");
    assert_eq!(v["similar"][0]["container"]["review"]["from"], oda);
    inv.review("Dolap", "kept", None).unwrap();
    let v = inv.suggest("anahtar", None).unwrap();
    assert_eq!(v["similar"][0]["container"]["review"]["status"], "kept");
}

#[test]
fn what_waits_in_a_parking_place_is_on_the_todo_list_until_it_moves_or_the_place_settles() {
    let (_d, mut inv) = setup();
    inv.edit("Çekmece", &["temporary=true".into()]).unwrap();
    let v = inv.todo().unwrap();
    let parked: Vec<(String, String)> = v["parked"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["name"].as_str().unwrap().to_string(),
                p["in"]["name"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    // What was put straight into it waits; what is inside a box in it moves with the box.
    assert!(
        parked.contains(&("Kale anahtar".into(), "Çekmece".into())),
        "{parked:?}"
    );
    assert!(
        parked.contains(&("Kutu".into(), "Çekmece".into())),
        "{parked:?}"
    );
    assert_eq!(v["counts"]["parked"], 2);
    assert_eq!(
        inv.show("Çekmece", false).unwrap()["node"]["temporary"],
        true
    );
    // A thing given its final place leaves the list.
    inv.move_to("Kale anahtar", "Dolap", false).unwrap();
    assert_eq!(inv.todo().unwrap()["counts"]["parked"], 1);
    // The place itself can become final: then nothing waits there.
    inv.edit("Çekmece", &["temporary=false".into()]).unwrap();
    assert_eq!(inv.todo().unwrap()["counts"]["parked"], 0);
    assert!(inv.edit("Çekmece", &["temporary=maybe".into()]).is_err());
}

#[test]
fn one_item_can_wait_among_things_that_do_belong_where_it_is() {
    let (_d, mut inv) = setup();
    // The cabinet is the keys' final place; one spare key only waits there.
    inv.edit("Yedek anahtar", &["temporary=true".into()])
        .unwrap();
    let v = inv.todo().unwrap();
    let parked: Vec<&str> = v["parked"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(parked, ["Yedek anahtar"]);
    assert_eq!(v["parked"][0]["in"]["name"], "Dolap");
    // The cabinet itself is still offered: only the one key is parked.
    let s = inv.suggest("anahtar", None).unwrap();
    assert_eq!(s["similar"][0]["container"]["name"], "Dolap");
    // Moved to its place, the key is no longer parked: the move takes the mark with it.
    inv.move_to("Yedek anahtar", "Kutu", false).unwrap();
    assert_eq!(
        inv.show("Yedek anahtar", false).unwrap()["node"]["temporary"],
        false
    );
    let h = inv.history("Yedek anahtar").unwrap()["events"].clone();
    assert_eq!(
        h.as_array().unwrap().last().unwrap()["data"]["was_temporary"],
        true
    );
}
