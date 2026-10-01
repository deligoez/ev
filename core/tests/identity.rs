//! Make, model and serial: what a thing is beyond its name (purchases spec §3.1).

use ev_core::{Inventory, NewNode};

fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent) in [
        ("Ev", "home", None),
        ("Oda", "room", Some("Ev")),
        ("Matkap", "item", Some("Oda")),
        ("Vidalama", "item", Some("Oda")),
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

fn names(inv: &Inventory, q: &str) -> Vec<String> {
    inv.find(q, None, None, false).unwrap()["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn make_model_and_serial_are_edited_shown_and_kept_in_the_history() {
    let (_d, mut inv) = setup();
    inv.edit(
        "Matkap",
        &[
            "make=Bosch".into(),
            "model=GSB 13 RE".into(),
            "serial=  583012345  ".into(),
        ],
    )
    .unwrap();
    let n = &inv.show("Matkap", false).unwrap()["node"];
    assert_eq!(
        (&n["make"], &n["model"], &n["serial"]),
        (&"Bosch".into(), &"GSB 13 RE".into(), &"583012345".into())
    );
    let events = inv.history("Matkap").unwrap()["events"].clone();
    let edit = events
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["type"] == "edit")
        .unwrap();
    assert_eq!(edit["data"]["model"]["after"], "GSB 13 RE");
    // An empty value clears the field.
    inv.edit("Matkap", &["serial=".into()]).unwrap();
    assert!(inv.show("Matkap", false).unwrap()["node"]["serial"].is_null());
}

#[test]
fn a_model_code_finds_a_thing_whose_name_does_not_carry_it() {
    let (_d, mut inv) = setup();
    inv.edit("Matkap", &["make=Bosch".into(), "model=GSB 13 RE".into()])
        .unwrap();
    assert_eq!(names(&inv, "gsb 13"), ["Matkap"]);
    assert_eq!(names(&inv, "bosch matkap"), ["Matkap"]);
}

#[test]
fn a_new_record_takes_make_model_and_serial() {
    let (_d, mut inv) = setup();
    inv.add(NewNode {
        name: "Darbeli matkap".into(),
        kind: "item".into(),
        parent: Some("Oda".into()),
        make: Some("Bosch".into()),
        model: Some("GSB 13 RE".into()),
        serial: Some(" ".into()),
        ..Default::default()
    })
    .unwrap();
    let n = &inv.show("Darbeli matkap", false).unwrap()["node"];
    assert_eq!(n["make"], "Bosch");
    assert_eq!(n["model"], "GSB 13 RE");
    // A blank value is no value.
    assert!(n["serial"].is_null());
}
