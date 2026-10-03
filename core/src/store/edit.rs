//! `field=value` edits: parsing each field, checking it and recording one `edit` event per
//! record.

use super::*;

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
        "make" => json!(n.make),
        "model" => json!(n.model),
        "serial" => json!(n.serial),
        _ => Value::Null,
    }
}

/// `WxDxH` or `WxD`, each a positive number (`,` or `.` for decimals, `x`, `×` or `*`
/// between); written back as `1x2x0.5`.
pub(crate) fn parse_size(s: &str) -> Result<Vec<f64>> {
    let bad = || Error::Usage(format!("size is WxDxH or WxD, like 1x2x0.5; got `{s}`"));
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
        .map_err(|_| Error::Usage(format!("{field} must be an integer, got `{v}`")))
}

/// One record's `field=value` assignments inside the caller's transaction, recorded as one
/// `edit` event with each field's value before the first and after the last assignment (so
/// `tags=+a` then `tags=+b` reads as one change). Returns the record and those changes.
pub(super) fn edit_in(
    conn: &Connection,
    reference: &str,
    assignments: &[String],
) -> Result<(i64, serde_json::Map<String, Value>)> {
    // A gone node is found by id only, and only its note may change: the record of why it
    // left belongs on it, while every other field describes a thing no longer here.
    let id = match resolve(conn, reference, false) {
        Err(Error::NotFound(_)) if reference.trim().chars().all(|c| c.is_ascii_digit()) => {
            let id = resolve(conn, reference, true)?;
            if let Some(a) = assignments
                .iter()
                .find(|a| a.split_once('=').is_none_or(|(f, _)| f.trim() != "note"))
            {
                return Err(refused(
                    format!("node {id} is gone; only its note can change, not `{a}`"),
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
            .ok_or_else(|| Error::Usage(format!("`{a}` is not field=value")))?;
        let field = field.trim();
        let before = load(conn, id)?;
        apply_edit(conn, &before, field, value)?;
        let after = load(conn, id)?;
        let first = changes
            .get(field)
            .map(|c| c["before"].clone())
            .unwrap_or_else(|| field_value(&before, field));
        let last = field_value(&after, field);
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
            let v = text(value).ok_or_else(|| Error::Usage("name cannot be empty".into()))?;
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
                return Err(refused(
                    "only a home has an address; clear it first",
                    Value::Null,
                ));
            }
            if !matches!(k, Kind::Home | Kind::Room) {
                let rooms: Vec<i64> = ids(
                    conn,
                    "SELECT id FROM nodes WHERE parent_id = ?1 AND kind = 'room' AND state != 'gone'",
                    [n.id],
                )?;
                if !rooms.is_empty() {
                    return Err(refused(
                        format!("{} holds rooms, so it must stay a home or a room", label(n)),
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
                return Err(refused("only a home has an address", Value::Null));
            }
            conn.execute(
                "UPDATE nodes SET address = ?1 WHERE id = ?2",
                params![v, n.id],
            )?;
        }
        // A note is a log the person adds to: `note=+text` appends it on a new line.
        "note" if value.trim_start().starts_with('+') => {
            let added = text(&value.trim_start()[1..])
                .ok_or_else(|| Error::Usage("note=+ needs the text to add".into()))?;
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
                    return Err(Error::Usage(format!(
                        "{field} takes true or false, got `{other}`"
                    )));
                }
            };
            conn.execute(
                &format!("UPDATE nodes SET {field} = ?1 WHERE id = ?2"),
                params![v, n.id],
            )?;
        }
        "to" | "owner" | "with" => {
            let column = format!("{field}_place");
            let place = text(value).map(|t| place_or_create(conn, &t)).transpose()?;
            if field == "with" && place.is_some() && n.owner.is_some() {
                return Err(refused(
                    format!("{} is not ours; it cannot be lent out", label(n)),
                    Value::Null,
                ));
            }
            conn.execute(
                &format!("UPDATE nodes SET {column} = ?1 WHERE id = ?2"),
                params![place, n.id],
            )?;
        }
        other => {
            return Err(Error::Usage(format!(
                "unknown or read-only field `{other}`; editable: name, code, kind, address, qty, note, theme, fill, tags, photos, to, owner, with, temporary, make, model, serial (how far a place is counted is `ev review`)"
            )));
        }
    }
    Ok(())
}

fn split_op<'a>(field: &str, value: &'a str) -> Result<(char, &'a str)> {
    let v = value.trim();
    match v.chars().next() {
        Some(c @ ('+' | '-')) => Ok((c, &v[1..])),
        _ => Err(Error::Usage(format!(
            "{field} takes +value or -value, got `{v}`"
        ))),
    }
}
