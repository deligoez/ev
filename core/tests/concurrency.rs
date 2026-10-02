//! Several processes write one inventory at once: an MCP client's parallel calls, the CLI and
//! `ev ui` beside it. Writers wait their turn instead of failing with "database is locked".

use std::sync::{Arc, Barrier};

use ev_core::{Inventory, NewNode};

#[test]
fn concurrent_writers_wait_for_each_other_instead_of_failing() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ev.db");
    let home = Inventory::open(&db)
        .unwrap()
        .add(NewNode {
            name: "Ev".into(),
            kind: "home".into(),
            ..Default::default()
        })
        .unwrap()["node"]["id"]
        .as_i64()
        .unwrap()
        .to_string();

    let writers = 12;
    let start = Arc::new(Barrier::new(writers));
    let handles: Vec<_> = (0..writers)
        .map(|w| {
            let (db, home, start) = (db.clone(), home.clone(), start.clone());
            std::thread::spawn(move || {
                let mut inv = Inventory::open(&db).unwrap();
                start.wait();
                (0..5)
                    .map(|i| {
                        inv.add(NewNode {
                            name: format!("Kutu {w}-{i}"),
                            kind: "container".into(),
                            parent: Some(home.clone()),
                            ..Default::default()
                        })
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();

    let failed: Vec<String> = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .filter_map(Result::err)
        .collect();
    assert!(failed.is_empty(), "writes failed: {failed:?}");
    let inv = Inventory::open(&db).unwrap();
    let count: i64 = inv.show(&home, false).unwrap()["children"]
        .as_array()
        .unwrap()
        .len() as i64;
    assert_eq!(count, (writers * 5) as i64);
}
