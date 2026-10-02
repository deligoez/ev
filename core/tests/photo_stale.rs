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
