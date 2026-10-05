//! Past belongings (spec/past-belongings.md): a thing that left long ago, recorded as it is
//! remembered.

use ev_core::{Disposition, Inventory, NewNode};

fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent) in [
        ("Ev", "home", None),
        ("Oda", "room", Some("Ev")),
        ("Eski telefon", "item", Some("Oda")),
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

#[test]
fn a_thing_that_left_long_ago_keeps_when_and_where() {
    let (_d, mut inv) = setup();
    inv.gone_left(
        "Eski telefon",
        Some(Disposition::Left),
        None,
        false,
        None,
        Some("2016-06"),
        Some("Eski ev"),
    )
    .unwrap();
    let v = inv.show("Eski telefon", true).unwrap();
    assert_eq!(v["node"]["disposition"], "left");
    assert_eq!(v["departure"]["at"], "2016-06");
    assert_eq!(v["departure"]["where"], "Eski ev");
    // A date nobody could say, and a how nothing is set aside for, are refused.
    let (_d, mut inv) = setup();
    let bad = inv.gone_left(
        "Eski telefon",
        Some(Disposition::Sell),
        None,
        false,
        None,
        Some("2016-13"),
        None,
    );
    assert_eq!(bad.unwrap_err().code(), 2);
    assert_eq!(
        inv.dispose("Eski telefon", Disposition::Stolen)
            .unwrap_err()
            .code(),
        2
    );
    // With no date said, the day it was recorded stands for it.
    inv.gone_left(
        "Eski telefon",
        Some(Disposition::Unknown),
        None,
        false,
        None,
        None,
        None,
    )
    .unwrap();
    let v = inv.show("Eski telefon", true).unwrap();
    assert_eq!(v["departure"]["at"].as_str().unwrap().len(), 10);
}

#[test]
fn a_past_thing_is_added_already_gone_in_no_holder() {
    let (_d, mut inv) = setup();
    let v = inv
        .add(NewNode {
            name: "Oyun konsolu".into(),
            kind: "item".into(),
            gone: Some("sell".into()),
            at: Some("2016".into()),
            came: Some("2012-11".into()),
            place: Some("Eski ev".into()),
            ..Default::default()
        })
        .unwrap();
    let id = v["node"]["id"].as_i64().unwrap();
    let v = inv.show(&id.to_string(), true).unwrap();
    assert_eq!(v["node"]["state"], "gone");
    assert_eq!(v["node"]["disposition"], "sell");
    assert_eq!(v["came"], "2012-11");
    assert_eq!(v["departure"]["at"], "2016");
    assert_eq!(v["departure"]["where"], "Eski ev");
    // Never in the tree or the tour.
    let tree = inv.tree(None, None).unwrap().to_string();
    assert!(!tree.contains("Oyun konsolu"), "{tree}");
    // A past thing in a holder, or a when without a gone, is refused.
    let in_a_room = inv.add(NewNode {
        name: "Klavye".into(),
        kind: "item".into(),
        parent: Some("Oda".into()),
        gone: Some("sell".into()),
        ..Default::default()
    });
    assert_eq!(in_a_room.unwrap_err().code(), 2);
    let at_alone = inv.add(NewNode {
        name: "Klavye".into(),
        kind: "item".into(),
        parent: Some("Oda".into()),
        at: Some("2016".into()),
        ..Default::default()
    });
    assert_eq!(at_alone.unwrap_err().code(), 2);
}

#[test]
fn when_a_thing_came_is_edited_as_remembered() {
    let (_d, mut inv) = setup();
    inv.edit("Eski telefon", &["came=2014-03".into()]).unwrap();
    assert_eq!(inv.show("Eski telefon", false).unwrap()["came"], "2014-03");
    assert_eq!(
        inv.edit("Eski telefon", &["came=14 Mart".into()])
            .unwrap_err()
            .code(),
        2
    );
    inv.edit("Eski telefon", &["came=".into()]).unwrap();
    assert!(inv.show("Eski telefon", false).unwrap()["came"].is_null());
}
