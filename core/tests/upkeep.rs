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
