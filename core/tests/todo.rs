//! Spec §18: labels, broken things, use-by dates, sales, needs, and `todo` gathering them all.

use ev_core::{Disposition, Inventory, NewNode};
use serde_json::Value;
use tempfile::TempDir;

fn add(
    inv: &mut Inventory,
    name: &str,
    kind: &str,
    parent: Option<&str>,
    code: Option<&str>,
) -> i64 {
    inv.add(NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: parent.map(Into::into),
        code: code.map(Into::into),
        ..Default::default()
    })
    .unwrap()["node"]["id"]
        .as_i64()
        .unwrap()
}

fn setup() -> (TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    add(&mut inv, "Ev", "home", None, None);
    add(&mut inv, "Oda", "room", Some("Ev"), None);
    add(&mut inv, "Samla", "container", Some("Oda"), Some("S5-01"));
    add(&mut inv, "Silikon", "item", Some("S5-01"), None);
    add(&mut inv, "Kulaklık", "item", Some("S5-01"), None);
    (dir, inv)
}

fn names(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|n| n["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_new_code_needs_a_label_until_it_is_printed() {
    let (_d, mut inv) = setup();
    let v = inv.label(&[], true).unwrap();
    assert_eq!(names(&v["labels"]), ["Samla"]);
    let v = inv.label(&["S5-01".into()], true).unwrap();
    assert!(v["labels"].as_array().unwrap().is_empty());
    // Changing the code makes the old label wrong.
    inv.edit("S5-01", &["code=S5-09".into()]).unwrap();
    assert_eq!(names(&inv.label(&[], true).unwrap()["labels"]), ["Samla"]);
    inv.edit("S5-09", &["code=".into()]).unwrap();
    assert!(
        inv.label(&[], true).unwrap()["labels"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(inv.label(&["Silikon".into()], true).unwrap_err().code(), 5);
}

