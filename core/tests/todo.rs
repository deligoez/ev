//! Spec §18: labels, broken things, use-by dates, sales, needs, and `todo` gathering them all.

use ev_core::{Disposition, Inventory, NewNode};
use serde_json::Value;
use tempfile::TempDir;

fn add(
    inv: &mut Inventory,
    name: &str,
    kind: &str,
    parent: Option<&str>,
    code: Option<&str>,
) -> i64 {
    inv.add(NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: parent.map(Into::into),
        code: code.map(Into::into),
        ..Default::default()
    })
    .unwrap()["node"]["id"]
        .as_i64()
        .unwrap()
}

fn setup() -> (TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    add(&mut inv, "Ev", "home", None, None);
    add(&mut inv, "Oda", "room", Some("Ev"), None);
    add(&mut inv, "Samla", "container", Some("Oda"), Some("S5-01"));
    add(&mut inv, "Silikon", "item", Some("S5-01"), None);
    add(&mut inv, "Kulaklık", "item", Some("S5-01"), None);
    (dir, inv)
}

fn names(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|n| n["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_place_not_counted_yet_is_listed_until_it_is_counted() {
    let (_d, mut inv) = setup();
    add(&mut inv, "Masa", "furniture", Some("Oda"), None);
    let v = inv.todo().unwrap();
    assert!(names(&v["uncounted"]).contains(&"Masa".to_string()), "{v}");
    assert_eq!(
        v["counts"]["uncounted"],
        v["uncounted"].as_array().unwrap().len()
    );
    // Being counted, it is still work to do; counted, it leaves the list.
    inv.review("Masa", "counting", None).unwrap();
    assert!(names(&inv.todo().unwrap()["uncounted"]).contains(&"Masa".to_string()));
    inv.review("Masa", "toured", None).unwrap();
    assert!(!names(&inv.todo().unwrap()["uncounted"]).contains(&"Masa".to_string()));
}

#[test]
fn a_new_code_needs_a_label_until_it_is_printed() {
    let (_d, mut inv) = setup();
    let v = inv.label(&[], true).unwrap();
    assert_eq!(names(&v["labels"]), ["Samla"]);
    let v = inv.label(&["S5-01".into()], true).unwrap();
    assert!(v["labels"].as_array().unwrap().is_empty());
    // Changing the code makes the old label wrong.
    inv.edit("S5-01", &["code=S5-09".into()]).unwrap();
    assert_eq!(names(&inv.label(&[], true).unwrap()["labels"]), ["Samla"]);
    inv.edit("S5-09", &["code=".into()]).unwrap();
    assert!(
        inv.label(&[], true).unwrap()["labels"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(inv.label(&["Silikon".into()], true).unwrap_err().code(), 5);
}

#[test]
fn broken_expiring_and_needs_show_on_the_node_and_in_todo() {
    let (_d, mut inv) = setup();
    inv.broken("Kulaklık", Some("sol taraf ses vermiyor"), false)
        .unwrap();
    inv.expires("Silikon", Some("2000-07")).unwrap();
    let v = inv
        .need_add("Samla 5 L", Some(2), false, Some("Oda"), None)
        .unwrap();
    let need = v["id"].as_i64().unwrap();
    let show = inv.show("Silikon", false).unwrap();
    assert_eq!(show["marks"]["expires"]["value"], "2000-07-31");
    assert_eq!(
        inv.show("Oda", false).unwrap()["needs"][0]["text"],
        "Samla 5 L"
    );

    let t = inv.todo().unwrap();
    assert_eq!(names(&t["repairs"]), ["Kulaklık"]);
    assert_eq!(t["repairs"][0]["note"], "sol taraf ses vermiyor");
    assert_eq!(names(&t["expiring"]), ["Silikon"]);
    assert!(t["expiring"][0]["days_left"].as_i64().unwrap() < 0);
    assert_eq!(t["counts"]["needs"], 1);

    // A date far away is not due yet; fixed and got leave the list.
    inv.expires("Silikon", Some("2999-01-01")).unwrap();
    inv.broken("Kulaklık", None, true).unwrap();
    inv.need_close(need, true, None).unwrap();
    let t = inv.todo().unwrap();
    assert_eq!(t["counts"]["expiring"], 0);
    assert_eq!(t["counts"]["repairs"], 0);
    assert_eq!(t["counts"]["needs"], 0);
    assert_eq!(inv.need_close(need, false, None).unwrap_err().code(), 5);
    assert_eq!(
        inv.expires("Silikon", Some("07/2026")).unwrap_err().code(),
        2
    );
}

#[test]
fn only_a_sell_candidate_has_a_sale() {
    let (_d, mut inv) = setup();
    assert_eq!(
        inv.sale("Kulaklık", Some("listed"), Some(500), None)
            .unwrap_err()
            .code(),
        5
    );
    inv.dispose("Kulaklık", Disposition::Sell).unwrap();
    inv.sale("Kulaklık", Some("listed"), Some(500), Some("Sahibinden"))
        .unwrap();
    // Reserving keeps the price and where it is listed.
    let v = inv.sale("Kulaklık", Some("reserved"), None, None).unwrap();
    assert_eq!(v["marks"]["sale"]["value"], "reserved");
    assert_eq!(v["marks"]["sale"]["amount"], 500);
    assert_eq!(v["marks"]["sale"]["note"], "Sahibinden");
    let d = inv.disposals(None).unwrap();
    assert_eq!(d["disposals"]["sell"][0]["sale"]["value"], "reserved");
    let v = inv.sale("Kulaklık", None, None, None).unwrap();
    assert!(v["marks"].get("sale").is_none());
}

#[test]
fn a_mistaken_record_closes_with_a_reason_and_never_counts_as_leaving() {
    let (_d, mut inv) = setup();
    assert_eq!(
        inv.gone_because("Kulaklık", Some(Disposition::Mistake), None)
            .unwrap_err()
            .code(),
        2
    );
    assert_eq!(
        inv.dispose("Kulaklık", Disposition::Mistake)
            .unwrap_err()
            .code(),
        2
    );
    let v = inv
        .gone_because(
            "Kulaklık",
            Some(Disposition::Mistake),
            Some("counted twice"),
        )
        .unwrap();
    assert_eq!(v["node"]["state"], "gone");
    assert_eq!(v["node"]["disposition"], "mistake");
    let d = inv.disposals(None).unwrap();
    assert!(
        d["disposals"]
            .as_object()
            .unwrap()
            .values()
            .all(|l| l.as_array().unwrap().is_empty())
    );
}

#[test]
fn a_place_needs_a_new_photo_once_its_contents_change() {
    let (d, mut inv) = setup();
    let codes = |inv: &Inventory| -> Vec<(String, String)> {
        inv.todo().unwrap()["photos"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                (
                    p["code"].as_str().unwrap_or_default().to_string(),
                    p["photo_reason"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    };
    assert_eq!(codes(&inv), [("S5-01".to_string(), "none".to_string())]);
    let img = d.path().join("p.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3]))
        .save(&img)
        .unwrap();
    inv.photo_add("S5-01", &img, None, None).unwrap();
    assert!(codes(&inv).is_empty());
    // A crop of one thing is not a picture of the place.
    inv.photo_add("Silikon", &img, Some("0,0,0.5,0.5".parse().unwrap()), None)
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    // Taking something out changes the place as much as putting something in.
    add(&mut inv, "Masa", "furniture", Some("Oda"), None);
    inv.move_to("Silikon", "Masa", false).unwrap();
    assert!(codes(&inv).contains(&("S5-01".to_string(), "changed".to_string())));
    // The person says the old photo is still good enough.
    inv.photo_current("S5-01").unwrap();
    assert!(!codes(&inv).iter().any(|(c, _)| c == "S5-01"));
}

#[test]
fn a_crop_cut_for_the_place_itself_is_its_current_photo() {
    let (d, mut inv) = setup();
    let img = d.path().join("drawer.png");
    image::RgbImage::from_pixel(16, 16, image::Rgb([9, 9, 9]))
        .save(&img)
        .unwrap();
    let needed = |inv: &Inventory| inv.todo().unwrap()["counts"]["photos"].as_u64().unwrap();
    assert_eq!(needed(&inv), 1);
    // The box cut out of a wider drawer photo pictures the box.
    inv.photo_add("S5-01", &img, Some("0,0,0.5,0.5".parse().unwrap()), None)
        .unwrap();
    assert_eq!(needed(&inv), 0);
}

#[test]
fn codes_rotate_between_boxes_in_one_step() {
    let (_d, mut inv) = setup();
    add(&mut inv, "Kutu B", "container", Some("Oda"), Some("S5-02"));
    add(&mut inv, "Kutu C", "container", Some("Oda"), Some("S5-03"));
    // One at a time, the first new code is still taken.
    assert_eq!(
        inv.edit("S5-01", &["code=S5-02".into()])
            .unwrap_err()
            .code(),
        5
    );
    let pairs = |p: &[(&str, &str)]| -> Vec<(String, String)> {
        p.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    };
    let v = inv
        .recode(&pairs(&[
            ("S5-01", "S5-02"),
            ("S5-02", "S5-03"),
            ("S5-03", "S5-01"),
        ]))
        .unwrap();
    assert_eq!(v["recoded"].as_array().unwrap().len(), 3);
    assert_eq!(inv.show("Samla", false).unwrap()["node"]["code"], "S5-02");
    assert_eq!(inv.show("Kutu C", false).unwrap()["node"]["code"], "S5-01");
    // Every new code needs a new label.
    assert_eq!(
        inv.show("Kutu B", false).unwrap()["marks"]["label"]["value"],
        "needed"
    );
    // A code that belongs to a node outside the set is still refused, and nothing changes.
    add(&mut inv, "Kutu D", "container", Some("Oda"), Some("S5-09"));
    assert_eq!(
        inv.recode(&pairs(&[("S5-01", "S5-09")]))
            .unwrap_err()
            .code(),
        5
    );
    assert_eq!(inv.show("Kutu C", false).unwrap()["node"]["code"], "S5-01");
    // Two nodes cannot end up with one code.
    assert_eq!(
        inv.recode(&pairs(&[("S5-01", "S5-07"), ("S5-02", "s5-07")]))
            .unwrap_err()
            .code(),
        2
    );
}

#[test]
fn a_whole_photo_goes_on_one_node_and_crops_on_the_rest() {
    let (d, mut inv) = setup();
    let img = d.path().join("drawer.png");
    image::RgbImage::from_pixel(16, 16, image::Rgb([5, 6, 7]))
        .save(&img)
        .unwrap();
    inv.photo_add("S5-01", &img, None, None).unwrap();
    // The same whole photo on a thing inside is the mistake: refused, pointing at the holder.
    let e = inv.photo_add("Silikon", &img, None, None).unwrap_err();
    assert_eq!(e.code(), 5);
    // A crop is the fix; --whole is the deliberate exception.
    inv.photo_add("Silikon", &img, Some("0,0,0.5,0.5".parse().unwrap()), None)
        .unwrap();
    assert_eq!(inv.todo().unwrap()["counts"]["shared_photos"], 0);
    inv.photo_add_with("Kulaklık", &img, None, None, true)
        .unwrap();
    let t = inv.todo().unwrap();
    assert_eq!(t["counts"]["shared_photos"], 1);
    assert_eq!(
        names(&t["shared_photos"][0]["nodes"]),
        ["Samla", "Kulaklık"]
    );
}

#[test]
fn focus_names_a_node_and_its_last_photo_by_default() {
    let (d, mut inv) = setup();
    assert!(inv.focus_request().unwrap().is_null());
    let v = inv.focus(Some("Silikon"), None).unwrap();
    assert!(v["focus"]["photo"].is_null());
    let img = d.path().join("p.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3]))
        .save(&img)
        .unwrap();
    inv.photo_add("Silikon", &img, None, None).unwrap();
    inv.photo_add("Silikon", &img, None, Some("second"))
        .unwrap();
    let v = inv.focus(Some("Silikon"), None).unwrap();
    assert_eq!(v["focus"]["photo"], 2);
    assert_eq!(inv.focus(Some("Silikon"), Some(3)).unwrap_err().code(), 3);
    assert_eq!(inv.focus_request().unwrap()["photo"], 2);
    inv.focus(None, None).unwrap();
    assert!(inv.focus_request().unwrap().is_null());
}

#[test]
fn todo_gathers_state_that_lives_elsewhere_without_copying_it() {
    let (_d, mut inv) = setup();
    add(&mut inv, "Kutu", "container", Some("Oda"), None);
    add(&mut inv, "Belirsiz parça", "item", Some("S5-01"), None);
    inv.move_to("Silikon", "Kutu", true).unwrap();
    inv.edit("Kulaklık", &["owner=Mahmutlar".into()]).unwrap();
    inv.mark_lost("Belirsiz parça").unwrap();

    let t = inv.todo().unwrap();
    let c = &t["counts"];
    assert_eq!(c["moves"], 1);
    assert_eq!(c["errands"], 1);
    assert_eq!(c["lost"], 1);
    assert!(names(&t["uncounted"]).contains(&"Kutu".to_string()));
    assert_eq!(names(&t["unclear"]), ["Belirsiz parça"]);

    // Doing the thing is what clears it; nothing to close in todo.
    inv.done("Silikon").unwrap();
    inv.found("Belirsiz parça").unwrap();
    let c = inv.todo().unwrap()["counts"].clone();
    assert_eq!(c["moves"], 0);
    assert_eq!(c["lost"], 0);
}

#[test]
fn one_photo_is_cut_up_among_a_place_and_its_boxes_in_one_step() {
    let (d, mut inv) = setup();
    add(&mut inv, "Kutu B", "container", Some("Oda"), Some("S5-02"));
    add(&mut inv, "Pil", "item", Some("S5-02"), None);
    let img = d.path().join("drawer.png");
    image::RgbImage::from_pixel(20, 10, image::Rgb([7, 8, 9]))
        .save(&img)
        .unwrap();
    let crop = |s: &str| s.parse::<ev_core::Crop>().unwrap();
    // A typo in the last reference attaches nothing at all.
    let e = inv
        .photo_cut(
            &img,
            Some("Oda"),
            &[
                ("S5-01".into(), crop("0,0,0.5,1")),
                ("S5-99".into(), crop("0.5,0,0.5,1")),
            ],
            None,
            None,
        )
        .unwrap_err();
    assert_eq!(e.code(), 3);
    assert!(
        inv.photo_list("S5-01").unwrap()["photos"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        inv.photo_list("Oda").unwrap()["photos"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let v = inv
        .photo_cut(
            &img,
            Some("Oda"),
            &[
                ("S5-01".into(), crop("0,0,0.5,1")),
                ("S5-02".into(), crop("0.5,0,0.5,1")),
            ],
            Some("son hali"),
            None,
        )
        .unwrap();
    let attached = v["attached"].as_array().unwrap();
    assert_eq!(attached.len(), 3);
    assert!(attached[0]["crop"].is_null());
    assert_eq!(attached[1]["crop"], "0.0000,0.0000,0.5000,1.0000");
    // Both boxes count as photographed now, and the whole view is on the room only.
    let todo = inv.todo().unwrap();
    let needing: Vec<&str> = todo["photos"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p["code"].as_str())
        .collect();
    assert!(!needing.contains(&"S5-01") && !needing.contains(&"S5-02"));
    assert_eq!(todo["counts"]["shared_photos"], 0);
}

#[test]
fn a_counted_box_with_nothing_waiting_is_not_open_and_any_work_on_it_opens_it() {
    let (_d, mut inv) = setup();
    let id = |inv: &Inventory, r: &str| inv.resolve(r, false).unwrap();
    // Not counted yet: open.
    assert!(inv.open_nodes().unwrap().contains(&id(&inv, "S5-01")));
    inv.label(&["S5-01".into()], true).unwrap();
    inv.photo_current("S5-01").unwrap();
    inv.review("S5-01", "toured", None).unwrap();
    let open = inv.open_nodes().unwrap();
    for r in ["S5-01", "Silikon", "Kulaklık"] {
        assert!(!open.contains(&id(&inv, r)), "{r}: {open:?}");
    }
    // A task, an observation, and both ends of a planned move each leave work on a node.
    let task = inv.task_add("Ayıkla", "karışık", &["S5-01".into()], None).unwrap()["id"]
        .as_i64()
        .unwrap();
    add(&mut inv, "Kutu", "container", Some("Oda"), None);
    inv.observe("Oda", "Kablolar dağınık", None).unwrap();
    inv.move_to("Kulaklık", "Kutu", true).unwrap();
    let open = inv.open_nodes().unwrap();
    for r in ["S5-01", "Oda", "Kulaklık", "Kutu"] {
        assert!(open.contains(&id(&inv, r)), "{r}: {open:?}");
    }
    assert!(!open.contains(&id(&inv, "Silikon")), "{open:?}");
    inv.task_set(task, "done", None).unwrap();
    inv.done("Kulaklık").unwrap();
    assert!(!inv.open_nodes().unwrap().contains(&id(&inv, "Kulaklık")));
}
