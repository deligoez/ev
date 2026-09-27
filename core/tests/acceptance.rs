//! Spec §9 acceptance rows, plus the §4 reference table.

use ev_core::{Disposition, Error, Inventory, NewNode};
use serde_json::Value;
use tempfile::TempDir;

fn inv() -> (TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    (dir, inv)
}

fn node(name: &str, kind: &str, parent: Option<&str>, code: Option<&str>) -> NewNode {
    NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: parent.map(Into::into),
        code: code.map(Into::into),
        ..Default::default()
    }
}

fn add(
    inv: &mut Inventory,
    name: &str,
    kind: &str,
    parent: Option<&str>,
    code: Option<&str>,
) -> i64 {
    let v = inv.add(node(name, kind, parent, code)).unwrap();
    v["node"]["id"].as_i64().unwrap()
}

fn code_of(e: &Error) -> i32 {
    e.code()
}

fn event_types(inv: &Inventory, reference: &str) -> Vec<String> {
    let h = inv.history(reference).unwrap();
    h["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["type"].as_str().unwrap().to_string())
        .collect()
}

/// Ev › Salon › K4x4 › K4x4-15-A › Flipper Zero, plus Ev › Kiler.
fn home(inv: &mut Inventory) {
    add(inv, "Ev", "home", None, None);
    add(inv, "Salon", "room", Some("Ev"), None);
    add(inv, "Kiler", "room", Some("Ev"), None);
    add(inv, "K4x4", "furniture", Some("Salon"), None);
    add(inv, "Çekmece", "container", Some("K4x4"), Some("K4x4-15-A"));
    add(inv, "Flipper Zero", "item", Some("K4x4-15-A"), None);
}

#[test]
fn a1_find_returns_full_path() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    let v = inv.find("flipper", None, None, false).unwrap();
    let results = v["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0]["path_text"],
        "Ev › Salon › K4x4 › K4x4-15-A › Flipper Zero"
    );
}

#[test]
fn a2_move_into_own_descendant_is_refused() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    add(
        &mut inv,
        "Samla",
        "container",
        Some("K4x4-15-A"),
        Some("S5-01"),
    );
    let e = inv.move_to("K4x4-15-A", "S5-01", false).unwrap_err();
    assert_eq!(code_of(&e), 5);
    let v = inv.show("K4x4-15-A", false).unwrap();
    assert_eq!(v["node"]["path_text"], "Ev › Salon › K4x4 › K4x4-15-A");
}

#[test]
fn a3_a4_planned_move_then_done() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    let box_id = add(
        &mut inv,
        "Elektrik",
        "container",
        Some("Kiler"),
        Some("S5-03"),
    );
    inv.move_to("Flipper Zero", "S5-03", true).unwrap();
    let v = inv.show("Flipper Zero", false).unwrap();
    assert_eq!(
        v["node"]["path_text"],
        "Ev › Salon › K4x4 › K4x4-15-A › Flipper Zero"
    );
    assert_eq!(v["pending"]["code"], "S5-03");

    inv.done("Flipper Zero").unwrap();
    let v = inv.show("Flipper Zero", false).unwrap();
    assert_eq!(v["node"]["parent_id"], box_id);
    assert!(v["pending"].is_null());
    assert_eq!(
        event_types(&inv, "Flipper Zero"),
        ["create", "plan", "done"]
    );
}

#[test]
fn a5_gone_without_disposition_is_refused_for_active() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    assert_eq!(code_of(&inv.gone("Flipper Zero", None).unwrap_err()), 5);
}

#[test]
fn a6_gone_with_disposition_in_one_step() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    let v = inv.gone("Flipper Zero", Some(Disposition::Trash)).unwrap();
    assert_eq!(v["node"]["state"], "gone");
    assert_eq!(v["node"]["disposition"], "trash");
    assert_eq!(
        event_types(&inv, "Flipper Zero"),
        ["create", "dispose", "gone"]
    );
}

#[test]
fn a7_gone_refuses_a_box_that_still_holds_something() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    add(&mut inv, "Kutu", "container", Some("Kiler"), Some("B1"));
    add(&mut inv, "Kalem", "item", Some("B1"), None);
    assert_eq!(
        code_of(&inv.dispose("B1", Disposition::Give).unwrap_err()),
        5
    );
    let e = inv.gone("B1", Some(Disposition::Give)).unwrap_err();
    assert_eq!(code_of(&e), 5);
    assert_eq!(
        e.to_json()["error"]["details"]["children"][0]["name"],
        "Kalem"
    );
}

#[test]
fn a8_a9_code_reuse_after_gone_only() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    add(&mut inv, "Eski", "container", Some("Kiler"), Some("S5-02"));
    let clash = inv
        .add(node("Kutu", "container", Some("Kiler"), Some("S5-02")))
        .unwrap_err();
    assert_eq!(code_of(&clash), 5);
    inv.gone("S5-02", Some(Disposition::Trash)).unwrap();
    inv.add(node("Kutu", "container", Some("Kiler"), Some("S5-02")))
        .unwrap();
    // After reuse the active node wins when gone nodes are included (§11.4).
    let id = inv.resolve("S5-02", true).unwrap();
    assert_eq!(inv.brief(id).unwrap().name, "Kutu");
    // Codes compare folded (§11.3): Ü and U collide.
    add(&mut inv, "Ç", "container", Some("K4x4"), Some("K4x4-07-Ü"));
    assert_eq!(
        code_of(
            &inv.add(node("X", "container", Some("K4x4"), Some("k4x4-07-u")))
                .unwrap_err()
        ),
        5
    );
}

#[test]
fn a10_digit_only_code_is_refused() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    assert_eq!(
        code_of(
            &inv.add(node("Kutu", "container", Some("Kiler"), Some("12")))
                .unwrap_err()
        ),
        5
    );
}

#[test]
fn a11_a12_rooms_nest_only_in_homes_and_rooms() {
    let (_d, mut inv) = inv();
    add(&mut inv, "Ev", "home", None, None);
    add(&mut inv, "Yatak odası", "room", Some("Ev"), None);
    inv.add(node("Giyinme odası", "room", Some("Yatak odası"), None))
        .unwrap();
    add(
        &mut inv,
        "Gardırop",
        "furniture",
        Some("Giyinme odası"),
        None,
    );
    assert_eq!(
        code_of(
            &inv.add(node("Oda", "room", Some("Gardırop"), None))
                .unwrap_err()
        ),
        5
    );
    assert_eq!(
        code_of(
            &inv.move_to("Yatak odası", "Giyinme odası", false)
                .unwrap_err()
        ),
        5
    );
    assert_eq!(
        code_of(&inv.move_to("Giyinme odası", "Gardırop", false).unwrap_err()),
        5
    );
}

#[test]
fn a13_a14_lost_then_moved() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    add(&mut inv, "Ses", "container", Some("Kiler"), Some("S5-01"));
    inv.mark_lost("Flipper Zero").unwrap();
    let v = inv.lost_list().unwrap();
    assert_eq!(
        v["lost"][0]["last_seen"]["path_text"],
        "Ev › Salon › K4x4 › K4x4-15-A"
    );
    let v = inv.move_to("Flipper Zero", "S5-01", false).unwrap();
    assert_eq!(v["node"]["lost"], false);
    assert!(
        inv.lost_list().unwrap()["lost"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

