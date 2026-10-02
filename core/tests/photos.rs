//! Spec §16: the photo store, crops and adoption.

use ev_core::{Inventory, NewNode};
use image::{Rgb, RgbImage};

fn setup() -> (tempfile::TempDir, Inventory, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    inv.add(NewNode {
        name: "Ev".into(),
        kind: "home".into(),
        ..Default::default()
    })
    .unwrap();
    inv.add(NewNode {
        name: "Çekmece".into(),
        kind: "room".into(),
        parent: Some("Ev".into()),
        ..Default::default()
    })
    .unwrap();
    // A 200×100 photo: left half red, right half blue.
    let mut img = RgbImage::new(200, 100);
    for (x, _, p) in img.enumerate_pixels_mut() {
        *p = if x < 100 {
            Rgb([255, 0, 0])
        } else {
            Rgb([0, 0, 255])
        };
    }
    let photo = dir.path().join("drawer.png");
    img.save(&photo).unwrap();
    (dir, inv, photo)
}

#[test]
fn photos_are_copied_into_the_store_and_survive_the_original() {
    let (dir, mut inv, photo) = setup();
    let v = inv
        .photo_add("Çekmece", &photo, None, Some("whole drawer"))
        .unwrap();
    let stored = v["photos"][0]["path"].as_str().unwrap().to_string();
    assert!(stored.starts_with(dir.path().join("photos").to_str().unwrap()));
    std::fs::remove_file(&photo).unwrap();
    let v = inv.photo_list("Çekmece").unwrap();
    assert_eq!(v["photos"][0]["exists"], true);
    assert_eq!(v["photos"][0]["n"], 1);
    assert_eq!(v["photos"][0]["note"], "whole drawer");
    // The same photo added twice is stored once.
    let again = dir.path().join("again.png");
    std::fs::copy(&stored, &again).unwrap();
    let v = inv.photo_add("Çekmece", &again, None, None).unwrap();
    assert_eq!(v["photos"][1]["path"], v["photos"][0]["path"]);
}

#[test]
fn a_crop_keeps_only_its_part_and_remembers_the_original() {
    let (_dir, mut inv, photo) = setup();
    let crop = "0.5,0,0.5,1".parse().unwrap();
    let v = inv.photo_add("Çekmece", &photo, Some(crop), None).unwrap();
    let p = &v["photos"][0];
    assert!(p["source"].as_str().is_some());
    assert_eq!(p["crop"], "0.5000,0.0000,0.5000,1.0000");
    let cut = image::open(p["path"].as_str().unwrap()).unwrap().to_rgb8();
    assert_eq!(cut.dimensions(), (100, 100));
    let px = cut.get_pixel(50, 50);
    assert!(
        px[2] > 200 && px[0] < 60,
        "crop should be the blue half, got {px:?}"
    );
}

#[test]
fn photos_are_removed_by_number_and_outside_paths_are_adopted() {
    let (dir, mut inv, photo) = setup();
    inv.edit("Çekmece", &[format!("photos=+{}", photo.display())])
        .unwrap();
    inv.edit("Çekmece", &["photos=+/nonexistent/lost.jpg".into()])
        .unwrap();
    let v = inv.photo_adopt().unwrap();
    assert_eq!(v["adopted"], 1);
    assert_eq!(v["missing"].as_array().unwrap().len(), 1);
    let v = inv.photo_list("Çekmece").unwrap();
    assert!(
        v["photos"][0]["path"]
            .as_str()
            .unwrap()
            .starts_with(dir.path().join("photos").to_str().unwrap())
    );
    let v = inv.photo_remove("Çekmece", 2).unwrap();
    assert_eq!(v["photos"].as_array().unwrap().len(), 1);
    assert!(inv.photo_remove("Çekmece", 5).is_err());
}

#[test]
fn a_removed_photo_stays_in_the_history() {
    let (_dir, mut inv, photo) = setup();
    let crop = "0.5,0,0.5,1".parse().unwrap();
    inv.photo_add("Çekmece", &photo, Some(crop), Some("mavi yarı"))
        .unwrap();
    inv.photo_remove("Çekmece", 1).unwrap();
    let events = inv.history("Çekmece").unwrap()["events"].clone();
    let last = events.as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["type"], "photo_remove");
    assert_eq!(last["data"]["note"], "mavi yarı");
    assert_eq!(last["data"]["crop"], "0.5000,0.0000,0.5000,1.0000");
    assert_eq!(last["data"]["n"], 1);
    assert!(last["data"]["path"].as_str().is_some());
}

#[test]
fn one_photo_gives_the_same_record_several_crops_in_one_cut() {
    let (_dir, mut inv, photo) = setup();
    let crop = |s: &str| s.parse::<ev_core::Crop>().unwrap();
    // Three probes of three sets in one photo are one record: each gets its own crop.
    let v = inv
        .photo_cut(
            &photo,
            None,
            &[
                ("Çekmece".into(), crop("0,0,0.5,1")),
                ("Çekmece".into(), crop("0.5,0,0.5,1")),
            ],
            Some("iki set"),
            None,
        )
        .unwrap();
    assert_eq!(v["attached"].as_array().unwrap().len(), 2);
    let photos = inv.photo_list("Çekmece").unwrap()["photos"].clone();
    assert_eq!(photos[0]["crop"], "0.0000,0.0000,0.5000,1.0000");
    assert_eq!(photos[1]["crop"], "0.5000,0.0000,0.5000,1.0000");
}

#[test]
fn a_cut_numbers_its_crops_on_the_whole_photo_in_the_order_given() {
    let (dir, mut inv, photo) = setup();
    for name in ["Pil", "Röle"] {
        inv.add(NewNode {
            name: name.into(),
            kind: "item".into(),
            parent: Some("Çekmece".into()),
            ..Default::default()
        })
        .unwrap();
    }
    let crop = |s: &str| s.parse::<ev_core::Crop>().unwrap();
    let crops = [
        ("Röle".to_string(), crop("0.5,0,0.5,1")),
        ("Pil".to_string(), crop("0,0,0.5,1")),
    ];
    // The preview numbers them the same way and attaches nothing.
    let p = inv
        .photo_cut_preview(&photo, None, &crops, None, None)
        .unwrap();
    assert_eq!(p["legend"][0]["ref"]["name"], "Röle");
    assert!(std::path::Path::new(p["marked"].as_str().unwrap()).is_file());
    let v = inv
        .photo_cut(&photo, Some("Çekmece"), &crops, None, None)
        .unwrap();
    let legend = v["legend"].as_array().unwrap();
    assert_eq!(legend.len(), 2);
    assert_eq!(legend[0]["n"], 1);
    assert_eq!(legend[0]["ref"]["name"], "Röle");
    assert_eq!(legend[0]["crop"], "0.5000,0.0000,0.5000,1.0000");
    assert_eq!(legend[1]["n"], 2);
    assert_eq!(legend[1]["ref"]["name"], "Pil");
    assert_eq!(legend[1]["ref"]["path_text"], "Ev › Çekmece › Pil");
    // The numbered copy is the whole photo, not a crop.
    let marked = image::open(v["marked"].as_str().unwrap()).unwrap();
    assert_eq!((marked.width(), marked.height()), (200, 100));
    // A cut with only the whole photo has nothing to number.
    let other = dir.path().join("other.png");
    RgbImage::from_pixel(50, 50, Rgb([0, 255, 0]))
        .save(&other)
        .unwrap();
    inv.add(NewNode {
        name: "Kutu".into(),
        kind: "container".into(),
        parent: Some("Çekmece".into()),
        ..Default::default()
    })
    .unwrap();
    let v = inv
        .photo_cut(&other, Some("Kutu"), &[], None, None)
        .unwrap();
    assert_eq!(v["legend"], serde_json::json!([]));
    assert!(v["marked"].is_null());
}
