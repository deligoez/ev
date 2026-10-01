//! A thing's links (purchases spec §3.4): its product page, manual, support and driver pages.
//! A page can die, so a link keeps an archive: a copy saved into `ev`'s document store, or a
//! Wayback Machine address.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::store::{Inventory, brief, now, resolve};
use crate::{Error, Result};

pub const LINK_KINDS: [&str; 5] = ["info", "manual", "support", "driver", "other"];

fn check_kind(kind: &str) -> Result<String> {
    let k = kind.trim().to_lowercase();
    if LINK_KINDS.contains(&k.as_str()) {
        Ok(k)
    } else {
        Err(Error::Usage(format!(
            "`{kind}` is not a link kind; use one of {}",
            LINK_KINDS.join(", ")
        )))
    }
}

fn web(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// An archive given as a saved file is copied into the store; an address is kept as it is.
fn archive_of(dir: &Path, archive: Option<&str>) -> Result<Option<String>> {
    let Some(a) = archive.map(str::trim).filter(|a| !a.is_empty()) else {
        return Ok(None);
    };
    if web(a) {
        return Ok(Some(a.to_string()));
    }
    let file = Path::new(a);
    if !file.is_file() {
        return Err(Error::Usage(format!(
            "archive `{a}`: neither a web address nor a file"
        )));
    }
    let bytes = std::fs::read(file).map_err(|e| Error::Usage(format!("{a}: {e}")))?;
    let ext = file
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| (1..=5).contains(&e.len()))
        .unwrap_or_else(|| "html".into());
    let stored = crate::photo::store_bytes(dir, &bytes, &ext)?;
    Ok(Some(stored.to_string_lossy().into_owned()))
}

pub(crate) fn links_of(conn: &Connection, node: i64) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, url, archive, note, added_at FROM links WHERE node_id = ?1
          ORDER BY CASE kind WHEN 'info' THEN 0 WHEN 'manual' THEN 1 WHEN 'support' THEN 2
                             WHEN 'driver' THEN 3 ELSE 4 END, id",
    )?;
    let rows = stmt
        .query_map([node], |r| {
            Ok(json!({
                "id": r.get::<_, i64>(0)?,
                "kind": r.get::<_, String>(1)?,
                "url": r.get::<_, String>(2)?,
                "archive": r.get::<_, Option<String>>(3)?,
                "note": r.get::<_, Option<String>>(4)?,
                "added_at": r.get::<_, String>(5)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Adds a link, or updates the kind, archive and note of the same address on the same node.
pub(crate) fn add_link(
    conn: &Connection,
    dir: &Path,
    node: i64,
    url: &str,
    kind: &str,
    archive: Option<&str>,
    note: Option<&str>,
) -> Result<i64> {
    let kind = check_kind(kind)?;
    let url = url.trim();
    if !web(url) {
        return Err(Error::Usage(format!("`{url}` is not a web address")));
    }
    let archive = archive_of(dir, archive)?;
    let note = note.map(str::trim).filter(|n| !n.is_empty());
    conn.execute(
        "INSERT INTO links (node_id, kind, url, archive, note, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (node_id, url) DO UPDATE SET kind = excluded.kind,
           archive = COALESCE(excluded.archive, archive), note = COALESCE(excluded.note, note)",
        params![node, kind, url, archive, note, now()],
    )?;
    Ok(conn.query_row(
        "SELECT id FROM links WHERE node_id = ?1 AND url = ?2",
        params![node, url],
        |r| r.get(0),
    )?)
}

impl Inventory {
    pub fn link_add(
        &mut self,
        reference: &str,
        url: &str,
        kind: &str,
        archive: Option<&str>,
        note: Option<&str>,
    ) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let node = resolve(&tx, reference, false)?;
        add_link(&tx, &self.doc_dir, node, url, kind, archive, note)?;
        tx.commit()?;
        self.link_list(reference)
    }

    pub fn link_list(&self, reference: &str) -> Result<Value> {
        let node = resolve(&self.conn, reference, false)?;
        Ok(json!({ "node": brief(&self.conn, node)?, "links": links_of(&self.conn, node)? }))
    }

    pub fn link_remove(&mut self, id: i64) -> Result<Value> {
        let node: Option<i64> = self
            .conn
            .query_row("SELECT node_id FROM links WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        let node = node.ok_or_else(|| Error::NotFound(format!("link {id}")))?;
        self.conn.execute("DELETE FROM links WHERE id = ?1", [id])?;
        Ok(json!({ "node": brief(&self.conn, node)?, "links": links_of(&self.conn, node)? }))
    }
}
