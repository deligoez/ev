//! Files in ev's own store (`photos/`, `docs/`) are kept relative to the database's directory,
//! so the data directory can move: another machine, another user name, a copy opened with
//! `--db`. SQL writes such a path through `ev_store(?)` and reads it through `ev_file(column)`;
//! a path outside the store (a photo not adopted yet) and a web address pass through unchanged.

use std::path::Path;

use rusqlite::Connection;
use rusqlite::functions::FunctionFlags;

/// The store's directories, as the start of a stored path.
const STORES: [&str; 2] = ["photos/", "docs/"];

/// Registers `ev_store` and `ev_file` on a connection to the database in `home`.
pub(super) fn register(conn: &Connection, home: &Path) -> rusqlite::Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    let prefix = format!("{}/", home.to_string_lossy().trim_end_matches('/'));
    let base = prefix.clone();
    conn.create_scalar_function("ev_store", 1, flags, move |ctx| {
        Ok(ctx.get::<Option<String>>(0)?.map(|p| stored(&p, &prefix)))
    })?;
    conn.create_scalar_function("ev_file", 1, flags, move |ctx| {
        Ok(ctx.get::<Option<String>>(0)?.map(|p| file(&p, &base)))
    })
}

/// A path under `home`'s store as it is kept: relative, `photos/…` or `docs/…`.
fn stored(path: &str, home: &str) -> String {
    match path.strip_prefix(home) {
        Some(rel) if STORES.iter().any(|s| rel.starts_with(s)) => rel.to_string(),
        _ => path.to_string(),
    }
}

/// A kept path as a file to open: a store path under `home`, anything else as it is.
fn file(path: &str, home: &str) -> String {
    if STORES.iter().any(|s| path.starts_with(s)) {
        format!("{home}{path}")
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_store_path_is_kept_relative_and_read_back_under_home() {
        let home = "/data/ev/";
        assert_eq!(super::stored("/data/ev/photos/a.jpg", home), "photos/a.jpg");
        assert_eq!(super::stored("/data/ev/docs/b.pdf", home), "docs/b.pdf");
        assert_eq!(super::file("photos/a.jpg", home), "/data/ev/photos/a.jpg");
        // Outside the store, a web address, another home: unchanged both ways.
        for p in [
            "/tmp/x.jpg",
            "/data/ev/x.jpg",
            "https://example.com/a",
            "/old/photos/a.jpg",
        ] {
            assert_eq!(super::stored(p, home), p);
            assert_eq!(super::file(p, home), p);
        }
    }
}
