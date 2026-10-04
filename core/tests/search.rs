//! Free-text search (`ev find`, the UI's `/`).

use ev_core::{Inventory, NewNode};

/// A box of LEDs and a few things that only look alike.
fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    let node = |name: &str, kind: &str, parent: Option<&str>| NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: parent.map(Into::into),
        ..Default::default()
    };
    for n in [
        node("Ev", "home", None),
        node("Oda", "room", Some("Ev")),
        node("LED kutusu", "container", Some("Oda")),
        node("Kırmızı LED, 5 mm", "item", Some("LED kutusu")),
        node("Yeşil LED, 5 mm", "item", Some("LED kutusu")),
        node("Tablo", "item", Some("Oda")),
        node("LDR modülü", "item", Some("Oda")),
        NewNode {
            note: Some("içinde kırmızı kablo var".into()),
            ..node("Kablo çantası", "container", Some("Oda"))
        },
        node("Kırmızı kablo", "item", Some("Oda")),
    ] {
        inv.add(n).unwrap();
    }
    (dir, inv)
}

fn names(inv: &Inventory, q: &str) -> Vec<String> {
    inv.find(q, None, None, false).unwrap()["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn every_word_must_match_but_in_any_order() {
    let (_d, inv) = setup();
    assert_eq!(names(&inv, "led kirmizi"), ["Kırmızı LED, 5 mm"]);
    assert_eq!(names(&inv, "kırmızı led"), ["Kırmızı LED, 5 mm"]);
}

#[test]
fn a_word_with_a_turkish_ending_finds_its_stem() {
    let (_d, inv) = setup();
    assert_eq!(names(&inv, "kırmızıları led"), ["Kırmızı LED, 5 mm"]);
}

#[test]
fn a_typo_is_forgiven_only_when_the_word_meets_nothing_as_written() {
    let (_d, inv) = setup();
    assert_eq!(names(&inv, "kirmzi led"), ["Kırmızı LED, 5 mm"]);
    // "kablo" is written correctly, so "Tablo" one letter away is not dragged in.
    assert!(!names(&inv, "kablo").contains(&"Tablo".to_string()));
}

#[test]
fn a_synonym_group_finds_the_other_words() {
    let (_d, mut inv) = setup();
    inv.synonym_add("ldr, fotodirenç").unwrap();
    assert_eq!(names(&inv, "fotodirenc"), ["LDR modülü"]);
}

#[test]
fn a_match_in_the_name_ranks_above_one_in_the_note() {
    let (_d, inv) = setup();
    assert_eq!(
        names(&inv, "kırmızı kablo"),
        ["Kırmızı kablo", "Kablo çantası"]
    );
}

#[test]
fn empty_lists_the_containers_known_to_hold_nothing_and_follows_what_moves() {
    let (_d, mut inv) = setup();
    let list = |inv: &Inventory, key: &str| -> Vec<String> {
        inv.find_with("", None, None, false, true).unwrap()[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["name"].as_str().unwrap().to_string())
            .collect()
    };
    // Nothing was ever recorded in the bag and its room was never counted: not known to be
    // empty, only never looked into.
    assert!(list(&inv, "results").is_empty());
    assert_eq!(list(&inv, "not_known"), ["Kablo çantası"]);
    inv.move_to("Kırmızı kablo", "Kablo çantası", false)
        .unwrap();
    assert!(list(&inv, "not_known").is_empty(), "something is in it now");
    inv.gone("Kırmızı kablo", Some(ev_core::Disposition::Trash))
        .unwrap();
    // What was in it left: now it is known to be empty.
    assert_eq!(list(&inv, "results"), ["Kablo çantası"]);
}

#[test]
fn siblings_come_by_kind_then_by_code_read_naturally_then_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    let node = |name: &str, kind: &str, parent: Option<&str>, code: Option<&str>| NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: parent.map(Into::into),
        code: code.map(Into::into),
        ..Default::default()
    };
    for n in [
        node("Ev", "home", None, None),
        node("Oda", "room", Some("Ev"), None),
        node("Kutu", "container", Some("Oda"), Some("S5-2")),
        node("Kalem", "item", Some("Oda"), None),
        node("Kutu", "container", Some("Oda"), Some("S45-1")),
        // Recorded between the boxes of one series: it must not split them.
        node("Masa", "furniture", Some("Oda"), None),
        node("Kutu", "container", Some("Oda"), Some("S5-10")),
        node("Kutu", "container", Some("Oda"), None),
    ] {
        inv.add(n).unwrap();
    }
    let v = inv.show("Oda", false).unwrap();
    let order: Vec<String> = v["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            c["code"]
                .as_str()
                .unwrap_or(c["name"].as_str().unwrap())
                .to_string()
        })
        .collect();
    assert_eq!(order, ["Masa", "S5-2", "S5-10", "S45-1", "Kutu", "Kalem"]);
}

#[test]
fn a_box_the_person_calls_empty_is_known_empty_and_one_with_things_is_refused() {
    let (_d, mut inv) = setup();
    let results = |inv: &Inventory| -> Vec<String> {
        inv.find_with("", None, None, false, true).unwrap()["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert!(results(&inv).is_empty(), "never counted, never said");
    inv.mark_empty(&["Kablo çantası".into()], Some("açtık, boş"))
        .unwrap();
    assert_eq!(results(&inv), ["Kablo çantası"]);
    // A box with records in it is not said to be empty, nor is a thing that is no box.
    assert!(inv.mark_empty(&["LED kutusu".into()], None).is_err());
    assert!(inv.mark_empty(&["Tablo".into()], None).is_err());
}

#[test]
fn a_box_called_empty_is_counted_and_waits_in_no_tour() {
    let (_d, mut inv) = setup();
    inv.mark_empty(&["Kablo çantası".into()], Some("açtık, boş"))
        .unwrap();
    let v = inv.show("Kablo çantası", false).unwrap();
    assert_eq!(v["review"]["status"], "toured", "{}", v["review"]);
    assert_eq!(v["review"]["note"], "açtık, boş");
}

#[test]
fn a_lost_thing_last_seen_in_a_box_does_not_keep_it_from_being_empty() {
    let (_d, mut inv) = setup();
    // Lost from the start: recorded with the box as where it was last seen.
    inv.add(NewNode {
        name: "Güneş gözlüğü".into(),
        kind: "item".into(),
        parent: Some("Kablo çantası".into()),
        lost: true,
        ..Default::default()
    })
    .unwrap();
    let v = inv
        .mark_empty(&["Kablo çantası".into()], Some("boşaldı"))
        .unwrap();
    assert_eq!(v["empty"][0]["name"], "Kablo çantası");
    let found = inv.find_with("", None, None, false, true).unwrap();
    assert!(
        found["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["name"] == "Kablo çantası"),
        "{found}"
    );
    // Toured with no photo: there is nothing in it to show.
    inv.review("Kablo çantası", "toured", None).unwrap();
}

#[test]
fn a_box_set_back_to_raw_is_no_longer_known_empty() {
    let (_d, mut inv) = setup();
    inv.mark_empty(&["Kablo çantası".into()], None).unwrap();
    let names = |inv: &Inventory, key: &str| -> Vec<String> {
        inv.find_with("", None, None, false, true).unwrap()[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert!(names(&inv, "results").contains(&"Kablo çantası".to_string()));
    // Called empty a second ago, it turns out full of things nobody counted.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    inv.review("Kablo çantası", "raw", Some("içi dolu, sayılmadı"))
        .unwrap();
    assert!(!names(&inv, "results").contains(&"Kablo çantası".to_string()));
    assert!(names(&inv, "not_known").contains(&"Kablo çantası".to_string()));
}
