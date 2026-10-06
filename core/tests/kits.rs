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
fn unlinking_a_record_that_is_not_linked_is_refused_as_elsewhere() {
    let (_d, mut inv) = setup();
    inv.kit_add("Set", None, None, &[("RC522 okuyucu".into(), 1)], None)
        .unwrap();
    let e = inv.kit_unlink("Set", 1, "RC522 okuyucu").unwrap_err();
    assert_eq!(e.code(), 5, "{e}");
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
            None,
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
    // A link answers with the part it changed and the kit's counts, not the whole list.
    assert_eq!(v["part"]["n"], 2);
    assert!(v.get("parts").is_none(), "{v}");
    let v = inv.kit_show("Proje seti").unwrap();
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
    inv.kit_add("Set", None, None, &[("Okuyucu".into(), 2)], None)
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
    assert_eq!(v["part"]["found"], 2);
    let v = inv.kit_unlink("Set", 1, "RC522 okuyucu").unwrap();
    assert_eq!(v["part"]["open"], 2);
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
    inv.kit_add("Set", None, None, &[("Okuyucu".into(), 1)], None)
        .unwrap();
    // One name, one kit (compared folded).
    assert_eq!(
        inv.kit_add("SET", None, None, &[], None)
            .unwrap_err()
            .code(),
        5
    );
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
    assert_eq!(v["added"][0]["n"], 2);
    // Removing a kit leaves the records.
    inv.kit_remove("Set").unwrap();
    assert!(inv.show("RC522 okuyucu", false).is_ok());
    assert_eq!(inv.kit_list().unwrap()["kits"].as_array().unwrap().len(), 0);
}

#[test]
fn a_part_that_left_the_home_no_longer_counts_as_found() {
    let (_d, mut inv) = setup();
    inv.kit_add("Set", None, None, &[("Kart".into(), 1)], None)
        .unwrap();
    let v = inv.kit_link("Set", 1, &["Beyaz kart".into()]).unwrap();
    assert_eq!(v["part"]["found"], 1);
    // Thrown out: the link stays in the history, but the part is missing again.
    inv.gone("Beyaz kart", Some(ev_core::Disposition::Trash))
        .unwrap();
    let v = inv.kit_show("Set").unwrap();
    assert_eq!(part(&v, 1)["found"], 0);
    assert_eq!(part(&v, 1)["lost"], 0);
    assert_eq!(part(&v, 1)["open"], 1);
}

#[test]
fn a_database_error_is_not_reported_as_a_name_already_taken() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ev.db");
    let mut inv = Inventory::open(&db).unwrap();
    // The kits table is gone from under it: an error of the database, not of the name.
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch("ALTER TABLE kits RENAME TO kits_away")
        .unwrap();
    let err = inv.kit_add("Set", None, None, &[], None).unwrap_err();
    assert_eq!(err.code(), 1, "{err}");
}

#[test]
fn linking_a_part_again_writes_no_second_history_event() {
    let (_d, mut inv) = setup();
    inv.kit_add("Set", None, None, &[("Okuyucu".into(), 2)], None)
        .unwrap();
    inv.kit_link("Set", 1, &["RC522 okuyucu".into()]).unwrap();
    inv.kit_link("Set", 1, &["RC522 okuyucu".into()]).unwrap();
    let events = inv.history("RC522 okuyucu").unwrap()["events"].clone();
    let links = events
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "kit_link")
        .count();
    assert_eq!(links, 1);
}

/// The set's purchase line, imported as an adapter gives it; its id.
fn set_line(inv: &mut Inventory) -> i64 {
    inv.buy_import(
        &serde_json::json!({"type": "purchase", "source": "shop", "key": "o1:set",
            "shop": "Shop", "name": "RFID proje seti, 3 parça", "qty": 1,
            "paid": "999.00", "currency": "TRY"})
        .to_string(),
    )
    .unwrap();
    inv.buy_list(false, None, None, None).unwrap()["purchases"][0]["id"]
        .as_i64()
        .unwrap()
}

#[test]
fn a_kit_bought_as_one_line_settles_it_and_its_parts_see_it() {
    let (_d, mut inv) = setup();
    let line = set_line(&mut inv);
    inv.kit_add("Set", None, None, &[("Okuyucu".into(), 2)], Some(line))
        .unwrap();
    inv.kit_link("Set", 1, &["RC522 okuyucu".into()]).unwrap();
    let open = inv.buy_list(true, None, None, None).unwrap();
    assert!(open["purchases"].as_array().unwrap().is_empty(), "{open}");
    let shown = inv.buy_show(line).unwrap();
    assert_eq!(shown["purchase"]["kits"][0], "Set");
    let v = inv.show("RC522 okuyucu", false).unwrap();
    assert_eq!(v["purchases"][0]["id"], line);
    assert_eq!(v["purchases"][0]["kit"], "Set");
    // Cleared, the line is open again and the part has no purchase.
    inv.kit_purchase("Set", None).unwrap();
    assert_eq!(inv.buy_show(line).unwrap()["purchase"]["open_qty"], 1);
    let v = inv.show("RC522 okuyucu", false).unwrap();
    assert!(v["purchases"].as_array().unwrap().is_empty(), "{v}");
}

#[test]
fn a_kit_shows_its_lines_quantity() {
    let (_d, mut inv) = setup();
    inv.buy_import(
        &serde_json::json!({"type": "purchase", "source": "shop", "key": "o2:set",
            "shop": "Shop", "name": "Proje seti", "qty": 2,
            "paid": "1999.00", "currency": "TRY"})
        .to_string(),
    )
    .unwrap();
    let line = inv.buy_list(false, None, None, None).unwrap()["purchases"][0]["id"]
        .as_i64()
        .unwrap();
    inv.kit_add("Set", Some(2), None, &[("Okuyucu".into(), 1)], None)
        .unwrap();
    let v = inv.kit_purchase("Set", Some(line)).unwrap();
    assert_eq!(v["kit"]["purchase"]["qty"], 2, "{}", v["kit"]);
}

#[test]
fn a_part_of_a_kit_bought_as_one_line_is_asked_no_purchase() {
    let (_d, mut inv) = setup();
    let line = set_line(&mut inv);
    inv.kit_add("Set", None, None, &[("Okuyucu".into(), 2)], None)
        .unwrap();
    inv.kit_link("Set", 1, &["RC522 okuyucu".into()]).unwrap();
    // Set afterwards: every record already linked hears of it in its history.
    inv.kit_purchase("Set", Some(line)).unwrap();
    let v = inv.edit("RC522 okuyucu", &["model=RC522".into()]).unwrap();
    assert!(v.get("purchase_candidates").is_none(), "{v}");
    let h = inv.history("RC522 okuyucu").unwrap();
    assert!(
        h["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "kit_purchase"),
        "{h}"
    );
}

#[test]
fn adding_parts_answers_with_the_new_parts_only() {
    let (_d, mut inv) = setup();
    inv.kit_add("Set", None, None, &[("RC522 okuyucu".into(), 1)], None)
        .unwrap();
    let v = inv
        .kit_parts_add("Set", &[("Kablo".into(), 3), ("Kart".into(), 2)])
        .unwrap();
    let added: Vec<(i64, &str)> = v["added"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["n"].as_i64().unwrap(), p["text"].as_str().unwrap()))
        .collect();
    assert_eq!(added, [(2, "Kablo"), (3, "Kart")]);
    assert!(v.get("parts").is_none(), "{v}");
    assert_eq!(v["counts"]["expected"], 6);
}

#[test]
fn a_part_entered_by_mistake_is_dropped_once_nothing_is_linked_to_it() {
    let (_d, mut inv) = setup();
    inv.kit_add(
        "Set",
        None,
        None,
        &[("RC522 okuyucu".into(), 1), ("Yanlış".into(), 2)],
        None,
    )
    .unwrap();
    inv.kit_link("Set", 1, &["RC522 okuyucu".into()]).unwrap();
    // A part with records linked to it is not dropped from under them.
    assert_eq!(inv.kit_part_drop("Set", 1).unwrap_err().code(), 5);
    let v = inv.kit_part_drop("Set", 2).unwrap();
    assert_eq!(v["dropped"]["text"], "Yanlış");
    let parts = inv.kit_show("Set").unwrap()["parts"].clone();
    assert_eq!(parts.as_array().unwrap().len(), 1);
    assert_eq!(parts[0]["n"], 1);
    assert_eq!(inv.kit_part_drop("Set", 2).unwrap_err().code(), 3);
}

#[test]
fn a_renamed_part_keeps_its_number_and_its_records() {
    let (_d, mut inv) = setup();
    inv.kit_add("Set", None, None, &[("Okuyucu".into(), 1)], None)
        .unwrap();
    inv.kit_link("Set", 1, &["RC522 okuyucu".into()]).unwrap();
    let v = inv.kit_part_set("Set", 1, "RC522 okuyucu", 2).unwrap();
    assert_eq!(v["part"]["n"], 1);
    assert_eq!(v["part"]["text"], "RC522 okuyucu");
    assert_eq!(v["part"]["expected"], 2);
    assert_eq!(v["part"]["found"], 2);
    assert_eq!(inv.kit_part_set("Set", 1, " ", 1).unwrap_err().code(), 2);
}

#[test]
fn a_missing_part_after_a_drop_says_how_far_the_numbers_go() {
    let (_d, mut inv) = setup();
    inv.kit_add(
        "Set",
        None,
        None,
        &[("A".into(), 1), ("B".into(), 1), ("C".into(), 1)],
        None,
    )
    .unwrap();
    inv.kit_part_drop("Set", 2).unwrap();
    let e = inv
        .kit_link("Set", 7, &["RC522 okuyucu".into()])
        .unwrap_err();
    assert!(e.to_string().contains("2 parts, numbered up to 3"), "{e}");
}

#[test]
fn a_kit_bought_as_one_line_is_no_open_purchase_in_todo_or_stats() {
    let (_d, mut inv) = setup();
    let line = set_line(&mut inv);
    assert_eq!(inv.todo().unwrap()["counts"]["purchases"], 1);
    inv.kit_add("Set", None, None, &[("Okuyucu".into(), 2)], Some(line))
        .unwrap();
    assert_eq!(inv.todo().unwrap()["counts"]["purchases"], 0);
    assert_eq!(inv.stats().unwrap()["purchases"]["open_durable"], 0);
}
