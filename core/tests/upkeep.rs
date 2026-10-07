//! Repairs and maintenance (spec/repairs.md).

use ev_core::{Inventory, NewNode, NewUpkeep};

fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    inv.add(NewNode {
        name: "Aile arabası".into(),
        kind: "vehicle".into(),
        code: Some("34 ABC 123".into()),
        ..Default::default()
    })
    .unwrap();
    (dir, inv)
}

fn work(kind: &str, what: &str) -> NewUpkeep {
    NewUpkeep {
        kind: kind.into(),
        work: what.into(),
        ..Default::default()
    }
}

#[test]
fn a_service_is_recorded_on_the_thing_and_shown_newest_first() {
    let (_d, mut inv) = setup();
    inv.upkeep_add(
        "34 ABC 123",
        &NewUpkeep {
            at: Some("2024-04".into()),
            km: Some(60000),
            by: Some("Servis A".into()),
            ..work("service", "yağ değişimi")
        },
    )
    .unwrap();
    inv.upkeep_add(
        "34 ABC 123",
        &NewUpkeep {
            at: Some("2025-04-10".into()),
            km: Some(75000),
            ..work("service", "yağ ve filtre")
        },
    )
    .unwrap();
    let shown = inv.show("34 ABC 123", false).unwrap();
    let list = shown["upkeep"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["work"], "yağ ve filtre");
    assert_eq!(list[1]["by"], "Servis A");
    // A kind ev does not know is refused, naming those it does.
    let e = inv
        .upkeep_add("34 ABC 123", &work("wash", "yıkama"))
        .unwrap_err();
    assert_eq!(e.id(), Some("upkeep_kind_unknown"));
}

#[test]
fn work_is_due_by_date_or_by_odometer_and_a_newer_one_of_its_kind_takes_over() {
    let (_d, mut inv) = setup();
    let soon = (chrono::Local::now().date_naive() + chrono::Duration::days(20))
        .format("%Y-%m-%d")
        .to_string();
    let later = (chrono::Local::now().date_naive() + chrono::Duration::days(400))
        .format("%Y-%m-%d")
        .to_string();
    // Due by date in 20 days.
    inv.upkeep_add(
        "34 ABC 123",
        &NewUpkeep {
            at: Some("2025-01".into()),
            next_at: Some(soon),
            ..work("inspection", "muayene")
        },
    )
    .unwrap();
    // Due by odometer: 500 km left from the last reading.
    inv.upkeep_add(
        "34 ABC 123",
        &NewUpkeep {
            at: Some("2025-05".into()),
            km: Some(80000),
            next_km: Some(80500),
            next_at: Some(later.clone()),
            ..work("service", "yağ değişimi")
        },
    )
    .unwrap();
    let due = inv.upkeep_list(None, true).unwrap()["upkeep"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(due.len(), 2, "{due:?}");
    let service = due.iter().find(|u| u["kind"] == "service").unwrap();
    assert_eq!(service["km_left"], 500);
    assert_eq!(inv.todo().unwrap()["counts"]["upkeep_due"], 2);
    // The next inspection is done: its own next date is far, so nothing of that kind is due.
    inv.upkeep_add(
        "34 ABC 123",
        &NewUpkeep {
            next_at: Some(later),
            ..work("inspection", "muayene")
        },
    )
    .unwrap();
    let due = inv.upkeep_list(None, true).unwrap()["upkeep"]
        .as_array()
        .unwrap()
        .clone();
    assert!(due.iter().all(|u| u["kind"] != "inspection"), "{due:?}");
}

#[test]
fn a_first_due_with_nothing_done_is_listed_until_work_of_its_kind_comes() {
    let (_d, mut inv) = setup();
    let soon = (chrono::Local::now().date_naive() + chrono::Duration::days(30))
        .format("%Y-%m")
        .to_string();
    let v = inv
        .upkeep_first_due(
            "34 ABC 123",
            "inspection",
            Some(&soon),
            None,
            Some("ilk muayene"),
        )
        .unwrap();
    assert!(v["upkeep"]["work"].is_null() && v["upkeep"]["at"].is_null());
    let due = inv.upkeep_list(None, true).unwrap();
    assert_eq!(due["upkeep"].as_array().unwrap().len(), 1, "{due}");
    // The inspection done: the first due date is answered and goes.
    inv.upkeep_add("34 ABC 123", &work("inspection", "muayene geçti"))
        .unwrap();
    let all = inv.upkeep_list(Some("34 ABC 123"), false).unwrap();
    let list = all["upkeep"].as_array().unwrap();
    assert_eq!(list.len(), 1, "{all}");
    assert_eq!(list[0]["work"], "muayene geçti");
    // Saying neither a date nor an odometer says nothing.
    let e = inv
        .upkeep_first_due("34 ABC 123", "service", None, None, None)
        .unwrap_err();
    assert_eq!(e.id(), Some("upkeep_due_needs_when"));
}
