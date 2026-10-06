//! Past belongings (spec/past-belongings.md): a thing that left long ago, recorded as it is
//! remembered.

use ev_core::{Disposition, Inventory, NewNode};

fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent) in [
        ("Ev", "home", None),
        ("Oda", "room", Some("Ev")),
        ("Eski telefon", "item", Some("Oda")),
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
fn a_thing_that_left_long_ago_keeps_when_and_where() {
    let (_d, mut inv) = setup();
    inv.gone_left(
        "Eski telefon",
        Some(Disposition::Left),
        None,
        false,
        None,
        Some("2016-06"),
        Some("Eski ev"),
    )
    .unwrap();
    let v = inv.show("Eski telefon", true).unwrap();
    assert_eq!(v["node"]["disposition"], "left");
    assert_eq!(v["departure"]["at"], "2016-06");
    assert_eq!(v["departure"]["where"], "Eski ev");
    // A date nobody could say, and a how nothing is set aside for, are refused.
    let (_d, mut inv) = setup();
    let bad = inv.gone_left(
        "Eski telefon",
        Some(Disposition::Sell),
        None,
        false,
        None,
        Some("2016-13"),
        None,
    );
    assert_eq!(bad.unwrap_err().code(), 2);
    assert_eq!(
        inv.dispose("Eski telefon", Disposition::Stolen)
            .unwrap_err()
            .code(),
        2
    );
    // With no date said, the day it was recorded stands for it.
    inv.gone_left(
        "Eski telefon",
        Some(Disposition::Unknown),
        None,
        false,
        None,
        None,
        None,
    )
    .unwrap();
    let v = inv.show("Eski telefon", true).unwrap();
    assert_eq!(v["departure"]["at"].as_str().unwrap().len(), 10);
}

#[test]
fn a_past_thing_is_added_already_gone_in_no_holder() {
    let (_d, mut inv) = setup();
    let v = inv
        .add(NewNode {
            name: "Oyun konsolu".into(),
            kind: "item".into(),
            gone: Some("sell".into()),
            at: Some("2016".into()),
            came: Some("2012-11".into()),
            place: Some("Eski ev".into()),
            ..Default::default()
        })
        .unwrap();
    let id = v["node"]["id"].as_i64().unwrap();
    let v = inv.show(&id.to_string(), true).unwrap();
    assert_eq!(v["node"]["state"], "gone");
    assert_eq!(v["node"]["disposition"], "sell");
    assert_eq!(v["came"], "2012-11");
    assert_eq!(v["departure"]["at"], "2016");
    assert_eq!(v["departure"]["where"], "Eski ev");
    // Never in the tree or the tour.
    let tree = inv.tree(None, None).unwrap().to_string();
    assert!(!tree.contains("Oyun konsolu"), "{tree}");
    // A past thing in a holder, or a when without a gone, is refused.
    let in_a_room = inv.add(NewNode {
        name: "Klavye".into(),
        kind: "item".into(),
        parent: Some("Oda".into()),
        gone: Some("sell".into()),
        ..Default::default()
    });
    assert_eq!(in_a_room.unwrap_err().code(), 2);
    let at_alone = inv.add(NewNode {
        name: "Klavye".into(),
        kind: "item".into(),
        parent: Some("Oda".into()),
        at: Some("2016".into()),
        ..Default::default()
    });
    assert_eq!(at_alone.unwrap_err().code(), 2);
}

#[test]
fn when_a_thing_came_is_edited_as_remembered() {
    let (_d, mut inv) = setup();
    inv.edit("Eski telefon", &["came=2014-03".into()]).unwrap();
    assert_eq!(inv.show("Eski telefon", false).unwrap()["came"], "2014-03");
    assert_eq!(
        inv.edit("Eski telefon", &["came=14 Mart".into()])
            .unwrap_err()
            .code(),
        2
    );
    inv.edit("Eski telefon", &["came=".into()]).unwrap();
    assert!(inv.show("Eski telefon", false).unwrap()["came"].is_null());
}

#[test]
fn a_purchase_of_a_past_thing_is_settled_by_linking_it() {
    let (_d, mut inv) = setup();
    inv.gone_left(
        "Eski telefon",
        Some(Disposition::Sell),
        None,
        false,
        None,
        Some("2015"),
        None,
    )
    .unwrap();
    let line = inv
        .buy_add(
            &serde_json::json!({"name": "Telefon", "qty": 1, "paid": "700.00", "currency": "EUR"}),
            None,
        )
        .unwrap()["purchase"]["id"]
        .as_i64()
        .unwrap();
    let open = |inv: &Inventory| {
        inv.buy_list(true, None, None, None).unwrap()["purchases"]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!(open(&inv), 1);
    inv.buy_link(line, "Eski telefon", None).unwrap();
    assert_eq!(open(&inv), 0);
    // A purchase entered by hand for a past thing links at once.
    inv.buy_add(
        &serde_json::json!({"name": "Ekran seti", "qty": 1, "paid": "40.00", "currency": "EUR"}),
        Some("Eski telefon"),
    )
    .unwrap();
    assert_eq!(open(&inv), 0);
}

#[test]
fn what_a_sale_brought_is_recorded_and_said_again_corrected() {
    let (_d, mut inv) = setup();
    // Nothing left as sold yet: refused.
    assert_eq!(
        inv.sold("Eski telefon", "1500", None, None, None, None)
            .unwrap_err()
            .code(),
        5
    );
    inv.gone_left(
        "Eski telefon",
        Some(Disposition::Sell),
        None,
        false,
        None,
        Some("2019"),
        None,
    )
    .unwrap();
    let v = inv
        .sold(
            "Eski telefon",
            "1500",
            None,
            Some("2019-05"),
            Some("Bir pazar yeri"),
            None,
        )
        .unwrap();
    assert_eq!(v["departure"]["price"], "1500.00");
    assert_eq!(v["departure"]["currency"], "TRY");
    assert_eq!(v["departure"]["at"], "2019-05");
    assert_eq!(v["departure"]["via"], "Bir pazar yeri");
    // Said again, the price is corrected and what was not said again stays.
    let v = inv
        .sold("Eski telefon", "120", Some("eur"), None, None, None)
        .unwrap();
    assert_eq!(v["departure"]["price"], "120.00");
    assert_eq!(v["departure"]["currency"], "EUR");
    assert_eq!(v["departure"]["via"], "Bir pazar yeri");
    for (price, currency) in [("0", None), ("120", Some("euro"))] {
        let bad = inv.sold("Eski telefon", price, currency, None, None, None);
        assert_eq!(bad.unwrap_err().code(), 2);
    }
}

#[test]
fn a_listed_sale_carries_where_it_went_through_not_what_it_asked() {
    let (_d, mut inv) = setup();
    inv.dispose("Eski telefon", Disposition::Sell).unwrap();
    inv.sale(
        "Eski telefon",
        Some("listed"),
        Some(2000),
        Some("Bir pazar yeri"),
        None,
    )
    .unwrap();
    inv.gone_left("Eski telefon", None, None, false, None, None, None)
        .unwrap();
    let v = inv.show("Eski telefon", true).unwrap();
    assert_eq!(v["departure"]["via"], "Bir pazar yeri");
    assert!(v["departure"]["price"].is_null(), "{v}");
}

/// A past thing added in one step, as remembered.
fn past_thing(inv: &mut Inventory, name: &str, how: &str, at: &str, came: Option<&str>) {
    inv.add(NewNode {
        name: name.into(),
        kind: "item".into(),
        gone: Some(how.into()),
        at: Some(at.into()),
        came: came.map(Into::into),
        place: Some("Eski ev".into()),
        ..Default::default()
    })
    .unwrap();
}

#[test]
fn the_past_lists_last_gone_first_with_what_each_year_cost_and_brought() {
    let (_d, mut inv) = setup();
    past_thing(&mut inv, "Oyun konsolu", "sell", "2019-05", Some("2016"));
    past_thing(&mut inv, "Eski klavye", "left", "2014", None);
    inv.sold("Oyun konsolu", "1500", None, None, None, None)
        .unwrap();
    inv.buy_add(
        &serde_json::json!({"name": "Konsol", "qty": 1, "paid": "700.00", "currency": "EUR"}),
        Some("Oyun konsolu"),
    )
    .unwrap();
    // A record that was never real is no past belonging.
    inv.gone_left(
        "Eski telefon",
        Some(Disposition::Mistake),
        Some("never here"),
        false,
        None,
        None,
        None,
    )
    .unwrap();
    let v = inv.past(None, None).unwrap()["remembered"].clone();
    let names: Vec<&str> = v["past"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Oyun konsolu", "Eski klavye"]);
    let first = &v["past"][0];
    assert_eq!(first["came"], "2016");
    assert_eq!(first["paid"]["EUR"], "700.00");
    assert_eq!(first["got"]["price"], "1500.00");
    assert_eq!(v["years"][0]["year"], 2019);
    assert_eq!(v["years"][0]["got"]["TRY"], "1500.00");
    assert_eq!(v["years"][1]["year"], 2014);
    assert_eq!(v["years"][1]["left"], 1);
    // By a word of the name, and by the place it was left in.
    assert_eq!(
        inv.past(Some("KLAVYE"), None).unwrap()["remembered"]["past"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        inv.past(None, Some("eski ev")).unwrap()["remembered"]["past"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    // The stats keep them apart from today's numbers.
    let stats = inv.stats().unwrap();
    assert_eq!(stats["past"]["records"], 2);
    assert_eq!(stats["past"]["got"]["TRY"], "1500.00");
}

#[test]
fn what_was_ours_in_a_year_counts_apart_what_nothing_dates() {
    let (_d, mut inv) = setup();
    past_thing(&mut inv, "Oyun konsolu", "sell", "2019-05", Some("2016"));
    past_thing(&mut inv, "Eski klavye", "left", "2014", Some("2012-03"));
    // A thing here today, its coming read off a linked purchase.
    inv.buy_add(
        &serde_json::json!({"name": "Telefon", "qty": 1, "paid": "500.00", "ordered_at": "2015-08-01"}),
        Some("Eski telefon"),
    )
    .unwrap();
    let names = |year: i32| -> Vec<String> {
        inv.past_year(year).unwrap()["owned"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(names(2014), ["Eski klavye"]);
    assert_eq!(names(2016), ["Eski telefon", "Oyun konsolu"]);
    assert_eq!(names(2019), ["Eski telefon", "Oyun konsolu"]);
    assert_eq!(names(2020), ["Eski telefon"]);
    // A thing nothing dates is counted apart, never guessed in.
    inv.add(NewNode {
        name: "Lamba".into(),
        kind: "item".into(),
        parent: Some("Oda".into()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(inv.past_year(2020).unwrap()["unknown"], 1);
    assert_eq!(inv.past_year(12).unwrap_err().code(), 2);
}

#[test]
fn a_batch_of_past_things_goes_in_at_once_as_things() {
    let (_d, mut inv) = setup();
    let lines: Vec<NewNode> = serde_json::from_value(serde_json::json!([
        {"name": "Eski tablet", "gone": "unknown", "at": "2017", "came": "2013", "where": "Eski ev"},
        {"name": "Kablo", "gone": "used", "at": "2018"},
    ]))
    .unwrap();
    inv.add_batch(lines).unwrap();
    let v = inv.past(None, None).unwrap();
    assert_eq!(v["remembered"]["past"].as_array().unwrap().len(), 2);
    let tablet = inv.show("Eski tablet", true).unwrap();
    assert_eq!(tablet["node"]["kind"], "item");
    assert_eq!(tablet["came"], "2013");
    assert_eq!(tablet["departure"]["where"], "Eski ev");
}

#[test]
fn a_past_thing_is_completed_as_remembered_but_never_placed() {
    let (_d, mut inv) = setup();
    past_thing(&mut inv, "Yalıtım paneli", "trash", "2024", Some("2023-10"));
    let id = inv.show("Yalıtım paneli", true).unwrap()["node"]["id"]
        .as_i64()
        .unwrap();
    let r = format!("#{id}");
    inv.edit(
        &r,
        &[
            "qty=10".into(),
            "model=XPS 30".into(),
            "name=Yalıtım levhası".into(),
        ],
    )
    .unwrap();
    let v = inv.show(&r, true).unwrap();
    assert_eq!(v["node"]["name"], "Yalıtım levhası");
    assert_eq!(v["node"]["qty"], 10);
    assert_eq!(v["node"]["model"], "XPS 30");
    // Where it stands describes a thing no longer here.
    assert_eq!(inv.edit(&r, &["fill=50".into()]).unwrap_err().code(), 5);
}

#[test]
fn a_past_thing_with_no_date_said_left_when_nothing_says() {
    let (_d, mut inv) = setup();
    inv.add(NewNode {
        name: "Yalıtım paneli".into(),
        kind: "item".into(),
        gone: Some("trash".into()),
        came: Some("2023-10".into()),
        ..Default::default()
    })
    .unwrap();
    // Not the day it was recorded: nobody said when.
    let v = inv.show("Yalıtım paneli", true).unwrap();
    assert!(v["departure"]["at"].is_null(), "{v}");
    let past = inv.past(None, None).unwrap()["remembered"].clone();
    assert!(past["years"].as_array().unwrap().is_empty(), "{past}");
    assert_eq!(past["undated"]["left"], 1);
    assert!(past["past"][0]["left"].is_null());
    // Ours the year it came; after that, not known, so counted apart.
    let names = |inv: &Inventory, year| {
        inv.past_year(year).unwrap()["owned"]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!(names(&inv, 2023), 1);
    assert_eq!(names(&inv, 2024), 0);
    // The panel, and the phone here today that nothing dates.
    assert_eq!(inv.past_year(2024).unwrap()["unknown"], 2);
    // A thing seen leaving today still left today.
    inv.gone_left(
        "Eski telefon",
        Some(Disposition::Trash),
        None,
        false,
        None,
        None,
        None,
    )
    .unwrap();
    let v = inv.show("Eski telefon", true).unwrap();
    assert_eq!(v["departure"]["at"].as_str().unwrap().len(), 10);
}

#[test]
fn a_past_thing_takes_its_documents_and_old_photos_by_id() {
    let (d, mut inv) = setup();
    past_thing(&mut inv, "Dizüstü", "give", "2020", Some("2017"));
    let id = inv.show("Dizüstü", true).unwrap()["node"]["id"]
        .as_i64()
        .unwrap();
    let r = format!("#{id}");
    let mail = d.path().join("sohbet.txt");
    std::fs::write(&mail, "price and serial").unwrap();
    inv.doc_add(
        &mail,
        &ev_core::NewDoc {
            kind: "other".into(),
            ..Default::default()
        },
        std::slice::from_ref(&r),
    )
    .unwrap();
    let photo = d.path().join("eski.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3]))
        .save(&photo)
        .unwrap();
    inv.photo_add(&r, &photo, None, None).unwrap();
    let v = inv.show(&r, true).unwrap();
    assert_eq!(v["documents"].as_array().unwrap().len(), 1, "{v}");
    assert_eq!(v["node"]["photos"].as_array().unwrap().len(), 1, "{v}");
    // By name a gone record stays out of reach, as everywhere.
    assert_eq!(
        inv.photo_add("Dizüstü", &photo, None, None)
            .unwrap_err()
            .code(),
        3
    );
}

#[test]
fn what_was_remembered_is_listed_apart_from_what_left_the_inventory() {
    let (_d, mut inv) = setup();
    // Seen leaving on a tour.
    inv.gone_left(
        "Eski telefon",
        Some(Disposition::Trash),
        None,
        false,
        None,
        None,
        None,
    )
    .unwrap();
    // Recorded long after it left, in one step or with a date said.
    past_thing(&mut inv, "Oyun konsolu", "sell", "2019", None);
    inv.add(NewNode {
        name: "Klavye".into(),
        kind: "item".into(),
        parent: Some("Oda".into()),
        ..Default::default()
    })
    .unwrap();
    inv.gone_left(
        "Klavye",
        Some(Disposition::Give),
        None,
        false,
        None,
        Some("2015"),
        None,
    )
    .unwrap();
    let v = inv.past(None, None).unwrap();
    let names = |key: &str| -> Vec<String> {
        v[key]["past"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(names("remembered"), ["Oyun konsolu", "Klavye"]);
    assert_eq!(names("left_inventory"), ["Eski telefon"]);
    // The stats still count both.
    assert_eq!(inv.stats().unwrap()["past"]["records"], 3);
}

#[test]
fn a_swap_leaves_as_a_trade_linked_to_what_came_in_exchange() {
    let (_d, mut inv) = setup();
    // First recorded as given, then corrected: it was a swap for the phone here today.
    past_thing(&mut inv, "Dizüstü", "give", "2021", Some("2018"));
    let laptop = inv.show("Dizüstü", true).unwrap()["node"]["id"]
        .as_i64()
        .unwrap();
    let v = inv
        .traded(&format!("#{laptop}"), Some("Eski telefon"))
        .unwrap();
    assert_eq!(v["node"]["disposition"], "trade");
    assert_eq!(v["departure"]["traded_for"]["name"], "Eski telefon");
    assert_eq!(v["departure"]["at"], "2021");
    // What came shows what went for it.
    let phone = inv.show("Eski telefon", false).unwrap();
    assert_eq!(phone["traded_from"][0]["id"], laptop);
    // Only a thing that left is traded, and never for itself.
    assert_eq!(inv.traded("Eski telefon", None).unwrap_err().code(), 5);
    assert_eq!(
        inv.traded(&format!("#{laptop}"), Some(&format!("#{laptop}")))
            .unwrap_err()
            .code(),
        2
    );
    // A trade can be set aside first, like a sale.
    inv.dispose("Eski telefon", Disposition::Trade).unwrap();
}

#[test]
fn what_is_never_a_thing_is_paid_for_apart_and_never_waits_to_be_linked() {
    let (_d, mut inv) = setup();
    for (name, bucket, paid) in [
        ("Geliştirici üyeliği", "digital", "99.00"),
        ("Kurulum hizmeti", "service", "3000.00"),
        ("Matkap", "durable", "1999.00"),
    ] {
        inv.buy_add(
            &serde_json::json!({"name": name, "qty": 1, "paid": paid, "bucket": bucket}),
            None,
        )
        .unwrap();
    }
    let p = inv.stats().unwrap()["purchases"].clone();
    assert_eq!(p["open_durable"], 1);
    let open = inv.buy_list(true, None, None, None).unwrap()["purchases"].clone();
    assert_eq!(open.as_array().unwrap().len(), 1, "{open}");
    let service = p["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["bucket"] == "service")
        .unwrap()
        .clone();
    assert_eq!(service["paid"]["TRY"], "3000.00");
    assert_eq!(service["lines"], 1);
}
