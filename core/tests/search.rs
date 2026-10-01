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
