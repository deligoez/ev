//! Spec §31: a home sketched from a Sweet Home 3D plan.

use std::io::Write;

use ev_core::{Inventory, NewNode};
use serde_json::Value;

fn add(inv: &mut Inventory, name: &str, kind: &str, parent: Option<&str>) {
    inv.add(NewNode {
        name: name.into(),
        kind: kind.into(),
        parent: parent.map(Into::into),
        ..Default::default()
    })
    .unwrap();
}

/// A plan with an L-shaped hall and a balcony beside it, a table, a bed, and a door between.
const PLAN: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<home version="7400">
  <room name="Salon">
    <point x="100" y="100"/><point x="500" y="100"/><point x="500" y="300"/>
    <point x="300" y="300"/><point x="300" y="500"/><point x="100" y="500"/>
  </room>
  <room name="Balkon">
    <point x="520" y="100"/><point x="620" y="100"/><point x="620" y="200"/><point x="520" y="200"/>
  </room>
  <room name="Hol">
    <point x="100" y="520"/><point x="300" y="520"/><point x="300" y="600"/><point x="100" y="600"/>
  </room>
  <pieceOfFurniture name="Table" catalogId="eTeks#table" x="200" y="200" width="160" depth="80" height="74"/>
  <pieceOfFurniture name="Bed" catalogId="eTeks#bed" x="200" y="400" width="100" depth="200" height="70" angle="1.5707964"/>
  <pieceOfFurniture name="Lamp" catalogId="eTeks#lamp" x="900" y="900" width="30" depth="30" height="170" visible="false"/>
  <doorOrWindow name="Door" catalogId="eTeks#door" x="200" y="510" width="90" depth="20" height="210"/>
  <pieceOfFurniture name="Cupboard" catalogId="eTeks#wardrobe" x="1500" y="1500" width="60" depth="60" height="200"/>
  <pieceOfFurniture name="Chair" catalogId="eTeks#chair" x="800" y="200" width="40" depth="40" height="90"/>
  <wall xStart="690" yStart="90" xEnd="910" yEnd="90" thickness="20"/>
  <wall xStart="910" yStart="90" xEnd="910" yEnd="310" thickness="20"/>
  <wall xStart="910" yStart="310" xEnd="690" yEnd="310" thickness="20"/>
  <wall xStart="690" yStart="310" xEnd="690" yEnd="90" thickness="20"/>
  <wall xStart="1000" yStart="90" xEnd="1200" yEnd="90" thickness="20"/>
</home>"#;

fn plan(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("home.sh3d");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("Home.xml", opts).unwrap();
    zip.write_all(PLAN.as_bytes()).unwrap();
    zip.finish().unwrap();
    path
}

fn setup() -> (tempfile::TempDir, Inventory, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    add(&mut inv, "Ev", "home", None);
    add(&mut inv, "Salon", "room", Some("Ev"));
    add(&mut inv, "Salon balkonu", "room", Some("Salon"));
    add(&mut inv, "Mutfak", "room", Some("Ev"));
    add(&mut inv, "Masa", "furniture", Some("Salon"));
    let p = plan(dir.path());
    (dir, inv, p)
}

#[test]
fn a_plan_gives_rooms_their_outlines_and_marks_what_is_no_record() {
    let (_d, mut inv, p) = setup();
    let v = inv
        .sketch_import(
            &p,
            &["Balkon=Salon balkonu".into()],
            &["Table#1=Masa".into()],
            &[],
            false,
        )
        .unwrap();
    // Rooms by name, or as mapped; a plan room with no record and a record with no plan room
    // are listed, not made up.
    assert_eq!(v["unmatched"], serde_json::json!(["Hol"]));
    assert_eq!(v["not_in_plan"][0]["name"], "Mutfak");
    let salon = inv.sketch("Salon").unwrap();
    assert_eq!(salon["x"], 100.0);
    assert_eq!(salon["w"], 400.0);
    assert_eq!(salon["points"].as_array().unwrap().len(), 6);
    // The balcony lies in the hall's frame: its corners less the hall's corner.
    assert_eq!(
        inv.sketch("Salon balkonu").unwrap()["points"][0],
        serde_json::json!([420.0, 0.0])
    );
    // The table placed the record it was named for, in the hall's frame.
    let masa = inv.sketch("Masa").unwrap();
    assert_eq!(
        (masa["x"].clone(), masa["y"].clone()),
        (20.0.into(), 60.0.into())
    );
    assert_eq!(v["pieces"][0]["linked"]["name"], "Masa");
    // The bed, turned a quarter, and the door in the wall are marks; the hidden lamp is not.
    assert_eq!(v["marks"], 2);
    let m = inv.map(Some("Salon")).unwrap();
    assert_eq!(m["layout"], "sketch");
    let marks: Vec<&Value> = m["size"]["marks"].as_array().unwrap().iter().collect();
    assert_eq!(marks[0]["name"], "Bed");
    assert_eq!(marks[1]["kind"], "door");
    assert!(m["size"]["floor"].as_array().unwrap().len() == 6);
    // The home: the hall a tile of its own shape, the balcony drawn as part of it.
    let home = inv.map(None).unwrap();
    let t = &home["tiles"][0];
    assert_eq!(t["name"], "Salon");
    assert_eq!(t["shapes"].as_array().unwrap().len(), 2);
    // Importing again changes nothing; a dry run writes nothing.
    let again = inv
        .sketch_import(
            &p,
            &["Balkon=Salon balkonu".into()],
            &["Table#1=Masa".into()],
            &[],
            false,
        )
        .unwrap();
    assert!(
        again["rooms"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["changed"] == false)
    );
    let h = inv.history("Salon").unwrap()["events"]
        .as_array()
        .unwrap()
        .len();
    inv.sketch_import(&p, &[], &[], &[], true).unwrap();
    assert_eq!(
        inv.history("Salon").unwrap()["events"]
            .as_array()
            .unwrap()
            .len(),
        h
    );
}

#[test]
fn a_room_the_plan_did_not_draw_is_found_from_its_walls() {
    let (_d, mut inv, p) = setup();
    let v = inv
        .sketch_import(&p, &[], &[], &["Mutfak@800,200".into()], false)
        .unwrap();
    // The space inside the four walls, to their inner faces.
    let m = inv.sketch("Mutfak").unwrap();
    assert_eq!(
        (m["x"].clone(), m["y"].clone()),
        (700.0.into(), 100.0.into())
    );
    assert_eq!(
        (m["w"].clone(), m["d"].clone()),
        (200.0.into(), 200.0.into())
    );
    assert_eq!(m["points"].as_array().unwrap().len(), 4);
    // What stands in it is marked there now; a piece in no room at all is left out.
    let chair = v["pieces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["ref"] == "Chair#1");
    assert_eq!(chair.unwrap()["room"]["name"], "Mutfak");
    let cupboard = v["pieces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["ref"] == "Cupboard#1");
    assert!(cupboard.unwrap()["room"].is_null());
    // Bed, table (not placed on a record this time), door and chair.
    assert_eq!(v["marks"], 4);
    // A space open to the outside, a point in a wall, a point off the plan: refused.
    for bad in [
        "Mutfak@1100,150",
        "Mutfak@690,200",
        "Mutfak@5000,5000",
        "Mutfak",
    ] {
        let e = inv
            .sketch_import(&p, &[], &[], &[bad.into()], false)
            .unwrap_err();
        assert_eq!(e.code(), 2, "{bad}");
    }
}

#[test]
fn a_file_that_is_no_plan_is_refused() {
    let (d, mut inv, p) = setup();
    let not = d.path().join("x.sh3d");
    std::fs::write(&not, "hello").unwrap();
    assert_eq!(
        inv.sketch_import(&not, &[], &[], &[], false)
            .unwrap_err()
            .code(),
        2
    );
    let missing = d.path().join("none.sh3d");
    assert_eq!(
        inv.sketch_import(&missing, &[], &[], &[], false)
            .unwrap_err()
            .code(),
        3
    );
    assert_eq!(
        inv.sketch_import(&p, &["Salon".into()], &[], &[], false)
            .unwrap_err()
            .code(),
        2
    );
}
