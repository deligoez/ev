//! What a thing is worth (purchases spec §3.3) and its links (§3.4).

use ev_core::{Inventory, NewNode, NewValuation};
use serde_json::json;

fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent) in [
        ("Ev", "home", None),
        ("Oda", "room", Some("Ev")),
        ("Matkap", "item", Some("Oda")),
    ] {
        inv.add(NewNode {
            name: name.into(),
            kind: kind.into(),
            parent: parent.map(Into::into),
            ..Default::default()
        })
        .unwrap();
    }
    (dir, inv)
}

fn worth(amount: &str, at: &str) -> NewValuation {
    NewValuation {
        amount: amount.into(),
        at: Some(at.into()),
        source: Some("ilan".into()),
        ..Default::default()
    }
}

#[test]
fn the_latest_observation_is_the_value_and_older_ones_stay() {
    let (_d, mut inv) = setup();
    inv.value("Matkap", Some(&worth("2.500,00", "2025-03-01")))
        .unwrap();
    inv.value("Matkap", Some(&worth("3000", "2026-09-20")))
        .unwrap();
    let show = inv.show("Matkap", false).unwrap();
    let vals = show["valuations"].as_array().unwrap();
    assert_eq!(vals.len(), 2);
    assert_eq!(vals[0]["amount"], "3000.00");
    assert_eq!(vals[0]["currency"], "TRY");
    assert_eq!(vals[1]["amount"], "2500.00");
}

#[test]
fn a_value_answers_a_closed_value_question() {
    let (_d, mut inv) = setup();
    inv.track("Matkap", "value", "no", Some("eski")).unwrap();
    assert!(inv.show("Matkap", false).unwrap()["tracking"]["value"].is_object());
    inv.value("Matkap", Some(&worth("900", "2026-09-20")))
        .unwrap();
    assert!(inv.show("Matkap", false).unwrap()["tracking"]["value"].is_null());
}

#[test]
fn a_bought_thing_with_no_value_is_counted_until_one_is_recorded() {
    let (_d, mut inv) = setup();
    inv.buy_add(
        &json!({"name": "Bosch GSB 13 RE", "ordered_at": "2024-05-01", "paid": "1999"}),
        Some("Matkap"),
    )
    .unwrap();
    let todo = inv.todo().unwrap();
    assert_eq!(todo["counts"]["values"], 1);
    assert_eq!(todo["values"]["top"][0]["name"], "Matkap");
    inv.value("Matkap", Some(&worth("1500", "2026-09-20")))
        .unwrap();
    assert_eq!(inv.todo().unwrap()["counts"]["values"], 0);
}

#[test]
fn a_link_keeps_its_archive_and_the_same_address_is_updated_not_added() {
    let (d, mut inv) = setup();
    let page = d.path().join("page.html");
    std::fs::write(&page, "<html>GSB 13 RE</html>").unwrap();
    let url = "https://example.com/gsb-13-re";
    inv.link_add("Matkap", url, "info", None, None).unwrap();
    let v = inv
        .link_add("Matkap", url, "manual", page.to_str(), Some("kılavuz"))
        .unwrap();
    let links = v["links"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["kind"], "manual");
    let archive = links[0]["archive"].as_str().unwrap();
    assert_eq!(
        std::fs::read_to_string(archive).unwrap(),
        "<html>GSB 13 RE</html>"
    );
    assert!(
        inv.link_add("Matkap", "ftp://x", "info", None, None)
            .is_err()
    );
}
