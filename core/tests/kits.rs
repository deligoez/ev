//! Spec §29: kits — what a bought set should contain, and which records are its parts.

use ev_core::{Inventory, NewNode};
use serde_json::Value;

fn item(inv: &mut Inventory, name: &str, qty: i64) {
    inv.add(NewNode {
        name: name.into(),
        kind: "item".into(),
        parent: Some("K".into()),
        qty: Some(qty),
        ..Default::default()
    })
    .unwrap();
}

/// A box holding two RFID readers, two cards and a card recorded as lost.
fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent, code) in [
        ("Ev", "home", None, None),
        ("Oda", "room", Some("Ev"), None),
        ("Kutu", "container", Some("Oda"), Some("K")),
    ] {
        inv.add(NewNode {
            name: name.into(),
            kind: kind.into(),
            parent: parent.map(Into::into),
            code: code.map(Into::into),
            ..Default::default()
        })
        .unwrap();
    }
    item(&mut inv, "RC522 okuyucu", 2);
    item(&mut inv, "Beyaz kart", 1);
    item(&mut inv, "Beyaz kart (eksik)", 1);
    inv.mark_lost("Beyaz kart (eksik)").unwrap();
    (dir, inv)
}

fn part(v: &Value, n: usize) -> &Value {
    &v["parts"][n - 1]
}

#[test]
fn a_kit_counts_each_part_found_lost_and_still_missing_across_its_copies() {
    let (_d, mut inv) = setup();
    let v = inv
        .kit_add(
            "Proje seti",
            Some(2),
            None,
            &[
                ("RC522 okuyucu".into(), 1),
                ("Beyaz kart".into(), 1),
                ("Step motor 28BYJ-48".into(), 1),
            ],
        )
        .unwrap();
    // Nothing linked yet: everything is still missing, two of each for two copies.
    assert_eq!(v["counts"]["expected"], 6);
    assert_eq!(v["counts"]["open"], 6);
    inv.kit_link("Proje seti", 1, &["RC522 okuyucu".into()])
        .unwrap();
    let v = inv
        .kit_link(
            "proje SETI",
            2,
            &["Beyaz kart".into(), "Beyaz kart (eksik)".into()],
        )
        .unwrap();
    // A record's count is how many it stands for; a lost one is neither found nor missing.
    assert_eq!(part(&v, 1)["found"], 2);
    assert_eq!(part(&v, 1)["open"], 0);
    assert_eq!(part(&v, 2)["found"], 1);
    assert_eq!(part(&v, 2)["lost"], 1);
    assert_eq!(part(&v, 2)["open"], 0);
    assert_eq!(part(&v, 3)["open"], 2);
    assert_eq!(part(&v, 2)["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(v["counts"]["found"], 3);
    assert_eq!(v["counts"]["lost"], 1);
    assert_eq!(v["counts"]["open"], 2);
    // Found where it belongs: the lost card no longer counts as lost.
    inv.found("Beyaz kart (eksik)").unwrap();
    let v = inv.kit_show("Proje seti").unwrap();
    assert_eq!(part(&v, 2)["found"], 2);
    assert_eq!(part(&v, 2)["lost"], 0);
    let list = inv.kit_list().unwrap();
    assert_eq!(list["kits"][0]["counts"]["open"], 2);
}

#[test]
fn a_link_is_in_the_records_history_and_details_and_can_be_undone() {
    let (_d, mut inv) = setup();
    inv.kit_add("Set", None, None, &[("Okuyucu".into(), 2)])
        .unwrap();
    inv.kit_link("Set", 1, &["RC522 okuyucu".into()]).unwrap();
    let shown = inv.show("RC522 okuyucu", false).unwrap();
    assert_eq!(shown["kits"][0]["kit"], "Set");
    assert_eq!(shown["kits"][0]["n"], 1);
    let events = inv.history("RC522 okuyucu").unwrap()["events"].clone();
    assert!(
        events
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "kit_link" && e["data"]["text"] == "Okuyucu")
    );
    // Linking the same record twice changes nothing.
    let v = inv.kit_link("Set", 1, &["RC522 okuyucu".into()]).unwrap();
    assert_eq!(part(&v, 1)["found"], 2);
    let v = inv.kit_unlink("Set", 1, "RC522 okuyucu").unwrap();
    assert_eq!(part(&v, 1)["open"], 2);
    assert!(
        inv.show("RC522 okuyucu", false).unwrap()["kits"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let events = inv.history("RC522 okuyucu").unwrap()["events"].clone();
    assert_eq!(
        events.as_array().unwrap().last().unwrap()["type"],
        "kit_unlink"
    );
}

#[test]
fn kits_refuse_what_they_cannot_mean() {
    let (_d, mut inv) = setup();
    inv.kit_add("Set", None, None, &[("Okuyucu".into(), 1)])
        .unwrap();
    // One name, one kit (compared folded).
    assert_eq!(inv.kit_add("SET", None, None, &[]).unwrap_err().code(), 2);
    assert_eq!(inv.kit_show("Yok").unwrap_err().code(), 3);
    assert_eq!(
        inv.kit_link("Set", 9, &["RC522 okuyucu".into()])
            .unwrap_err()
            .code(),
        3
    );
    // A typo among the records links none of them.
    assert_eq!(
        inv.kit_link("Set", 1, &["RC522 okuyucu".into(), "Yok".into()])
            .unwrap_err()
            .code(),
        3
    );
    assert_eq!(inv.kit_show("Set").unwrap()["parts"][0]["found"], 0);
    // Parts come at least once; the list numbers on.
    assert_eq!(
        inv.kit_parts_add("Set", &[("Kart".into(), 0)])
            .unwrap_err()
            .code(),
        2
    );
    let v = inv.kit_parts_add("Set", &[("Kart".into(), 1)]).unwrap();
    assert_eq!(v["parts"][1]["n"], 2);
    // Removing a kit leaves the records.
    inv.kit_remove("Set").unwrap();
    assert!(inv.show("RC522 okuyucu", false).is_ok());
    assert_eq!(inv.kit_list().unwrap()["kits"].as_array().unwrap().len(), 0);
}

#[test]
fn a_part_that_left_the_home_no_longer_counts_as_found() {
    let (_d, mut inv) = setup();
    inv.kit_add("Set", None, None, &[("Kart".into(), 1)])
        .unwrap();
    let v = inv.kit_link("Set", 1, &["Beyaz kart".into()]).unwrap();
    assert_eq!(part(&v, 1)["found"], 1);
    // Thrown out: the link stays in the history, but the part is missing again.
    inv.gone("Beyaz kart", Some(ev_core::Disposition::Trash))
        .unwrap();
    let v = inv.kit_show("Set").unwrap();
    assert_eq!(part(&v, 1)["found"], 0);
    assert_eq!(part(&v, 1)["lost"], 0);
    assert_eq!(part(&v, 1)["open"], 1);
}
