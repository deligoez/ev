//! When a tour closes, the place's final photo is checked against its records: which are shown on
//! it, and which are not yet.

use ev_core::{Inventory, NewNode};

#[test]
fn a_tour_closed_says_which_records_the_final_photo_does_not_show_yet() {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent, code) in [
        ("Ev", "home", None, None),
        ("Oda", "room", Some("Ev"), None),
        ("Çekmece", "container", Some("Oda"), Some("C1")),
        ("Pense", "item", Some("C1"), None),
        ("Tornavida", "item", Some("C1"), None),
    ] {
        inv.add(NewNode {
            name: name.into(),
            kind: kind.into(),
            parent: parent.map(Into::into),
            code: code.map(Into::into),
            ..Default::default()
        })
        .unwrap();
    }
    let photo = dir.path().join("cekmece.png");
    image::RgbImage::from_pixel(40, 30, image::Rgb([9, 9, 9]))
        .save(&photo)
        .unwrap();
    inv.photo_add("C1", &photo, None, None).unwrap();
    // The pliers are framed on it; the screwdriver is not.
    inv.photo_add("Pense", &photo, Some("0,0,0.5,0.5".parse().unwrap()), None)
        .unwrap();
    let v = inv.review("C1", "toured", None).unwrap();
    let names = |key: &str| -> Vec<String> {
        v["photo_check"][key]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(names("located"), ["Pense"], "{v}");
    assert_eq!(names("not_located"), ["Tornavida"], "{v}");
}
