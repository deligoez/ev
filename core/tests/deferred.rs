//! The rough edges deferred from the past-belongings release, each held by a test.

use ev_core::{Inventory, NewNode};

fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent) in [
        ("Ev", "home", None),
        ("Oda", "room", Some("Ev")),
        ("Kutu", "container", Some("Oda")),
        ("Telefon", "item", Some("Oda")),
        ("Tablet", "item", Some("Oda")),
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
fn several_planned_moves_are_made_at_once_or_none_is() {
    let (_d, mut inv) = setup();
    inv.move_to("Telefon", "Kutu", true).unwrap();
    inv.move_to("Tablet", "Kutu", true).unwrap();
    // One without a plan refuses the lot.
    let refused = inv.done_many(&["Telefon".into(), "Kutu".into()]);
    assert_eq!(refused.unwrap_err().code(), 5);
    assert!(!inv.show("Telefon", false).unwrap()["node"]["pending_to"].is_null());
    let v = inv.done_many(&["Telefon".into(), "Tablet".into()]).unwrap();
    assert_eq!(v["done"].as_array().unwrap().len(), 2, "{v}");
    assert!(
        inv.pending().unwrap()["pending"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn several_planned_moves_are_dropped_at_once() {
    let (_d, mut inv) = setup();
    inv.move_to("Telefon", "Kutu", true).unwrap();
    inv.move_to("Tablet", "Kutu", true).unwrap();
    let v = inv
        .cancel_many(&["Telefon".into(), "Tablet".into()])
        .unwrap();
    assert_eq!(v["cancelled"].as_array().unwrap().len(), 2, "{v}");
    assert!(
        inv.pending().unwrap()["pending"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

fn line(inv: &mut Inventory, name: &str) -> i64 {
    inv.buy_add(&serde_json::json!({"name": name, "qty": 1}), None)
        .unwrap()["purchase"]["id"]
        .as_i64()
        .unwrap()
}

#[test]
fn a_lines_bucket_is_said_by_hand_but_not_for_a_linked_line() {
    let (_d, mut inv) = setup();
    let l = line(&mut inv, "Üyelik");
    let v = inv.buy_bucket(l, "digital").unwrap();
    assert_eq!(v["purchase"]["bucket"], "digital");
    assert_eq!(inv.buy_bucket(l, "digital").unwrap_err().code(), 5);
    assert_eq!(inv.buy_bucket(l, "consumable").unwrap_err().code(), 2);
    let linked = line(&mut inv, "Telefon");
    inv.buy_link(linked, "Telefon", None).unwrap();
    assert_eq!(inv.buy_bucket(linked, "service").unwrap_err().code(), 5);
}

#[test]
fn an_import_that_does_not_say_a_lines_bucket_keeps_the_one_set_by_hand() {
    let (_d, mut inv) = setup();
    let line = r#"{"source":"mail","key":"r1","name":"Üyelik","qty":1}"#;
    inv.buy_import(line).unwrap();
    let id = inv.buy_list(false, None, None, None).unwrap()["purchases"][0]["id"]
        .as_i64()
        .unwrap();
    inv.buy_bucket(id, "digital").unwrap();
    inv.buy_import(line).unwrap();
    assert_eq!(inv.buy_show(id).unwrap()["purchase"]["bucket"], "digital");
    let said = r#"{"source":"mail","key":"r1","name":"Üyelik","qty":1,"bucket":"service"}"#;
    inv.buy_import(said).unwrap();
    assert_eq!(inv.buy_show(id).unwrap()["purchase"]["bucket"], "service");
}

#[test]
fn an_import_names_the_fields_it_does_not_read() {
    let (_d, mut inv) = setup();
    let v = inv
        .buy_import(r#"{"source":"mail","key":"r2","name":"Kablo","orderd_at":"2024-01-02"}"#)
        .unwrap();
    assert_eq!(v["imported"]["unknown_fields"]["orderd_at"], 1, "{v}");
}

fn vida(inv: &mut Inventory) {
    inv.add(NewNode {
        name: "Vida".into(),
        kind: "item".into(),
        parent: Some("Kutu".into()),
        qty: Some(10),
        ..Default::default()
    })
    .unwrap();
}

#[test]
fn some_of_a_thing_that_never_left_joins_the_rest_again() {
    use ev_core::Disposition;
    let (_d, mut inv) = setup();
    vida(&mut inv);
    let gone = inv
        .gone_left(
            "Vida",
            Some(Disposition::Trash),
            None,
            false,
            Some(4),
            None,
            None,
        )
        .unwrap();
    let portion = format!("#{}", gone["node"]["id"]);
    let back = inv.correct_gone(&portion, "they were in the box").unwrap();
    assert_eq!(back["node"]["qty"], 10, "{back}");
    assert_eq!(back["node"]["name"], "Vida");
}

#[test]
fn some_of_a_thing_set_aside_and_kept_joins_the_rest_again() {
    use ev_core::Disposition;
    let (_d, mut inv) = setup();
    vida(&mut inv);
    let aside = inv
        .dispose_qty("Vida", Disposition::Give, false, Some(3), None)
        .unwrap();
    let portion = format!("#{}", aside["node"]["id"]);
    let back = inv.restore(&portion).unwrap();
    assert_eq!(back["node"]["qty"], 10, "{back}");
}

#[test]
fn a_leaving_taken_back_takes_back_its_reason_from_the_note() {
    use ev_core::Disposition;
    let (_d, mut inv) = setup();
    inv.edit("Telefon", &["note=şarjı zayıf".into()]).unwrap();
    inv.gone_left(
        "Telefon",
        Some(Disposition::Mistake),
        Some("typed twice"),
        false,
        None,
        None,
        None,
    )
    .unwrap();
    let back = inv.correct_gone("4", "it was not a duplicate").unwrap();
    assert_eq!(back["node"]["note"], "şarjı zayıf", "{back}");
}

#[test]
fn a_split_off_part_carries_its_origin_in_history_not_in_an_english_note() {
    let (_d, mut inv) = setup();
    inv.split("Telefon", &[("Şarj kablosu".into(), Some(1))], None, None)
        .unwrap();
    let part = inv.show("Şarj kablosu", false).unwrap();
    assert!(part["node"]["note"].is_null(), "{part}");
}

#[test]
fn lines_are_listed_and_found_by_the_account_they_were_billed_to() {
    let (_d, mut inv) = setup();
    inv.buy_import(concat!(
        r#"{"source":"apple","key":"a1","name":"Uygulama","billed_to":"ayse@example.com"}"#,
        "\n",
        r#"{"source":"apple","key":"a2","name":"Oyun","billed_to":"mehmet@example.com"}"#,
    ))
    .unwrap();
    let v = inv
        .buy_list_billed(false, None, None, None, None, Some("AYSE"))
        .unwrap();
    let rows = v["purchases"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{v}");
    assert_eq!(rows[0]["billed_to"], "ayse@example.com");
    let q = inv
        .buy_list_matching(false, None, None, None, Some("mehmet"))
        .unwrap();
    assert_eq!(q["purchases"].as_array().unwrap().len(), 1, "{q}");
}

#[test]
fn a_reason_a_line_could_be_a_thing_carries_its_kind_and_value() {
    let (_d, mut inv) = setup();
    inv.edit("Telefon", &["make=Nokia".into()]).unwrap();
    inv.buy_import(r#"{"source":"mail","key":"t1","name":"Nokia telefon","brand":"Nokia"}"#)
        .unwrap();
    let v = inv.buy_for("Telefon").unwrap();
    let why = &v["candidates"][0]["why"];
    let brand = why
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["kind"] == "brand")
        .unwrap_or_else(|| panic!("{v}"));
    assert_eq!(brand["value"], "nokia");
    assert_eq!(brand["why"], "brand nokia");
}

#[test]
fn a_currency_is_a_code_in_use_not_any_three_letters() {
    let (_d, mut inv) = setup();
    let typo = inv.buy_import(r#"{"source":"mail","key":"c1","name":"Kablo","currency":"EUO"}"#);
    assert_eq!(typo.unwrap_err().code(), 2);
    let v = ev_core::NewValuation {
        amount: "100".into(),
        currency: Some("eur".into()),
        at: None,
        approximate: false,
        source: None,
        note: None,
    };
    let added = inv.value("Telefon", Some(&v)).unwrap();
    assert_eq!(added["valuations"][0]["currency"], "EUR", "{added}");
}

#[test]
fn a_batch_line_records_a_swap_or_nothing() {
    let (_d, mut inv) = setup();
    let bad = inv.add_batch(vec![NewNode {
        name: "Eski bisiklet".into(),
        gone: Some("trade".into()),
        traded_for: Some("Odaa".into()),
        ..Default::default()
    }]);
    assert_eq!(bad.unwrap_err().code(), 3);
    assert!(
        inv.past(None, None).unwrap()["remembered"]["past"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    inv.add_batch(vec![NewNode {
        name: "Eski bisiklet".into(),
        gone: Some("trade".into()),
        traded_for: Some("Tablet".into()),
        ..Default::default()
    }])
    .unwrap();
    let bike = inv.show("Eski bisiklet", true).unwrap();
    assert_eq!(bike["departure"]["traded_for"]["name"], "Tablet", "{bike}");
}

#[test]
fn purchase_stats_count_only_the_households_own_spending() {
    let (_d, mut inv) = setup();
    inv.buy_import(concat!(
        r#"{"source":"apple","key":"s1","name":"Uygulama","bucket":"digital","paid":"100"}"#,
        "\n",
        r#"{"source":"apple","key":"s2","name":"Oyun","bucket":"digital","paid":"50"}"#,
        "\n",
        r#"{"source":"apple","key":"s3","name":"Kitap","bucket":"digital","paid":"20"}"#,
    ))
    .unwrap();
    let ids: Vec<i64> = inv.buy_list(false, None, None, None).unwrap()["purchases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_i64().unwrap())
        .collect();
    let by_name = |n: &str| -> i64 {
        let v = inv
            .buy_list_matching(false, None, None, None, Some(n))
            .unwrap();
        v["purchases"][0]["id"].as_i64().unwrap()
    };
    let (oyun, kitap) = (by_name("oyun"), by_name("kitap"));
    assert_eq!(ids.len(), 3);
    inv.buy_dismiss(oyun, Some("not-mine"), None).unwrap();
    inv.buy_dismiss(kitap, Some("consumed"), None).unwrap();
    let stats = inv.stats().unwrap();
    let digital = stats["purchases"]["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["bucket"] == "digital")
        .cloned()
        .unwrap();
    assert_eq!(digital["lines"], 2, "{digital}");
}
