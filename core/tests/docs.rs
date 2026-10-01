//! Documents (purchases spec §3.5): copied into ev's own store, linked to what they belong to.

use ev_core::{Inventory, NewDoc, NewNode};
use serde_json::Value;

fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent) in [
        ("Ev", "home", None),
        ("Oda", "room", Some("Ev")),
        ("Matkap", "item", Some("Oda")),
        ("Matkap ucu seti", "item", Some("Oda")),
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

fn invoice() -> NewDoc {
    NewDoc {
        kind: "invoice".into(),
        number: Some("402-123".into()),
        issued: Some("2024-05-03".into()),
        issuer: Some("Amazon".into()),
        ..Default::default()
    }
}

fn file(dir: &tempfile::TempDir, name: &str, body: &str) -> std::path::PathBuf {
    let p = dir.path().join(name);
    std::fs::write(&p, body).unwrap();
    p
}

fn events(inv: &Inventory, r: &str) -> Vec<String> {
    inv.history(r).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["type"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_document_is_copied_beside_the_database_and_outlives_its_original() {
    let (d, mut inv) = setup();
    let original = file(&d, "fatura.pdf", "%PDF invoice");
    let v = inv
        .doc_add(&original, &invoice(), &["Matkap".into()])
        .unwrap();
    let stored = v["document"]["file"].as_str().unwrap().to_string();
    assert!(stored.starts_with(d.path().join("docs").to_str().unwrap()));
    assert!(stored.ends_with(".pdf"));
    std::fs::remove_file(&original).unwrap();
    assert_eq!(std::fs::read_to_string(&stored).unwrap(), "%PDF invoice");
    let docs = inv.show("Matkap", false).unwrap()["documents"].clone();
    assert_eq!(docs.as_array().unwrap().len(), 1);
    assert_eq!(docs[0]["number"], "402-123");
    assert_eq!(docs[0]["original_name"], "fatura.pdf");
    assert!(events(&inv, "Matkap").contains(&"doc_linked".to_string()));
}

#[test]
fn the_same_file_again_is_the_same_document_with_one_more_link() {
    let (d, mut inv) = setup();
    let f = file(&d, "fatura.pdf", "%PDF invoice");
    inv.doc_add(&f, &invoice(), &["Matkap".into()]).unwrap();
    let again = NewDoc {
        kind: "other".into(),
        ..Default::default()
    };
    let v = inv
        .doc_add(&f, &again, &["Matkap ucu seti".into(), "Matkap".into()])
        .unwrap();
    assert_eq!(v["existing"], true);
    // The first description stands; only the new link is added.
    assert_eq!(v["document"]["kind"], "invoice");
    let on: Vec<&Value> = v["document"]["nodes"].as_array().unwrap().iter().collect();
    assert_eq!(on.len(), 2);
    assert_eq!(
        inv.doc_list(None).unwrap()["documents"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn unlinking_takes_the_document_off_the_thing_and_keeps_it_in_the_store() {
    let (d, mut inv) = setup();
    let f = file(&d, "kilavuz.pdf", "%PDF manual");
    let id = inv
        .doc_add(
            &f,
            &NewDoc {
                kind: "manual".into(),
                ..Default::default()
            },
            &["Matkap".into()],
        )
        .unwrap()["document"]["id"]
        .as_i64()
        .unwrap();
    inv.doc_unlink(id, "Matkap").unwrap();
    assert!(
        inv.show("Matkap", false).unwrap()["documents"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(inv.doc_show(id).unwrap()["document"]["kind"], "manual");
    assert!(events(&inv, "Matkap").contains(&"doc_unlinked".to_string()));
    assert!(inv.doc_unlink(id, "Matkap").is_err());
    inv.doc_link(id, "Matkap ucu seti").unwrap();
    assert_eq!(
        inv.doc_list(Some("Matkap ucu seti")).unwrap()["documents"][0]["id"],
        id
    );
}

#[test]
fn an_unknown_kind_or_a_bad_date_stores_nothing() {
    let (d, mut inv) = setup();
    let f = file(&d, "fis.jpg", "jpeg");
    let bad_kind = NewDoc {
        kind: "receipt".into(),
        ..Default::default()
    };
    assert!(inv.doc_add(&f, &bad_kind, &["Matkap".into()]).is_err());
    let bad_date = NewDoc {
        kind: "invoice".into(),
        issued: Some("03.05.2024".into()),
        ..Default::default()
    };
    assert!(inv.doc_add(&f, &bad_date, &["Matkap".into()]).is_err());
    assert!(
        inv.doc_add(&f, &invoice(), &["Yok böyle bir şey".into()])
            .is_err()
    );
    assert!(!d.path().join("docs").exists());
    assert!(
        inv.doc_list(None).unwrap()["documents"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
