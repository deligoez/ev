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

#[test]
fn moved_is_a_homes_and_a_home_leaves_only_moved_out_of_or_sold() {
    let (_d, mut inv) = setup();
    let id = |e: ev_core::Error| e.id().map(str::to_string);
    // A pan that stayed behind at a move is left behind, not moved.
    let pan = inv.gone("Tava", Some(Disposition::Moved)).unwrap_err();
    assert_eq!(id(pan).as_deref(), Some("moved_is_a_homes"));
    // Nothing is set aside to be moved out of.
    let aside = inv
        .dispose_qty("Yeni ev", Disposition::Moved, false, None, None)
        .unwrap_err();
    assert_eq!(id(aside).as_deref(), Some("nothing_set_aside"));
    // A home is not thrown out or given away.
    let thrown = inv.gone("Yeni ev", Some(Disposition::Trash)).unwrap_err();
    assert_eq!(id(thrown).as_deref(), Some("home_leaves_moved_or_sold"));
    // An owned one is sold, empty.
    let sold = inv.gone("Yeni ev", Some(Disposition::Sell)).unwrap();
    assert_eq!(sold["node"]["disposition"], "sell");
}

fn past_thing(
    name: &str,
    gone: &str,
    kind: &str,
    at: Option<&str>,
    place: Option<&str>,
) -> NewNode {
    NewNode {
        name: name.into(),
        kind: kind.into(),
        gone: Some(gone.into()),
        at: at.map(Into::into),
        place: place.map(Into::into),
        ..Default::default()
    }
}

#[test]
fn a_former_home_is_added_already_left_and_holds_what_was_left_there() {
    let (_d, mut inv) = setup();
    let flat = inv
        .add(NewNode {
            came: Some("2012-11".into()),
            address: Some("Örnek Sok. 1".into()),
            ..past_thing("Kiralık daire", "moved", "home", Some("2015-04"), None)
        })
        .unwrap();
    assert_eq!(flat["node"]["state"], "gone");
    assert_eq!(flat["node"]["address"], "Örnek Sok. 1");
    let flat_id = flat["node"]["id"].as_i64().unwrap();
    // Left there: by the home's name, or by its id.
    inv.add(past_thing(
        "Koltuk",
        "left",
        "item",
        Some("2015-04"),
        Some("Kiralık daire"),
    ))
    .unwrap();
    inv.add(past_thing(
        "Masa",
        "left",
        "item",
        None,
        Some(&format!("#{flat_id}")),
    ))
    .unwrap();
    let shown = inv.show("Kiralık daire", true).unwrap();
    let inside: Vec<&str> = shown["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(inside, ["Koltuk", "Masa"]);
    let past = inv.past(None, Some("Kiralık daire")).unwrap();
    let names: Vec<&str> = past["remembered"]["past"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert_eq!(past["remembered"]["past"][0]["where"], "Kiralık daire");
    // A home is not thrown out, and a room is never added on its own.
    let id = |e: ev_core::Error| e.id().map(str::to_string);
    let trash = inv
        .add(past_thing("X", "trash", "home", None, None))
        .unwrap_err();
    assert_eq!(id(trash).as_deref(), Some("home_leaves_moved_or_sold"));
    let room = inv
        .add(past_thing("Oda", "moved", "room", None, None))
        .unwrap_err();
    assert_eq!(id(room).as_deref(), Some("past_in_a_place"));
}
