//! Things to do that hang on a node, a list of things to get, and `ev todo`, the one list of
//! everything waiting (spec §18).
//!
//! Most of what is "to do" in a home is already state on a node: a planned move, a thing meant
//! for another household, a disposal, a lost thing. Those stay where they are and close with
//! their own verbs (`done`, `gone`, `back`, `found`); `todo` only gathers them, so nothing is
//! kept twice. This module adds the few kinds that had no state yet — a label to print, broken,
//! a use-by date, a sale in progress — and needs, which are not in the tree at all.

use chrono::{Datelike, NaiveDate};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::error::refused;
use crate::model::{Disposition, Kind, Node, State};
use crate::store::{Inventory, brief, ids, live_nodes, now, place_errands, resolve, show};
use crate::{Error, Result};

/// How close a use-by date has to be before it shows in `todo`.
const EXPIRY_WINDOW_DAYS: i64 = 60;

/// Words that mark a record as a guess to clear up with the person.
const UNSURE: [&str; 3] = ["belirsiz", "muhtemelen", "?"];

fn brief_value(conn: &Connection, id: i64) -> Result<Value> {
    serde_json::to_value(brief(conn, id)?).map_err(|e| Error::Internal(e.to_string()))
}

pub(crate) fn mark(conn: &Connection, id: i64, kind: &str) -> Result<Value> {
    Ok(conn
        .query_row(
            "SELECT value, amount, note, at FROM marks WHERE node_id = ?1 AND kind = ?2",
            params![id, kind],
            |r| {
                Ok(json!({
                    "value": r.get::<_, Option<String>>(0)?,
                    "amount": r.get::<_, Option<i64>>(1)?,
                    "note": r.get::<_, Option<String>>(2)?,
                    "at": r.get::<_, String>(3)?,
                }))
            },
        )
        .optional()?
        .unwrap_or(Value::Null))
}

pub(crate) fn marks_of(conn: &Connection, id: i64) -> Result<Value> {
    let mut out = serde_json::Map::new();
    for kind in ["label", "broken", "expires", "sale", "photo_ok"] {
        let m = mark(conn, id, kind)?;
        if !m.is_null() {
            out.insert(kind.into(), m);
        }
    }
    Ok(Value::Object(out))
}

fn set_mark(
    conn: &Connection,
    id: i64,
    kind: &str,
    value: Option<&str>,
    amount: Option<i64>,
    note: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO marks (node_id, kind, value, amount, note, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(node_id, kind) DO UPDATE SET value = excluded.value,
           amount = excluded.amount, note = excluded.note, at = excluded.at",
        params![
            id,
            kind,
            value,
            amount,
            note.map(str::trim).filter(|n| !n.is_empty()),
            now()
        ],
    )?;
    Ok(())
}

fn clear_mark(conn: &Connection, id: i64, kind: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM marks WHERE node_id = ?1 AND kind = ?2",
        params![id, kind],
    )?;
    Ok(())
}

/// A new or changed code needs a new label; a removed code needs none.
pub(crate) fn code_changed(conn: &Connection, id: i64, has_code: bool) -> Result<()> {
    if has_code {
        set_mark(conn, id, "label", Some("needed"), None, None)
    } else {
        clear_mark(conn, id, "label")
    }
}

/// A use-by date: `YYYY-MM-DD`, or `YYYY-MM` meaning the end of that month.
fn parse_date(s: &str) -> Result<NaiveDate> {
    let s = s.trim();
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(d);
    }
    let bad = || Error::Usage(format!("`{s}` is not YYYY-MM-DD or YYYY-MM"));
    let first = NaiveDate::parse_from_str(&format!("{s}-01"), "%Y-%m-%d").map_err(|_| bad())?;
    let next = if first.month() == 12 {
        NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
    };
    next.and_then(|d| d.pred_opt()).ok_or_else(bad)
}

fn today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}

fn need_json(conn: &Connection, id: i64) -> Result<Value> {
    let row = conn
        .query_row(
            "SELECT text, qty, make, for_node, status, note, created_at, closed_at FROM needs WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    json!({
                        "id": id,
                        "text": r.get::<_, String>(0)?,
                        "qty": r.get::<_, Option<i64>>(1)?,
                        "make": r.get::<_, bool>(2)?,
                        "status": r.get::<_, String>(4)?,
                        "note": r.get::<_, Option<String>>(5)?,
                        "created_at": r.get::<_, String>(6)?,
                        "closed_at": r.get::<_, Option<String>>(7)?,
                    }),
                    r.get::<_, Option<i64>>(3)?,
                ))
            },
        )
        .optional()?;
    let (mut v, for_node) = row.ok_or_else(|| Error::NotFound(format!("no need with id {id}")))?;
    v["for"] = json!(for_node.map(|n| brief_value(conn, n)).transpose()?);
    Ok(v)
}

/// Open needs meant for a node.
pub(crate) fn needs_for(conn: &Connection, id: i64) -> Result<Vec<Value>> {
    ids(
        conn,
        "SELECT id FROM needs WHERE for_node = ?1 AND status = 'open' ORDER BY id",
        [id],
    )?
    .into_iter()
    .map(|n| need_json(conn, n))
    .collect()
}

/// A record whose *name* is still a guess. Notes are left out: they often keep a history
/// ("was recorded as …, probably …") that is not an open question.
fn unsure(n: &Node) -> bool {
    let name = n.name.to_lowercase();
    UNSURE.iter().any(|w| name.contains(w))
}

/// Event types that change what a place physically holds; adding a photo, a note or a review
/// does not.
const CONTENT_EVENTS: &str = "'create','move','done','gone','restore','lost','found'";

/// Places whose picture of the current state is missing or out of date: a unit with contents
/// and no photo of its own, or one whose contents changed after its newest one. A crop attached
/// to the place itself counts: it was cut from a wider view to show that place (a box cut out of
/// a drawer photo). Crops on the things inside do not. Things moved out count as a change.
/// When what `id` physically holds last changed: something below it was added, moved, found,
/// lost or left, or something was moved out of it. A record born of `ev split` is not a thing
/// added: the same things lie there, only recorded apart.
pub(crate) fn contents_changed_at(conn: &Connection, id: i64) -> Result<Option<String>> {
    Ok(conn.query_row(
        &format!(
            "WITH RECURSIVE d(id) AS (
                 SELECT ?1 UNION ALL SELECT n.id FROM nodes n JOIN d ON n.parent_id = d.id
             )
             SELECT MAX(e.at) FROM events e
              WHERE (e.node_id IN d AND e.node_id != ?1 AND e.type IN ({CONTENT_EVENTS})
                     AND NOT (e.type = 'create' AND EXISTS (
                         SELECT 1 FROM events s WHERE s.node_id = e.node_id AND s.type = 'split_from')))
                 OR (e.type IN ('move','done') AND json_extract(e.data, '$.from') IN d)"
        ),
        [id],
        |r| r.get(0),
    )?)
}

/// Why a node's photos no longer show it, and when its newest photo was taken and its contents
/// last changed.
pub(crate) type Stale = (&'static str, Option<String>, Option<String>);

/// Why a node's photos no longer show it, if they do not: `none` (no photo at all) or
/// `changed` (its contents changed after its newest photo), with both times.
pub(crate) fn photo_stale(conn: &Connection, id: i64) -> Result<Option<Stale>> {
    let photo_at: Option<Option<String>> = conn
        .query_row(
            // A photo from before photos carried a date is the one the place was first
            // recorded from, so it stands for the place's creation time.
            "SELECT MAX(COALESCE(added_at, (SELECT created_at FROM nodes WHERE id = ?1)))
               FROM photos WHERE node_id = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    // The person may say the photo is still close enough after a small change; that counts as
    // a fresh photo from then on.
    let accepted = mark(conn, id, "photo_ok")?["at"]
        .as_str()
        .map(str::to_string);
    let photo_at = match (photo_at.flatten(), accepted) {
        (Some(p), Some(a)) => Some(p.max(a)),
        (p, a) => p.or(a),
    };
    let changed = contents_changed_at(conn, id)?;
    let reason = match (&photo_at, &changed) {
        (None, _) => Some("none"),
        (Some(p), Some(c)) if c > p => Some("changed"),
        _ => None,
    };
    Ok(reason.map(|r| (r, photo_at.filter(|p| !p.is_empty()), changed)))
}

fn photos_needed(conn: &Connection, units: &[Value]) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    let stale_entry = |mut v: Value, (reason, photo_at, changed): Stale| {
        v["photo_reason"] = json!(reason);
        v["photo_at"] = json!(photo_at);
        v["changed_at"] = json!(changed);
        v
    };
    // A box in a grid is cut from its drawer's photo, so the drawer's photo counts too: a box
    // added or changed after it leaves the drawer's photo out of date, though the drawer
    // itself is no unit of its own.
    let mut grids = Vec::new();
    for u in units {
        let id = u["id"].as_i64().unwrap_or_default();
        let parent: Option<i64> = conn
            .query_row("SELECT parent_id FROM nodes WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?
            .flatten();
        if let Some(p) = parent
            && !grids.contains(&p)
            && crate::grid::grid_of(conn, p)?.is_some()
        {
            grids.push(p);
        }
        // An empty place needs no photo, and a room is not photographed as one.
        if u["children"].as_u64().unwrap_or(0) == 0 || u["kind"] == "room" {
            continue;
        }
        if let Some(s) = photo_stale(conn, id)? {
            out.push(stale_entry(u.clone(), s));
        }
    }
    for g in grids {
        if let Some(s) = photo_stale(conn, g)? {
            let mut v = brief_value(conn, g)?;
            v["grid"] = json!(true);
            out.push(stale_entry(v, s));
        }
    }
    Ok(out)
}

/// Whole (uncropped) photos attached to more than one live node, with the nodes: a group
/// photo left on the things in it instead of crops.
fn shared_photos(conn: &Connection) -> Result<Vec<Value>> {
    let paths: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT p.path FROM photos p JOIN nodes n ON n.id = p.node_id
              WHERE p.crop IS NULL AND n.state != 'gone'
              GROUP BY p.path HAVING COUNT(DISTINCT p.node_id) > 1 ORDER BY MIN(p.node_id)",
        )?;
        stmt.query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    paths
        .into_iter()
        .map(|p| {
            let nodes = ids(
                conn,
                "SELECT DISTINCT p.node_id FROM photos p JOIN nodes n ON n.id = p.node_id
                  WHERE p.path = ?1 AND p.crop IS NULL AND n.state != 'gone' ORDER BY p.node_id",
                [&p],
            )?
            .into_iter()
            .map(|n| brief_value(conn, n))
            .collect::<Result<Vec<_>>>()?;
            Ok(json!({ "path": p, "nodes": nodes }))
        })
        .collect()
}

impl Inventory {
    /// The person says a place's newest photo still shows it well enough, despite changes
    /// since; it leaves the photo-needed list until the next change.
    pub fn photo_current(&mut self, reference: &str) -> Result<Value> {
        let id = resolve(&self.conn, reference, false)?;
        set_mark(&self.conn, id, "photo_ok", None, None, None)?;
        show(&self.conn, id)
    }

    /// Marks labels printed (or needed again). Without references, lists the labels to print.
    pub fn label(&mut self, references: &[String], printed: bool) -> Result<Value> {
        if references.is_empty() {
            return self.labels_needed();
        }
        let tx = self.conn.transaction()?;
        for r in references {
            let id = resolve(&tx, r, false)?;
            let has_code: Option<String> =
                tx.query_row("SELECT code FROM nodes WHERE id = ?1", [id], |r| r.get(0))?;
            if has_code.is_none() {
                return Err(refused(
                    format!("node {id} has no code, so there is no label to print"),
                    Value::Null,
                ));
            }
            let value = if printed { "printed" } else { "needed" };
            set_mark(&tx, id, "label", Some(value), None, None)?;
        }
        tx.commit()?;
        self.labels_needed()
    }

    fn labels_needed(&self) -> Result<Value> {
        let list = ids(
            &self.conn,
            "SELECT m.node_id FROM marks m JOIN nodes n ON n.id = m.node_id
              WHERE m.kind = 'label' AND m.value = 'needed' AND n.state != 'gone'
              ORDER BY n.code_folded",
            [],
        )?;
        let labels = list
            .into_iter()
            .map(|id| {
                let mut v = brief_value(&self.conn, id)?;
                let theme: Option<String> =
                    self.conn
                        .query_row("SELECT theme FROM nodes WHERE id = ?1", [id], |r| r.get(0))?;
                v["theme"] = json!(theme);
                Ok(v)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({ "labels": labels }))
    }

    /// Marks a node broken with what is wrong, or clears it once fixed.
    pub fn broken(&mut self, reference: &str, note: Option<&str>, fixed: bool) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let id = resolve(&tx, reference, false)?;
        if fixed {
            clear_mark(&tx, id, "broken")?;
        } else {
            set_mark(&tx, id, "broken", None, None, note)?;
        }
        crate::store::event(
            &tx,
            id,
            if fixed { "fixed" } else { "broken" },
            json!({ "note": note }),
        )?;
        tx.commit()?;
        show(&self.conn, id)
    }

    /// Records a use-by date (`YYYY-MM-DD` or `YYYY-MM`), or clears it.
    pub fn expires(&mut self, reference: &str, date: Option<&str>) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let id = resolve(&tx, reference, false)?;
        match date {
            Some(d) => {
                let parsed = parse_date(d)?;
                set_mark(&tx, id, "expires", Some(&parsed.to_string()), None, None)?;
            }
            None => clear_mark(&tx, id, "expires")?,
        }
        tx.commit()?;
        show(&self.conn, id)
    }

    /// Where a sale stands: `listed` (with price and where) or `reserved`; `None` clears it.
    /// Only a sell candidate has a sale; selling it is `ev gone`.
    pub fn sale(
        &mut self,
        reference: &str,
        status: Option<&str>,
        price: Option<i64>,
        place: Option<&str>,
    ) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let id = resolve(&tx, reference, false)?;
        let (state, disposition): (String, Option<String>) = tx.query_row(
            "SELECT state, disposition FROM nodes WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if state != "candidate" || disposition.as_deref() != Some(Disposition::Sell.as_str()) {
            return Err(refused(
                format!("node {id} is not set aside to sell; `ev dispose {id} --as sell` first"),
                Value::Null,
            ));
        }
        match status {
            None => clear_mark(&tx, id, "sale")?,
            Some(s @ ("listed" | "reserved")) => {
                let before = mark(&tx, id, "sale")?;
                let price = price.or_else(|| before["amount"].as_i64());
                let place = place
                    .map(str::to_string)
                    .or_else(|| before["note"].as_str().map(str::to_string));
                set_mark(&tx, id, "sale", Some(s), price, place.as_deref())?;
            }
            Some(other) => {
                return Err(Error::Usage(format!(
                    "`{other}` is not a sale state; use listed or reserved"
                )));
            }
        }
        tx.commit()?;
        show(&self.conn, id)
    }

    pub fn need_add(
        &mut self,
        text: &str,
        qty: Option<i64>,
        make: bool,
        for_ref: Option<&str>,
        note: Option<&str>,
    ) -> Result<Value> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Error::Usage("need text is empty".into()));
        }
        let tx = self.conn.transaction()?;
        let for_node = for_ref.map(|r| resolve(&tx, r, false)).transpose()?;
        tx.execute(
            "INSERT INTO needs (text, qty, make, for_node, note, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![text, qty, make, for_node, note.map(str::trim), now()],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        need_json(&self.conn, id)
    }

    pub fn need_list(&self, all: bool) -> Result<Value> {
        let sql = if all {
            "SELECT id FROM needs ORDER BY CASE status WHEN 'open' THEN 0 ELSE 1 END, id"
        } else {
            "SELECT id FROM needs WHERE status = 'open' ORDER BY id"
        };
        let list = ids(&self.conn, sql, [])?
            .into_iter()
            .map(|n| need_json(&self.conn, n))
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({ "needs": list }))
    }

    /// Closes a need as got (bought or made) or dropped.
    pub fn need_close(&mut self, id: i64, got: bool, note: Option<&str>) -> Result<Value> {
        let current = need_json(&self.conn, id)?;
        if current["status"] != "open" {
            return Err(refused(format!("need {id} is already closed"), Value::Null));
        }
        self.conn.execute(
            "UPDATE needs SET status = ?1, closed_at = ?2, note = COALESCE(?3, note) WHERE id = ?4",
            params![
                if got { "got" } else { "dropped" },
                now(),
                note.map(str::trim),
                id
            ],
        )?;
        need_json(&self.conn, id)
    }

    /// Everything waiting, in one place. Stored state is only read here: each entry closes with
    /// its own verb (`done`, `gone`, `back`, `found`, `task done`, `need got`, …), and leaves
    /// this list by itself.
    pub fn todo(&self) -> Result<Value> {
        let next = self.next()?;
        let goal = next["goal"].as_str().map(str::to_string);
        let organize = goal.as_deref() != Some("track");
        let tasks = self.task_list(false)?["tasks"].clone();
        let moves = self.pending()?["pending"].clone();
        let errands: Vec<Value> = ids(
            &self.conn,
            "SELECT DISTINCT p.id FROM places p JOIN nodes n
               ON p.id IN (n.to_place, n.owner_place, n.with_place)
             WHERE n.state != 'gone' ORDER BY p.name",
            [],
        )?
        .into_iter()
        .map(|p| place_errands(&self.conn, p))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .filter(|e| {
            ["take", "return", "collect"]
                .iter()
                .any(|k| e[*k].as_array().is_some_and(|a| !a.is_empty()))
        })
        .collect();
        let disposals = self.disposals(None)?["disposals"].clone();
        let lost = self.lost_list()?["lost"].clone();
        let labels = self.labels_needed()?["labels"].clone();
        let needs = self.need_list(false)?["needs"].clone();

        let all = live_nodes(&self.conn)?;
        let mut repairs = Vec::new();
        let mut expiring = Vec::new();
        let mut unclear = Vec::new();
        let today = today();
        for n in all.iter().filter(|n| n.state != State::Gone) {
            let broken = mark(&self.conn, n.id, "broken")?;
            if !broken.is_null() {
                let mut v = brief_value(&self.conn, n.id)?;
                v["note"] = broken["note"].clone();
                repairs.push(v);
            }
            let exp = mark(&self.conn, n.id, "expires")?;
            if let Some(d) = exp["value"].as_str().and_then(|d| parse_date(d).ok()) {
                let days = (d - today).num_days();
                if days <= EXPIRY_WINDOW_DAYS {
                    let mut v = brief_value(&self.conn, n.id)?;
                    v["expires"] = json!(d.to_string());
                    v["days_left"] = json!(days);
                    expiring.push(v);
                }
            }
            if unsure(n) {
                unclear.push(brief_value(&self.conn, n.id)?);
            }
        }
        expiring.sort_by_key(|v| v["days_left"].as_i64().unwrap_or_default());

        let progress = self.progress()?;
        let places = progress["places"].as_array().cloned().unwrap_or_default();
        // Places not counted yet, or being counted: their counts say nothing yet.
        let uncounted: Vec<Value> = places
            .iter()
            .filter(|p| matches!(p["review"]["status"].as_str(), Some("raw" | "counting")))
            .cloned()
            .collect();
        // What waits for its final place: the things put straight into a place marked
        // `temporary`, and an item marked `temporary` itself (one thing parked among things
        // that do belong there), each with the place it waits in.
        let mut parked = Vec::new();
        for n in all.iter().filter(|n| n.state != State::Gone) {
            if let Some(p) = n.parent_id.and_then(|p| all.iter().find(|h| h.id == p))
                && (p.temporary || (n.temporary && n.kind == Kind::Item))
            {
                let mut v = brief_value(&self.conn, n.id)?;
                v["in"] = brief_value(&self.conn, p.id)?;
                parked.push(v);
            }
        }
        let stale: Vec<Value> = if organize {
            places
                .iter()
                .filter(|p| p["review"]["changed_since"] == true)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        let photos = photos_needed(&self.conn, &places)?;
        let shared = shared_photos(&self.conn)?;
        let count = |v: &Value| v.as_array().map_or(0, Vec::len);
        let disposal_count: usize = disposals
            .as_object()
            .map_or(0, |o| o.values().map(count).sum());
        Ok(json!({
            "goal": goal,
            "progress": next["progress"],
            "counts": {
                "tasks": count(&tasks),
                "moves": count(&moves),
                "errands": errands.len(),
                "disposals": disposal_count,
                "labels": count(&labels),
                "needs": count(&needs),
                "repairs": repairs.len(),
                "expiring": expiring.len(),
                "lost": count(&lost),
                "uncounted": uncounted.len(),
                "parked": parked.len(),
                "stale": stale.len(),
                "unclear": unclear.len(),
                "photos": photos.len(),
                "shared_photos": shared.len(),
            },
            "tasks": tasks,
            "moves": moves,
            "errands": errands,
            "disposals": disposals,
            "labels": labels,
            "needs": needs,
            "repairs": repairs,
            "expiring": expiring,
            "lost": lost,
            "uncounted": uncounted,
            "parked": parked,
            "stale": stale,
            "unclear": unclear,
            "photos": photos,
            "shared_photos": shared,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::parse_date;

    #[test]
    fn a_month_means_its_last_day() {
        assert_eq!(parse_date("2026-07").unwrap().to_string(), "2026-07-31");
        assert_eq!(parse_date("2026-12").unwrap().to_string(), "2026-12-31");
        assert_eq!(parse_date("2028-02").unwrap().to_string(), "2028-02-29");
        assert_eq!(parse_date("2026-07-15").unwrap().to_string(), "2026-07-15");
        assert!(parse_date("07/2026").is_err());
    }
}
