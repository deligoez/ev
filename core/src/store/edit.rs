//! `field=value` edits: parsing each field, checking it and recording one `edit` event per
//! record.

use super::*;
use crate::error::usage;

/// What a gone record still lets change: what it was, never where it stands.
const GONE_FIELDS: [&str; 10] = [
    "name", "note", "came", "left", "left_in", "qty", "make", "model", "serial", "tags",
];

/// When (`left`, as said; null when only the day it was recorded stands for it) or where
/// (`left_in`) a gone record left.
fn departure_field(conn: &Connection, id: i64, field: &str) -> Result<Value> {
    let v: Option<Option<String>> = conn
        .query_row(
            "SELECT CASE ?2 WHEN 'left' THEN d.at ELSE p.name END
               FROM departures d LEFT JOIN places p ON p.id = d.place_id WHERE d.node_id = ?1",
            params![id, field],
            |r| r.get(0),
        )
        .optional()?;
    Ok(json!(v.flatten()))
}

pub(super) fn field_value(n: &Node, field: &str) -> Value {
    match field {
        "name" => json!(n.name),
        "code" => json!(n.code),
        "kind" => json!(n.kind),
        "address" => json!(n.address),
        "qty" => json!(n.qty),
        "note" => json!(n.note),
        "theme" => json!(n.theme),
        "fill" => json!(n.fill),
        "size" => json!(n.size),
        "tags" => json!(n.tags),
        "photos" => json!(n.photos),
        "to" => json!(n.to),
        "owner" => json!(n.owner),
        "with" => json!(n.with),
        "temporary" => json!(n.temporary),
        "waits_for" => json!(n.waits_for),
        "make" => json!(n.make),
        "model" => json!(n.model),
        "serial" => json!(n.serial),
        "came" => json!(n.came_at),
        _ => Value::Null,
    }
}

/// `WxDxH` or `WxD`, each a positive number (`,` or `.` for decimals, `x`, `×` or `*`
/// between); written back as `1x2x0.5`.
pub(crate) fn parse_size(s: &str) -> Result<Vec<f64>> {
    let bad = || usage("edit_size_bad", json!({ "size": s }));
    let parts: Vec<f64> = s
        .trim()
        .to_lowercase()
        .split(['x', '×', '*'])
        .map(|p| p.trim().replace(',', ".").parse::<f64>())
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| bad())?;
    if !(2..=3).contains(&parts.len()) || parts.iter().any(|p| !(p.is_finite() && *p > 0.0)) {
        return Err(bad());
    }
    Ok(parts)
}

pub(super) fn normalize_size(s: &str) -> Result<String> {
    Ok(parse_size(s)?
        .iter()
        .map(|p| format!("{p}"))
        .collect::<Vec<_>>()
        .join("x"))
}

/// The size a name carries (`Gridfinity 1x2x0.5 — …` carries `1x2x0.5`), normalized: the first
/// word made only of digits, separators and `x`/`×` that reads as a size.
pub(super) fn size_in_name(name: &str) -> Option<String> {
    name.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !(c.is_ascii_digit() || c == '.' || c == ',')))
        .filter(|w| {
            w.contains(['x', '×']) && w.chars().all(|c| c.is_ascii_digit() || ".,x×".contains(c))
        })
        .find_map(|w| normalize_size(w).ok())
}

fn parse_int(field: &str, value: &str) -> Result<Option<i64>> {
    let v = value.trim();
    if v.is_empty() {
        return Ok(None);
    }
    v.parse::<i64>()
        .map(Some)
        .map_err(|_| usage("edit_not_integer", json!({ "field": field, "value": v })))
}

/// One record's `field=value` assignments inside the caller's transaction, recorded as one
/// `edit` event with each field's value before the first and after the last assignment (so
/// `tags=+a` then `tags=+b` reads as one change). Returns the record and those changes.
pub(super) fn edit_in(
    conn: &Connection,
    reference: &str,
    assignments: &[String],
) -> Result<(i64, serde_json::Map<String, Value>)> {
    // A gone node is found by id only, and only what it was may change: its note (why it left),
    // when it came, how many there were and what it is beyond its name, so a past thing
    // (spec/past-belongings.md) can be completed as it is remembered. Where it stands describes
    // a thing no longer here.
    let id = match resolve(conn, reference, false) {
        Err(e)
            if e.is_not_found()
                && reference
                    .trim()
                    .trim_start_matches('#')
                    .chars()
                    .all(|c| c.is_ascii_digit()) =>
        {
            let id = resolve(conn, reference, true)?;
            if let Some(a) = assignments.iter().find(|a| {
                a.split_once('=')
                    .is_none_or(|(f, _)| !GONE_FIELDS.contains(&f.trim()))
            }) {
                return Err(refuse(
                    "gone_fields_only",
                    json!({ "id": id, "fields": GONE_FIELDS.join(", "), "given": a }),
                    Value::Null,
                ));
            }
            id
        }
        other => other?,
    };
    let mut changes = serde_json::Map::new();
    for a in assignments {
        let (field, value) = a
            .split_once('=')
            .ok_or_else(|| usage("edit_not_assignment", json!({ "assignment": a })))?;
        let field = field.trim();
        let before = load(conn, id)?;
        // When and where a gone record left are on its departure, not on the record.
        let value_of = |n: &Node| -> Result<Value> {
            Ok(match field {
                "left" | "left_in" => departure_field(conn, id, field)?,
                _ => field_value(n, field),
            })
        };
        let was = value_of(&before)?;
        apply_edit(conn, &before, field, value)?;
        // Said again, a field is the person's word, not a guess any more (spec/guesses.md).
        crate::marks::field_said(conn, id, field)?;
        let after = load(conn, id)?;
        let first = changes
            .get(field)
            .map(|c| c["before"].clone())
            .unwrap_or(was);
        let last = value_of(&after)?;
        if first == last {
            changes.remove(field);
        } else {
            changes.insert(field.to_string(), json!({ "before": first, "after": last }));
        }
    }
    if !changes.is_empty() {
        touch(conn, id)?;
        event(conn, id, "edit", Value::Object(changes.clone()))?;
    }
    // What the thing is, set on one portion, is set on all of them (spec/portions.md §3).
    crate::portions::share_identity(conn, id, assignments)?;
    Ok((id, changes))
}

pub(crate) fn apply_edit(conn: &Connection, n: &Node, field: &str, value: &str) -> Result<()> {
    let text = |v: &str| -> Option<String> { Some(v.trim().to_string()).filter(|s| !s.is_empty()) };
    match field {
        "name" => {
            let v = text(value).ok_or_else(|| usage("edit_name_empty", Value::Null))?;
            conn.execute("UPDATE nodes SET name = ?1 WHERE id = ?2", params![v, n.id])?;
        }
        "code" => {
            let v = text(value).map(|c| expand_code(conn, &c)).transpose()?;
            let folded = v
                .as_deref()
                .map(|c| check_code(conn, c, Some(n.id)))
                .transpose()?;
            conn.execute(
                "UPDATE nodes SET code = ?1, code_folded = ?2 WHERE id = ?3",
                params![v, folded, n.id],
            )?;
            if v != n.code {
                crate::marks::code_changed(conn, n.id, v.is_some())?;
            }
        }
        "kind" => {
            let k: Kind = value.parse()?;
            check_placement(conn, k, n.parent_id, n.lost, None)?;
            if n.address.is_some() && k != Kind::Home {
                return Err(refuse("address_clear_first", Value::Null, Value::Null));
            }
            if !matches!(k, Kind::Home | Kind::Room) {
                let rooms: Vec<i64> = ids(
                    conn,
                    "SELECT id FROM nodes WHERE parent_id = ?1 AND kind = 'room' AND state != 'gone'",
                    [n.id],
                )?;
                if !rooms.is_empty() {
                    return Err(refuse(
                        "holds_rooms",
                        json!({ "node": label(n) }),
                        Value::Null,
                    ));
                }
            }
            conn.execute(
                "UPDATE nodes SET kind = ?1 WHERE id = ?2",
                params![k.as_str(), n.id],
            )?;
        }
        "address" => {
            let v = text(value);
            if v.is_some() && n.kind != Kind::Home {
                return Err(refuse("address_only_home", Value::Null, Value::Null));
            }
            conn.execute(
                "UPDATE nodes SET address = ?1 WHERE id = ?2",
                params![v, n.id],
            )?;
        }
        // A note is a log the person adds to: `note=+text` appends it on a new line.
        "note" if value.trim_start().starts_with('+') => {
            let added = text(&value.trim_start()[1..])
                .ok_or_else(|| usage("edit_note_add_empty", Value::Null))?;
            let note = match n.note.as_deref() {
                Some(old) if !old.trim().is_empty() => format!("{old}\n{added}"),
                _ => added,
            };
            conn.execute(
                "UPDATE nodes SET note = ?1 WHERE id = ?2",
                params![note, n.id],
            )?;
        }
        "note" | "theme" | "make" | "model" | "serial" => {
            conn.execute(
                &format!("UPDATE nodes SET {field} = ?1 WHERE id = ?2"),
                params![text(value), n.id],
            )?;
        }
        "size" => {
            let v = text(value).map(|s| normalize_size(&s)).transpose()?;
            conn.execute("UPDATE nodes SET size = ?1 WHERE id = ?2", params![v, n.id])?;
        }
        "qty" | "fill" => {
            let v = parse_int(field, value)?;
            let (q, f) = if field == "qty" { (v, None) } else { (None, v) };
            check_ranges(q, f)?;
            conn.execute(
                &format!("UPDATE nodes SET {field} = ?1 WHERE id = ?2"),
                params![v, n.id],
            )?;
        }
        "tags" => {
            let (op, t) = split_op(field, value)?;
            let t = normalize_tag(t)?;
            if op == '+' {
                conn.execute(
                    "INSERT OR IGNORE INTO tags (node_id, tag) VALUES (?1, ?2)",
                    params![n.id, t],
                )?;
            } else {
                conn.execute(
                    "DELETE FROM tags WHERE node_id = ?1 AND tag = ?2",
                    params![n.id, t],
                )?;
            }
        }
        "photos" => {
            let (op, p) = split_op(field, value)?;
            let p = absolute(p)?;
            if op == '+' {
                let next: i64 = conn.query_row(
                    "SELECT COALESCE(MAX(position) + 1, 0) FROM photos WHERE node_id = ?1",
                    [n.id],
                    |r| r.get(0),
                )?;
                conn.execute(
                    "INSERT INTO photos (node_id, position, path) VALUES (?1, ?2, ev_store(?3))",
                    params![n.id, next, p],
                )?;
            } else {
                conn.execute(
                    "DELETE FROM photos WHERE node_id = ?1 AND path = ev_store(?2)",
                    params![n.id, p],
                )?;
            }
        }
        "temporary" => {
            let v = match value.trim() {
                "true" | "yes" | "1" => true,
                "false" | "no" | "0" | "" => false,
                other => {
                    return Err(usage(
                        "edit_not_boolean",
                        json!({ "field": field, "value": other }),
                    ));
                }
            };
            conn.execute(
                &format!("UPDATE nodes SET {field} = ?1 WHERE id = ?2"),
                params![v, n.id],
            )?;
        }
        "came" => {
            // When it came, as remembered (spec/past-belongings.md); empty clears it.
            let v = text(value)
                .map(|d| super::past::partial_date(&d))
                .transpose()?;
            if let (Some(c), Some(l)) = (&v, departure_field(conn, n.id, "left")?.as_str()) {
                super::past::came_before_left(c, l)?;
            }
            conn.execute(
                "UPDATE nodes SET came_at = ?1 WHERE id = ?2",
                params![v, n.id],
            )?;
        }
        "left" | "left_in" => {
            // When a gone record left, and where it was then, said again as remembered;
            // empty clears it.
            if n.state != State::Gone {
                return Err(refuse("has_not_left", json!({ "id": n.id }), Value::Null));
            }
            let v = text(value);
            if field == "left" {
                let at = v.map(|d| super::past::partial_date(&d)).transpose()?;
                if let Some(a) = &at
                    && let Some(c) = super::past::came_of(conn, n.id)?
                {
                    super::past::came_before_left(&c, a)?;
                }
                conn.execute(
                    "INSERT INTO departures (node_id, at) VALUES (?1, ?2)
                     ON CONFLICT(node_id) DO UPDATE SET at = excluded.at",
                    params![n.id, at],
                )?;
            } else {
                if v.as_deref()
                    .is_some_and(|p| p.starts_with('#') || p.chars().all(|c| c.is_ascii_digit()))
                {
                    return Err(usage("edit_left_in_not_place", Value::Null));
                }
                let place = v
                    .map(|p| super::places::place_or_create(conn, &p))
                    .transpose()?;
                conn.execute(
                    "INSERT INTO departures (node_id, place_id) VALUES (?1, ?2)
                     ON CONFLICT(node_id) DO UPDATE SET place_id = excluded.place_id",
                    params![n.id, place],
                )?;
            }
        }
        "waits_for" => {
            // What its place waits on (spec/waits-for.md): a record, lost or not; never itself or
            // something inside it, which could not turn up apart from it.
            let other = text(value).map(|r| resolve(conn, &r, false)).transpose()?;
            if let Some(o) = other
                && (o == n.id || is_descendant(conn, o, n.id)?)
            {
                return Err(refuse(
                    "waits_for_itself",
                    json!({ "node": label(n) }),
                    Value::Null,
                ));
            }
            conn.execute(
                "UPDATE nodes SET waits_for = ?1 WHERE id = ?2",
                params![other, n.id],
            )?;
        }
        "to" | "owner" | "with" => {
            let column = format!("{field}_place");
            let place = text(value).map(|t| place_or_create(conn, &t)).transpose()?;
            if field == "with" && place.is_some() && n.owner.is_some() {
                return Err(refuse(
                    "not_ours_to_lend",
                    json!({ "node": label(n) }),
                    Value::Null,
                ));
            }
            conn.execute(
                &format!("UPDATE nodes SET {column} = ?1 WHERE id = ?2"),
                params![place, n.id],
            )?;
        }
        other => {
            return Err(usage("edit_field_unknown", json!({ "field": other })));
        }
    }
    Ok(())
}

fn split_op<'a>(field: &str, value: &'a str) -> Result<(char, &'a str)> {
    let v = value.trim();
    match v.chars().next() {
        Some(c @ ('+' | '-')) => Ok((c, &v[1..])),
        _ => Err(usage(
            "edit_not_plus_minus",
            json!({ "field": field, "value": v }),
        )),
    }
}
