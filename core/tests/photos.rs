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

