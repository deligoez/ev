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

#[test]
fn a_thing_nothing_here_is_like_asks_for_a_new_group() {
    let (_d, inv) = setup();
    let v = inv.suggest("RFID okuyucu kartı", None).unwrap();
    assert_eq!(v["new_group_likely"], true);
    // Every holder is still listed, so nothing is decided by omission.
    assert_eq!(
        v["complete"]["containers"],
        v["containers"].as_array().unwrap().len()
    );
}

#[test]
fn an_existing_thing_is_placed_by_its_own_words_never_into_itself() {
    let (_d, inv) = setup();
    let v = inv
        .suggest_with("", None, Some("DS18B20 sıcaklık sensörü, su geçirmez"))
        .unwrap();
    assert_eq!(top(&v), "D-C1");
    // A box is never suggested into itself.
    let v = inv.suggest_with("", None, Some("D-C1")).unwrap();
    assert!(
        v["similar"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["container"]["code"] != "D-C1")
    );
}

#[test]
fn synonyms_carry_a_query_to_the_words_the_inventory_uses() {
    let (_d, mut inv) = setup();
    assert_eq!(
        inv.suggest("fotosel", None).unwrap()["new_group_likely"],
        true
    );
    inv.synonym_add("fotosel, ldr").unwrap();
    let v = inv.suggest("fotosel", None).unwrap();
    assert_eq!(top(&v), "D-A1");
    assert_eq!(v["synonyms_added"][0], "ldr");
    let id = inv.synonym_list().unwrap()["synonyms"][0]["id"]
        .as_i64()
        .unwrap();
    inv.synonym_remove(id).unwrap();
    assert!(
        inv.synonym_list().unwrap()["synonyms"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // A group needs two phrases.
    assert_eq!(inv.synonym_add("ldr").unwrap_err().code(), 2);
}

#[test]
fn room_comes_from_fill_and_goes_stale_when_the_contents_change() {
    let (_d, mut inv) = setup();
    let room = |inv: &Inventory| -> Value {
        inv.suggest("fotodirenç", None).unwrap()["similar"][0]["container"]["room"].clone()
    };
    assert_eq!(room(&inv)["room"], "unknown");
    inv.edit("D-A1", &["fill=50".into()]).unwrap();
    assert_eq!(room(&inv)["room"], "yes");
    assert_eq!(room(&inv)["stale"], false);
    inv.edit("D-A1", &["fill=95".into()]).unwrap();
    assert_eq!(room(&inv)["room"], "none");
    // The timestamps are to the second; the change has to come after the estimate.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    inv.add(node("LDR 3 mm", "item", "D-A1")).unwrap();
    assert_eq!(room(&inv)["stale"], true);
    // A size is WxDxH in grid units.
    assert_eq!(
        inv.edit("D-A1", &["size=big".into()]).unwrap_err().code(),
        2
    );
}

#[test]
fn regroup_finds_the_stray_the_full_box_and_where_a_bigger_one_fits() {
    let (_d, mut inv) = setup();
    inv.grid_set("D", 3, 2).unwrap();
    let pairs: Vec<(String, String)> = [
        ("D-A1", "A1"),
        ("D-B1", "B1"),
        ("D-C1", "C1"),
        ("D-A2", "A2"),
    ]
    .iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    inv.cells_set(&pairs, false).unwrap();
    inv.edit("D-B1", &["fill=95".into()]).unwrap();

    let v = inv.regroup(Some("D")).unwrap();
    let moved: Vec<(&str, &str)> = v["elsewhere"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["item"]["name"].as_str().unwrap(),
                e["better"]["holder"]["code"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        moved,
        [("DS18B20 sıcaklık sensörü, su geçirmez", "D-C1")],
        "only the misplaced sensor, not its neighbours"
    );
    let full = &v["full"][0];
    assert_eq!(full["holder"]["code"], "D-B1");
    let spare = &full["bigger_spares"][0];
    assert_eq!(spare["size"], "1x2x1");
    // The 1×2 spare fits where the full box stands plus the free cell in front of it.
    assert_eq!(spare["fits_at"][0], "B1");
    assert!(v["checked"]["items"].as_i64().unwrap() >= 7);
}
