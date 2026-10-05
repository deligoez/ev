//! One page of numbers about the home (spec/stats.md): how big it is, what it cost as far as the
//! linked purchases say, how far the counting has come, where the purchases stand and what moved
//! lately. Facts computed when asked; nothing here is stored or scored.

use std::collections::{BTreeMap, HashMap};

use rusqlite::Connection;
use serde_json::{Map, Value, json};

use crate::Result;
use crate::model::{Kind, Node};
use crate::purchases::money;
use crate::store::{Inventory, brief_json, ids, live_nodes};

/// How many of the most of something a section lists.
const TOP: usize = 5;

/// What one link of a purchase line to a record cost: the line's paid amount shared by the units
/// linked, in its currency, on its date.
struct Cost {
    node: i64,
    cents: i64,
    currency: String,
    date: Option<String>,
}

fn costs(conn: &Connection) -> Result<Vec<Cost>> {
    let mut stmt = conn.prepare(
        "SELECT l.node_id, p.paid * l.qty / MAX(p.qty * p.pack, 1), COALESCE(p.currency, ''),
                COALESCE(p.delivered_at, p.ordered_at)
           FROM purchase_links l JOIN purchases p ON p.id = l.purchase_id
           JOIN nodes n ON n.id = l.node_id
          WHERE p.paid IS NOT NULL AND n.state != 'gone'",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Cost {
            node: r.get(0)?,
            cents: r.get(1)?,
            currency: r.get(2)?,
            date: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Cents per currency as `{"TRY": "1999.00"}`, the empty currency read as the home one.
fn per_currency(sums: &BTreeMap<String, i64>, home: &str) -> Value {
    let mut out = Map::new();
    for (c, cents) in sums.iter().filter(|(_, c)| **c != 0) {
        let c = if c.is_empty() { home } else { c };
        let had = out
            .get(c)
            .and_then(Value::as_str)
            .and_then(|s| crate::purchases::parse_money(s).ok())
            .unwrap_or(0);
        out.insert(c.to_string(), json!(money(had + cents)));
    }
    Value::Object(out)
}

/// The room `id` is in, walking up the tree.
fn room_of(by_id: &HashMap<i64, &Node>, id: i64) -> Option<i64> {
    let mut cur = Some(id);
    while let Some(n) = cur.and_then(|c| by_id.get(&c)) {
        if n.kind == Kind::Room {
            return Some(n.id);
        }
        cur = n.parent_id;
    }
    None
}

fn refs(conn: &Connection, list: &[(i64, Value)]) -> Result<Vec<Value>> {
    list.iter()
        .map(|(id, extra)| {
            let mut v = brief_json(conn, *id)?;
            if let Some(o) = extra.as_object() {
                for (k, x) in o {
                    v[k] = x.clone();
                }
            }
            Ok(v)
        })
        .collect()
}

impl Inventory {
    /// Every section of spec/stats.md §3.
    pub fn stats(&self) -> Result<Value> {
        let conn = &self.conn;
        let all = live_nodes(conn)?;
        let by_id: HashMap<i64, &Node> = all.iter().map(|n| (n.id, n)).collect();
        let items: Vec<&Node> = all.iter().filter(|n| n.kind == Kind::Item).collect();
        let units = |n: &Node| n.qty.unwrap_or(1).max(0);
        let home = crate::money::home_currency(conn)?;

        // Overview.
        let kinds = |k: Kind| all.iter().filter(|n| n.kind == k).count();
        let photographed = ids(
            conn,
            "SELECT DISTINCT p.node_id FROM photos p JOIN nodes n ON n.id = p.node_id
              WHERE n.state != 'gone'",
            [],
        )?
        .len();
        let documents: i64 = conn.query_row("SELECT COUNT(*) FROM documents", [], |r| r.get(0))?;
        let spread: i64 = conn.query_row(
            "SELECT COUNT(*) FROM (SELECT thing FROM nodes WHERE thing IS NOT NULL
               AND state != 'gone' GROUP BY thing HAVING COUNT(*) > 1)",
            [],
            |r| r.get(0),
        )?;
        let overview = json!({
            "records": items.len(),
            "units": items.iter().map(|n| units(n)).sum::<i64>(),
            "rooms": kinds(Kind::Room),
            "furniture": kinds(Kind::Furniture),
            "containers": kinds(Kind::Container),
            "in_several_places": spread,
            "with_photo": photographed,
            "documents": documents,
        });

        // Value: what the linked purchases say the things still here cost.
        let costs = costs(conn)?;
        let mut sums: BTreeMap<String, i64> = BTreeMap::new();
        // Per record: what it cost as paid, its currency, what that is today (as paid where
        // it cannot be converted), and whether every line of it could be.
        let mut per_node: HashMap<i64, (i64, String, i64, bool)> = HashMap::new();
        let (mut today_cents, mut converted) = (0i64, 0usize);
        for c in &costs {
            *sums.entry(c.currency.clone()).or_default() += c.cents;
            let today = crate::money::today_money(
                conn,
                c.cents,
                Some(c.currency.as_str()).filter(|s| !s.is_empty()),
                c.date.as_deref(),
            )?;
            let t = today
                .as_ref()
                .and_then(|t| t["amount"].as_str())
                .and_then(|a| crate::purchases::parse_money(a).ok());
            if let Some(t) = t {
                today_cents += t;
                converted += 1;
            }
            let e = per_node
                .entry(c.node)
                .or_insert((0, c.currency.clone(), 0, true));
            e.0 += c.cents;
            e.2 += t.unwrap_or(c.cents);
            e.3 &= t.is_some();
        }
        // The dearest by what they cost in today's money, so a price from ten years ago does
        // not read as cheap.
        let mut dearest: Vec<(i64, (i64, String, i64, bool))> =
            per_node.clone().into_iter().collect();
        dearest.sort_by_key(|(id, (_, _, rank, _))| (-rank, *id));
        let dearest: Vec<(i64, Value)> = dearest
            .into_iter()
            .take(TOP)
            .map(|(id, (cents, cur, today, all))| {
                let cur = if cur.is_empty() { home.clone() } else { cur };
                let today = all.then(|| json!({ "amount": money(today), "currency": home }));
                (
                    id,
                    json!({ "cost": money(cents), "currency": cur, "today": today }),
                )
            })
            .collect();
        let mut valued: BTreeMap<String, i64> = BTreeMap::new();
        let mut valued_n = 0;
        {
            let mut stmt = conn.prepare(
                "SELECT v.amount, v.currency FROM valuations v JOIN nodes n ON n.id = v.node_id
                  WHERE n.state != 'gone' AND v.id = (SELECT v2.id FROM valuations v2
                    WHERE v2.node_id = v.node_id ORDER BY v2.at DESC, v2.id DESC LIMIT 1)",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows {
                let (a, c) = row?;
                *valued.entry(c).or_default() += a;
                valued_n += 1;
            }
        }
        let value = json!({
            "cost": per_currency(&sums, &home),
            "today": (converted > 0).then(|| json!({
                "amount": money(today_cents), "currency": home,
                "lines": converted, "of": costs.len(),
            })),
            "things_with_cost": per_node.len(),
            "things": items.len(),
            "dearest": refs(conn, &dearest)?,
            "valued": { "things": valued_n, "latest": per_currency(&valued, &home) },
        });

        // Rooms.
        let mut rooms: BTreeMap<i64, (usize, i64, usize, BTreeMap<String, i64>)> = all
            .iter()
            .filter(|n| n.kind == Kind::Room)
            .map(|n| (n.id, (0, 0, 0, BTreeMap::new())))
            .collect();
        for n in &all {
            if n.kind == Kind::Room {
                continue;
            }
            if let Some(r) = room_of(&by_id, n.id).and_then(|r| rooms.get_mut(&r)) {
                if n.kind == Kind::Item {
                    r.0 += 1;
                    r.1 += units(n);
                } else {
                    r.2 += 1;
                }
            }
        }
        for c in &costs {
            if let Some(r) = room_of(&by_id, c.node).and_then(|r| rooms.get_mut(&r)) {
                *r.3.entry(c.currency.clone()).or_default() += c.cents;
            }
        }
        let mut rooms: Vec<(i64, Value)> = rooms
            .into_iter()
            .map(|(id, (records, u, holders, cost))| {
                (
                    id,
                    json!({ "records": records, "units": u, "holders": holders,
                            "cost": per_currency(&cost, &home) }),
                )
            })
            .collect();
        rooms.sort_by_key(|(id, v)| (-v["records"].as_i64().unwrap_or(0), *id));

        // Tour.
        let p = self.progress()?;
        let counted = items
            .iter()
            .filter(|n| {
                crate::plan::review_inherited(conn, n.id).is_ok_and(|r| r["status"] == "toured")
            })
            .count();
        let tour = json!({
            "places": p["units"], "toured": p["toured"], "counting": p["counting"],
            "kept": p["kept"], "raw": p["raw"], "changed_since": p["changed_since_tour"],
            "things_in_counted_places": counted, "things": items.len(),
        });

        let purchases = purchases_section(conn, &home)?;
        let activity = activity_section(conn)?;
        // Holders.
        let mut children: HashMap<i64, usize> = HashMap::new();
        for n in &all {
            if let Some(p) = n.parent_id {
                *children.entry(p).or_default() += 1;
            }
        }
        let containers: Vec<&Node> = all.iter().filter(|n| n.kind == Kind::Container).collect();
        let (mut empty, mut not_known) = (0, 0);
        for c in containers.iter().filter(|c| !children.contains_key(&c.id)) {
            if crate::store::known_empty(conn, c.id)? {
                empty += 1;
            } else {
                not_known += 1;
            }
        }
        let fills: Vec<i64> = all.iter().filter_map(|n| n.fill).collect();
        let mut fullest: Vec<(i64, usize)> = all
            .iter()
            .filter(|n| n.kind != Kind::Item && n.kind != Kind::Room && n.kind != Kind::Home)
            .map(|n| (n.id, children.get(&n.id).copied().unwrap_or(0)))
            .filter(|(_, c)| *c > 0)
            .collect();
        fullest.sort_by_key(|(id, c)| (std::cmp::Reverse(*c), *id));
        let fullest: Vec<(i64, Value)> = fullest
            .into_iter()
            .take(TOP)
            .map(|(id, c)| (id, json!({ "records": c })))
            .collect();
        let holders = json!({
            "containers": containers.len(),
            "empty": empty,
            "empty_not_known": not_known,
            "with_fill": fills.len(),
            "average_fill": (!fills.is_empty())
                .then(|| fills.iter().sum::<i64>() / fills.len() as i64),
            "full": fills.iter().filter(|f| **f >= 90).count(),
            "most_records": refs(conn, &fullest)?,
        });

        // Coverage.
        let covers = self.cover_list(false)?;
        let list = covers["coverages"].as_array().cloned().unwrap_or_default();
        let coverage = json!({
            "coverages": list.len(),
            "active": list.iter().filter(|c| c["status"] == "active" || c["status"] == "ending").count(),
            "ending": list.iter().filter(|c| c["status"] == "ending").count(),
        });

        // Tags.
        let mut tags: BTreeMap<&str, usize> = BTreeMap::new();
        for n in &all {
            for t in &n.tags {
                *tags.entry(t.as_str()).or_default() += 1;
            }
        }
        let mut tags: Vec<(&str, usize)> = tags.into_iter().collect();
        tags.sort_by_key(|(t, n)| (std::cmp::Reverse(*n), *t));
        let tags: Vec<Value> = tags
            .into_iter()
            .take(10)
            .map(|(t, n)| json!({ "tag": t, "records": n }))
            .collect();

        // The things bought longest ago.
        let mut oldest: Vec<(i64, String)> = Vec::new();
        {
            let mut stmt = conn.prepare(
                "SELECT l.node_id, MIN(COALESCE(p.ordered_at, p.delivered_at))
                   FROM purchase_links l JOIN purchases p ON p.id = l.purchase_id
                   JOIN nodes n ON n.id = l.node_id
                  WHERE n.state != 'gone' AND COALESCE(p.ordered_at, p.delivered_at) IS NOT NULL
                  GROUP BY l.node_id ORDER BY 2, 1 LIMIT ?1",
            )?;
            let rows = stmt.query_map([TOP as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
            for row in rows {
                oldest.push(row?);
            }
        }
        let oldest: Vec<(i64, Value)> = oldest
            .into_iter()
            .map(|(id, d)| (id, json!({ "bought": d })))
            .collect();

        Ok(json!({
            "overview": overview,
            "value": value,
            "rooms": refs(conn, &rooms)?,
            "tour": tour,
            "purchases": purchases,
            "activity": activity,
            "holders": holders,
            "coverage": coverage,
            "tags": tags,
            "oldest": refs(conn, &oldest)?,
            "past": self.past_summary()?,
        }))
    }
}

/// Lines, linked, dismissed and still open; lines and amount per year; the shops with most lines.
fn purchases_section(conn: &Connection, home: &str) -> Result<Value> {
    let count = |sql: &str| -> Result<i64> { Ok(conn.query_row(sql, [], |r| r.get(0))?) };
    let mut stmt = conn.prepare(
        "SELECT COALESCE(substr(ordered_at, 1, 4), '?'), COUNT(*),
                COALESCE(currency, ''), COALESCE(SUM(paid), 0)
           FROM purchases WHERE same_as IS NULL GROUP BY 1, 3",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
        ))
    })?;
    let mut by_year: BTreeMap<String, (i64, BTreeMap<String, i64>)> = BTreeMap::new();
    for row in rows {
        let (y, n, c, paid) = row?;
        let e = by_year.entry(y).or_default();
        e.0 += n;
        *e.1.entry(c).or_default() += paid;
    }
    let years: Vec<Value> = by_year
        .into_iter()
        .rev()
        .map(|(y, (n, paid))| {
            let year = (y != "?").then_some(y);
            json!({ "year": year, "lines": n, "paid": per_currency(&paid, home) })
        })
        .collect();
    let mut stmt = conn.prepare(
        "SELECT shop, COUNT(*), COALESCE(currency, ''), COALESCE(SUM(paid), 0)
           FROM purchases WHERE same_as IS NULL AND shop IS NOT NULL
          GROUP BY shop, 3 ORDER BY 2 DESC, shop LIMIT ?1",
    )?;
    let rows = stmt.query_map([TOP as i64], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
        ))
    })?;
    let mut shops = Vec::new();
    for row in rows {
        let (shop, n, c, paid) = row?;
        let sum = BTreeMap::from([(c, paid)]);
        shops.push(json!({ "shop": shop, "lines": n, "paid": per_currency(&sum, home) }));
    }
    Ok(json!({
        "lines": count("SELECT COUNT(*) FROM purchases WHERE same_as IS NULL")?,
        "linked": count("SELECT COUNT(DISTINCT purchase_id) FROM purchase_links")?,
        "dismissed": count("SELECT COUNT(*) FROM purchases WHERE dismissed IS NOT NULL")?,
        "open_durable": count(
            "SELECT COUNT(*) FROM purchases WHERE same_as IS NULL AND dismissed IS NULL
               AND bucket = 'durable' AND status = 'delivered'
               AND id NOT IN (SELECT purchase_id FROM purchase_links)",
        )?,
        "years": years,
        "shops": shops,
    }))
}

/// What happened in the last 30 days: records added, moves, things gone by how, photos, and
/// the busiest day.
fn activity_section(conn: &Connection) -> Result<Value> {
    let since = (chrono::Utc::now() - chrono::Duration::days(30))
        .format("%Y-%m-%d")
        .to_string();
    let since_count =
        |sql: &str| -> Result<i64> { Ok(conn.query_row(sql, [&since], |r| r.get(0))?) };
    let mut gone = Map::new();
    let mut stmt = conn.prepare(
        "SELECT COALESCE(json_extract(data, '$.as'), '?'), COUNT(*) FROM events
          WHERE type = 'gone' AND at >= ?1 GROUP BY 1 ORDER BY 2 DESC",
    )?;
    let rows = stmt.query_map([&since], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (how, n) = row?;
        gone.insert(how, json!(n));
    }
    let busiest: Option<(String, i64)> = conn
        .query_row(
            "SELECT substr(at, 1, 10), COUNT(*) FROM events WHERE at >= ?1
              GROUP BY 1 ORDER BY 2 DESC, 1 DESC LIMIT 1",
            [&since],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    Ok(json!({
        "since": since,
        "added": since_count("SELECT COUNT(*) FROM events WHERE type = 'create' AND at >= ?1")?,
        "moved": since_count(
            "SELECT COUNT(*) FROM events WHERE type IN ('move', 'done') AND at >= ?1"
        )?,
        "gone": gone,
        "photos": since_count("SELECT COUNT(*) FROM events WHERE type = 'photo' AND at >= ?1")?,
        "busiest_day": busiest.map(|(d, n)| json!({ "day": d, "events": n })),
    }))
}
