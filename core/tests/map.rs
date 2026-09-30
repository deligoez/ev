//! Spec §31: the map — any place drawn as the tiles of what is in it.

use ev_core::{Inventory, NewNode};
use serde_json::Value;

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

/// A flat with two rooms; in the study a 2×2 Kallax with a 2×1 one on top and a desk.
fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    add(&mut inv, "Ev", "home", None, None);
    add(&mut inv, "Çalışma odası", "room", Some("Ev"), None);
    add(&mut inv, "Mutfak", "room", Some("Ev"), None);
    add(
        &mut inv,
        "Kallax 2x2",
        "furniture",
        Some("Çalışma odası"),
        Some("K22"),
    );
    add(
        &mut inv,
        "Kallax 2x1",
        "furniture",
        Some("Çalışma odası"),
        Some("K21"),
    );
    add(&mut inv, "Masa", "furniture", Some("Çalışma odası"), None);
    for n in 1..=4 {
        let code = format!("K22-0{n}");
        add(&mut inv, "Bölme", "container", Some("K22"), Some(&code));
    }
    for n in 1..=2 {
        let code = format!("K21-0{n}");
        add(&mut inv, "Raf", "container", Some("K21"), Some(&code));
    }
    add(
        &mut inv,
        "Çekmece",
        "container",
        Some("K22-01"),
        Some("K22-01-Ü"),
    );
    add(&mut inv, "Kalem", "item", Some("K22-01-Ü"), None);
    inv.grid_set("K22", 2, 2).unwrap();
    inv.grid_set("K21", 2, 1).unwrap();
    let cells = |p: &[(&str, &str)]| -> Vec<(String, String)> {
        p.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    };
    inv.cells_set(
        &cells(&[
            ("K22-01", "A1"),
            ("K22-02", "B1"),
            ("K22-03", "A2"),
            ("K22-04", "B2"),
            ("K21-01", "A1"),
            ("K21-02", "B1"),
        ]),
        false,
    )
    .unwrap();
    (dir, inv)
}

fn rect(t: &Value) -> [f64; 4] {
    serde_json::from_value(t["rect"].clone()).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

fn names(v: &Value, key: &str) -> Vec<String> {
    v[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            t["code"]
                .as_str()
                .or(t["name"].as_str())
                .unwrap()
                .to_string()
        })
        .collect()
}

#[test]
fn a_place_with_no_layout_is_still_a_map_of_tiles() {
    let (_d, inv) = setup();
    // Without a reference: the home.
    let v = inv.map(None).unwrap();
    assert_eq!(v["layout"], "tiles");
    // By name as `ev find` folds it: Ç sorts with C, not after Z.
    assert_eq!(names(&v, "tiles"), ["Çalışma odası", "Mutfak"]);
    // The tiles do not overlap and stay inside the place.
    for t in v["tiles"].as_array().unwrap() {
        let [x, y, w, h] = rect(t);
        assert!(x >= 0.0 && y >= 0.0 && x + w <= 1.0 + 1e-9 && y + h <= 1.0 + 1e-9);
    }
    // In the study the Kallax on top is drawn with the one it stands on, not beside it.
    inv.map(Some("Çalışma odası")).unwrap();
}

#[test]
fn a_tile_names_its_contents_and_its_theme() {
    let (_d, mut inv) = setup();
    add(&mut inv, "Silgi", "item", Some("K22-01-Ü"), None);
    inv.edit("K22-01-Ü", &["theme=Kırtasiye".into()]).unwrap();
    inv.edit("Silgi", &["qty=3".into()]).unwrap();
    let v = inv.map(Some("K22-01")).unwrap();
    let t = &v["tiles"][0];
    assert_eq!(t["theme"], "Kırtasiye");
    assert_eq!(t["children"], 2);
    // Holders by code first, then things by name, a count when there are several.
    assert_eq!(t["contents"], serde_json::json!(["Kalem", "Silgi ×3"]));
    let v = inv.map(Some("K22")).unwrap();
    assert_eq!(v["tiles"][0]["contents"], serde_json::json!(["K22-01-Ü"]));
}

#[test]
fn a_kallax_on_another_is_drawn_with_it_front_on_top_first() {
    let (_d, mut inv) = setup();
    inv.sketch_set("K21", None, None, Some("K22"), None)
        .unwrap();
    let room = inv.map(Some("Çalışma odası")).unwrap();
    assert_eq!(names(&room, "tiles"), ["K22", "Masa"]);
    assert_eq!(room["tiles"][0]["stacked"][0]["code"], "K21");
    // Either member shows the whole stack: the 2×1 on top (a third), the 2×2 below.
    for r in ["K22", "K21"] {
        let v = inv.map(Some(r)).unwrap();
        assert_eq!(v["layout"], "stack");
        let bands = v["bands"].as_array().unwrap();
        assert_eq!(bands[0]["code"], "K21");
        assert_eq!(bands[1]["code"], "K22");
        assert!(close(rect(&bands[0])[3], 1.0 / 3.0));
        let top = v["tiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["code"] == "K21-02")
            .unwrap()
            .clone();
        assert_eq!(
            rect(&top).map(|x| (x * 1000.0).round()),
            [500.0, 0.0, 500.0, 333.0]
        );
        let low = v["tiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["code"] == "K22-04")
            .unwrap()
            .clone();
        let [x, y, w, h] = rect(&low);
        assert!(
            close(x, 0.5)
                && close(y, 1.0 / 3.0 + 1.0 / 3.0)
                && close(w, 0.5)
                && close(h, 1.0 / 3.0)
        );
    }
    // Reading order walks top to bottom, left to right.
    let v = inv.map(Some("K22")).unwrap();
    let order: Vec<String> = ev_core::reading_order(&v)
        .iter()
        .map(|id| {
            inv.show(&id.to_string(), false).unwrap()["node"]["code"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        order,
        ["K21-01", "K21-02", "K22-01", "K22-02", "K22-03", "K22-04"]
    );
}

#[test]
fn a_room_with_a_size_is_a_sketch_of_what_lies_in_it() {
    let (_d, mut inv) = setup();
    inv.sketch_set("Çalışma odası", None, Some("400,300"), None, None)
        .unwrap();
    inv.sketch_set("K22", Some("0,0"), Some("150,40"), None, None)
        .unwrap();
    inv.sketch_set("Masa", Some("200,150"), Some("120,60"), None, None)
        .unwrap();
    let v = inv.map(Some("Çalışma odası")).unwrap();
    assert_eq!(v["layout"], "sketch");
    assert_eq!(v["size"]["w"], 400.0);
    let masa = v["tiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "Masa")
        .unwrap()
        .clone();
    assert_eq!(rect(&masa), [0.5, 0.5, 0.3, 0.2]);
    // What does not say where it lies is listed apart.
    assert_eq!(names(&v, "unplaced"), ["K21"]);
    assert_eq!(inv.sketch("Masa").unwrap()["w"], 120.0);
    // A grid is drawn as its cells, with counts.
    let g = inv.map(Some("K22-01")).unwrap();
    assert_eq!(g["layout"], "tiles");
    assert_eq!(g["tiles"][0]["items"], 1);
}

#[test]
fn sketches_refuse_what_cannot_be() {
    let (_d, mut inv) = setup();
    assert_eq!(
        inv.sketch_set("K22", None, None, None, None)
            .unwrap_err()
            .code(),
        2
    );
    assert_eq!(
        inv.sketch_set("K22", None, Some("0,40"), None, None)
            .unwrap_err()
            .code(),
        2
    );
    assert_eq!(
        inv.sketch_set("K22", Some("-1,0"), None, None, None)
            .unwrap_err()
            .code(),
        2
    );
    assert_eq!(
        inv.sketch_set("K22", None, None, Some("K22"), None)
            .unwrap_err()
            .code(),
        5
    );
    inv.sketch_set("K21", None, None, Some("K22"), None)
        .unwrap();
    // No circle: the one below cannot stand on the one on it.
    assert_eq!(
        inv.sketch_set("K22", None, None, Some("K21"), None)
            .unwrap_err()
            .code(),
        5
    );
    inv.sketch_clear("K21").unwrap();
    assert!(inv.sketch("K21").unwrap().is_null());
    let h = inv.history("K21").unwrap()["events"].clone();
    assert_eq!(h.as_array().unwrap().last().unwrap()["type"], "sketch");
}
