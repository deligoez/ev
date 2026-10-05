//! Spec §17: the tidy-up plan — units, reviews, observations, tasks, `next`, goal.

use ev_core::{Error, Inventory, NewNode};
use serde_json::Value;
use tempfile::TempDir;

fn add(
    inv: &mut Inventory,
    name: &str,
    kind: &str,
    parent: Option<&str>,
    code: Option<&str>,
) -> i64 {
    let v = inv
        .add(NewNode {
            name: name.into(),
            kind: kind.into(),
            parent: parent.map(Into::into),
            code: code.map(Into::into),
            ..Default::default()
        })
        .unwrap();
    v["node"]["id"].as_i64().unwrap()
}

/// A Kallax with two labelled drawers (one holding an unlabelled box) and a loose box in the
/// room: three places to go through.
fn setup() -> (TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    add(&mut inv, "Ev", "home", None, None);
    add(&mut inv, "Oda", "room", Some("Ev"), None);
    add(&mut inv, "Kallax", "furniture", Some("Oda"), Some("K1"));
    add(&mut inv, "Bölme", "container", Some("K1"), Some("K1-01"));
    add(&mut inv, "Üst", "container", Some("K1-01"), Some("K1-01-U"));
    add(&mut inv, "Alt", "container", Some("K1-01"), Some("K1-01-A"));
    add(&mut inv, "Vida kutusu", "container", Some("K1-01-A"), None);
    add(&mut inv, "Vida", "item", Some("Vida kutusu"), None);
    add(&mut inv, "Karton kutu", "container", Some("Oda"), None);
    (dir, inv)
}

fn status_of(progress: &Value, name: &str) -> String {
    progress["places"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("{name} is not a unit: {progress}"))["review"]["status"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn units_are_the_innermost_labelled_places_and_loose_holders() {
    let (_d, inv) = setup();
    let p = inv.progress().unwrap();
    let names: Vec<&str> = p["places"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Üst", "Alt", "Karton kutu"], "{p}");
    assert_eq!(p["raw"], 3);
}

#[test]
fn a_drawer_with_labelled_boxes_of_another_series_stays_one_place() {
    let (_d, mut inv) = setup();
    // Bins labelled from their own series stand in the upper drawer; a drawer of the unit carries
    // the unit's code. The drawer is still the place gone through, its bins with it.
    add(
        &mut inv,
        "Kutu",
        "container",
        Some("K1-01-U"),
        Some("B1_001"),
    );
    add(
        &mut inv,
        "Kutu",
        "container",
        Some("K1-01-U"),
        Some("B1-002"),
    );
    add(&mut inv, "Kalem kutusu", "container", Some("K1-01-U"), None);
    let p = inv.progress().unwrap();
    let names: Vec<&str> = p["places"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Üst", "Alt", "Karton kutu"], "{p}");
    // A bin in it is counted with the drawer, not on its own.
    inv.photo_current("K1-01-U").unwrap();
    inv.review("K1-01-U", "toured", None).unwrap();
    let v = inv.progress().unwrap();
    assert_eq!(status_of(&v, "Üst"), "toured");
    assert_eq!(v["toured"], 1);
}

#[test]
fn a_lost_thing_found_elsewhere_leaves_its_last_seen_place_unchanged() {
    let (_d, mut inv) = setup();
    add(&mut inv, "Gözlük", "item", Some("K1-01-U"), None);
    inv.mark_lost("Gözlük").unwrap();
    inv.photo_current("K1-01-U").unwrap();
    inv.review("K1-01-U", "toured", None).unwrap();
    // A second later the glasses turn up in the cardboard box.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    inv.found_in("Gözlük", "Karton kutu").unwrap();
    let p = inv.progress().unwrap();
    // The drawer never held them since it was counted: nothing in it changed.
    assert_eq!(p["changed_since_tour"], 0, "{p}");
    let tree = inv.tree(Some("K1-01"), None).unwrap();
    assert!(!tree.to_string().contains("changed_since"), "{tree}");
}

#[test]
fn records_catching_up_minutes_after_a_photo_are_named_as_such() {
    let (d, mut inv) = setup();
    let photo = d.path().join("p.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3]))
        .save(&photo)
        .unwrap();
    inv.photo_add("K1-01-A", &photo, None, None).unwrap();
    // A second later what the photo shows is recorded: the photo reads as older.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    add(&mut inv, "Kapak", "item", Some("K1-01-A"), None);
    let err = inv.review("K1-01-A", "toured", None).unwrap_err();
    assert_eq!(err.code(), 5);
    assert!(err.to_string().contains("caught up"), "{err}");
    let ev_core::Error::Refused { details, .. } = err else {
        panic!("refused");
    };
    assert_eq!(details["stale"][0]["minutes_after"], 0);
}

#[test]
fn a_review_covers_everything_below_and_notices_later_changes() {
    let (_d, mut inv) = setup();
    // No photo of the drawer: touring it needs one, or the person's word that none is needed.
    assert_eq!(inv.review("K1-01", "toured", None).unwrap_err().code(), 5);
    inv.photo_current("K1-01").unwrap();
    inv.review("K1-01", "toured", None).unwrap();
    let p = inv.progress().unwrap();
    assert_eq!(status_of(&p, "Alt"), "toured");
    assert_eq!(status_of(&p, "Üst"), "toured");
    assert_eq!(p["changed_since_tour"], 0);

    // A second later a new thing lands in the toured drawer.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    add(&mut inv, "Pul", "item", Some("K1-01-A"), None);
    let p = inv.progress().unwrap();
    assert_eq!(p["changed_since_tour"], 1, "{p}");

    inv.review("Karton kutu", "kept", Some("leave it")).unwrap();
    assert_eq!(status_of(&inv.progress().unwrap(), "Karton kutu"), "kept");
    inv.review("Karton kutu", "raw", None).unwrap();
    assert_eq!(status_of(&inv.progress().unwrap(), "Karton kutu"), "raw");
    assert_eq!(inv.review("K1-01-U", "done", None).unwrap_err().code(), 2);
}

#[test]
fn observations_show_on_the_place_and_can_be_removed() {
    let (_d, mut inv) = setup();
    let v = inv
        .observe("K1-01-A", "screws loose in a bag", None)
        .unwrap();
    let obs = v["observations"].as_array().unwrap();
    assert_eq!(obs.len(), 1);
    assert_eq!(obs[0]["text"], "screws loose in a bag");
    assert_eq!(
        inv.observe("K1-01-A", "x", Some(1)).unwrap_err().code(),
        3,
        "no photo 1"
    );
    let id = obs[0]["id"].as_i64().unwrap();
    let v = inv.unobserve(id).unwrap();
    assert!(v["observations"].as_array().unwrap().is_empty());
    // The removal is in the place's history with the text it removed.
    let h = inv.history("K1-01-A").unwrap();
    let last = h["events"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["type"], "unobserve");
    assert_eq!(last["data"]["text"], "screws loose in a bag");
    assert_eq!(inv.unobserve(id).unwrap_err().code(), 3);
}

#[test]
fn tasks_keep_an_order_and_one_is_in_progress() {
    let (_d, mut inv) = setup();
    let a = inv
        .task_add("Tour Alt", "bags", &["K1-01-A".into()], None)
        .unwrap();
    let b = inv
        .task_add("Open box", "never opened", &["Karton kutu".into()], Some(1))
        .unwrap();
    let (a, b) = (a["id"].as_i64().unwrap(), b["id"].as_i64().unwrap());
    let order = |inv: &Inventory| -> Vec<i64> {
        inv.task_list(false).unwrap()["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_i64().unwrap())
            .collect()
    };
    assert_eq!(order(&inv), [b, a]);
    inv.task_edit(a, None, None, &[], &[], Some(1)).unwrap();
    assert_eq!(order(&inv), [a, b]);

    inv.task_set(b, "doing", None).unwrap();
    inv.task_set(a, "doing", None).unwrap();
    assert_eq!(inv.task_show(b).unwrap()["status"], "open");

    let done = inv.task_set(a, "done", Some("person said so")).unwrap();
    assert!(done["position"].is_null() && done["closed_at"].is_string());
    assert_eq!(order(&inv), [b]);
    assert_eq!(
        inv.task_list(true).unwrap()["tasks"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(inv.task_add("  ", "why", &[], None).unwrap_err().code(), 2);
    assert!(matches!(inv.task_show(99).unwrap_err(), Error::NotFound(_)));
}

#[test]
fn a_done_task_is_not_dropped_without_reopening_it() {
    let (_d, mut inv) = setup();
    let t = inv
        .task_add("Tour Alt", "bags", &["K1-01-A".into()], None)
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    inv.task_set(t, "done", None).unwrap();
    assert_eq!(inv.task_set(t, "dropped", None).unwrap_err().code(), 5);
    assert_eq!(inv.task_show(t).unwrap()["status"], "done");
    inv.task_set(t, "open", None).unwrap();
    inv.task_set(t, "dropped", None).unwrap();
    assert_eq!(inv.task_show(t).unwrap()["status"], "dropped");
}

#[test]
fn a_node_shows_the_tasks_on_it_and_on_the_places_holding_it() {
    let (_d, mut inv) = setup();
    let drawer = inv
        .task_add("Tour Alt", "bags", &["K1-01-A".into()], None)
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let screw = inv
        .task_add("Measure", "unknown size", &["Vida".into()], None)
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let v = inv.show("Vida", false).unwrap();
    let tasks: Vec<(i64, i64)> = v["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| (t["id"].as_i64().unwrap(), t["via"].as_i64().unwrap()))
        .collect();
    let box_id = inv.resolve("Vida kutusu", false).unwrap();
    let alt = inv.resolve("K1-01-A", false).unwrap();
    let vida = inv.resolve("Vida", false).unwrap();
    assert_eq!(tasks, [(screw, vida), (drawer, alt)]);
    assert_eq!(
        inv.show("Vida kutusu", false).unwrap()["tasks"][0]["via"],
        alt
    );
    inv.task_set(drawer, "done", None).unwrap();
    assert!(
        inv.show(&box_id.to_string(), false).unwrap()["tasks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn next_brings_the_task_its_places_what_arrives_and_the_gaps() {
    let (_d, mut inv) = setup();
    add(&mut inv, "Alyan", "item", Some("Karton kutu"), None);
    inv.move_to("Alyan", "K1-01-U", true).unwrap();
    inv.task_add(
        "Tour Üst",
        "the hex keys go there",
        &["K1-01-U".into()],
        None,
    )
    .unwrap();
    let v = inv.next().unwrap();
    assert_eq!(v["task"]["title"], "Tour Üst");
    let place = &v["task"]["places"][0];
    assert_eq!(place["node"]["code"], "K1-01-U");
    assert_eq!(place["arriving"][0]["name"], "Alyan");
    let gaps: Vec<&str> = v["unplanned"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(gaps, ["Alt", "Karton kutu"]);

    // A household that only wants records gets no tidy-up gaps.
    inv.goal(Some("track")).unwrap();
    let v = inv.next().unwrap();
    assert_eq!(v["goal"], "track");
    assert!(v["unplanned"].as_array().unwrap().is_empty());
    assert_eq!(inv.goal(Some("tidy")).unwrap_err().code(), 2);
}

/// How far a place is counted, as `ev tree` shows it.
fn count(inv: &Inventory, r: &str) -> Value {
    inv.tree(Some(r), Some(0)).unwrap()["tree"][0]["count"].clone()
}

#[test]
fn a_task_begun_counts_nothing_until_work_in_its_place_starts() {
    let (_d, mut inv) = setup();
    let t = inv
        .task_add("Kutuyu say", "hiç açılmadı", &["Karton kutu".into()], None)
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    // Started, nobody has opened it yet (spec/counting.md).
    inv.task_set(t, "doing", None).unwrap();
    assert_eq!(count(&inv, "Karton kutu"), "raw");
    assert_eq!(inv.progress().unwrap()["counting"], 0);
    // Something recorded inside it: it is being counted.
    add(&mut inv, "Pense", "item", Some("Karton kutu"), None);
    assert_eq!(count(&inv, "Karton kutu"), "counting");
    // Dropped half way: what was begun stays begun.
    inv.task_set(t, "dropped", None).unwrap();
    assert_eq!(count(&inv, "Karton kutu"), "counting");
    // A place settled stays so when its task closes.
    inv.task_set(t, "doing", None).unwrap();
    inv.review("Karton kutu", "kept", None).unwrap();
    inv.task_set(t, "done", None).unwrap();
    assert_eq!(count(&inv, "Karton kutu"), "kept");
}

#[test]
fn an_empty_room_is_a_place_and_what_is_above_places_has_no_count() {
    let (_d, mut inv) = setup();
    add(&mut inv, "Mutfak", "room", Some("Ev"), None);
    let places: Vec<String> = inv.progress().unwrap()["places"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_string())
        .collect();
    assert!(places.contains(&"Mutfak".to_string()), "{places:?}");
    assert_eq!(count(&inv, "Mutfak"), "raw");
    // A room with furniture is counted through it, a box inside a drawer with the drawer.
    assert!(count(&inv, "Oda").is_null());
    assert!(count(&inv, "Vida kutusu").is_null());
    assert_eq!(count(&inv, "K1-01-A"), "raw");
}

#[test]
fn next_and_todo_say_how_many_places_are_being_counted() {
    let (_d, mut inv) = setup();
    let t = inv
        .task_add("Kutuyu say", "hiç açılmadı", &["Karton kutu".into()], None)
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    inv.task_set(t, "doing", None).unwrap();
    add(&mut inv, "Pense", "item", Some("Karton kutu"), None);
    // The summary both carry is the full progress, not a part of it.
    assert_eq!(inv.next().unwrap()["progress"]["counting"], 1);
    assert_eq!(inv.todo().unwrap()["progress"]["counting"], 1);
}

#[test]
fn next_leaves_out_the_fields_of_a_place_that_have_no_value() {
    let (_d, mut inv) = setup();
    inv.task_add("Tour Alt", "bags", &["K1-01-A".into()], None)
        .unwrap();
    let v = inv.next().unwrap();
    let place = v["task"]["places"][0].as_object().unwrap();
    let empty: Vec<&String> = place
        .iter()
        .filter(|(_, x)| {
            x.is_null()
                || x.as_array().is_some_and(Vec::is_empty)
                || x.as_object()
                    .is_some_and(|o| o.values().all(serde_json::Value::is_null))
        })
        .map(|(k, _)| k)
        .collect();
    assert!(empty.is_empty(), "{empty:?}");
    assert!(v.get("hints").is_none(), "{v}");
    // The task the places are for is said above, not again on each place.
    assert!(place.get("tasks").is_none(), "{v}");
}

#[test]
fn work_outside_the_task_in_progress_counts_no_place() {
    let (_d, mut inv) = setup();
    // A thing put away with no tour going on.
    add(&mut inv, "Pense", "item", Some("Karton kutu"), None);
    assert_eq!(count(&inv, "Karton kutu"), "raw");
    // A task about another place: still nothing here.
    let t = inv
        .task_add("Çekmeceyi say", "hiç açılmadı", &["K1-01-A".into()], None)
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    inv.task_set(t, "doing", None).unwrap();
    add(&mut inv, "Tornavida", "item", Some("Karton kutu"), None);
    assert_eq!(count(&inv, "Karton kutu"), "raw");
    // In the task's place, it is work in it; the history says what began it.
    add(&mut inv, "Matkap ucu", "item", Some("K1-01-A"), None);
    assert_eq!(count(&inv, "K1-01-A"), "counting");
    let events = inv.history("K1-01-A").unwrap()["events"].clone();
    assert!(
        events
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "review" && e["data"]["by"] == "create"),
        "{events}"
    );
}
