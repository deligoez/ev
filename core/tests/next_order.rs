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
