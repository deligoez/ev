//! Documents (purchases spec §3.5): invoices, warranty certificates, manuals, service forms,
//! appraisals and policies, copied into `ev`'s own store like photos and linked to what they
//! belong to. A copy outlives the folder it came from.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::store::{Inventory, brief, event, ids, now, resolve};
use crate::{Error, Result};

/// The kinds of document, in the order a list shows them.
pub const DOC_KINDS: [&str; 7] = [
    "invoice",
    "warranty",
    "manual",
    "service",
    "appraisal",
    "policy",
    "other",
];

/// What a new document says about itself; every field but the kind is optional.
#[derive(Debug, Clone, Default)]
pub struct NewDoc {
    pub kind: String,
    pub number: Option<String>,
    /// An e-Archive invoice's UUID, verifiable at the tax authority.
    pub ettn: Option<String>,
    pub issued: Option<String>,
    pub issuer: Option<String>,
    pub note: Option<String>,
}

fn text(v: &Option<String>) -> Option<String> {
    v.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn check_kind(kind: &str) -> Result<String> {
    let k = kind.trim().to_lowercase();
    if DOC_KINDS.contains(&k.as_str()) {
        Ok(k)
    } else {
        Err(Error::Usage(format!(
            "`{kind}` is not a document kind; use one of {}",
            DOC_KINDS.join(", ")
        )))
    }
}

/// `YYYY-MM-DD`, `YYYY-MM` or `YYYY`.
fn check_date(d: &str) -> Result<String> {
    let ok = match d.len() {
        10 => chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok(),
        7 => chrono::NaiveDate::parse_from_str(&format!("{d}-01"), "%Y-%m-%d").is_ok(),
        4 => d.chars().all(|c| c.is_ascii_digit()),
        _ => false,
    };
    if ok {
        Ok(d.to_string())
    } else {
        Err(Error::Usage(format!(
            "`{d}` is not a date; use YYYY-MM-DD, YYYY-MM or YYYY"
        )))
    }
}

/// The extension a stored copy keeps: the file's own when it is a short plain one.
fn extension(file: &Path) -> String {
    file.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| (1..=5).contains(&e.len()) && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_else(|| "bin".to_string())
}

pub(crate) fn doc_json(conn: &Connection, id: i64) -> Result<Value> {
    let row = conn
        .query_row(
            "SELECT kind, file, original_name, number, ettn, issued_at, issuer, note, added_at
               FROM documents WHERE id = ?1",
            [id],
            |r| {
                Ok(json!({
                    "id": id,
                    "kind": r.get::<_, String>(0)?,
                    "file": r.get::<_, String>(1)?,
                    "original_name": r.get::<_, Option<String>>(2)?,
                    "number": r.get::<_, Option<String>>(3)?,
                    "ettn": r.get::<_, Option<String>>(4)?,
                    "issued_at": r.get::<_, Option<String>>(5)?,
                    "issuer": r.get::<_, Option<String>>(6)?,
                    "note": r.get::<_, Option<String>>(7)?,
                    "added_at": r.get::<_, String>(8)?,
                }))
            },
        )
        .optional()?;
    let mut doc = row.ok_or_else(|| Error::NotFound(format!("no document with id {id}")))?;
    let nodes = ids(
        conn,
        "SELECT target_id FROM document_links WHERE document_id = ?1 AND target = 'node'
          ORDER BY target_id",
        [id],
    )?
    .into_iter()
    .map(|n| brief(conn, n))
    .collect::<Result<Vec<_>>>()?;
    doc["nodes"] = json!(nodes);
    Ok(doc)
}

/// The documents linked to a node, newest issue first, without the node list on each.
pub(crate) fn docs_of(conn: &Connection, node: i64) -> Result<Vec<Value>> {
    ids(
        conn,
        "SELECT d.id FROM documents d JOIN document_links l ON l.document_id = d.id
          WHERE l.target = 'node' AND l.target_id = ?1
          ORDER BY COALESCE(d.issued_at, d.added_at) DESC, d.id",
        [node],
    )?
    .into_iter()
    .map(|d| {
        let mut v = doc_json(conn, d)?;
        if let Some(o) = v.as_object_mut() {
            o.remove("nodes");
        }
        Ok(v)
    })
    .collect()
}

fn link_node(conn: &Connection, doc: i64, node: i64, kind: &str) -> Result<bool> {
    let added = conn.execute(
        "INSERT OR IGNORE INTO document_links (document_id, target, target_id, at)
         VALUES (?1, 'node', ?2, ?3)",
        params![doc, node, now()],
    )? > 0;
    if added {
        event(
            conn,
            node,
            "doc_linked",
            json!({ "document": doc, "kind": kind }),
        )?;
    }
    Ok(added)
}

fn kind_of(conn: &Connection, doc: i64) -> Result<String> {
    conn.query_row("SELECT kind FROM documents WHERE id = ?1", [doc], |r| {
        r.get(0)
    })
    .optional()?
    .ok_or_else(|| Error::NotFound(format!("no document with id {doc}")))
}

/// Copies a file into the store and records it, or finds the document already holding the
/// same bytes. Returns its id and whether it was already there. Nothing is written when the
/// description is refused.
pub(crate) fn store_doc(
    conn: &Connection,
    dir: &Path,
    file: &Path,
    new: &NewDoc,
) -> Result<(i64, bool)> {
    let kind = check_kind(&new.kind)?;
    let issued = text(&new.issued).map(|d| check_date(&d)).transpose()?;
    if !file.is_file() {
        return Err(Error::Usage(format!("{}: no such file", file.display())));
    }
    let bytes =
        std::fs::read(file).map_err(|e| Error::Usage(format!("{}: {e}", file.display())))?;
    let stored = crate::photo::store_bytes(dir, &bytes, &extension(file))?;
    let stored = stored.to_string_lossy().into_owned();
    let existing: Option<i64> = conn
        .query_row("SELECT id FROM documents WHERE file = ?1", [&stored], |r| {
            r.get(0)
        })
        .optional()?;
    if let Some(id) = existing {
        return Ok((id, true));
    }
    conn.execute(
        "INSERT INTO documents (kind, file, original_name, number, ettn, issued_at, issuer, note,
                                added_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            kind,
            stored,
            file.file_name().map(|n| n.to_string_lossy().into_owned()),
            text(&new.number),
            text(&new.ettn),
            issued,
            text(&new.issuer),
            text(&new.note),
            now()
        ],
    )?;
    Ok((conn.last_insert_rowid(), false))
}

impl Inventory {
    /// Copies a file into the document store and links it to each of `for_refs`. The same file
    /// added again is the same document: its fields are kept and the new links are added.
    pub fn doc_add(&mut self, file: &Path, new: &NewDoc, for_refs: &[String]) -> Result<Value> {
        // Refuse a bad description or an unknown thing before anything is copied.
        check_kind(&new.kind)?;
        text(&new.issued).map(|d| check_date(&d)).transpose()?;
        let tx = self.conn.transaction()?;
        let nodes = for_refs
            .iter()
            .map(|r| resolve(&tx, r, false))
            .collect::<Result<Vec<_>>>()?;
        let (id, again) = store_doc(&tx, &self.doc_dir, file, new)?;
        let doc_kind = kind_of(&tx, id)?;
        for n in nodes {
            link_node(&tx, id, n, &doc_kind)?;
        }
        tx.commit()?;
        Ok(json!({ "document": doc_json(&self.conn, id)?, "existing": again }))
    }

    /// Every document, or those linked to one node.
    pub fn doc_list(&self, reference: Option<&str>) -> Result<Value> {
        let list = match reference {
            Some(r) => {
                let node = resolve(&self.conn, r, true)?;
                docs_of(&self.conn, node)?
            }
            None => ids(
                &self.conn,
                "SELECT id FROM documents ORDER BY COALESCE(issued_at, added_at) DESC, id",
                [],
            )?
            .into_iter()
            .map(|d| doc_json(&self.conn, d))
            .collect::<Result<Vec<_>>>()?,
        };
        Ok(json!({ "documents": list }))
    }

    pub fn doc_show(&self, id: i64) -> Result<Value> {
        Ok(json!({ "document": doc_json(&self.conn, id)? }))
    }

    /// Links a stored document to one more node.
    pub fn doc_link(&mut self, id: i64, reference: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let kind = kind_of(&tx, id)?;
        let node = resolve(&tx, reference, false)?;
        link_node(&tx, id, node, &kind)?;
        tx.commit()?;
        self.doc_show(id)
    }

    /// Takes a document off a node; the document stays in the store.
    pub fn doc_unlink(&mut self, id: i64, reference: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let kind = kind_of(&tx, id)?;
        let node = resolve(&tx, reference, true)?;
        let removed = tx.execute(
            "DELETE FROM document_links WHERE document_id = ?1 AND target = 'node' AND target_id = ?2",
            params![id, node],
        )?;
        if removed == 0 {
            return Err(Error::Usage(format!(
                "document {id} is not linked to node {node}"
            )));
        }
        event(
            &tx,
            node,
            "doc_unlinked",
            json!({ "document": id, "kind": kind }),
        )?;
        tx.commit()?;
        self.doc_show(id)
    }
}
