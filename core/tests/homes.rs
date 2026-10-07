//! Leaving a home (spec/vehicles-homes.md): when it is empty, its rooms going with it.

use ev_core::{Disposition, Inventory, NewNode};
use tempfile::TempDir;

fn add(inv: &mut Inventory, name: &str, kind: &str, parent: Option<&str>) -> i64 {
    inv.add(NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: parent.map(Into::into),
        ..Default::default()
    })
    .unwrap()["node"]["id"]
        .as_i64()
        .unwrap()
}

/// The old flat with a shelf and a pan on it, and the new one.
fn setup() -> (TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    add(&mut inv, "Eski ev", "home", None);
    add(&mut inv, "Mutfak", "room", Some("Eski ev"));
    add(&mut inv, "Raf", "container", Some("Mutfak"));
    add(&mut inv, "Tava", "item", Some("Raf"));
    add(&mut inv, "Yeni ev", "home", None);
    (dir, inv)
}

#[test]
fn a_home_is_left_only_once_empty_and_its_rooms_go_with_it() {
    let (_d, mut inv) = setup();
    // The shelf is still there: refused, with what is left by room and the ways on.
    let e = inv.gone("Eski ev", Some(Disposition::Moved)).unwrap_err();
    let said = e.said_parts().unwrap();
    assert_eq!(said.id, "leaving_still_holds");
    let left = &said.details["remaining"][0];
    assert_eq!(left["in"]["name"], "Mutfak");
    // The shelf only: the pan on it goes with it.
    assert_eq!(left["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(left["nodes"][0]["name"], "Raf");
    assert!(left["move"].as_str().unwrap().starts_with("ev move --to"));
    // Moved to the new home, the old one is left, and its kitchen with it.
    inv.move_to("Raf", "Yeni ev", false).unwrap();
    let v = inv.gone("Eski ev", Some(Disposition::Moved)).unwrap();
    assert_eq!(v["node"]["state"], "gone");
    assert_eq!(v["node"]["disposition"], "moved");
    let kitchen = inv.show("Mutfak", true).unwrap();
    assert_eq!(kitchen["node"]["state"], "gone");
    assert_eq!(kitchen["node"]["disposition"], "moved");
}
