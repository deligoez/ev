//! Spec §22: a holder laid out in cells, and boxes placed in them.

use ev_core::{Inventory, NewNode};
use serde_json::Value;
use tempfile::TempDir;

fn add(inv: &mut Inventory, name: &str, kind: &str, parent: Option<&str>, code: Option<&str>) {
    inv.add(NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: parent.map(Into::into),
        code: code.map(Into::into),
        ..Default::default()
    })
    .unwrap();
}

/// A drawer `D` with a tall box and two small ones, none placed yet.
fn setup() -> (TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    add(&mut inv, "Ev", "home", None, None);
    add(&mut inv, "Oda", "room", Some("Ev"), None);
    add(&mut inv, "Çekmece", "container", Some("Oda"), Some("D"));
    add(&mut inv, "Uzun kutu", "container", Some("D"), Some("D-A4"));
    add(&mut inv, "Sıcaklık", "container", Some("D"), Some("D-A3"));
    add(&mut inv, "Giriş", "container", Some("D"), Some("D-B3"));
    (dir, inv)
}

fn pairs(p: &[(&str, &str)]) -> Vec<(String, String)> {
    p.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

fn free(inv: &Inventory) -> Vec<String> {
    inv.grid("D").unwrap()["grid"]["free"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn boxes_are_placed_checked_and_mapped() {
    let (_d, mut inv) = setup();
    // No grid yet: placing is refused and says how to set one.
    assert_eq!(
        inv.cells_set(&pairs(&[("D-A3", "A3")]), false)
            .unwrap_err()
            .code(),
        5
    );
    inv.grid_set("D", 3, 4).unwrap();
    inv.cells_set(
        &pairs(&[("D-A4", "A4-B4"), ("D-A3", "A3"), ("D-B3", "B3")]),
        false,
    )
    .unwrap();
    assert_eq!(free(&inv).len(), 12 - 4);
    let g = &inv.grid("D").unwrap()["grid"];
    let a4: Value = g["map"][3][1].clone();
    assert_eq!(a4, g["map"][3][0], "a box covers every cell of its range");
    assert!(g["map"][0][0].is_null());

    // Outside the grid, and on top of another box, are refused and change nothing.
    assert_eq!(
        inv.cells_set(&pairs(&[("D-A3", "D3")]), false)
            .unwrap_err()
            .code(),
        5
    );
    assert_eq!(
        inv.cells_set(&pairs(&[("D-A3", "B4")]), false)
            .unwrap_err()
            .code(),
        5
    );
    assert_eq!(inv.show("D-A3", false).unwrap()["cells"], "A3");
    // A grid cannot shrink under a placed box.
    assert_eq!(inv.grid_set("D", 3, 3).unwrap_err().code(), 5);
    // Nor be removed while boxes sit in it.
    assert_eq!(inv.grid_clear("D").unwrap_err().code(), 5);
}

#[test]
fn boxes_trade_places_and_codes_in_one_step() {
    let (_d, mut inv) = setup();
    inv.grid_set("D", 3, 4).unwrap();
    inv.cells_set(
        &pairs(&[("D-A4", "A4-B4"), ("D-A3", "A3"), ("D-B3", "B3")]),
        false,
    )
    .unwrap();
    // The tall box goes back a row and the two small ones come forward: one at a time each
    // move would overlap, together they fit, and the codes follow the cells.
    let v = inv
        .cells_set(
            &pairs(&[("Uzun kutu", "A3-B3"), ("Sıcaklık", "A4"), ("Giriş", "B4")]),
            true,
        )
        .unwrap();
    assert_eq!(v["placed"].as_array().unwrap().len(), 3);
    assert_eq!(
        inv.show("Uzun kutu", false).unwrap()["node"]["code"],
        "D-A3"
    );
    assert_eq!(inv.show("Sıcaklık", false).unwrap()["node"]["code"], "D-A4");
    assert_eq!(inv.show("Giriş", false).unwrap()["node"]["code"], "D-B4");
    assert_eq!(inv.show("D-A3", false).unwrap()["cells"], "A3-B3");
    // New codes need their labels printed.
    assert_eq!(
        inv.show("D-B4", false).unwrap()["marks"]["label"]["value"],
        "needed"
    );
}

#[test]
fn a_box_moved_out_leaves_its_cells_free() {
    let (_d, mut inv) = setup();
    inv.grid_set("D", 2, 2).unwrap();
    inv.cells_set(&pairs(&[("D-A3", "A1")]), false).unwrap();
    assert_eq!(free(&inv), ["B1", "A2", "B2"]);
    inv.move_to("D-A3", "Oda", false).unwrap();
    assert_eq!(free(&inv).len(), 4);
    assert!(inv.show("D-A3", false).unwrap()["cells"].is_null());
    // Taking a box out of the grid by hand works too.
    inv.cells_set(&pairs(&[("D-B3", "B2")]), false).unwrap();
    inv.cells_set(&pairs(&[("D-B3", "")]), false).unwrap();
    assert_eq!(free(&inv).len(), 4);
    // The holder's suggest entry reports its free cells.
    let s = inv.suggest("sensör modülü", None).unwrap();
    let d = s["containers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["code"] == "D")
        .unwrap()
        .clone();
    assert_eq!(d["grid"]["free"].as_array().unwrap().len(), 4);
}

/// Crops are stored to four decimals.
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

/// Drawer `D` as a 3×2 grid with a one-cell box at A1 and a two-cell box at B1–C1, each holding a
/// thing, and a photo of it on disk.
fn photographed() -> (TempDir, Inventory, std::path::PathBuf) {
    let (dir, mut inv) = setup();
    inv.grid_set("D", 3, 2).unwrap();
    inv.cells_set(&pairs(&[("D-A4", "A1"), ("D-A3", "B1-C1")]), false)
        .unwrap();
    add(&mut inv, "Vida", "item", Some("D-A4"), None);
    add(&mut inv, "Somun", "item", Some("D-A3"), None);
    let photo = dir.path().join("drawer.png");
    image::RgbImage::from_pixel(600, 700, image::Rgb([200, 200, 200]))
        .save(&photo)
        .unwrap();
    (dir, inv, photo)
}

fn last_crop(inv: &Inventory, r: &str) -> Value {
    inv.photo_list(r).unwrap()["photos"]
        .as_array()
        .unwrap()
        .last()
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn a_grid_photo_crops_every_placed_box_from_the_grid_corners() {
    let (_d, mut inv, photo) = photographed();
    // The grid fills the middle 80% of the photo, square to it.
    let corners: ev_core::GridCorners = "0.1,0.1,0.9,0.1,0.9,0.9,0.1,0.9".parse().unwrap();
    inv.photo_cut(&photo, Some("D"), &[], Some("son hali"), Some(&corners))
        .unwrap();
    // The drawer gets the whole photo, each box a crop.
    assert!(last_crop(&inv, "D")["crop"].is_null());
    let a1 = last_crop(&inv, "D-A4");
    let b1 = last_crop(&inv, "D-A3");
    let nums = |v: &Value| -> Vec<f64> {
        v["crop"]
            .as_str()
            .unwrap()
            .split(',')
            .map(|x| x.parse().unwrap())
            .collect()
    };
    // A1 is the back-left third by half, plus a margin of 0.15 of a cell each side.
    let a = nums(&a1);
    assert!(close(a[0], 0.1 + 0.8 * (-0.05)), "{a:?}");
    assert!(close(a[1], 0.1 + 0.8 * (-0.075)), "{a:?}");
    assert!(close(a[0] + a[2], 0.1 + 0.8 * (1.0 / 3.0 + 0.05)), "{a:?}");
    assert!(close(a[1] + a[3], 0.1 + 0.8 * (0.5 + 0.075)), "{a:?}");
    // B1–C1 spans two columns and reaches the right edge of the grid.
    let b = nums(&b1);
    assert!(close(b[0] + b[2], 0.1 + 0.8 * (1.0 + 0.05)), "{b:?}");
    // Without --place there is no grid to read.
    assert_eq!(
        inv.photo_cut(
            &photo,
            None,
            &[("D-A4".into(), "0,0,1,1".parse().unwrap())],
            None,
            Some(&corners)
        )
        .unwrap_err()
        .code(),
        2
    );
}

#[test]
fn a_marked_copy_frames_a_rectangle_and_a_grids_cells_and_stores_nothing() {
    let (d, mut inv, photo) = photographed();
    let corners: ev_core::GridCorners = "0.1,0.1,0.9,0.1,0.9,0.9,0.1,0.9".parse().unwrap();
    inv.photo_cut(&photo, Some("D"), &[], None, Some(&corners))
        .unwrap();
    let photos_before = inv.photo_list("D").unwrap()["photos"]
        .as_array()
        .unwrap()
        .len();
    let red = |img: &image::RgbImage, x: u32, y: u32| {
        let p = img.get_pixel(x, y);
        p[0] > 200 && p[1] < 80 && p[2] < 80
    };
    // By cell, through the corners the cut kept: C2 is the front-right cell, 0.633–0.9 across
    // and 0.5–0.9 down; its frame runs along those edges.
    let out = d.path().join("marked.jpg");
    let v = inv
        .photo_mark("D", &[("1".into(), "C2".into())], None, Some(&out))
        .unwrap();
    assert_eq!(v["marked"], out.to_string_lossy().as_ref());
    let img = image::open(&out).unwrap().to_rgb8();
    let (w, h) = (img.width() as f64, img.height() as f64);
    assert!(
        red(&img, (0.9 * w) as u32 - 1, (0.7 * h) as u32),
        "right edge of C2"
    );
    assert!(
        red(&img, (0.8 * w) as u32, (0.9 * h) as u32 - 1),
        "front edge of C2"
    );
    assert!(
        !red(&img, (0.2 * w) as u32, (0.2 * h) as u32),
        "A1 is untouched"
    );
    // By rectangle on a file; a cell needs a place.
    let v = inv
        .photo_mark(
            photo.to_str().unwrap(),
            &[("2 → B6".into(), "0.2,0.3,0.2,0.2".into())],
            None,
            Some(&out),
        )
        .unwrap();
    assert_eq!(v["marks"][0]["label"], "2 → B6");
    let img = image::open(&out).unwrap().to_rgb8();
    assert!(
        red(&img, (0.2 * w) as u32, (0.4 * h) as u32),
        "left edge of the rectangle"
    );
    let e = inv
        .photo_mark(
            photo.to_str().unwrap(),
            &[("1".into(), "A1".into())],
            None,
            Some(&out),
        )
        .unwrap_err();
    assert_eq!(e.code(), 2);
    // A cell outside the grid is refused; nothing was attached along the way.
    let e = inv
        .photo_mark("D", &[("1".into(), "D1".into())], None, Some(&out))
        .unwrap_err();
    assert_eq!(e.code(), 2);
    let photos_after = inv.photo_list("D").unwrap()["photos"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(photos_before, photos_after);
    // It can be sent to a running `ev ui`, which shows it once.
    let f = inv
        .focus_file(&[out.clone(), photo.clone()], Some("1 → A6"))
        .unwrap();
    assert_eq!(f["focus"]["note"], "1 → A6");
    assert_eq!(f["focus"]["files"].as_array().unwrap().len(), 2);
    assert_eq!(inv.focus_request().unwrap()["files"], f["focus"]["files"]);
}

#[test]
fn a_place_without_kept_corners_is_marked_by_cell_only_with_corners_given() {
    let (d, mut inv, photo) = photographed();
    inv.photo_add("D", &photo, None, None).unwrap();
    let out = d.path().join("marked.jpg");
    let e = inv
        .photo_mark("D", &[("1".into(), "A1".into())], None, Some(&out))
        .unwrap_err();
    assert_eq!(e.code(), 5);
    let corners: ev_core::GridCorners = "0.1,0.1,0.9,0.1,0.9,0.9,0.1,0.9".parse().unwrap();
    inv.photo_mark(
        "D",
        &[("1".into(), "A1".into())],
        Some(&corners),
        Some(&out),
    )
    .unwrap();
    assert!(out.exists());
}

#[test]
fn a_drawer_is_toured_only_with_photos_that_show_it_as_it_is() {
    let (_d, mut inv, photo) = photographed();
    // No photo yet: refused, and the answer names every box that needs one.
    let e = inv.review("D", "toured", None).unwrap_err();
    assert_eq!(e.code(), 5);
    let corners: ev_core::GridCorners = "0.1,0.1,0.9,0.1,0.9,0.9,0.1,0.9".parse().unwrap();
    inv.photo_cut(&photo, Some("D"), &[], None, Some(&corners))
        .unwrap();
    inv.review("D", "toured", None).unwrap();
    // A box changes after the photo: touring again is refused until it is photographed.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    add(&mut inv, "Pul", "item", Some("D-A4"), None);
    let e = inv.review("D", "toured", None).unwrap_err();
    assert_eq!(e.code(), 5);
    let stale = e.to_json()["error"]["details"]["stale"]
        .as_array()
        .unwrap()
        .clone();
    let codes: Vec<&str> = stale
        .iter()
        .map(|s| s["node"]["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"D-A4"), "{codes:?}");
    assert!(!codes.contains(&"D-A3"), "{codes:?}");
}
