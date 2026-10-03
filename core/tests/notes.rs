//! A note is a log the person adds to: `note=+text` appends, `note=text` replaces.

use ev_core::{Inventory, NewNode};
use tempfile::TempDir;

fn inv() -> (TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    (dir, inv)
}

#[test]
fn a_plus_note_appends_on_a_new_line_and_a_plain_one_replaces() {
    let (_d, mut inv) = inv();
    let v = inv
        .add(NewNode {
            name: "Ev".into(),
            kind: "home".into(),
            ..Default::default()
        })
        .unwrap();
    let id = v["node"]["id"].as_i64().unwrap().to_string();
    let v = inv.edit(&id, &["note=+first".into()]).unwrap();
    assert_eq!(v["changed"]["note"]["after"], "first");
    let v = inv.edit(&id, &["note=+ second".into()]).unwrap();
    assert_eq!(v["changed"]["note"]["before"], "first");
    assert_eq!(v["changed"]["note"]["after"], "first\nsecond");
    let v = inv.edit(&id, &["note=fresh".into()]).unwrap();
    assert_eq!(v["changed"]["note"]["after"], "fresh");
    assert!(inv.edit(&id, &["note=+  ".into()]).is_err());
}
