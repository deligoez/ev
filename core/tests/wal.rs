//! The inventory runs in write-ahead-log mode, and `ev.db` alone stays whole: the data
//! repository commits that one file, never the log beside it.

use ev_core::{Inventory, NewNode};

#[test]
fn after_a_write_the_database_file_alone_holds_it_even_while_ev_ui_is_open() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ev.db");
    // A long-lived reader, as a running `ev ui` is; it keeps the log from being removed.
    let ui = Inventory::open(&db).unwrap();
    let mode: String = rusqlite::Connection::open(&db)
        .unwrap()
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    {
        let mut cli = Inventory::open(&db).unwrap();
        cli.add(NewNode {
            name: "Ev".into(),
            kind: "home".into(),
            ..Default::default()
        })
        .unwrap();
    }
    assert!(
        dir.path().join("ev.db-wal").exists(),
        "the reader keeps the log"
    );
    // What git would commit: the database file without its log.
    let copy = dir.path().join("copy");
    std::fs::create_dir(&copy).unwrap();
    std::fs::copy(&db, copy.join("ev.db")).unwrap();
    let names: Vec<String> = rusqlite::Connection::open(copy.join("ev.db"))
        .unwrap()
        .prepare("SELECT name FROM nodes")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(names, ["Ev"]);
    drop(ui);
}
