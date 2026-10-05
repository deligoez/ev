//! A photo can be out of date in ways the records never saw, and an emptied place's photo still
//! shows what left.

use ev_core::{Inventory, NewNode};
use image::{Rgb, RgbImage};

fn setup() -> (tempfile::TempDir, Inventory, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent) in [
        ("Ev", "home", None),
        ("Oda", "room", Some("Ev")),
        ("Çekmece", "container", Some("Oda")),
    ] {
        inv.add(NewNode {
            name: name.into(),
            kind: kind.into(),
            parent: parent.map(Into::into),
            ..Default::default()
        })
        .unwrap();
    }
    let photo = dir.path().join("drawer.png");
    RgbImage::from_pixel(40, 30, Rgb([90, 90, 90]))
        .save(&photo)
        .unwrap();
    (dir, inv, photo)
}

fn needing(inv: &Inventory) -> Vec<(String, String)> {
    inv.todo().unwrap()["photos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["name"].as_str().unwrap().to_string(),
                p["photo_reason"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn a_photo_marked_out_of_date_is_listed_until_a_newer_one() {
    let (dir, mut inv, photo) = setup();
    inv.photo_add("Çekmece", &photo, None, None).unwrap();
    assert!(needing(&inv).is_empty());
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let v = inv
        .photo_stale_mark("Çekmece", Some("emptied before it was recorded"))
        .unwrap();
    assert_eq!(
        v["marks"]["photo_stale"]["note"],
        "emptied before it was recorded"
    );
    assert_eq!(needing(&inv), [("Çekmece".into(), "marked".into())]);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let fresh = dir.path().join("fresh.png");
    RgbImage::from_pixel(40, 30, Rgb([10, 10, 10]))
        .save(&fresh)
        .unwrap();
    inv.photo_add("Çekmece", &fresh, None, None).unwrap();
    assert!(needing(&inv).is_empty());
}

#[test]
fn an_emptied_place_with_an_older_photo_is_listed_and_one_never_photographed_is_not() {
    let (_dir, mut inv, photo) = setup();
    for (name, kind, parent) in [("Pil", "item", "Çekmece"), ("Raf", "container", "Oda")] {
        inv.add(NewNode {
            name: name.into(),
            kind: kind.into(),
            parent: Some(parent.into()),
            ..Default::default()
        })
        .unwrap();
    }
    inv.photo_add("Çekmece", &photo, None, None).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    inv.move_to("Pil", "Raf", false).unwrap();
    let list = needing(&inv);
    assert!(
        list.contains(&("Çekmece".into(), "changed".into())),
        "{list:?}"
    );
    // Raf now holds the battery and has no photo, so it is listed for that; an empty place
    // never photographed is not.
    inv.move_to("Pil", "Oda", false).unwrap();
    assert!(!needing(&inv).iter().any(|(n, _)| n == "Raf"));
}

#[test]
fn an_emptied_place_is_toured_only_with_a_photo_of_it_empty() {
    let (dir, mut inv, photo) = setup();
    for (name, kind, parent) in [("Pil", "item", "Çekmece"), ("Raf", "container", "Oda")] {
        inv.add(NewNode {
            name: name.into(),
            kind: kind.into(),
            parent: Some(parent.into()),
            ..Default::default()
        })
        .unwrap();
    }
    inv.photo_add("Çekmece", &photo, None, None).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    inv.move_to("Pil", "Raf", false).unwrap();
    // The photo still shows the battery: the empty drawer is photographed first.
    assert!(inv.review("Çekmece", "toured", None).is_err());
    let empty = dir.path().join("empty.png");
    RgbImage::from_pixel(40, 30, Rgb([200, 200, 200]))
        .save(&empty)
        .unwrap();
    inv.photo_add("Çekmece", &empty, None, None).unwrap();
    inv.review("Çekmece", "toured", None).unwrap();
}

#[test]
fn a_place_with_no_photo_is_refused_by_name_without_offering_photo_current() {
    let (_d, mut inv, _photo) = setup();
    inv.add(NewNode {
        name: "Vida".into(),
        kind: "item".into(),
        parent: Some("Çekmece".into()),
        ..Default::default()
    })
    .unwrap();
    let e = inv.review("Çekmece", "toured", None).unwrap_err();
    assert_eq!(e.code(), 5);
    let msg = e.to_string();
    assert!(msg.contains("Çekmece has no photo"), "{msg}");
    assert!(msg.contains("ev photo add"), "{msg}");
    assert!(
        !msg.contains("photo current") && !msg.contains("--grid"),
        "{msg}"
    );
}
