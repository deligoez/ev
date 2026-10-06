//! Indexes no schema version depends on, added when a file is opened without them.

use ev_core::Inventory;

#[test]
fn a_file_without_the_index_of_joined_purchase_lines_gets_it_when_opened() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ev.db");
    drop(Inventory::open(&db).unwrap());
    let index = |db: &std::path::Path| -> bool {
        rusqlite::Connection::open(db)
            .unwrap()
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = 'purchases_same_as')",
                [],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert!(index(&db));
    // A file an older ev wrote has none; opening it adds the index, and nothing else changes.
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch("DROP INDEX purchases_same_as")
        .unwrap();
    assert!(!index(&db));
    let inv = Inventory::open(&db).unwrap();
    assert!(inv.buy_list(false, None, None, None).is_ok());
    drop(inv);
    assert!(index(&db));
}
