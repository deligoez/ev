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

#[test]
fn broken_expiring_and_needs_show_on_the_node_and_in_todo() {
    let (_d, mut inv) = setup();
    inv.broken("Kulaklık", Some("sol taraf ses vermiyor"), false)
        .unwrap();
    inv.expires("Silikon", Some("2000-07")).unwrap();
    let v = inv
        .need_add("Samla 5 L", Some(2), false, Some("Oda"), None)
        .unwrap();
    let need = v["id"].as_i64().unwrap();
    let show = inv.show("Silikon", false).unwrap();
    assert_eq!(show["marks"]["expires"]["value"], "2000-07-31");
    assert_eq!(
        inv.show("Oda", false).unwrap()["needs"][0]["text"],
        "Samla 5 L"
    );

    let t = inv.todo().unwrap();
    assert_eq!(names(&t["repairs"]), ["Kulaklık"]);
    assert_eq!(t["repairs"][0]["note"], "sol taraf ses vermiyor");
    assert_eq!(names(&t["expiring"]), ["Silikon"]);
    assert!(t["expiring"][0]["days_left"].as_i64().unwrap() < 0);
    assert_eq!(t["counts"]["needs"], 1);

    // A date far away is not due yet; fixed and got leave the list.
    inv.expires("Silikon", Some("2999-01-01")).unwrap();
    inv.broken("Kulaklık", None, true).unwrap();
    inv.need_close(need, true, None).unwrap();
    let t = inv.todo().unwrap();
    assert_eq!(t["counts"]["expiring"], 0);
    assert_eq!(t["counts"]["repairs"], 0);
    assert_eq!(t["counts"]["needs"], 0);
    assert_eq!(inv.need_close(need, false, None).unwrap_err().code(), 5);
    assert_eq!(
        inv.expires("Silikon", Some("07/2026")).unwrap_err().code(),
        2
    );
}

#[test]
fn only_a_sell_candidate_has_a_sale() {
    let (_d, mut inv) = setup();
    assert_eq!(
        inv.sale("Kulaklık", Some("listed"), Some(500), None)
            .unwrap_err()
            .code(),
        5
    );
    inv.dispose("Kulaklık", Disposition::Sell).unwrap();
    inv.sale("Kulaklık", Some("listed"), Some(500), Some("Sahibinden"))
        .unwrap();
    // Reserving keeps the price and where it is listed.
    let v = inv.sale("Kulaklık", Some("reserved"), None, None).unwrap();
    assert_eq!(v["marks"]["sale"]["value"], "reserved");
    assert_eq!(v["marks"]["sale"]["amount"], 500);
    assert_eq!(v["marks"]["sale"]["note"], "Sahibinden");
    let d = inv.disposals(None).unwrap();
    assert_eq!(d["disposals"]["sell"][0]["sale"]["value"], "reserved");
    let v = inv.sale("Kulaklık", None, None, None).unwrap();
    assert!(v["marks"].get("sale").is_none());
}

#[test]
fn todo_gathers_state_that_lives_elsewhere_without_copying_it() {
    let (_d, mut inv) = setup();
    add(&mut inv, "Kutu", "container", Some("Oda"), None);
    inv.edit("Kutu", &["unknown=true".into()]).unwrap();
    add(&mut inv, "Belirsiz parça", "item", Some("S5-01"), None);
    inv.move_to("Silikon", "Kutu", true).unwrap();
    inv.edit("Kulaklık", &["owner=Mahmutlar".into()]).unwrap();
    inv.mark_lost("Belirsiz parça").unwrap();

    let t = inv.todo().unwrap();
    let c = &t["counts"];
    assert_eq!(c["moves"], 1);
    assert_eq!(c["errands"], 1);
    assert_eq!(c["lost"], 1);
    assert_eq!(c["unknown"], 1);
    assert_eq!(names(&t["unclear"]), ["Belirsiz parça"]);

    // Doing the thing is what clears it; nothing to close in todo.
    inv.done("Silikon").unwrap();
    inv.found("Belirsiz parça").unwrap();
    let c = inv.todo().unwrap()["counts"].clone();
    assert_eq!(c["moves"], 0);
    assert_eq!(c["lost"], 0);
}
