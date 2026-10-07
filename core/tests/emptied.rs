//! A place that was just emptied (spec/emptied-place.md).

use ev_core::{Inventory, NewNode};

/// A box with a theme and an observation, holding one cable, and an empty drawer beside it.
fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    let node = |name: &str, kind: &str, parent: Option<&str>, code: Option<&str>| NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: parent.map(Into::into),
        code: code.map(Into::into),
        ..Default::default()
    };
    for n in [
        node("Ev", "home", None, None),
        node("Oda", "room", Some("Ev"), None),
        NewNode {
            theme: Some("kablolar".into()),
            ..node("Kutu", "container", Some("Oda"), Some("K1"))
        },
        node("Çekmece", "container", Some("Oda"), Some("C1")),
        node("HDMI kablo", "item", Some("K1"), None),
    ] {
        inv.add(n).unwrap();
    }
    inv.observe("K1", "HDMI kabloları ayrılacak", None).unwrap();
    (dir, inv)
}

#[test]
fn moving_the_last_thing_out_says_what_of_the_place_went_stale() {
    let (_d, mut inv) = setup();
    let v = inv.move_to("HDMI kablo", "C1", false).unwrap();
    let e = &v["emptied"][0];
    assert_eq!(e["node"]["code"], "K1", "{v}");
    assert_eq!(e["theme"], "kablolar");
    assert_eq!(e["observations"][0]["text"], "HDMI kabloları ayrılacak");
    // Nothing is cleared on its own.
    assert_eq!(inv.show("K1", false).unwrap()["node"]["theme"], "kablolar");
}
