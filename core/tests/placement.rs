//! Spec §23: where a thing should go, why, how much room there is, and what could regroup.

use ev_core::{Inventory, NewNode};
use serde_json::Value;
use tempfile::TempDir;

fn node(name: &str, kind: &str, parent: &str) -> NewNode {
    NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: Some(parent.into()),
        ..Default::default()
    }
}

fn boxed(code: &str, theme: &str) -> NewNode {
    NewNode {
        code: Some(code.into()),
        theme: Some(theme.into()),
        size: Some("1x1x1".into()),
        ..node("Kutu", "container", "D")
    }
}

/// A drawer `D` in a 3×2 grid: an LDR box, a light/flame sensor box, a temperature box and a
/// button box that also holds a temperature sensor by mistake; a bigger spare box on the side.
fn setup() -> (TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    let mut lines = vec![
        NewNode {
            name: "Ev".into(),
            kind: "home".into(),
            ..Default::default()
        },
        node("Oda", "room", "Ev"),
        NewNode {
            code: Some("D".into()),
            ..node("Çekmece", "container", "Oda")
        },
        boxed("D-A1", "LDR fotodirenç"),
        boxed("D-B1", "ışık / alev sensörleri"),
        boxed("D-C1", "sıcaklık sensörleri"),
        boxed("D-A2", "butonlar"),
        node("LDR 5 mm", "item", "D-A1"),
        node("Işık sensörü modülü", "item", "D-B1"),
        node("Alev sensörü modülü", "item", "D-B1"),
        node("DS18B20 sıcaklık sensörü", "item", "D-C1"),
        node("DHT11 sıcaklık ve nem sensörü", "item", "D-C1"),
        node("Buton 12 mm", "item", "D-A2"),
        node("Buton kapağı", "item", "D-A2"),
        node("DS18B20 sıcaklık sensörü, su geçirmez", "item", "D-A2"),
        NewNode {
            tags: vec!["boş kap".into()],
            size: Some("1x2x1".into()),
            ..node("Boş kutu 1x2x1", "container", "Oda")
        },
    ];
    for l in &mut lines {
        l.key = None;
    }
    inv.add_batch(lines).unwrap();
    (dir, inv)
}

fn top(v: &Value) -> String {
    v["similar"][0]["container"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn a_part_code_or_an_inflected_word_finds_its_box_and_says_why() {
    let (_d, inv) = setup();
    let v = inv.suggest("DS18B20 sıcaklık sensörü", None).unwrap();
    assert_eq!(top(&v), "D-C1");
    assert_eq!(v["similar"][0]["coverage"], 1.0);
    assert_eq!(v["new_group_likely"], false);
    let m = v["similar"][0]["matched"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["term"] == "ds18b20")
        .unwrap();
    assert_eq!(m["specific"], true);
    assert!(m["from"].as_str().unwrap().starts_with("item: "));

    // "sıcaklığı" meets the inventory's "sıcaklık" (ğ hardens back to k).
    assert_eq!(
        top(&inv.suggest("sıcaklığı ölçen parça", None).unwrap()),
        "D-C1"
    );
    // "fotodirençler" meets "fotodirenç".
    assert_eq!(top(&inv.suggest("fotodirençler", None).unwrap()), "D-A1");
    // The same question gets the same answer.
    assert_eq!(
        inv.suggest("ışık sensörü", None).unwrap()["similar"],
        inv.suggest("ışık sensörü", None).unwrap()["similar"]
    );
}

