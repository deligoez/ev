//! `ev next` informs the order without changing it: a due date, what waits in a place while
//! it is open, and notes on tasks the records say are done or settle planned moves.

use ev_core::{Inventory, NewNode};
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

/// Two drawers in a Kallax, each a place of its own.
fn setup() -> (TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    add(&mut inv, "Ev", "home", None, None);
    add(&mut inv, "Oda", "room", Some("Ev"), None);
    add(&mut inv, "Kallax", "furniture", Some("Oda"), Some("K1"));
    add(&mut inv, "Üst", "container", Some("K1"), Some("K1-U"));
    add(&mut inv, "Alt", "container", Some("K1"), Some("K1-A"));
    (dir, inv)
}

fn day(offset: i64) -> String {
    (chrono::Utc::now().date_naive() + chrono::Duration::days(offset)).to_string()
}

#[test]
fn a_task_due_tomorrow_comes_before_the_order_and_a_bad_date_adds_nothing() {
    let (_d, mut inv) = setup();
    inv.task_add("Üstü say", "dağınık", &["K1-U".into()], None)
        .unwrap();
    let later = inv
        .task_add_with(
            "Anahtarı dene",
            "kapıda dene",
            &["K1-A".into()],
            None,
            Some(&day(1)),
        )
        .unwrap();
    assert_eq!(later["position"], 2);
    assert_eq!(later["days_left"], 1);
    let next = inv.next().unwrap();
    assert_eq!(next["task"]["title"], "Anahtarı dene");
    assert_eq!(next["task"]["picked"], "due");
    assert!(
        next["hints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["kind"] == "due" && h["days_left"] == 1)
    );
    // A date far off leaves the order alone.
    inv.task_due(later["id"].as_i64().unwrap(), Some(&day(30)))
        .unwrap();
    assert_eq!(inv.next().unwrap()["task"]["title"], "Üstü say");
    assert!(
        inv.task_add_with("Yanlış", "tarih", &[], None, Some("2026-13-40"))
            .is_err()
    );
    assert_eq!(
        inv.task_list(false).unwrap()["tasks"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn next_bundles_what_waits_in_the_task_place_and_nothing_from_elsewhere() {
    let (_d, mut inv) = setup();
    add(&mut inv, "Belirsiz kablo", "item", Some("K1-U"), None);
    add(&mut inv, "Kırık şarj aleti", "item", Some("K1-U"), None);
    add(&mut inv, "Pil", "item", Some("K1-U"), None);
    add(
        &mut inv,
        "Başka kutu",
        "container",
        Some("K1-A"),
        Some("GF-002"),
    );
    inv.dispose("Kırık şarj aleti", ev_core::Disposition::Trash)
        .unwrap();
    inv.move_to("Pil", "K1-A", true).unwrap();
    inv.task_add("Üstü say", "dağınık", &["K1-U".into()], None)
        .unwrap();
    let next = inv.next().unwrap();
    let w = &next["task"]["places"][0]["while_there"];
    let names = |key: &str, field: &str| -> Vec<String> {
        w[key]
            .as_array()
            .unwrap_or_else(|| panic!("no {key} in {w}"))
            .iter()
            .map(|e| {
                let n = if e["node"].is_object() { &e["node"] } else { e };
                n[field].as_str().unwrap_or_default().to_string()
            })
            .collect()
    };
    // The drawer's own label is printed while the drawer is open.
    assert_eq!(names("labels", "code"), ["K1-U"]);
    assert_eq!(names("unclear", "name"), ["Belirsiz kablo"]);
    assert_eq!(names("disposals", "name"), ["Kırık şarj aleti"]);
    assert_eq!(names("leaving", "name"), ["Pil"]);
    assert_eq!(names("photos", "code"), ["K1-U"]);
    assert!(!w.to_string().contains("GF-002"), "{w}");
}
