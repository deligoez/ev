//! Photographed, then thrown out: `--as digitize` leaves only with a copy on the record, and
//! the record stays findable.

use ev_core::{Disposition, Inventory, NewDoc, NewNode};
use image::{Rgb, RgbImage};

fn setup() -> (tempfile::TempDir, Inventory) {
    let dir = tempfile::tempdir().unwrap();
    let mut inv = Inventory::open(&dir.path().join("ev.db")).unwrap();
    for (name, kind, parent) in [
        ("Ev", "home", None),
        ("Çekmece", "container", Some("Ev")),
        ("Konser bileti 2015", "item", Some("Çekmece")),
        ("Eski kimlik kartı", "item", Some("Çekmece")),
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

fn photo(dir: &tempfile::TempDir, name: &str, w: u32, h: u32) -> std::path::PathBuf {
    let p = dir.path().join(name);
    RgbImage::from_pixel(w, h, Rgb([200, 200, 200]))
        .save(&p)
        .unwrap();
    p
}

#[test]
fn a_digitized_thing_leaves_only_once_it_has_a_copy() {
    let (dir, mut inv) = setup();
    inv.dispose("Konser bileti 2015", Disposition::Digitize)
        .unwrap();
    let e = inv.gone("Konser bileti 2015", None).unwrap_err();
    assert_eq!(e.code(), 5, "{e}");
    let ticket = photo(&dir, "ticket.png", 1600, 900);
    inv.photo_add("Konser bileti 2015", &ticket, None, None)
        .unwrap();
    let v = inv.gone("Konser bileti 2015", None).unwrap();
    assert_eq!(v["node"]["state"], "gone");
    assert_eq!(v["node"]["disposition"], "digitize");
    assert!(v.get("warnings").is_none());
}
