//! Spec §9 acceptance rows, plus the §4 reference table.

use ev_core::{Disposition, Error, Inventory, NewNode};
use serde_json::Value;
use tempfile::TempDir;

fn inv() -> (TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    (dir, inv)
}

fn node(name: &str, kind: &str, parent: Option<&str>, code: Option<&str>) -> NewNode {
    NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: parent.map(Into::into),
        code: code.map(Into::into),
        ..Default::default()
    }
}

fn add(
    inv: &mut Inventory,
    name: &str,
    kind: &str,
    parent: Option<&str>,
    code: Option<&str>,
) -> i64 {
    let v = inv.add(node(name, kind, parent, code)).unwrap();
    v["node"]["id"].as_i64().unwrap()
}

fn code_of(e: &Error) -> i32 {
    e.code()
}

fn event_types(inv: &Inventory, reference: &str) -> Vec<String> {
    let h = inv.history(reference).unwrap();
    h["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["type"].as_str().unwrap().to_string())
        .collect()
}

/// Ev › Salon › K4x4 › K4x4-15-A › Flipper Zero, plus Ev › Kiler.
fn home(inv: &mut Inventory) {
    add(inv, "Ev", "home", None, None);
    add(inv, "Salon", "room", Some("Ev"), None);
    add(inv, "Kiler", "room", Some("Ev"), None);
    add(inv, "K4x4", "furniture", Some("Salon"), None);
    add(inv, "Çekmece", "container", Some("K4x4"), Some("K4x4-15-A"));
    add(inv, "Flipper Zero", "item", Some("K4x4-15-A"), None);
}

#[test]
fn a_tag_alone_lists_everything_tagged_with_it() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    add(&mut inv, "PTFE rakor", "item", Some("Kiler"), None);
    inv.edit("PTFE rakor", &["tags=+3d yazıcı".into()]).unwrap();
    inv.edit("Flipper Zero", &["tags=+3d yazıcı".into()])
        .unwrap();
    let v = inv.find("", Some("3d yazıcı"), None, false).unwrap();
    let names: Vec<&str> = v["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["Flipper Zero", "PTFE rakor"]);
    // No text and no filter is still a usage error.
    assert_eq!(code_of(&inv.find("", None, None, false).unwrap_err()), 2);
}

#[test]
fn a1_find_returns_full_path() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    let v = inv.find("flipper", None, None, false).unwrap();
    let results = v["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0]["path_text"],
        "Ev › Salon › K4x4 › K4x4-15-A › Flipper Zero"
    );
}

#[test]
fn a2_move_into_own_descendant_is_refused() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    add(
        &mut inv,
        "Samla",
        "container",
        Some("K4x4-15-A"),
        Some("S5-01"),
    );
    let e = inv.move_to("K4x4-15-A", "S5-01", false).unwrap_err();
    assert_eq!(code_of(&e), 5);
    let v = inv.show("K4x4-15-A", false).unwrap();
    assert_eq!(v["node"]["path_text"], "Ev › Salon › K4x4 › K4x4-15-A");
}

#[test]
fn a3_a4_planned_move_then_done() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    let box_id = add(
        &mut inv,
        "Elektrik",
        "container",
        Some("Kiler"),
        Some("S5-03"),
    );
    inv.move_to("Flipper Zero", "S5-03", true).unwrap();
    let v = inv.show("Flipper Zero", false).unwrap();
    assert_eq!(
        v["node"]["path_text"],
        "Ev › Salon › K4x4 › K4x4-15-A › Flipper Zero"
    );
    assert_eq!(v["pending"]["code"], "S5-03");

    inv.done("Flipper Zero").unwrap();
    let v = inv.show("Flipper Zero", false).unwrap();
    assert_eq!(v["node"]["parent_id"], box_id);
    assert!(v["pending"].is_null());
    assert_eq!(
        event_types(&inv, "Flipper Zero"),
        ["create", "plan", "done"]
    );
}

#[test]
fn a5_gone_without_disposition_is_refused_for_active() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    assert_eq!(code_of(&inv.gone("Flipper Zero", None).unwrap_err()), 5);
}

#[test]
fn a6_gone_with_disposition_in_one_step() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    let v = inv.gone("Flipper Zero", Some(Disposition::Trash)).unwrap();
    assert_eq!(v["node"]["state"], "gone");
    assert_eq!(v["node"]["disposition"], "trash");
    assert_eq!(
        event_types(&inv, "Flipper Zero"),
        ["create", "dispose", "gone"]
    );
}

#[test]
fn a7_gone_refuses_a_box_that_still_holds_something() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    add(&mut inv, "Kutu", "container", Some("Kiler"), Some("B1"));
    add(&mut inv, "Kalem", "item", Some("B1"), None);
    assert_eq!(
        code_of(&inv.dispose("B1", Disposition::Give).unwrap_err()),
        5
    );
    let e = inv.gone("B1", Some(Disposition::Give)).unwrap_err();
    assert_eq!(code_of(&e), 5);
    assert_eq!(
        e.to_json()["error"]["details"]["children"][0]["name"],
        "Kalem"
    );
}

#[test]
fn a8_a9_code_reuse_after_gone_only() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    add(&mut inv, "Eski", "container", Some("Kiler"), Some("S5-02"));
    let clash = inv
        .add(node("Kutu", "container", Some("Kiler"), Some("S5-02")))
        .unwrap_err();
    assert_eq!(code_of(&clash), 5);
    inv.gone("S5-02", Some(Disposition::Trash)).unwrap();
    inv.add(node("Kutu", "container", Some("Kiler"), Some("S5-02")))
        .unwrap();
    // After reuse the active node wins when gone nodes are included (§11.4).
    let id = inv.resolve("S5-02", true).unwrap();
    assert_eq!(inv.brief(id).unwrap().name, "Kutu");
    // Codes compare folded (§11.3): Ü and U collide.
    add(&mut inv, "Ç", "container", Some("K4x4"), Some("K4x4-07-Ü"));
    assert_eq!(
        code_of(
            &inv.add(node("X", "container", Some("K4x4"), Some("k4x4-07-u")))
                .unwrap_err()
        ),
        5
    );
}

#[test]
fn a10_digit_only_code_is_refused() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    assert_eq!(
        code_of(
            &inv.add(node("Kutu", "container", Some("Kiler"), Some("12")))
                .unwrap_err()
        ),
        5
    );
}

#[test]
fn a11_a12_rooms_nest_only_in_homes_and_rooms() {
    let (_d, mut inv) = inv();
    add(&mut inv, "Ev", "home", None, None);
    add(&mut inv, "Yatak odası", "room", Some("Ev"), None);
    inv.add(node("Giyinme odası", "room", Some("Yatak odası"), None))
        .unwrap();
    add(
        &mut inv,
        "Gardırop",
        "furniture",
        Some("Giyinme odası"),
        None,
    );
    assert_eq!(
        code_of(
            &inv.add(node("Oda", "room", Some("Gardırop"), None))
                .unwrap_err()
        ),
        5
    );
    assert_eq!(
        code_of(
            &inv.move_to("Yatak odası", "Giyinme odası", false)
                .unwrap_err()
        ),
        5
    );
    assert_eq!(
        code_of(&inv.move_to("Giyinme odası", "Gardırop", false).unwrap_err()),
        5
    );
}

#[test]
fn a13_a14_lost_then_moved() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    add(&mut inv, "Ses", "container", Some("Kiler"), Some("S5-01"));
    inv.mark_lost("Flipper Zero").unwrap();
    let v = inv.lost_list().unwrap();
    assert_eq!(
        v["lost"][0]["last_seen"]["path_text"],
        "Ev › Salon › K4x4 › K4x4-15-A"
    );
    let v = inv.move_to("Flipper Zero", "S5-01", false).unwrap();
    assert_eq!(v["node"]["lost"], false);
    assert!(
        inv.lost_list().unwrap()["lost"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn lost_without_place_and_found_in_place() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    let mut n = node("Etiket makinesi", "item", None, None);
    n.lost = true;
    inv.add(n).unwrap();
    assert_eq!(code_of(&inv.found("Etiket makinesi").unwrap_err()), 5);
    assert_eq!(
        code_of(&inv.add(node("Sahipsiz", "item", None, None)).unwrap_err()),
        5
    );
    inv.mark_lost("Flipper Zero").unwrap();
    let v = inv.found("Flipper Zero").unwrap();
    assert_eq!(v["node"]["lost"], false);
}

#[test]
fn a15_batch_is_all_or_nothing() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    let mut first = node("Kutu", "container", Some("Kiler"), Some("S5-09"));
    first.key = Some("k".into());
    let second = node("Kalem", "item", Some("@k"), None);
    let third = node("Bozuk", "container", Some("Kiler"), Some("S5-09"));
    let e = inv
        .add_batch(vec![first.clone(), second.clone(), third])
        .unwrap_err();
    assert_eq!(code_of(&e), 5);
    assert!(e.to_string().starts_with("line 3:"), "{e}");
    assert_eq!(code_of(&inv.show("S5-09", false).unwrap_err()), 3);
    let v = inv.add_batch(vec![first, second]).unwrap();
    assert_eq!(v["created"][1]["path_text"], "Ev › Kiler › S5-09 › Kalem");
}

#[test]
fn a16_newer_schema_is_refused_and_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ev.db");
    drop(Inventory::open(&path).unwrap());
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("PRAGMA user_version = 99;")
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    let e = Inventory::open(&path).err().unwrap();
    assert_eq!(code_of(&e), 6);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn reference_table_of_section_4() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    add(
        &mut inv,
        "Ses ve kablo",
        "container",
        Some("Kiler"),
        Some("S5-01"),
    );
    add(
        &mut inv,
        "Çekmece",
        "container",
        Some("K4x4"),
        Some("K4x4-07-Ü"),
    );
    let masa = add(&mut inv, "Masa", "furniture", Some("Salon"), None);
    add(&mut inv, "Anten", "item", Some("K4x4-07-Ü"), None);
    add(&mut inv, "Anten", "item", Some(&masa.to_string()), None);

    let s501 = inv.resolve("S5-01", false).unwrap();
    assert_eq!(inv.resolve(&s501.to_string(), false).unwrap(), s501);
    // `#id`, as ev ui prints it, names the same node.
    assert_eq!(inv.resolve(&format!("#{s501}"), false).unwrap(), s501);
    assert_eq!(inv.resolve("s5-01", false).unwrap(), s501);
    let k = inv.resolve("k4x4-07-u", false).unwrap();
    assert_eq!(inv.brief(k).unwrap().code.as_deref(), Some("K4x4-07-Ü"));
    let f = inv.resolve("flipper zero", false).unwrap();
    assert_eq!(inv.brief(f).unwrap().name, "Flipper Zero");
    assert_eq!(code_of(&inv.resolve("flipper", false).unwrap_err()), 3);
    let e = inv.resolve("anten", false).unwrap_err();
    assert_eq!(code_of(&e), 4);
    let v: Value = e.to_json();
    assert_eq!(v["error"]["candidates"].as_array().unwrap().len(), 2);
}

#[test]
fn edit_fields_and_history() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    let v = inv
        .edit(
            "Flipper Zero",
            &[
                "note=Flipper için ESP kartı da var".into(),
                "tags=+maker".into(),
                "qty=2".into(),
            ],
        )
        .unwrap();
    assert_eq!(v["node"]["qty"], 2);
    assert_eq!(v["node"]["tags"][0], "maker");
    assert_eq!(
        code_of(&inv.edit("Flipper Zero", &["fill=150".into()]).unwrap_err()),
        2
    );
    assert_eq!(
        code_of(&inv.edit("Flipper Zero", &["parent=1".into()]).unwrap_err()),
        2
    );
    let found = inv.find("MAKER", Some("maker"), None, false).unwrap();
    assert_eq!(found["results"].as_array().unwrap().len(), 1);
    assert_eq!(event_types(&inv, "Flipper Zero"), ["create", "edit"]);
}

#[test]
fn disposals_and_restore() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    inv.dispose("Flipper Zero", Disposition::Sell).unwrap();
    assert_eq!(
        code_of(&inv.dispose("Flipper Zero", Disposition::Give).unwrap_err()),
        5
    );
    let v = inv.disposals(None).unwrap();
    assert_eq!(v["disposals"]["sell"][0]["name"], "Flipper Zero");
    inv.restore("Flipper Zero").unwrap();
    assert!(
        inv.disposals(None).unwrap()["disposals"]["sell"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_bundle_of_candidates_is_disposed_and_leaves_together() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    let set = add(&mut inv, "Dock seti", "item", Some("Kiler"), None);
    let cable = add(&mut inv, "Kablo", "item", Some(&set.to_string()), None);
    // An active part still blocks the set.
    assert_eq!(
        code_of(
            &inv.dispose(&set.to_string(), Disposition::Sell)
                .unwrap_err()
        ),
        5
    );
    inv.dispose(&cable.to_string(), Disposition::Sell).unwrap();
    inv.dispose(&set.to_string(), Disposition::Sell).unwrap();
    let v = inv.disposals(None).unwrap();
    let sell = v["disposals"]["sell"].as_array().unwrap();
    assert_eq!(
        sell.len(),
        1,
        "a part is listed under its set, not beside it"
    );
    assert_eq!(sell[0]["parts"][0]["id"], cable);
    inv.gone(&set.to_string(), None).unwrap();
    let h = inv.history(&cable.to_string()).unwrap();
    let last = h["events"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["type"], "gone");
    assert_eq!(last["data"]["with"], set);
    assert!(
        inv.disposals(None).unwrap()["disposals"]["sell"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn data_version_moves_when_another_connection_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ev.db");
    let reader = Inventory::open(&path).unwrap();
    let mut writer = Inventory::open(&path).unwrap();
    let before = reader.data_version().unwrap();
    writer.add(node("Ev", "home", None, None)).unwrap();
    assert_ne!(reader.data_version().unwrap(), before);
}

#[test]
fn places_with_aliases_answer_what_goes_where() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    inv.place_add("Mahmutlar", &["Saliha'lar".into(), "Güneşler".into()])
        .unwrap();
    inv.edit("Flipper Zero", &["owner=saliha'lar".into()])
        .unwrap();
    add(&mut inv, "Merdiven", "item", Some("Kiler"), None);
    inv.lend("Merdiven", "GUNESLER").unwrap();
    add(&mut inv, "Kek kalıbı", "item", Some("Kiler"), None);
    inv.edit("Kek kalıbı", &["to=Mahmutlar".into()]).unwrap();

    assert_eq!(
        inv.errands(Some("salihalar")).unwrap()["place"]["name"],
        "Mahmutlar"
    );
    let v = inv.errands(Some("güneşler")).unwrap();
    assert_eq!(v["place"]["name"], "Mahmutlar");
    assert_eq!(v["take"][0]["name"], "Kek kalıbı");
    assert_eq!(v["return"][0]["name"], "Flipper Zero");
    assert_eq!(v["collect"][0]["name"], "Merdiven");

    // An alias taken by another place is refused; a lent node of someone else is refused.
    inv.place_add("Ofis", &[]).unwrap();
    assert_eq!(
        code_of(&inv.place_alias("Ofis", "saliha'lar").unwrap_err()),
        5
    );
    assert_eq!(code_of(&inv.lend("Flipper Zero", "Ofis").unwrap_err()), 5);

    inv.back("Merdiven").unwrap();
    assert!(
        inv.errands(Some("Mahmutlar")).unwrap()["collect"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(event_types(&inv, "Merdiven"), ["create", "lend", "back"]);
    assert_eq!(code_of(&inv.errands(Some("Bilinmeyen")).unwrap_err()), 3);
}

#[test]
fn merging_places_moves_references_and_aliases() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    inv.edit("Flipper Zero", &["to=Salihalar".into()]).unwrap();
    inv.place_add("Mahmutlar", &[]).unwrap();
    inv.place_merge("Salihalar", "Mahmutlar").unwrap();
    let v = inv.errands(Some("salihalar")).unwrap();
    assert_eq!(v["place"]["name"], "Mahmutlar");
    assert_eq!(v["take"][0]["name"], "Flipper Zero");
    assert_eq!(
        inv.place_list().unwrap()["places"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn returning_what_is_not_ours_is_a_disposition() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    inv.edit("Flipper Zero", &["owner=Mahmutlar".into()])
        .unwrap();
    let v = inv.gone("Flipper Zero", Some(Disposition::Return)).unwrap();
    assert_eq!(v["node"]["disposition"], "return");
    assert!(
        inv.errands(None).unwrap()["errands"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_thing_meant_for_its_owner_is_listed_once_as_a_return() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    inv.edit(
        "Flipper Zero",
        &["owner=Mahmutlar".into(), "to=Mahmutlar".into()],
    )
    .unwrap();
    let v = inv.errands(Some("Mahmutlar")).unwrap();
    assert!(v["take"].as_array().unwrap().is_empty(), "{v}");
    assert_eq!(v["return"].as_array().unwrap().len(), 1);
}

#[test]
fn suggest_lists_every_holder_and_where_alike_things_are() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    let screws = add(&mut inv, "Vida kutusu", "container", Some("K4x4"), None);
    add(
        &mut inv,
        "Havşa vida 2 cm",
        "item",
        Some(&screws.to_string()),
        None,
    );
    add(
        &mut inv,
        "Boş gridfinity kutu",
        "container",
        Some("Kiler"),
        None,
    );
    let bag = add(&mut inv, "Çanta", "item", Some("Kiler"), None);
    add(&mut inv, "Şarj aleti", "item", Some(&bag.to_string()), None);
    inv.rule_add("Pahalı eşya kilere gitmez").unwrap();

    let v = inv.suggest("uzun vida", None).unwrap();
    assert_eq!(v["rules"][0]["text"], "Pahalı eşya kilere gitmez");
    assert_eq!(v["similar"][0]["container"]["id"], screws);

    // Completeness: every room, furniture, container and item that holds something is listed,
    // including the empty box and the bag, and nothing else.
    let listed: Vec<i64> = v["containers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_i64().unwrap())
        .collect();
    for name in [
        "Salon",
        "Kiler",
        "K4x4",
        "K4x4-15-A",
        "Vida kutusu",
        "Boş gridfinity kutu",
        "Çanta",
    ] {
        let id = inv.resolve(name, false).unwrap();
        assert!(listed.contains(&id), "{name} missing from suggest");
    }
    let flipper = inv.resolve("Flipper Zero", false).unwrap();
    assert!(!listed.contains(&flipper));
    assert_eq!(v["complete"]["containers"], listed.len());
    assert_eq!(code_of(&inv.suggest("  ", None).unwrap_err()), 2);
}

#[test]
fn audit_finds_alike_things_split_up_and_gaps() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    let a = add(&mut inv, "Çekmece A", "container", Some("K4x4"), None);
    let b = add(&mut inv, "Çekmece B", "container", Some("K4x4"), None);
    add(
        &mut inv,
        "SanDisk hafıza kartı",
        "item",
        Some(&a.to_string()),
        None,
    );
    add(
        &mut inv,
        "Transcend hafıza kartı",
        "item",
        Some(&b.to_string()),
        None,
    );
    add(&mut inv, "Sehpada duran kalem", "item", Some("Salon"), None);
    let v = inv.audit().unwrap();
    let words: Vec<&str> = v["spread"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["word"].as_str().unwrap())
        .collect();
    assert!(words.contains(&"hafiza"), "{words:?}");
    assert!(
        v["loose"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["name"] == "Sehpada duran kalem")
    );
    assert!(
        v["no_theme"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"] == a)
    );
    // A set being disposed of holds its parts but is not a place to put things.
    let set = add(&mut inv, "Dock seti", "item", Some("Kiler"), None);
    let part = add(&mut inv, "Kablo", "item", Some(&set.to_string()), None);
    inv.dispose(&part.to_string(), Disposition::Sell).unwrap();
    inv.dispose(&set.to_string(), Disposition::Sell).unwrap();
    let v = inv.audit().unwrap();
    assert!(
        !v["no_theme"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"] == set)
    );
}

#[test]
fn rules_are_added_listed_and_removed() {
    let (_d, mut inv) = inv();
    let v = inv.rule_add("Çekmeceler günlük kullanım içindir").unwrap();
    let id = v["rules"][0]["id"].as_i64().unwrap();
    assert_eq!(
        inv.rule_list().unwrap()["rules"].as_array().unwrap().len(),
        1
    );
    inv.rule_remove(id).unwrap();
    assert_eq!(code_of(&inv.rule_remove(id).unwrap_err()), 3);
}

#[test]
fn suggest_matches_word_starts_not_fragments() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    let tools = add(&mut inv, "Alet çekmecesi", "container", Some("K4x4"), None);
    add(
        &mut inv,
        "Bosch matkap",
        "item",
        Some(&tools.to_string()),
        None,
    );
    let cards = add(&mut inv, "Kart kutusu", "container", Some("K4x4"), None);
    add(&mut inv, "SD kartı", "item", Some(&cards.to_string()), None);
    let v = inv.suggest("boş", None).unwrap();
    assert!(
        v["similar"].as_array().unwrap().is_empty(),
        "boş must not find Bosch"
    );
    let v = inv.suggest("kart", None).unwrap();
    assert_eq!(v["similar"][0]["container"]["id"], cards);
}

#[test]
fn uninventoried_holders_are_flagged() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    let mut n = node("Karton kutu", "container", Some("Kiler"), None);
    n.unknown = true;
    let id = inv.add(n).unwrap()["node"]["id"].as_i64().unwrap();
    let v = inv.suggest("herhangi", None).unwrap();
    let c = v["containers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap()
        .clone();
    assert_eq!(c["unknown"], true);
    assert!(
        inv.audit().unwrap()["unknown"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"] == id)
    );
    inv.edit(&id.to_string(), &["unknown=false".into()])
        .unwrap();
    assert!(
        inv.audit().unwrap()["unknown"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_mistaken_gone_can_be_corrected_with_a_reason() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    inv.gone("Flipper Zero", Some(Disposition::Trash)).unwrap();
    assert_eq!(code_of(&inv.restore("Flipper Zero").unwrap_err()), 3);
    assert_eq!(
        code_of(&inv.correct_gone("Flipper Zero", " ").unwrap_err()),
        2
    );
    let v = inv
        .correct_gone("Flipper Zero", "not thrown out after all")
        .unwrap();
    assert_eq!(v["node"]["state"], "active");
    assert!(v["node"]["disposition"].is_null());
    let h = inv.history("Flipper Zero").unwrap();
    let last = h["events"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["data"]["correction"], "not thrown out after all");
}

#[test]
fn a_places_history_shows_what_came_in_went_out_and_was_added() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    add(&mut inv, "Kutu", "container", Some("Kiler"), Some("S5-01"));
    add(&mut inv, "Kablo", "item", Some("K4x4-15-A"), None);
    inv.move_to("Flipper Zero", "S5-01", false).unwrap();
    inv.move_to("Kablo", "S5-01", true).unwrap();
    let h = inv.history_with_contents("K4x4-15-A").unwrap();
    let seen: Vec<(String, String, String)> = h["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["type"].as_str().unwrap().to_string(),
                e["relation"].as_str().unwrap_or("own").to_string(),
                e["item"]["name"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    let row = |a: &str, b: &str, c: &str| (a.to_string(), b.to_string(), c.to_string());
    assert_eq!(
        seen,
        vec![
            row("create", "own", ""),
            row("create", "added", "Flipper Zero"),
            row("create", "added", "Kablo"),
            row("move", "out", "Flipper Zero"),
        ]
    );
    // The planned move shows at its destination; plain `history` stays the node's own.
    let h = inv.history_with_contents("S5-01").unwrap();
    let last = h["events"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(
        (last["type"].as_str(), last["relation"].as_str()),
        (Some("plan"), Some("in"))
    );
    assert_eq!(event_types(&inv, "S5-01"), vec!["create"]);
}

#[test]
fn a_gone_node_keeps_its_reason_and_takes_a_note_by_id_only() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    let v = inv
        .gone_because(
            "Flipper Zero",
            Some(Disposition::Trash),
            Some("broken screen"),
        )
        .unwrap();
    let id = v["node"]["id"].as_i64().unwrap().to_string();
    assert_eq!(v["node"]["note"], "broken screen");
    let h = inv.history(&id).unwrap();
    let last = h["events"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["data"]["why"], "broken screen");

    // By name it stays out of reach; by id only the note may change.
    assert_eq!(
        code_of(&inv.edit("Flipper Zero", &["note=x".into()]).unwrap_err()),
        3
    );
    assert_eq!(
        code_of(&inv.edit(&id, &["name=Other".into()]).unwrap_err()),
        5
    );
    let v = inv.edit(&id, &["note=probably thrown out".into()]).unwrap();
    assert_eq!(v["node"]["note"], "probably thrown out");
    assert_eq!(v["node"]["state"], "gone");
}

#[test]
fn audit_groups_turkish_word_forms_under_one_stem() {
    let (_d, mut inv) = inv();
    home(&mut inv);
    let places: Vec<String> = ["Çekmece A", "Çekmece B", "Çekmece C"]
        .iter()
        .map(|n| add(&mut inv, n, "container", Some("K4x4"), None).to_string())
        .collect();
    add(&mut inv, "Ahşap vida", "item", Some(&places[0]), None);
    add(&mut inv, "Sunta vidası", "item", Some(&places[1]), None);
    add(&mut inv, "Uzun vidalar", "item", Some(&places[2]), None);
    add(&mut inv, "USB kablolar", "item", Some(&places[0]), None);
    add(&mut inv, "Şarj kablosu", "item", Some(&places[1]), None);
    add(&mut inv, "Hafıza kartı", "item", Some(&places[0]), None);
    add(&mut inv, "Hafıza kart", "item", Some(&places[1]), None);
    add(&mut inv, "Yazıcı kartuşu", "item", Some(&places[2]), None);
    let v = inv.audit().unwrap();
    let row = |word: &str| {
        v["spread"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["word"] == word)
            .unwrap_or_else(|| panic!("no `{word}` row in {v}"))
            .clone()
    };
    let vida = row("vida");
    assert_eq!(vida["places"].as_array().unwrap().len(), 3);
    assert_eq!(
        vida["forms"],
        serde_json::json!(["vida", "vidalar", "vidasi"])
    );
    // Neither form stands alone, but they share the stem "kablo".
    assert_eq!(
        row("kablosu")["forms"],
        serde_json::json!(["kablolar", "kablosu"])
    );
    // "kartuşu" folds to "kartusu" and must not collapse into "kart".
    assert_eq!(row("kart")["forms"], serde_json::json!(["kart", "karti"]));
}
