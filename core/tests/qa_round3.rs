//! What the third QA round of the past-belongings release found, each held by a test.

use ev_core::{Disposition, Inventory, NewNode};

fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent) in [
        ("Ev", "home", None),
        ("Oda", "room", Some("Ev")),
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

fn id(inv: &Inventory, name: &str) -> String {
    format!(
        "#{}",
        inv.show(name, true).unwrap()["node"]["id"]
            .as_i64()
            .unwrap()
    )
}

fn gone(inv: &mut Inventory, name: &str, how: Disposition, at: Option<&str>) {
    inv.gone_left(name, Some(how), None, false, None, at, None)
        .unwrap();
}

#[test]
fn a_leaving_before_the_purchase_that_brought_it_is_refused() {
    let (_d, mut inv) = setup();
    let line = inv
        .buy_add(
            &serde_json::json!({"name": "Telefon", "qty": 1, "ordered_at": "2025-05-05"}),
            None,
        )
        .unwrap()["purchase"]["id"]
        .as_i64()
        .unwrap();
    inv.buy_link(line, "Telefon", None).unwrap();
    let early = inv.gone_left(
        "Telefon",
        Some(Disposition::Trash),
        None,
        false,
        None,
        Some("2016"),
        None,
    );
    assert_eq!(early.unwrap_err().code(), 2);
    gone(&mut inv, "Telefon", Disposition::Trash, None);
    let phone = id(&inv, "Telefon");
    assert_eq!(
        inv.edit(&phone, &["left=2014".into()]).unwrap_err().code(),
        2
    );
}

#[test]
fn a_former_place_given_as_a_bare_id_is_refused() {
    let (_d, mut inv) = setup();
    let bare = inv.gone_left(
        "Telefon",
        Some(Disposition::Give),
        None,
        false,
        None,
        None,
        Some("612"),
    );
    assert_eq!(bare.unwrap_err().code(), 2);
}

#[test]
fn a_sale_said_while_waiting_is_dropped_when_the_thing_is_given_instead() {
    let (_d, mut inv) = setup();
    inv.dispose("Telefon", Disposition::Sell).unwrap();
    let phone = id(&inv, "Telefon");
    inv.sold(&phone, "700", None, None, None, None).unwrap();
    gone(&mut inv, &phone, Disposition::Give, None);
    let v = inv.show(&phone, true).unwrap();
    assert!(v["departure"].is_object(), "{v}");
    assert!(v["departure"]["price"].is_null(), "{v}");
}

#[test]
fn a_sale_dated_before_the_thing_left_keeps_it_in_the_left_inventory_list() {
    let (_d, mut inv) = setup();
    inv.dispose("Telefon", Disposition::Sell).unwrap();
    let phone = id(&inv, "Telefon");
    inv.sold(&phone, "700", None, Some("2026-01"), None, None)
        .unwrap();
    gone(&mut inv, &phone, Disposition::Sell, None);
    let past = inv.past(None, None).unwrap();
    assert_eq!(past["left_inventory"]["past"].as_array().unwrap().len(), 1);
    assert!(past["remembered"]["past"].as_array().unwrap().is_empty());
}

#[test]
fn a_swap_is_said_of_a_giving_or_a_sale_only_and_once() {
    let (_d, mut inv) = setup();
    gone(&mut inv, "Telefon", Disposition::Trash, None);
    let phone = id(&inv, "Telefon");
    assert_eq!(inv.traded(&phone, None).unwrap_err().code(), 5);
}

#[test]
fn a_swap_already_recorded_is_not_written_again() {
    let (_d, mut inv) = setup();
    gone(&mut inv, "Telefon", Disposition::Trade, None);
    let phone = id(&inv, "Telefon");
    let before = inv.history(&phone).unwrap().to_string().len();
    assert_eq!(inv.traded(&phone, None).unwrap_err().code(), 5);
    assert_eq!(inv.history(&phone).unwrap().to_string().len(), before);
}

#[test]
fn a_swap_for_a_thing_that_had_already_left_or_was_ours_before_is_refused() {
    let (_d, mut inv) = setup();
    inv.add(NewNode {
        name: "Konsol".into(),
        gone: Some("give".into()),
        at: Some("2020".into()),
        ..Default::default()
    })
    .unwrap();
    inv.add(NewNode {
        name: "Saat".into(),
        gone: Some("give".into()),
        at: Some("2018".into()),
        ..Default::default()
    })
    .unwrap();
    let (konsol, saat) = (id(&inv, "Konsol"), id(&inv, "Saat"));
    assert_eq!(inv.traded(&konsol, Some(&saat)).unwrap_err().code(), 2);
    inv.edit("Tablet", &["came=2010".into()]).unwrap();
    assert_eq!(inv.traded(&konsol, Some("Tablet")).unwrap_err().code(), 2);
}

#[test]
fn a_gone_thing_is_traded_by_its_name() {
    let (_d, mut inv) = setup();
    gone(&mut inv, "Telefon", Disposition::Give, None);
    inv.traded("Telefon", Some("Tablet")).unwrap();
}

fn line(inv: &mut Inventory, name: &str, ordered: &str, bucket: &str) -> i64 {
    inv.buy_add(
        &serde_json::json!({"name": name, "qty": 1, "ordered_at": ordered, "bucket": bucket}),
        None,
    )
    .unwrap()["purchase"]["id"]
        .as_i64()
        .unwrap()
}

#[test]
fn a_thing_that_left_is_offered_no_line_bought_after_it_left() {
    let (_d, mut inv) = setup();
    gone(&mut inv, "Telefon", Disposition::Give, Some("2018"));
    let later = line(&mut inv, "Telefon", "2024-05-01", "durable");
    let earlier = line(&mut inv, "Telefon", "2017-05-01", "durable");
    let offered: Vec<i64> = inv.buy_for(&id(&inv, "Telefon")).unwrap()["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["purchase"]["id"].as_i64())
        .collect();
    assert!(offered.contains(&earlier), "{offered:?}");
    assert!(!offered.contains(&later), "{offered:?}");
}

#[test]
fn a_link_of_no_units_or_of_a_service_says_so() {
    let (_d, mut inv) = setup();
    let durable = line(&mut inv, "Telefon", "2024-05-01", "durable");
    let zero = inv.buy_link(durable, "Telefon", Some(0)).unwrap_err();
    assert!(zero.to_string().contains("at least 1"), "{zero}");
    let service = line(&mut inv, "Kurulum", "2024-05-01", "service");
    let never = inv.buy_link(service, "Telefon", None).unwrap_err();
    assert!(never.to_string().contains("service"), "{never}");
}

#[test]
fn a_past_things_line_can_be_declined_and_a_linked_one_cannot() {
    let (_d, mut inv) = setup();
    let first = line(&mut inv, "Telefon", "2017-05-01", "durable");
    let second = line(&mut inv, "Telefon kılıfı", "2017-05-01", "durable");
    inv.buy_link(first, "Telefon", None).unwrap();
    assert_eq!(
        inv.buy_decline(first, "Telefon", None, false)
            .unwrap_err()
            .code(),
        5
    );
    gone(&mut inv, "Telefon", Disposition::Give, None);
    let phone = id(&inv, "Telefon");
    inv.buy_decline(second, &phone, None, false).unwrap();
}

#[test]
fn a_purchase_is_never_linked_to_a_place_or_a_mistake() {
    let (_d, mut inv) = setup();
    let l = line(&mut inv, "Masa", "2017-05-01", "durable");
    assert_eq!(inv.buy_link(l, "Oda", None).unwrap_err().code(), 2);
    inv.gone_left(
        "Tablet",
        Some(Disposition::Mistake),
        Some("typed twice"),
        false,
        None,
        None,
        None,
    )
    .unwrap();
    let tablet = id(&inv, "Tablet");
    assert_eq!(inv.buy_link(l, &tablet, None).unwrap_err().code(), 5);
}

fn place(inv: &mut Inventory, name: &str, kind: &str, parent: &str, code: &str) {
    inv.add(NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: Some(parent.into()),
        code: Some(code.into()),
        ..Default::default()
    })
    .unwrap();
}

#[test]
fn drawers_labelled_after_their_cabinet_began_to_be_counted_are_not_counting() {
    let (_d, mut inv) = setup();
    place(&mut inv, "Dolap", "furniture", "Oda", "Q9");
    let t = inv
        .task_add("Dolabı say", "hiç sayılmadı", &["Q9".into()], None)
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    inv.task_set(t, "doing", None).unwrap();
    inv.move_to("Tablet", "Q9", false).unwrap();
    let count =
        |inv: &Inventory, r: &str| inv.tree(Some(r), Some(0)).unwrap()["tree"][0]["count"].clone();
    assert_eq!(count(&inv, "Q9"), "counting");
    place(&mut inv, "Üst çekmece", "container", "Q9", "Q9-A");
    place(&mut inv, "Alt çekmece", "container", "Q9", "Q9-B");
    assert_eq!(count(&inv, "Q9-B"), "raw");
}

#[test]
fn a_thing_is_never_reviewed_as_a_place() {
    let (_d, mut inv) = setup();
    assert_eq!(inv.review("Telefon", "toured", None).unwrap_err().code(), 2);
    assert_eq!(inv.review("Telefon", "kept", None).unwrap_err().code(), 2);
}

#[test]
fn a_place_with_no_photo_has_none_to_call_current() {
    let (_d, mut inv) = setup();
    assert_eq!(inv.photo_current("Oda").unwrap_err().code(), 5);
}

#[test]
fn a_leaving_taken_back_or_a_mistake_is_not_counted_as_gone() {
    let (_d, mut inv) = setup();
    gone(&mut inv, "Telefon", Disposition::Stolen, None);
    let phone = id(&inv, "Telefon");
    inv.correct_gone(&phone, "it was in the car").unwrap();
    inv.gone_left(
        "Tablet",
        Some(Disposition::Mistake),
        Some("typed twice"),
        false,
        None,
        None,
        None,
    )
    .unwrap();
    let gone = &inv.stats().unwrap()["activity"]["gone"];
    assert!(gone.as_object().unwrap().is_empty(), "{gone}");
}

#[test]
fn a_past_thing_is_not_counted_as_added() {
    let (_d, mut inv) = setup();
    let before = inv.stats().unwrap()["activity"]["added"].as_i64().unwrap();
    inv.add(NewNode {
        name: "Eski saat".into(),
        gone: Some("give".into()),
        ..Default::default()
    })
    .unwrap();
    let after = inv.stats().unwrap()["activity"]["added"].as_i64().unwrap();
    assert_eq!(after, before);
}

#[test]
fn a_file_that_is_no_image_is_refused_as_a_photo() {
    let (d, mut inv) = setup();
    let text = d.path().join("not-a-photo.jpg");
    std::fs::write(&text, "hello").unwrap();
    assert_eq!(
        inv.photo_add("Telefon", &text, None, None)
            .unwrap_err()
            .code(),
        2
    );
}

#[test]
fn broken_said_again_without_a_note_keeps_the_note() {
    let (_d, mut inv) = setup();
    inv.broken("Telefon", Some("ekran çatlak"), false).unwrap();
    let v = inv.broken("Telefon", None, false).unwrap();
    assert_eq!(v["marks"]["broken"]["note"], "ekran çatlak", "{v}");
}

#[test]
fn a_need_for_none_is_refused() {
    let (_d, mut inv) = setup();
    assert_eq!(
        inv.need_add("Vida", Some(0), false, None, None)
            .unwrap_err()
            .code(),
        2
    );
}

#[test]
fn a_value_dated_in_the_future_is_refused() {
    let (_d, mut inv) = setup();
    let v = ev_core::NewValuation {
        amount: "100".into(),
        currency: None,
        at: Some("2099-01-01".into()),
        approximate: false,
        source: None,
        note: None,
    };
    assert_eq!(inv.value("Telefon", Some(&v)).unwrap_err().code(), 2);
}

#[test]
fn a_purchase_recorded_by_hand_in_the_future_is_refused() {
    let (_d, mut inv) = setup();
    let future = inv.buy_add(
        &serde_json::json!({"name": "Telefon", "qty": 1, "ordered_at": "2099-01-01"}),
        None,
    );
    assert_eq!(future.unwrap_err().code(), 2);
}

#[test]
fn closing_a_task_as_it_already_is_is_refused() {
    let (_d, mut inv) = setup();
    let t = inv
        .task_add("Odayı say", "hiç sayılmadı", &["Oda".into()], None)
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    inv.task_set(t, "done", None).unwrap();
    assert_eq!(inv.task_set(t, "done", None).unwrap_err().code(), 5);
    inv.task_set(t, "open", None).unwrap();
    inv.task_set(t, "dropped", None).unwrap();
    assert_eq!(inv.task_set(t, "dropped", None).unwrap_err().code(), 5);
}

#[test]
fn clearing_a_line_a_coverage_never_had_is_refused() {
    let (_d, mut inv) = setup();
    let c = inv
        .cover_add(
            &["Telefon".into()],
            &ev_core::NewCoverage {
                kind: "extended".into(),
                term: Some("2y".into()),
                ..Default::default()
            },
        )
        .unwrap()["coverage"]["id"]
        .as_i64()
        .unwrap();
    assert_eq!(inv.cover_purchase(c, None).unwrap_err().code(), 5);
}

#[test]
fn a_thing_that_left_is_proposed_no_statutory_warranty() {
    let (_d, mut inv) = setup();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let l = line(&mut inv, "Telefon", &today, "durable");
    inv.buy_link(l, "Telefon", None).unwrap();
    let here = inv.show("Telefon", false).unwrap();
    assert!(here["coverage_proposal"].is_object(), "{here}");
    gone(&mut inv, "Telefon", Disposition::Give, None);
    let v = inv.show(&id(&inv, "Telefon"), true).unwrap();
    assert!(v["coverage_proposal"].is_null(), "{v}");
}

#[test]
fn a_portion_that_joined_another_says_which_one() {
    let (_d, mut inv) = setup();
    inv.add(NewNode {
        name: "Vida".into(),
        kind: "item".into(),
        parent: Some("Oda".into()),
        qty: Some(4),
        ..Default::default()
    })
    .unwrap();
    place(&mut inv, "Kutu", "container", "Oda", "Q1");
    let vida = id(&inv, "Vida");
    let portion = inv.move_qty(&vida, "Q1", false, Some(1)).unwrap()["node"]["id"]
        .as_i64()
        .unwrap();
    inv.move_qty(&format!("#{portion}"), "Oda", false, None)
        .unwrap();
    let e = inv.show(&format!("#{portion}"), false).unwrap_err();
    assert!(e.to_string().contains(&format!("joined {vida}")), "{e}");
}

#[test]
fn a_thing_here_this_year_with_no_coming_date_was_ours_this_year() {
    use chrono::Datelike;
    let (_d, inv) = setup();
    let year = chrono::Local::now().year();
    let v = inv.past_year(year).unwrap();
    let names: Vec<&str> = v["owned"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| n["name"].as_str())
        .collect();
    assert!(names.contains(&"Telefon"), "{v}");
    assert_eq!(v["unknown"], 0, "{v}");
    let last = inv.past_year(year - 1).unwrap();
    assert_eq!(last["unknown"], 2, "{last}");
}
