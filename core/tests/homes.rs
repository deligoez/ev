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

#[test]
fn a_place_that_was_a_home_of_ours_becomes_one_and_a_household_is_refused() {
    let (_d, mut inv) = setup();
    // Until now a former home was a place named where things were left.
    inv.add(past_thing("Lamba", "left", "item", None, Some("Eski yurt")))
        .unwrap();
    let home = inv
        .place_home("Eski yurt", Some("2010"), Some("2011-06"), None)
        .unwrap();
    assert_eq!(home["node"]["kind"], "home");
    assert_eq!(home["node"]["disposition"], "moved");
    assert_eq!(home["moved_in"], 1);
    assert_eq!(home["children"][0]["name"], "Lamba");
    assert!(
        inv.place_list().unwrap()["places"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // A place with errands is another household: refused.
    inv.add(NewNode {
        name: "Ödünç matkap".into(),
        kind: "item".into(),
        parent: Some("Yeni ev".into()),
        owner: Some("Annemler".into()),
        ..Default::default()
    })
    .unwrap();
    let e = inv.place_home("Annemler", None, None, None).unwrap_err();
    assert_eq!(e.id(), Some("place_is_a_household"));
}

#[test]
fn homes_and_vehicles_lead_the_past_and_leave_the_lists_of_things() {
    let (_d, mut inv) = setup();
    inv.add(NewNode {
        came: Some("2012-11".into()),
        address: Some("Örnek Sok. 1".into()),
        ..past_thing("Kiralık daire", "moved", "home", Some("2015-04"), None)
    })
    .unwrap();
    // A car sold, its glovebox with it.
    inv.add(NewNode {
        name: "Eski araba".into(),
        kind: "vehicle".into(),
        code: Some("06 XY 99".into()),
        came: Some("2018-03".into()),
        ..Default::default()
    })
    .unwrap();
    add(&mut inv, "Torpido", "container", Some("06 XY 99"));
    inv.gone("06 XY 99", Some(Disposition::Sell)).unwrap();
    inv.sold("06 XY 99", "900000", None, None, None, None)
        .unwrap();
    let past = inv.past(None, None).unwrap();
    let section: Vec<(&str, Option<&str>)> = past["homes_and_vehicles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| (h["name"].as_str().unwrap(), h["left"].as_str()))
        .collect();
    // Here now first, then the last left first.
    assert_eq!(section[0], ("Eski ev", None));
    assert_eq!(section[1], ("Yeni ev", None));
    assert_eq!(section[3], ("Kiralık daire", Some("2015-04")));
    let car = &past["homes_and_vehicles"][2];
    assert_eq!(car["code"], "06 XY 99");
    assert_eq!(car["how"], "sell");
    assert_eq!(car["got"]["price"], "900000.00");
    let flat = &past["homes_and_vehicles"][3];
    assert_eq!(flat["address"], "Örnek Sok. 1");
    // None of them, nor the glovebox that went with the car, is a past thing of its own.
    for list in ["remembered", "left_inventory"] {
        for p in past[list]["past"].as_array().unwrap() {
            assert!(
                !["Kiralık daire", "Eski araba", "Torpido"].contains(&p["name"].as_str().unwrap()),
                "{p}"
            );
        }
    }
    // ev stats' past still counts what left, the sale's money with it.
    let summary = &inv.stats().unwrap()["past"];
    assert_eq!(summary["records"], 2, "{summary}");
    assert_eq!(summary["got"]["TRY"], "900000.00", "{summary}");
}

#[test]
fn a_year_names_the_home_we_lived_in_first_and_skips_a_home_nothing_dates() {
    let (_d, mut inv) = setup();
    inv.add(NewNode {
        came: Some("2012-11".into()),
        ..past_thing("Kiralık daire", "moved", "home", Some("2015-04"), None)
    })
    .unwrap();
    inv.add(NewNode {
        came: Some("2013".into()),
        ..past_thing("Bisiklet", "sell", "item", Some("2016"), None)
    })
    .unwrap();
    let y = inv.past_year(2014).unwrap();
    let names: Vec<&str> = y["owned"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Kiralık daire", "Bisiklet"]);
    // The setup's two homes say no date: neither listed nor counted among the undated things,
    // which are the pan alone.
    assert_eq!(y["unknown"], 1, "{y}");
}

#[test]
fn a_task_may_be_about_a_former_home() {
    let (_d, mut inv) = setup();
    inv.add(past_thing("Kiralık daire", "moved", "home", None, None))
        .unwrap();
    // Its dates are still to find: a task about it.
    let t = inv
        .task_add(
            "Taşınma tarihini bul",
            "adres geçmişi",
            &["Kiralık daire".to_string()],
            None,
        )
        .unwrap();
    assert_eq!(t["nodes"][0]["name"], "Kiralık daire");
    assert_eq!(t["nodes"][0]["state"], "gone");
}

#[test]
fn a_task_is_moved_onto_a_former_home() {
    let (_d, mut inv) = setup();
    inv.add(past_thing("Kiralık daire", "moved", "home", None, None))
        .unwrap();
    let t = inv
        .task_add("Kontratı bul", "adres geçmişi", &["Raf".to_string()], None)
        .unwrap();
    let id = t["id"].as_i64().unwrap();
    // From the shelf it was parked on to the former home it is about.
    let t = inv
        .task_edit(
            id,
            None,
            None,
            &["Kiralık daire".to_string()],
            &["Raf".to_string()],
            None,
        )
        .unwrap();
    let names: Vec<&str> = t["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Kiralık daire"]);
}
