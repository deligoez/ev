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
    let pairs = |key: &str| -> Vec<(String, String)> {
        v[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["item"]["name"].as_str().unwrap().to_string(),
                    e["better"]["holder"]["code"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    };
    // The sensor among buttons shares no word with them, so its home says nothing either way
    // and the move is listed as a guess, not as a sure thing; its neighbours are not listed.
    assert!(pairs("elsewhere").is_empty(), "{}", v["elsewhere"]);
    assert_eq!(
        pairs("alone"),
        [(
            "DS18B20 sıcaklık sensörü, su geçirmez".to_string(),
            "D-C1".to_string()
        )]
    );
    let full = &v["full"][0];
    assert_eq!(full["holder"]["code"], "D-B1");
    let spare = &full["bigger_spares"][0];
    assert_eq!(spare["size"], "1x2x1");
    // The 1×2 spare fits where the full box stands plus the free cell in front of it.
    assert_eq!(spare["fits_at"][0], "B1");
    assert!(v["checked"]["items"].as_i64().unwrap() >= 7);
}

#[test]
fn a_thing_that_holds_things_is_never_its_own_better_place() {
    let (_d, mut inv) = setup();
    // A kit recorded as an item with its parts inside: it is a holder, and the part inside
    // it shares the kit's rare words, so the kit matches itself best.
    inv.add(NewNode {
        code: Some("KIT".into()),
        ..node("LiPo pil kutusu + Seeed Rider Pro seti", "item", "D-A2")
    })
    .unwrap();
    inv.add(node("Seeed LiPo Rider Pro kartı", "item", "KIT"))
        .unwrap();
    let kit = inv.show("KIT", false).unwrap()["node"]["id"].clone();
    let v = inv.regroup(Some("D")).unwrap();
    for e in v["elsewhere"].as_array().unwrap() {
        if e["item"]["id"] == kit {
            assert_ne!(e["better"]["holder"]["id"], kit, "{e}");
        }
    }
}

#[test]
fn a_place_without_a_theme_shows_what_its_contents_share() {
    let (_d, mut inv) = setup();
    // A themed antenna box, and an unthemed box of antennas next to it.
    inv.add(boxed("D-B2", "Antenler")).unwrap();
    inv.add(node("Wi-Fi anteni, RP-SMA", "item", "D-B2"))
        .unwrap();
    inv.add(NewNode {
        code: Some("D-C2".into()),
        ..node("Kutu", "container", "D")
    })
    .unwrap();
    for name in [
        "RP-SMA çubuk anten 2.4 GHz",
        "Kırmızı anten, 5 dBi",
        "U.FL anten kablosu",
    ] {
        inv.add(node(name, "item", "D-C2")).unwrap();
    }
    // A kit recorded as an item is not a place: it gets no theme hints.
    inv.add(NewNode {
        code: Some("KIT".into()),
        ..node("Anten seti", "item", "D-C2")
    })
    .unwrap();
    inv.add(node("Anten", "item", "KIT")).unwrap();

    let v = inv.themes(None).unwrap();
    let list = v["themes"].as_array().unwrap();
    let codes: Vec<&str> = list
        .iter()
        .map(|e| e["holder"]["code"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(codes, ["D-C2"], "themed boxes and kits are not listed: {v}");
    let e = &list[0];
    assert_eq!(e["things"], 4);
    // The word every antenna shares comes first, written as the inventory writes it; colours
    // and numbers are no theme.
    assert_eq!(e["words"][0]["word"], "anten");
    assert_eq!(e["words"][0]["things"], 4);
    let words: Vec<&str> = e["words"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["word"].as_str().unwrap())
        .collect();
    assert!(!words.contains(&"Kırmızı"), "{words:?}");
    assert!(
        !words.iter().any(|w| w.chars().any(|c| c.is_ascii_digit())),
        "{words:?}"
    );
    // It reads like the themed antenna box.
    assert_eq!(e["like"]["code"], "D-B2");
    assert_eq!(e["like"]["theme"], "Antenler");
    // Once themed, it leaves the list.
    inv.edit("D-C2", &["theme=Antenler (kablolu)".into()])
        .unwrap();
    assert!(
        inv.themes(None).unwrap()["themes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

/// A drawer `M` with a bare-buzzer box and an output-module box that also holds a buzzer module:
/// words alone send the module to the bare buzzers.
fn module_drawer() -> (TempDir, Inventory) {
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
            code: Some("M".into()),
            ..node("Çekmece", "container", "Oda")
        },
        NewNode {
            code: Some("M-A1".into()),
            theme: Some("Buzzer (çıplak)".into()),
            ..node("Kutu", "container", "M")
        },
        NewNode {
            code: Some("M-B1".into()),
            // Not "modülleri": that would share the module's own stem and hold it by words.
            theme: Some("Çıkış kartları: lazer, RGB".into()),
            ..node("Kutu", "container", "M")
        },
        node("Buzzer, çıplak 12 mm", "item", "M-A1"),
        node("Pasif buzzer, çıplak", "item", "M-A1"),
        node("Aktif buzzer modülü", "item", "M-B1"),
        node("Lazer diyot kartı, 650 nm", "item", "M-B1"),
        node("RGB LED kartı, 5 mm", "item", "M-B1"),
    ];
    for l in &mut lines {
        l.key = None;
    }
    inv.add_batch(lines).unwrap();
    (dir, inv)
}

fn flagged(inv: &Inventory, name: &str) -> Option<String> {
    let v = inv.regroup(Some("M")).unwrap();
    ["elsewhere", "alone"]
        .iter()
        .flat_map(|k| v[*k].as_array().unwrap().clone())
        .find(|e| e["item"]["name"] == name)
        .map(|e| e["better"]["holder"]["code"].as_str().unwrap().to_string())
}

#[test]
fn facets_keep_modules_and_bare_parts_apart() {
    let (_d, mut inv) = module_drawer();
    // Without facets, the buzzer module is flagged for the bare buzzers.
    assert_eq!(
        flagged(&inv, "Aktif buzzer modülü").as_deref(),
        Some("M-A1")
    );
    inv.facet_add("modül", Some("modül, kart")).unwrap();
    inv.facet_add("çıplak", None).unwrap();
    inv.edit("M-B1", &["tags=+modül".into()]).unwrap();
    inv.edit("M-A1", &["tags=+çıplak".into()]).unwrap();
    // With them, it stays among the modules.
    assert_eq!(flagged(&inv, "Aktif buzzer modülü"), None);

    // A new module is not offered the bare-buzzer box, which is listed apart instead.
    let v = inv.suggest("buzzer modülü", None).unwrap();
    assert_eq!(v["facet"][0], "modül");
    assert!(
        v["similar"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["container"]["code"] != "M-A1"),
        "{}",
        v["similar"]
    );
    assert_eq!(v["other_facet"][0]["container"]["code"], "M-A1");
    assert_eq!(v["other_facet"][0]["container"]["facet"][0], "çıplak");
    // Any form of a facet word names the facet, even one the inventory never writes bare.
    for q in ["joystick modülleri", "ses sensörü kartı"] {
        assert_eq!(inv.suggest(q, None).unwrap()["facet"][0], "modül", "{q}");
    }
    // A thing whose words name no facet is free to go anywhere.
    let v = inv.suggest("buzzer", None).unwrap();
    assert_eq!(v["similar"][0]["container"]["code"], "M-A1");
    assert!(v["other_facet"].as_array().unwrap().is_empty());
    // --for reads the facet from where the thing is.
    let v = inv
        .suggest_with("", None, Some("Buzzer, çıplak 12 mm"))
        .unwrap();
    assert_eq!(v["facet"][0], "çıplak");

    // The list shows each facet's holders; removing one frees the tags.
    let l = inv.facet_list().unwrap();
    assert_eq!(l["facets"].as_array().unwrap().len(), 2);
    inv.facet_remove("modül").unwrap();
    assert_eq!(inv.facet_remove("modül").unwrap_err().code(), 3);
    assert_eq!(inv.facet_add("x", None).unwrap_err().code(), 2);
}

#[test]
fn the_tree_carries_theme_fill_size_and_tags() {
    let (_d, mut inv) = module_drawer();
    inv.edit(
        "M-A1",
        &["fill=40".into(), "size=1x1x1".into(), "tags=+çıplak".into()],
    )
    .unwrap();
    let v = inv.tree(Some("M"), None).unwrap();
    let a1 = v["tree"][0]["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["code"] == "M-A1")
        .unwrap()
        .clone();
    assert_eq!(a1["theme"], "Buzzer (çıplak)");
    assert_eq!(a1["fill"], 40);
    assert_eq!(a1["size"], "1x1x1");
    assert_eq!(a1["tags"][0], "çıplak");
}
