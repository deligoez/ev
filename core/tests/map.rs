//! Spec §31: the map — any place drawn as the tiles of what is in it.

use ev_core::{Inventory, NewNode, SketchChange, parse_pair, parse_points};
use serde_json::Value;

/// `ev sketch <r> [--at] [--size] [--on] [--points]`.
fn sketch(
    inv: &mut Inventory,
    r: &str,
    at: Option<&str>,
    size: Option<&str>,
    on: Option<&str>,
    points: Option<&str>,
) -> ev_core::Result<Value> {
    inv.sketch_set(&SketchChange {
        reference: r.into(),
        at: at.map(|s| parse_pair(s, "--at")).transpose()?,
        size: size.map(|s| parse_pair(s, "--size")).transpose()?,
        on: on.map(Into::into),
        points: points.map(parse_points).transpose()?,
        ..Default::default()
    })
}

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
    inv.cells_set(&cells(&[
        ("K22-01", "A1"),
        ("K22-02", "B1"),
        ("K22-03", "A2"),
        ("K22-04", "B2"),
        ("K21-01", "A1"),
        ("K21-02", "B1"),
    ]))
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
    sketch(&mut inv, "K21", None, None, Some("K22"), None).unwrap();
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
    sketch(&mut inv, "Çalışma odası", None, Some("400,300"), None, None).unwrap();
    sketch(&mut inv, "K22", Some("0,0"), Some("150,40"), None, None).unwrap();
    sketch(
        &mut inv,
        "Masa",
        Some("200,150"),
        Some("120,60"),
        None,
        None,
    )
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
        sketch(&mut inv, "K22", None, None, None, None)
            .unwrap_err()
            .code(),
        2
    );
    assert_eq!(
        sketch(&mut inv, "K22", None, Some("0,40"), None, None)
            .unwrap_err()
            .code(),
        2
    );
    assert_eq!(
        sketch(&mut inv, "K22", Some("a,0"), None, None, None)
            .unwrap_err()
            .code(),
        2
    );
    assert_eq!(
        sketch(&mut inv, "K22", None, None, Some("K22"), None)
            .unwrap_err()
            .code(),
        5
    );
    sketch(&mut inv, "K21", None, None, Some("K22"), None).unwrap();
    // No circle: the one below cannot stand on the one on it.
    assert_eq!(
        sketch(&mut inv, "K22", None, None, Some("K21"), None)
            .unwrap_err()
            .code(),
        5
    );
    inv.sketch_clear("K21").unwrap();
    assert!(inv.sketch("K21").unwrap().is_null());
    let h = inv.history("K21").unwrap()["events"].clone();
    assert_eq!(h.as_array().unwrap().last().unwrap()["type"], "sketch");
}

/// Mutfak with a size, beside the study on a side, slid along it.
fn beside(side: &str, other: &str, offset: Option<f64>) -> SketchChange {
    let mut c = SketchChange {
        reference: "Mutfak".into(),
        size: Some([300.0, 500.0]),
        offset,
        ..Default::default()
    };
    let o = Some(other.to_string());
    match side {
        "right" => c.right_of = o,
        "left" => c.left_of = o,
        "above" => c.above = o,
        _ => c.below = o,
    }
    c
}

#[test]
fn a_room_is_placed_beside_another_by_its_size_and_side() {
    let (_d, mut inv) = setup();
    sketch(
        &mut inv,
        "Çalışma odası",
        Some("0,0"),
        Some("400,300"),
        None,
        None,
    )
    .unwrap();
    // Touching it on that side, slid along from its top or left edge.
    let at = |v: Value| (v["sketch"]["x"].clone(), v["sketch"]["y"].clone());
    let v = inv
        .sketch_set(&beside("right", "Çalışma odası", Some(50.0)))
        .unwrap();
    assert_eq!(at(v), (400.0.into(), 50.0.into()));
    let v = inv
        .sketch_set(&beside("left", "Çalışma odası", None))
        .unwrap();
    assert_eq!(at(v), ((-300.0).into(), 0.0.into()));
    let v = inv
        .sketch_set(&beside("above", "Çalışma odası", None))
        .unwrap();
    assert_eq!(at(v), (0.0.into(), (-500.0).into()));
    let v = inv
        .sketch_set(&beside("below", "Çalışma odası", Some(100.0)))
        .unwrap();
    assert_eq!(at(v), (100.0.into(), 300.0.into()));
    // The home is drawn around both, wherever they lie.
    let home = inv.map(None).unwrap();
    assert_eq!(home["layout"], "sketch");
    assert_eq!(home["size"]["d"], 800.0);
    // An outline moves with its room.
    sketch(
        &mut inv,
        "Mutfak",
        None,
        None,
        None,
        Some("0,0 300,0 300,500 0,500"),
    )
    .unwrap();
    let v = inv
        .sketch_set(&SketchChange {
            reference: "Mutfak".into(),
            right_of: Some("Çalışma odası".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(v["sketch"]["points"][2], serde_json::json!([700.0, 500.0]));
}

#[test]
fn a_room_is_not_placed_beside_what_cannot_say_where() {
    let (_d, mut inv) = setup();
    // Without a size it cannot be placed beside anything.
    let no_size = SketchChange {
        reference: "Mutfak".into(),
        right_of: Some("Çalışma odası".into()),
        ..Default::default()
    };
    assert_eq!(inv.sketch_set(&no_size).unwrap_err().code(), 2);
    // Beside something with no place yet, or in another holder: refused.
    let e = inv
        .sketch_set(&beside("right", "Çalışma odası", None))
        .unwrap_err();
    assert_eq!(e.code(), 5);
    sketch(&mut inv, "K22", Some("0,0"), Some("150,40"), None, None).unwrap();
    assert_eq!(
        inv.sketch_set(&beside("right", "K22", None))
            .unwrap_err()
            .code(),
        5
    );
    // Two sides, or a slide with no side: a usage error.
    let mut two = beside("right", "K22", None);
    two.below = Some("K22".into());
    assert_eq!(inv.sketch_set(&two).unwrap_err().code(), 2);
    let slide = SketchChange {
        reference: "Mutfak".into(),
        offset: Some(10.0),
        ..Default::default()
    };
    assert_eq!(inv.sketch_set(&slide).unwrap_err().code(), 2);
    assert!(inv.sketch("Mutfak").unwrap().is_null());
}

#[test]
fn many_sketches_come_from_lines_in_order_all_or_none() {
    let (_d, mut inv) = setup();
    let text = r#"{"ref": "Çalışma odası", "points": [[0, 0], [400, 0], [400, 300], [0, 300]]}

{"ref": "Mutfak", "size": [300, 500], "right_of": "Çalışma odası"}"#;
    let v = inv.sketch_many(text).unwrap();
    assert_eq!(v["sketched"].as_array().unwrap().len(), 2);
    assert_eq!(inv.sketch("Mutfak").unwrap()["x"], 400.0);
    // A bad line changes nothing and is named.
    let bad = "{\"ref\": \"Mutfak\", \"at\": [0, 0]}\n{\"ref\": \"Mutfak\", \"sise\": [1, 2]}";
    let e = inv.sketch_many(bad).unwrap_err();
    assert_eq!(e.code(), 2);
    assert!(e.to_string().starts_with("line 2:"), "{e}");
    assert_eq!(inv.sketch("Mutfak").unwrap()["x"], 400.0);
}

#[test]
fn a_room_given_by_its_size_is_a_floor_and_a_cupboard_in_it_is_drawn_to_scale() {
    let (_d, mut inv) = setup();
    // A 2 m × 2 m room, and a 1 m × 1 m cupboard in its back right corner.
    sketch(&mut inv, "Mutfak", Some("0,0"), Some("200,200"), None, None).unwrap();
    add(&mut inv, "Dolap", "furniture", Some("Mutfak"), None);
    sketch(
        &mut inv,
        "Dolap",
        Some("100,0"),
        Some("100,100"),
        None,
        None,
    )
    .unwrap();
    let m = inv.map(Some("Mutfak")).unwrap();
    assert_eq!(m["layout"], "sketch");
    // The room's own floor is its rectangle; the cupboard is a quarter of it, not a floor.
    assert_eq!(
        m["size"]["floor"],
        serde_json::json!([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
    );
    let dolap = &m["tiles"][0];
    assert_eq!(dolap["rect"], serde_json::json!([0.5, 0.0, 0.5, 0.5]));
    assert!(dolap["shapes"].is_null());
    // On the home, the room given by its size is a floor like a room given by its corners.
    let home = inv.map(None).unwrap();
    let room = home["tiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "Mutfak")
        .unwrap();
    assert_eq!(room["shapes"][0].as_array().unwrap().len(), 4);
}

#[test]
fn a_room_moved_out_of_its_room_stays_where_it_lies_on_the_plan() {
    let (_d, mut inv) = setup();
    // The kitchen's top-left corner is at 100,200 in the flat; its balcony, inside it, is
    // written in the kitchen's frame, above its top edge.
    sketch(
        &mut inv,
        "Mutfak",
        None,
        None,
        None,
        Some("100,200 400,200 400,500 100,500"),
    )
    .unwrap();
    add(&mut inv, "Balkon", "room", Some("Mutfak"), None);
    sketch(
        &mut inv,
        "Balkon",
        None,
        None,
        None,
        Some("0,-100 300,-100 300,0 0,0"),
    )
    .unwrap();
    inv.move_to("Balkon", "Ev", false).unwrap();
    let s = inv.sketch("Balkon").unwrap();
    assert_eq!(
        (s["x"].as_f64(), s["y"].as_f64()),
        (Some(100.0), Some(100.0))
    );
    assert_eq!(
        s["points"],
        serde_json::json!([
            [100.0, 100.0],
            [400.0, 100.0],
            [400.0, 200.0],
            [100.0, 200.0]
        ])
    );
}

#[test]
fn a_move_out_of_a_holder_with_no_place_leaves_the_sketch_as_it_was() {
    let (_d, mut inv) = setup();
    // The study has no place in the flat, so where its desk lies in the flat is unknown.
    sketch(&mut inv, "Masa", Some("10,20"), Some("160,80"), None, None).unwrap();
    sketch(
        &mut inv,
        "Mutfak",
        None,
        None,
        None,
        Some("100,200 400,200 400,500 100,500"),
    )
    .unwrap();
    inv.move_to("Masa", "Mutfak", false).unwrap();
    let s = inv.sketch("Masa").unwrap();
    assert_eq!((s["x"].as_f64(), s["y"].as_f64()), (Some(10.0), Some(20.0)));
}
