//! Splitting one record into a record per kind of thing (`ev split`).

use ev_core::{Inventory, NewNode};
use serde_json::Value;

fn add(inv: &mut Inventory, new: NewNode) {
    inv.add(new).unwrap();
}

/// A box holding one record for three soil-moisture sets, and a photo of the box.
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
    add(&mut inv, node("Ev", "home", None, None));
    add(&mut inv, node("Oda", "room", Some("Ev"), None));
    add(&mut inv, node("Kutu", "container", Some("Oda"), Some("B7")));
    add(
        &mut inv,
        NewNode {
            qty: Some(3),
            tags: vec!["modül".into()],
            ..node("Toprak nemi seti", "item", Some("B7"), None)
        },
    );
    (dir, inv)
}

fn events(inv: &Inventory, r: &str) -> Vec<Value> {
    inv.history(r).unwrap()["events"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn a_set_becomes_a_record_per_part_linked_both_ways() {
    let (_d, mut inv) = setup();
    let v = inv
        .split(
            "Toprak nemi seti",
            &[("LM393 kart".into(), Some(3)), ("Kablo".into(), Some(3))],
            Some("HW-080 prob"),
            None,
        )
        .unwrap();
    assert_eq!(v["node"]["name"], "HW-080 prob");
    assert_eq!(v["node"]["qty"], 3);
    let into = v["into"].as_array().unwrap();
    assert_eq!(into.len(), 2);
    // Each part lies where the set lay, with its count and the set's tags.
    for (name, part) in [("LM393 kart", &into[0]), ("Kablo", &into[1])] {
        assert_eq!(part["name"], name);
        assert_eq!(part["qty"], 3);
        let shown = inv.show(&part["id"].to_string(), false).unwrap();
        assert_eq!(
            shown["node"]["path_text"],
            format!("Ev › Oda › B7 › {name}")
        );
        assert_eq!(shown["node"]["tags"], serde_json::json!(["modül"]));
        let from = events(&inv, &part["id"].to_string());
        assert!(
            from.iter()
                .any(|e| e["type"] == "split_from" && e["data"]["name"] == "Toprak nemi seti"),
            "{from:?}"
        );
    }
    // The original's history names what came off it and its rename.
    let own = events(&inv, "HW-080 prob");
    let split = own.iter().find(|e| e["type"] == "split").unwrap();
    assert_eq!(split["data"]["into"].as_array().unwrap().len(), 2);
    assert!(
        own.iter()
            .any(|e| e["type"] == "edit" && e["data"]["name"]["before"] == "Toprak nemi seti")
    );
}

#[test]
fn splitting_leaves_the_box_photo_current() {
    let (d, mut inv) = setup();
    let img = d.path().join("box.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([9, 9, 9]))
        .save(&img)
        .unwrap();
    inv.photo_add("B7", &img, None, None).unwrap();
    let on_list = |inv: &Inventory| {
        inv.todo().unwrap()["photos"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["code"] == "B7")
    };
    assert!(!on_list(&inv));
    // The same things lie in the box, only recorded apart: its photo still shows it. (Times
    // are to the second, so each step waits past the one before.)
    std::thread::sleep(std::time::Duration::from_millis(1100));
    inv.split("Toprak nemi seti", &[("Kablo".into(), Some(3))], None, None)
        .unwrap();
    assert!(!on_list(&inv));
    // A thing really added is a change.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    inv.add(NewNode {
        name: "Yeni".into(),
        kind: "item".into(),
        parent: Some("B7".into()),
        ..Default::default()
    })
    .unwrap();
    assert!(on_list(&inv));
}

#[test]
fn a_holder_with_things_inside_is_not_split_and_nothing_is_half_done() {
    let (_d, mut inv) = setup();
    let e = inv
        .split("B7", &[("Başka kutu".into(), None)], None, None)
        .unwrap_err();
    assert_eq!(e.code(), 5);
    // An empty part name refuses the whole split: no part is left behind.
    let e = inv
        .split(
            "Toprak nemi seti",
            &[("Kablo".into(), Some(3)), (" ".into(), Some(1))],
            Some("Prob"),
            None,
        )
        .unwrap_err();
    assert_eq!(e.code(), 2);
    assert!(
        inv.find("Kablo", None, None, false).unwrap()["results"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        inv.show("Toprak nemi seti", false).unwrap()["node"]["name"],
        "Toprak nemi seti"
    );
    assert!(inv.split("Toprak nemi seti", &[], None, None).is_err());
}

#[test]
fn with_take_the_parts_are_some_of_the_units_and_come_off_the_count() {
    let (_d, mut inv) = setup();
    // Of the 3 sets, 1 is another make: the original keeps 2.
    let v = inv
        .split_with(
            "Toprak nemi seti",
            &[("Toprak nemi seti, beyaz".into(), Some(1))],
            None,
            None,
            true,
        )
        .unwrap();
    assert_eq!(v["node"]["qty"], 2);
    assert_eq!(v["into"][0]["qty"], 1);
    // Taking all of them leaves nothing on the original: refused, nothing changed.
    let e = inv
        .split_with(
            "Toprak nemi seti",
            &[("Başka".into(), Some(2))],
            None,
            None,
            true,
        )
        .unwrap_err();
    assert_eq!(e.code(), 5);
    assert_eq!(
        inv.show("Toprak nemi seti", false).unwrap()["node"]["qty"],
        2
    );
}

#[test]
fn take_splits_an_empty_case_off_two_recorded_as_one_and_the_cells_stay() {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    let node = |name: &str, kind: &str, parent: Option<&str>, qty: Option<i64>| NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: parent.map(Into::into),
        qty,
        ..Default::default()
    };
    for n in [
        node("Ev", "home", None, None),
        node("Çekmece", "container", Some("Ev"), None),
        node("Pil kutusu", "container", Some("Çekmece"), Some(2)),
        node("AA pil", "item", Some("Pil kutusu"), Some(4)),
    ] {
        inv.add(n).unwrap();
    }
    // Without --take, the parts would be what each case is made of: refused while it holds cells.
    assert!(
        inv.split_with(
            "Pil kutusu",
            &[("Pil kutusu".into(), Some(1))],
            None,
            None,
            false
        )
        .is_err()
    );
    let v = inv
        .split_with(
            "Pil kutusu",
            &[("Pil kutusu".into(), Some(1))],
            None,
            None,
            true,
        )
        .unwrap();
    assert_eq!(v["node"]["qty"], 1);
    // The original case is #3 in this house.
    let original = inv.show("3", false).unwrap();
    assert_eq!(
        original["children"].as_array().unwrap().len(),
        1,
        "the cells stay"
    );
    let off = v["into"][0]["id"].clone();
    let off = inv.show(&off.to_string(), false).unwrap();
    assert_eq!(off["node"]["qty"], 1);
    assert!(
        off["children"].as_array().unwrap().is_empty(),
        "taken empty"
    );
}

#[test]
fn a_part_goes_to_its_place_or_leaves_in_the_same_step() {
    let (_d, mut inv) = setup();
    inv.add(NewNode {
        name: "Çekmece".into(),
        kind: "container".into(),
        parent: Some("Oda".into()),
        code: Some("S5-01".into()),
        ..Default::default()
    })
    .unwrap();
    let parts = [
        ev_core::SplitPart {
            name: "LM393 kart".into(),
            qty: Some(3),
            to: Some("S5-01".into()),
            ..Default::default()
        },
        ev_core::SplitPart {
            name: "Kablo".into(),
            qty: Some(3),
            gone: Some(ev_core::Disposition::Trash),
            why: Some("uçları kopuk".into()),
            ..Default::default()
        },
    ];
    let v = inv
        .split_parts("Toprak nemi seti", &parts, Some("HW-080 prob"), None, false)
        .unwrap();
    let into = v["into"].as_array().unwrap();
    assert!(
        into[0]["path_text"].as_str().unwrap().contains("S5-01"),
        "{into:?}"
    );
    assert_eq!(into[1]["state"], "gone");
    assert_eq!(into[1]["disposition"], "trash");
    // The history keeps where it came from, then the move or the leaving.
    let kinds = |r: &str| -> Vec<String> {
        events(&inv, r)
            .iter()
            .map(|e| e["type"].as_str().unwrap().to_string())
            .collect()
    };
    let card = format!("#{}", into[0]["id"]);
    assert!(kinds(&card).contains(&"split_from".to_string()));
    assert!(kinds(&card).contains(&"move".to_string()));
    let shown = inv.show(&format!("#{}", into[1]["id"]), true).unwrap();
    assert!(
        shown["node"]["note"]
            .as_str()
            .unwrap()
            .contains("uçları kopuk")
    );
}

#[test]
fn a_part_whose_place_is_unknown_undoes_the_whole_split() {
    let (_d, mut inv) = setup();
    let parts = [
        ev_core::SplitPart {
            name: "LM393 kart".into(),
            ..Default::default()
        },
        ev_core::SplitPart {
            name: "Kablo".into(),
            to: Some("YOK-99".into()),
            ..Default::default()
        },
    ];
    assert!(
        inv.split_parts("Toprak nemi seti", &parts, None, None, false)
            .is_err()
    );
    // Nothing was made: the first part, already added in the step, went back with it.
    assert!(inv.show("LM393 kart", false).is_err());
    // A part told both to go and to leave is refused before anything is made.
    let both = [ev_core::SplitPart {
        name: "Kablo".into(),
        to: Some("B7".into()),
        gone: Some(ev_core::Disposition::Trash),
        ..Default::default()
    }];
    let e = inv
        .split_parts("Toprak nemi seti", &both, None, None, false)
        .unwrap_err();
    assert_eq!(e.id(), Some("split_part_moves_and_leaves"));
}
