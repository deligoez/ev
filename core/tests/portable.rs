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
