//! The data directory can move: files in the store are kept relative to the database, so a
//! copy elsewhere (another machine, a copy opened with --db) shows its own photos and documents.

use std::path::Path;

use ev_core::{Inventory, NewNode};

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

#[test]
fn a_moved_data_directory_shows_its_own_photos_and_documents() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first");
    let photo = dir.path().join("drawer.png");
    image::RgbImage::from_pixel(20, 10, image::Rgb([9, 9, 9]))
        .save(&photo)
        .unwrap();
    let invoice = dir.path().join("invoice.pdf");
    std::fs::write(&invoice, b"%PDF-1.4\n%%EOF\n").unwrap();
    {
        let mut inv = Inventory::open(&first.join("ev.db")).unwrap();
        inv.add(NewNode {
            name: "Ev".into(),
            kind: "home".into(),
            ..Default::default()
        })
        .unwrap();
        inv.photo_add("Ev", &photo, None, None).unwrap();
        inv.doc_add(
            &invoice,
            &ev_core::NewDoc {
                kind: "invoice".into(),
                ..Default::default()
            },
            &["Ev".to_string()],
        )
        .unwrap();
    }
    let second = dir.path().join("second");
    copy_dir(&first, &second);
    std::fs::remove_dir_all(&first).unwrap();

    let inv = Inventory::open(&second.join("ev.db")).unwrap();
    let photos = inv.photo_list("Ev").unwrap()["photos"].clone();
    let path = photos[0]["path"].as_str().unwrap();
    assert!(path.starts_with(second.to_str().unwrap()), "{path}");
    assert_eq!(photos[0]["exists"], true);
    let shown = inv.show("Ev", false).unwrap();
    let file = shown["documents"][0]["file"].as_str().unwrap();
    assert!(file.starts_with(second.to_str().unwrap()), "{file}");
    assert!(Path::new(file).exists());
}

#[test]
fn schema_29_keeps_the_store_paths_of_an_older_inventory_relative() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ev.db");
    Inventory::open(&db)
        .unwrap()
        .add(NewNode {
            name: "Ev".into(),
            kind: "home".into(),
            ..Default::default()
        })
        .unwrap();
    // As schema 28 left it: absolute paths, one in the store and one outside it.
    let inside = dir.path().join("photos").join("a.jpg");
    {
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute(
            "INSERT INTO photos (node_id, position, path) VALUES (1, 0, ?1), (1, 1, '/elsewhere/b.jpg')",
            [inside.to_str().unwrap()],
        )
        .unwrap();
        // Back to 28: what schema 30 added goes too, or reopening adds it twice.
        c.execute_batch(
            "DROP INDEX nodes_thing; ALTER TABLE nodes DROP COLUMN thing;
             PRAGMA user_version = 28;",
        )
        .unwrap();
    }
    drop(Inventory::open(&db).unwrap());
    let c = rusqlite::Connection::open(&db).unwrap();
    let kept: Vec<String> = c
        .prepare("SELECT path FROM photos ORDER BY position")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(kept, ["photos/a.jpg", "/elsewhere/b.jpg"]);
    let version: i64 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, ev_core::SCHEMA_VERSION);
}
