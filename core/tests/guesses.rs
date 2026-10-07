//! What is only a guess (spec/guesses.md): `ev guess`.

use ev_core::{Inventory, NewNode};

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
        node("Dolap", "container", Some("Oda")),
        node("Mini bilgisayar", "item", Some("Dolap")),
    ] {
        inv.add(n).unwrap();
    }
    (dir, inv)
}

#[test]
fn a_record_known_from_words_is_found_shown_and_listed_as_a_guess() {
    let (_d, mut inv) = setup();
    inv.guess(
        &["Mini bilgisayar".into()],
        &[],
        Some("Ayşe söyledi, görülmedi"),
        false,
    )
    .unwrap();
    let shown = inv.show("Mini bilgisayar", false).unwrap();
    assert_eq!(shown["guess"]["note"], "Ayşe söyledi, görülmedi");
    let found = inv.find("mini bilgisayar", None, None, false).unwrap();
    assert_eq!(found["results"][0]["guess"], true);
    let todo = inv.todo().unwrap();
    assert_eq!(todo["counts"]["guesses"], 1);
    assert_eq!(todo["guesses"][0]["name"], "Mini bilgisayar");
    // Seen: the guess is cleared and the record stays.
    inv.guess(&["Mini bilgisayar".into()], &[], None, true)
        .unwrap();
    assert!(inv.show("Mini bilgisayar", false).unwrap()["guess"].is_null());
    assert_eq!(inv.todo().unwrap()["counts"]["guesses"], 0);
}

#[test]
fn a_field_said_again_is_no_longer_a_guess() {
    let (_d, mut inv) = setup();
    inv.edit("Mini bilgisayar", &["came=2021".into()]).unwrap();
    inv.guess(
        &["Mini bilgisayar".into()],
        &["came".into()],
        Some("kutusundan tahmin"),
        false,
    )
    .unwrap();
    let shown = inv.show("Mini bilgisayar", false).unwrap();
    assert_eq!(shown["guessed"][0]["field"], "came");
    assert!(shown["guess"].is_null());
    // The person says the year: the field is theirs now.
    inv.edit("Mini bilgisayar", &["came=2022".into()]).unwrap();
    let shown = inv.show("Mini bilgisayar", false).unwrap();
    assert_eq!(shown["guessed"], serde_json::json!([]));
    // A field that cannot be a guess is refused, naming those that can.
    let e = inv
        .guess(&["Mini bilgisayar".into()], &["colour".into()], None, false)
        .unwrap_err();
    assert_eq!(e.id(), Some("guess_field_unknown"));
}

#[test]
fn a_coverage_end_assumed_says_so_until_it_is_said() {
    let (_d, mut inv) = setup();
    let v = inv
        .cover_add(
            &["Mini bilgisayar".into()],
            &ev_core::NewCoverage {
                kind: "manufacturer".into(),
                from: Some("2025-01-10".into()),
                term: Some("1y".into()),
                ..Default::default()
            },
        )
        .unwrap();
    let id = v["coverage"]["id"]
        .as_i64()
        .unwrap_or_else(|| panic!("{v}"));
    inv.guess(
        &["Mini bilgisayar".into()],
        &[format!("cover:{id}")],
        Some("bir yıl varsayıldı"),
        false,
    )
    .unwrap();
    let list = inv.cover_list(false).unwrap();
    let cv = list["coverages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap()
        .clone();
    assert_eq!(cv["end_guessed"], true);
    assert_eq!(cv["guess_note"], "bir yıl varsayıldı");
    // The certificate read: its end is said, and no longer a guess.
    inv.cover_edit(id, &[], &[], &["ends=2026-01-10".into()])
        .unwrap();
    let list = inv.cover_list(false).unwrap();
    assert!(
        list["coverages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["end_guessed"].is_null())
    );
    // A coverage of another record is not one of this one's.
    let e = inv
        .guess(&["Dolap".into()], &[format!("cover:{id}")], None, false)
        .unwrap_err();
    assert_eq!(e.id(), Some("guess_cover_not_its"));
}
