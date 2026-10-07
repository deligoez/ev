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

#[test]
fn a_removed_crop_no_record_uses_is_deleted_and_a_whole_photo_is_kept() {
    let (_dir, mut inv, photo) = setup();
    inv.add(NewNode {
        name: "Kutu".into(),
        kind: "container".into(),
        parent: Some("Çekmece".into()),
        ..Default::default()
    })
    .unwrap();
    let crop = "0.5,0,0.5,1".parse().unwrap();
    inv.photo_add("Çekmece", &photo, None, None).unwrap();
    let v = inv.photo_add("Kutu", &photo, Some(crop), None).unwrap();
    let cut = v["photos"][0]["path"].as_str().unwrap().to_string();
    let whole = v["photos"][0]["source"].as_str().unwrap().to_string();
    // The same crop on a second record: removing it from one keeps the file.
    inv.photo_add("Çekmece", &photo, Some(crop), None).unwrap();
    let v = inv.photo_remove("Kutu", 1).unwrap();
    assert!(v.get("deleted_file").is_none(), "{v}");
    assert!(std::path::Path::new(&cut).exists());
    // The last record using it lets it go; the whole photo it was cut from stays.
    let v = inv.photo_remove("Çekmece", 2).unwrap();
    assert_eq!(v["deleted_file"], cut.as_str());
    assert!(!std::path::Path::new(&cut).exists());
    inv.photo_remove("Çekmece", 1).unwrap();
    assert!(std::path::Path::new(&whole).exists());
}

#[test]
fn turning_a_photo_turns_its_crops_and_cuts_them_again() {
    let (_dir, mut inv, photo) = setup();
    inv.add(NewNode {
        name: "Kutu".into(),
        kind: "container".into(),
        parent: Some("Çekmece".into()),
        ..Default::default()
    })
    .unwrap();
    // The red half on the box, the whole photo on the drawer.
    inv.photo_cut(
        &photo,
        Some("Çekmece"),
        &[("Kutu".into(), "0,0,0.5,1".parse().unwrap())],
        None,
        None,
    )
    .unwrap();
    let old = inv.photo_list("Kutu").unwrap()["photos"][0]["path"]
        .as_str()
        .unwrap()
        .to_string();
    // Turned from the crop: its source turns, and the drawer's whole photo with it.
    let v = inv.photo_rotate("Kutu", 1, 90).unwrap();
    assert_eq!(v["rotated"]["records"].as_array().unwrap().len(), 2, "{v}");
    let crop = &v["photos"][0];
    assert_eq!(crop["crop"], "0.0000,0.0000,1.0000,0.5000");
    let cut = image::open(crop["path"].as_str().unwrap())
        .unwrap()
        .to_rgb8();
    assert_eq!((cut.width(), cut.height()), (100, 100));
    assert!(
        cut.pixels().all(|p| p[0] > 200 && p[2] < 60),
        "still the red half"
    );
    assert!(!std::path::Path::new(&old).exists(), "the old crop is gone");
    let whole = inv.photo_list("Çekmece").unwrap()["photos"][0]["path"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(image::image_dimensions(&whole).unwrap(), (100, 200));
    let h = inv.history("Çekmece").unwrap();
    assert_eq!(
        h["events"].as_array().unwrap().last().unwrap()["type"],
        "photo_rotate"
    );
}

#[test]
fn a_photo_added_turned_is_stored_turned() {
    let (_dir, mut inv, photo) = setup();
    let turned = inv.turned_copy(&photo, 270).unwrap();
    // The crop is a fraction of the turned photo: its top half was the right, blue half.
    let v = inv
        .photo_add("Çekmece", &turned, Some("0,0,1,0.5".parse().unwrap()), None)
        .unwrap();
    let cut = image::open(v["photos"][0]["path"].as_str().unwrap())
        .unwrap()
        .to_rgb8();
    assert!(
        cut.pixels().all(|p| p[2] > 200 && p[0] < 60),
        "the blue half"
    );
    assert!(inv.turned_copy(&photo, 45).is_err());
}

/// Marks `marks` on `file` for the series on screen and sends the copy there, as `ev photo mark`
/// does; the labels as drawn.
fn show_marked(inv: &mut Inventory, file: &std::path::Path, marks: &[&str]) -> Vec<String> {
    let marks: Vec<(String, String)> = marks
        .iter()
        .map(|m| {
            let (l, a) = m.split_once('=').unwrap();
            (l.to_string(), a.to_string())
        })
        .collect();
    let v = inv
        .photo_mark_numbered(file.to_str().unwrap(), &marks, None, None, false)
        .unwrap();
    let frames = v["frames"].as_array().unwrap().clone();
    let marked = std::path::PathBuf::from(v["marked"].as_str().unwrap());
    inv.focus_marked(&[marked], None, &frames).unwrap();
    v["marks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["label"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn numbers_in_a_series_never_repeat_and_a_photo_marked_again_keeps_its_own() {
    let (dir, mut inv, photo) = setup();
    let shelf = dir.path().join("shelf.png");
    std::fs::copy(&photo, &shelf).unwrap();
    let two = ["1=0.1,0.1,0.2,0.2", "2=0.5,0.5,0.2,0.2"];
    assert_eq!(show_marked(&mut inv, &photo, &two), ["1", "2"]);
    assert_eq!(
        show_marked(
            &mut inv,
            &shelf,
            &["1=0.1,0.1,0.2,0.2", "2 → A6=0.5,0.5,0.2,0.2"]
        ),
        ["3", "4 → A6"]
    );
    // Marked again with one frame more: its own numbers stay, the new frame takes the next.
    let three = [
        "1=0.1,0.1,0.3,0.3",
        "2=0.5,0.5,0.2,0.2",
        "3=0.7,0.1,0.2,0.2",
    ];
    assert_eq!(show_marked(&mut inv, &photo, &three), ["1", "2", "5"]);
    let series = &inv.focus_list().unwrap()["series"];
    assert_eq!(series["pictures"].as_array().unwrap().len(), 2);
    assert_eq!(series["pictures"][0]["frames"][2]["n"], 5);
    assert_eq!(series["next"], 6);
    // Once the series ends, the next one starts at 1.
    inv.focus(None, None).unwrap();
    assert_eq!(show_marked(&mut inv, &shelf, &two[..1]), ["1"]);
}

#[test]
fn a_frame_left_out_of_a_second_mark_never_hands_its_number_to_another() {
    let (_dir, mut inv, photo) = setup();
    let two = ["1=0.1,0.1,0.2,0.2", "2=0.5,0.5,0.2,0.2"];
    assert_eq!(show_marked(&mut inv, &photo, &two), ["1", "2"]);
    // Frame 1 fixed, frame 2 left out, a new frame 3: 2 was told to the person and stays its.
    let again = ["1=0.1,0.1,0.3,0.3", "3=0.7,0.1,0.2,0.2"];
    assert_eq!(show_marked(&mut inv, &photo, &again), ["1", "3"]);
    // Left out, 2 is retired: a frame labelled 2 again is a new frame with a new number.
    let back = ["2=0.5,0.5,0.2,0.2", "3=0.7,0.1,0.2,0.2"];
    assert_eq!(show_marked(&mut inv, &photo, &back), ["4", "3"]);
    assert_eq!(inv.focus_list().unwrap()["series"]["next"], 5);
}

#[test]
fn a_cut_of_a_photo_marked_in_the_series_takes_the_numbers_it_was_marked_with() {
    let (dir, mut inv, photo) = setup();
    for name in ["Pil", "Röle", "Kablo"] {
        inv.add(NewNode {
            name: name.into(),
            kind: "item".into(),
            parent: Some("Çekmece".into()),
            ..Default::default()
        })
        .unwrap();
    }
    let shelf = dir.path().join("shelf.png");
    std::fs::copy(&photo, &shelf).unwrap();
    show_marked(
        &mut inv,
        &shelf,
        &["1=0.1,0.1,0.2,0.2", "2=0.5,0.5,0.2,0.2"],
    );
    show_marked(&mut inv, &photo, &["1=0,0,0.5,1", "2=0.5,0,0.5,1"]);
    let crop = |s: &str| s.parse::<ev_core::Crop>().unwrap();
    let crops = [
        ("Pil".to_string(), crop("0,0,0.5,1")),
        ("Röle".to_string(), crop("0.5,0,0.5,1")),
        ("Kablo".to_string(), crop("0.4,0.4,0.2,0.2")),
    ];
    let v = inv
        .photo_cut_in(&photo, None, &crops, None, None, true)
        .unwrap();
    let n: Vec<_> = v["legend"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["n"].clone())
        .collect();
    assert_eq!(n, [3, 4, 5]);
    // Outside a series a cut numbers from 1.
    let v = inv
        .photo_cut(&photo, None, &crops[..1], None, None)
        .unwrap();
    assert_eq!(v["legend"][0]["n"], 1);
}

#[test]
fn a_label_numbered_across_photos_takes_the_next_free_number_without_a_gap() {
    let (dir, mut inv, photo) = setup();
    let shelf = dir.path().join("shelf.png");
    std::fs::copy(&photo, &shelf).unwrap();
    assert_eq!(show_marked(&mut inv, &photo, &["1=0.1,0.1,0.2,0.2"]), ["1"]);
    // The agent counted on across photos and wrote `2`: it is this photo's first frame.
    assert_eq!(show_marked(&mut inv, &shelf, &["2=0.1,0.1,0.2,0.2"]), ["2"]);
    assert_eq!(inv.focus_list().unwrap()["series"]["next"], 3);
}

#[test]
fn a_frame_is_edged_in_dark_so_it_reads_on_a_red_photo() {
    // The setup photo is pure red and blue: nothing in it is dark.
    let (_dir, inv, photo) = setup();
    let v = inv
        .photo_mark(
            photo.to_str().unwrap(),
            &[("1".into(), "0.1,0.1,0.3,0.6".into())],
            None,
            None,
        )
        .unwrap();
    let marked = image::open(v["marked"].as_str().unwrap())
        .unwrap()
        .to_rgb8();
    let dark = marked
        .pixels()
        .filter(|p| p.0.iter().all(|c| *c < 60))
        .count();
    assert!(dark > 100, "{dark} dark pixels");
}

#[test]
fn a_sheet_puts_the_series_pictures_on_one_image_titled_by_number() {
    let (dir, mut inv, photo) = setup();
    let files: Vec<std::path::PathBuf> = (1..=3)
        .map(|i| {
            let f = dir.path().join(format!("p{i}.png"));
            std::fs::copy(&photo, &f).unwrap();
            f
        })
        .collect();
    inv.focus_marked(&files, Some("Çekmece"), &[]).unwrap();
    // The whole series by default, four to a row: three pictures make one row.
    let out = dir.path().join("sheet.jpg");
    let v = inv.photo_sheet(&[], Some(&out)).unwrap();
    let ns: Vec<u64> = v["pictures"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["n"].as_u64().unwrap())
        .collect();
    assert_eq!(ns, [1, 2, 3]);
    assert_eq!(v["pictures"][0]["note"], "Çekmece");
    let sheet = image::open(&out).unwrap();
    assert!(sheet.width() > sheet.height(), "{}x{}", sheet.width(), sheet.height());
    // Some of them, and a number the series lacks is refused.
    let v = inv.photo_sheet(&[2, 3], Some(&out)).unwrap();
    assert_eq!(v["pictures"].as_array().unwrap().len(), 2);
    let e = inv.photo_sheet(&[9], Some(&out)).unwrap_err();
    assert_eq!(e.id(), Some("plan_series_has_no"));
}

#[test]
fn series_references_read_single_pictures_and_ranges() {
    let refs = |t: &[&str]| {
        ev_core::series_numbers(&t.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    };
    assert_eq!(refs(&["f2", "f5..f7"]).unwrap(), [2, 5, 6, 7]);
    // The second end may leave out its `f`.
    assert_eq!(refs(&["F16..18"]).unwrap(), [16, 17, 18]);
    for bad in ["12", "f7..f5", "f0", "fx..f2"] {
        let e = refs(&[bad]).unwrap_err();
        assert_eq!(e.id(), Some("series_ref_bad"), "{bad}");
    }
}
