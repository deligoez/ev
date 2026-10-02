//! Which purchase lines could be this thing (purchases spec §5). Measured on a shop's export
//! against real records: a shared model code is near-certain, shared generic words ("kart",
//! "led", "64 gb") are wrong, and two numbers of one unit that differ (125 kHz, 13.56 MHz) rule
//! a line out however many words match. The person still decides; this only orders the
//! question.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use serde_json::{Value, json};

use crate::Result;
use crate::fold;
use crate::model::Node;
use crate::placement::{candidates, terms};
use crate::purchases::purchase_json;
use crate::store::{Inventory, ids, load, resolve};

/// Below this a line is not offered when a thing is recorded. Measured on one shop's 158 lines
/// against 451 records: every known right match scored above it except one part of a set whose
/// sibling did, and one record in about thirty got a wrong first offer.
pub const OFFER_AT: f64 = 15.0;

const ALIAS: f64 = 80.0;
const EXACT_KEY: f64 = 60.0;
const CODE: f64 = 25.0;
const BRAND: f64 = 12.0;
const WORDS_CAP: f64 = 30.0;
const CONFLICT: f64 = -40.0;

/// Letters and digits only, lowercased: `GSB 13-RE` and `gsb13re` read alike.
fn squash(s: &str) -> String {
    fold(s).chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Whether `phrase` (squashed) is one to four whole words of `text` run together, or starts a
/// word when it is long enough to mean something on its own: `Pro's Kit` is in "Kombine pense,
/// Pro'sKit", `Ecotag` in "ecotagPLUS", `HP` is not in "siyah plastik".
fn has_phrase(text: &str, phrase: &str) -> bool {
    let words: Vec<String> = fold(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    (0..words.len()).any(|i| {
        phrase.chars().count() >= 4 && words[i].starts_with(phrase)
            || (1..=4)
                .filter(|n| i + n <= words.len())
                .any(|n| words[i..i + n].concat() == phrase)
    })
}

/// Tokens that mix letters and digits (`lr1130`, `tbx306f`, `1pk052ds`), hyphens dropped.
fn codes(text: &str) -> HashSet<String> {
    fold(text)
        .split(|c: char| !(c.is_alphanumeric() || c == '-'))
        .map(|t| t.replace('-', ""))
        // Three characters (`m10`, `pc4`) are sizes and fittings shared by unrelated things;
        // `3x3` is a size.
        .filter(|t| {
            t.chars().count() >= 4
                && t.chars().any(|c| c.is_ascii_digit())
                && t.chars().any(|c| c.is_alphabetic())
                && !is_measure(t)
                && !t
                    .split(['x', '×'])
                    .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        })
        .collect()
}

/// A number with a unit written together (`64gb`, `5v`, `2450mah`): a size, not a code.
fn is_measure(t: &str) -> bool {
    let unit = t.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ',');
    unit.len() < t.len()
        && [
            "mm", "cm", "m", "kb", "mb", "gb", "tb", "hz", "khz", "mhz", "ghz", "g", "gr", "kg",
            "ml", "l", "lt", "v", "w", "mah", "a", "x", "li", "lu", "adet",
        ]
        .contains(&unit)
}

/// Numbers with a unit, each turned into one base unit per dimension: `128 GB` is
/// `(storage, 131072)`, `2,5 cm` is `(length, 25)`, `13.56 MHz` is `(frequency, 13560000)`.
fn measures(text: &str) -> Vec<(&'static str, f64)> {
    let t = fold(text);
    let chars: Vec<char> = t.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        // A number inside a code (`lr1130`) is no measure; after a size's `x` (`3,5x30`) it is.
        if !chars[i].is_ascii_digit()
            || i > 0 && chars[i - 1].is_alphanumeric() && !matches!(chars[i - 1], 'x' | '×')
        {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == ',') {
            i += 1;
        }
        let number: String = chars[start..i].iter().collect();
        let Ok(n) = number
            .trim_end_matches(['.', ','])
            .replace(',', ".")
            .parse::<f64>()
        else {
            continue;
        };
        let mut j = i;
        while j < chars.len() && chars[j] == ' ' {
            j += 1;
        }
        let unit_start = j;
        while j < chars.len() && chars[j].is_alphabetic() {
            j += 1;
        }
        let unit: String = chars[unit_start..j].iter().collect();
        let base = match unit.as_str() {
            "mm" => Some(("length", 1.0)),
            "cm" => Some(("length", 10.0)),
            "kb" => Some(("storage", 1.0 / 1024.0)),
            "mb" => Some(("storage", 1.0)),
            "gb" => Some(("storage", 1024.0)),
            "tb" => Some(("storage", 1024.0 * 1024.0)),
            "hz" => Some(("frequency", 1.0)),
            "khz" => Some(("frequency", 1e3)),
            "mhz" => Some(("frequency", 1e6)),
            "ghz" => Some(("frequency", 1e9)),
            "g" | "gr" => Some(("mass", 1.0)),
            "kg" => Some(("mass", 1000.0)),
            "ml" => Some(("volume", 1.0)),
            "l" | "lt" => Some(("volume", 1000.0)),
            "v" => Some(("voltage", 1.0)),
            "w" => Some(("power", 1.0)),
            "mah" => Some(("charge", 1.0)),
            _ => None,
        };
        if let Some((dim, k)) = base {
            out.push((dim, n * k));
        }
        i = j.max(i + 1);
    }
    out
}

/// Dimensions where both texts give values and none of them agree.
#[cfg(test)]
fn conflicts(a: &str, b: &str) -> Vec<&'static str> {
    conflicting(&measures(a), &measures(b))
}

/// `conflicts` on measures already read.
fn conflicting(ma: &[(&'static str, f64)], mb: &[(&'static str, f64)]) -> Vec<&'static str> {
    let dims: HashSet<&str> = ma.iter().map(|(d, _)| *d).collect();
    let mut out: Vec<&str> = dims
        .into_iter()
        .filter(|d| {
            let va: Vec<f64> = ma.iter().filter(|(x, _)| x == d).map(|(_, v)| *v).collect();
            let vb: Vec<f64> = mb.iter().filter(|(x, _)| x == d).map(|(_, v)| *v).collect();
            !vb.is_empty()
                && !va
                    .iter()
                    .any(|x| vb.iter().any(|y| (x - y).abs() <= 1e-6 * x.abs().max(1.0)))
        })
        .collect();
    out.sort_unstable();
    out
}

/// The searchable words of a text, each with its stems.
fn words(text: &str) -> Vec<(String, Vec<String>)> {
    terms(text)
        .into_iter()
        .filter(|t| !t.key.chars().any(|c| c.is_ascii_digit()))
        .map(|t| {
            let stems = candidates(&t.key);
            (t.key, stems)
        })
        .collect()
}

struct Corpus {
    /// How many lines a word (any of its stems) appears in.
    df: HashMap<String, usize>,
    lines: usize,
}

impl Corpus {
    fn weight(&self, stems: &[String]) -> f64 {
        let df = stems
            .iter()
            .filter_map(|s| self.df.get(s))
            .max()
            .copied()
            .unwrap_or(0);
        (1.0 + self.lines as f64 / (1.0 + df as f64)).ln()
    }
}

/// The node's own text: name, make, model, note and tags.
fn node_text(n: &Node) -> String {
    [
        Some(n.name.clone()),
        n.make.clone(),
        n.model.clone(),
        n.note.clone(),
        Some(n.tags.join(" ")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ")
}

/// A purchase line with what scoring reads from it, read once however many things it is
/// scored against.
struct Line {
    value: Value,
    name: String,
    squashed: String,
    codes: HashSet<String>,
    words: Vec<(String, Vec<String>)>,
    measures: Vec<(&'static str, f64)>,
    /// The line's brand, squashed, unless it is the shop's own name.
    brand: Option<String>,
    /// Things this product was linked to before: `(id, folded name)`.
    aliases: Vec<(i64, String)>,
}

impl Line {
    fn new(conn: &Connection, value: Value) -> Result<Line> {
        let name = value["name"].as_str().unwrap_or_default().to_string();
        let mut aliases = Vec::new();
        if let (Some(shop), Some(sku)) = (value["shop"].as_str(), value["shop_sku"].as_str()) {
            for a in ids(
                conn,
                "SELECT node_id FROM purchase_aliases WHERE shop = ?1 AND shop_sku = ?2",
                [shop, sku],
            )? {
                aliases.push((a, fold(&load(conn, a)?.name)));
            }
        }
        let shop = value["shop"].as_str().map(squash).unwrap_or_default();
        let brand = value["brand"]
            .as_str()
            .map(squash)
            .filter(|b| b.len() >= 2 && *b != shop);
        Ok(Line {
            squashed: squash(&name),
            codes: codes(&name),
            words: words(&name),
            measures: measures(&name),
            brand,
            aliases,
            name,
            value,
        })
    }
}

/// A thing with what scoring reads from it.
struct Thing<'a> {
    node: &'a Node,
    folded_name: String,
    /// Model and serial, squashed, when long enough to mean something.
    keys: Vec<(&'static str, String)>,
    codes: HashSet<String>,
    words: Vec<(String, Vec<String>)>,
    measures: Vec<(&'static str, f64)>,
    /// Name and make: where a line's brand is looked for.
    own: String,
    make: Option<String>,
}

impl<'a> Thing<'a> {
    fn new(n: &'a Node) -> Thing<'a> {
        let text = node_text(n);
        Thing {
            node: n,
            folded_name: fold(&n.name),
            keys: [("model", &n.model), ("serial", &n.serial)]
                .into_iter()
                .filter_map(|(f, v)| {
                    v.as_deref()
                        .map(squash)
                        .filter(|v| v.len() >= 3)
                        .map(|v| (f, v))
                })
                .collect(),
            codes: codes(&text),
            words: words(&text),
            measures: measures(&text),
            own: format!("{} {}", n.name, n.make.as_deref().unwrap_or_default()),
            make: n.make.as_deref().map(squash).filter(|m| m.len() >= 2),
        }
    }
}

/// The score of line `p` for thing `n`, with its reasons as `(why, points)`.
fn score(corpus: &Corpus, n: &Thing, p: &Line) -> (f64, Vec<(String, f64)>) {
    let mut why = Vec::new();
    let mut total = 0.0;
    let mut add = |what: String, points: f64| {
        total += points;
        why.push((what, points));
    };
    // 1. A product linked before to a thing of the same name.
    if let Some((a, _)) = p
        .aliases
        .iter()
        .find(|(a, name)| *a != n.node.id && *name == n.folded_name)
    {
        add(format!("bought before for #{a}"), ALIAS);
    }
    // 2. The thing's model or serial written in the line.
    for (field, v) in &n.keys {
        if p.squashed.contains(v.as_str()) {
            add(format!("{field} {v}"), EXACT_KEY);
        }
    }
    // 3. Model codes both carry.
    let mut shared: Vec<&String> = n.codes.intersection(&p.codes).collect();
    shared.sort();
    for c in shared.into_iter().take(2) {
        add(format!("code {c}"), CODE);
    }
    // 4. The brand, compared without spaces or marks (`Pro's Kit` is `Pro'sKit`), in the
    //    thing's name or make only (a note says "for Arduino" of every module), never the
    //    shop's own name (a shop's own service names it as the brand).
    if let Some(b) = p.brand.as_ref().filter(|b| has_phrase(&n.own, b)) {
        add(format!("brand {b}"), BRAND);
    } else if let Some(m) = n.make.as_ref().filter(|m| has_phrase(&p.name, m)) {
        add(format!("brand {m}"), BRAND);
    }
    // 5. Words, weighted by how rare they are among the lines.
    let mut points = 0.0;
    let mut matched: Vec<&str> = Vec::new();
    for (w, stems) in &n.words {
        if p.words
            .iter()
            .any(|(_, ls)| ls.iter().any(|s| stems.contains(s)))
            && !matched.contains(&w.as_str())
        {
            points += corpus.weight(stems);
            matched.push(w);
        }
    }
    if !matched.is_empty() {
        add(
            format!("words {}", matched.join(", ")),
            points.min(WORDS_CAP),
        );
    }
    // 6. Numbers of one unit that differ.
    for d in conflicting(&n.measures, &p.measures) {
        add(format!("{d} differs"), CONFLICT);
    }
    (total, why)
}

impl Inventory {
    /// Purchase lines that could be this thing, best first, with the reasons: lines still open
    /// and not dismissed, and any already linked to it.
    pub fn buy_for(&self, reference: &str) -> Result<Value> {
        let id = resolve(&self.conn, reference, true)?;
        Ok(
            json!({ "node": crate::store::brief(&self.conn, id)?, "candidates": candidates_for(&self.conn, id, 0.0, 12)? }),
        )
    }

    /// The back-fill (purchases spec §12.2): every thing in a toured place that no purchase is
    /// linked to yet, with the one open line that could be it, best first. Only lines scoring
    /// above `OFFER_AT` are offered, the bar `ev add` uses. A thing counts when the nearest
    /// reviewed place above it is `toured` (a toured drawer covers the boxes in it, unless a
    /// box has a review of its own).
    pub fn buy_backfill(&self) -> Result<Value> {
        let conn = &self.conn;
        let nodes = crate::store::live_nodes(conn)?;
        let mut reviews: HashMap<i64, String> = HashMap::new();
        {
            let mut stmt = conn.prepare("SELECT node_id, status FROM reviews")?;
            for r in stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))? {
                let (n, s) = r?;
                reviews.insert(n, s);
            }
        }
        let parent: HashMap<i64, Option<i64>> = nodes.iter().map(|n| (n.id, n.parent_id)).collect();
        let linked: HashSet<i64> = ids(conn, "SELECT DISTINCT node_id FROM purchase_links", [])?
            .into_iter()
            .collect();
        let toured = |id: i64| {
            let mut cur = parent.get(&id).copied().flatten();
            for _ in 0..10_000 {
                let Some(c) = cur else { break };
                if let Some(s) = reviews.get(&c) {
                    return s == "toured";
                }
                cur = parent.get(&c).copied().flatten();
            }
            false
        };
        let things: Vec<&Node> = nodes
            .iter()
            .filter(|n| !linked.contains(&n.id) && toured(n.id))
            .collect();
        let matcher = Matcher::new(conn, &nodes)?;
        let mut out = Vec::new();
        for n in &things {
            if let Some(c) = matcher.rank(n, OFFER_AT, 1).into_iter().next() {
                out.push(json!({ "node": crate::store::brief(conn, n.id)?, "candidate": c }));
            }
        }
        out.sort_by(|a, b| {
            b["candidate"]["score"]
                .as_f64()
                .unwrap_or(0.0)
                .total_cmp(&a["candidate"]["score"].as_f64().unwrap_or(0.0))
        });
        Ok(json!({ "backfill": out, "toured_things": things.len() }))
    }
}

/// The lines that can still be offered, and how rare each word is among all lines and the
/// records: built once and used for any number of things.
struct Matcher {
    lines: Vec<Line>,
    corpus: Corpus,
}

impl Matcher {
    fn new(conn: &Connection, nodes: &[Node]) -> Result<Matcher> {
        let all = ids(conn, "SELECT id FROM purchases ORDER BY id", [])?
            .into_iter()
            .map(|p| purchase_json(conn, p))
            .collect::<Result<Vec<_>>>()?;
        // How rare a word is, among the lines and the records together: "sensör" or "vida" are
        // rare among purchases but common in a workshop's records, and say little about which.
        let mut df: HashMap<String, usize> = HashMap::new();
        let mut count = |text: &str| {
            let mut seen = HashSet::new();
            for (_, stems) in words(text) {
                for s in stems {
                    if seen.insert(s.clone()) {
                        *df.entry(s).or_default() += 1;
                    }
                }
            }
        };
        for p in &all {
            count(p["name"].as_str().unwrap_or_default());
        }
        for other in nodes {
            count(&node_text(other));
        }
        let corpus = Corpus {
            df,
            lines: all.len() + nodes.len(),
        };
        // Only a line still open, or linked to something (to show as linked there), is ever
        // offered.
        let lines = all
            .into_iter()
            .filter(|p| open(p) || p["linked"].as_array().is_some_and(|l| !l.is_empty()))
            .map(|p| Line::new(conn, p))
            .collect::<Result<Vec<_>>>()?;
        Ok(Matcher { lines, corpus })
    }

    /// The lines scoring above `at` for `n`, at most `limit`, best first, and any linked to it.
    fn rank(&self, n: &Node, at: f64, limit: usize) -> Vec<Value> {
        let thing = Thing::new(n);
        let mut out = Vec::new();
        for line in &self.lines {
            let p = &line.value;
            let linked_here = p["linked"]
                .as_array()
                .is_some_and(|l| l.iter().any(|x| x["node"]["id"] == n.id));
            if !(open(p) || linked_here) {
                continue;
            }
            let (s, why) = score(&self.corpus, &thing, line);
            if s > at || linked_here {
                let why: Vec<Value> = why
                    .into_iter()
                    .map(
                        |(w, points)| json!({ "why": w, "points": (points * 10.0).round() / 10.0 }),
                    )
                    .collect();
                let mut c = json!({
                    "purchase": {
                        "id": p["id"], "name": p["name"], "shop": p["shop"], "brand": p["brand"],
                        "ordered_at": p["ordered_at"], "delivered_at": p["delivered_at"],
                        "qty": p["qty"], "open_qty": p["open_qty"], "paid": p["paid"],
                        "currency": p["currency"],
                    },
                    "score": (s * 10.0).round() / 10.0,
                    "why": why,
                });
                if linked_here {
                    c["linked"] = json!(true);
                }
                out.push(c);
            }
        }
        out.sort_by(|a, b| {
            b["score"]
                .as_f64()
                .unwrap_or(0.0)
                .total_cmp(&a["score"].as_f64().unwrap_or(0.0))
        });
        out.truncate(limit);
        out
    }
}

/// Something left to link and not dismissed.
fn open(p: &Value) -> bool {
    p["dismissed"].is_null() && p["open_qty"].as_i64().unwrap_or(0) > 0
}

/// The lines scoring above `at` for node `id`, at most `limit`, best first.
pub(crate) fn candidates_for(
    conn: &Connection,
    id: i64,
    at: f64,
    limit: usize,
) -> Result<Vec<Value>> {
    let n = load(conn, id)?;
    if ids(conn, "SELECT id FROM purchases LIMIT 1", [])?.is_empty() {
        return Ok(Vec::new());
    }
    let nodes = crate::store::live_nodes(conn)?;
    Ok(Matcher::new(conn, &nodes)?.rank(&n, at, limit))
}

#[cfg(test)]
mod tests {
    use super::{codes, conflicts, measures};

    #[test]
    fn units_are_converted_before_numbers_are_compared() {
        assert_eq!(measures("3,5x30 mm vida"), vec![("length", 30.0)]);
        assert_eq!(measures("2,5 cm"), vec![("length", 25.0)]);
        assert_eq!(conflicts("Kart 16 GB", "Kart 16GB"), Vec::<&str>::new());
        assert_eq!(
            conflicts("RFID kart 13.56 MHz", "125 kHz RFID kart"),
            ["frequency"]
        );
        assert_eq!(conflicts("vida 30 mm", "vida 3 cm"), Vec::<&str>::new());
        assert_eq!(conflicts("128 GB kart", "64GB kart"), ["storage"]);
        assert!(conflicts("kart", "64 GB kart").is_empty());
    }

    #[test]
    fn a_brand_is_whole_words_never_letters_inside_one() {
        assert!(super::has_phrase("Kombine pense, Pro'sKit", "proskit"));
        assert!(super::has_phrase("Pro's Kit pense", "proskit"));
        assert!(!super::has_phrase("Siyah plastik köşe", "hp"));
        assert!(super::has_phrase("ecotagPLUS şerit", "ecotag"));
        assert!(!super::has_phrase("hpx kablo", "hp"));
    }

    #[test]
    fn codes_mix_letters_and_digits_and_drop_hyphens() {
        let c = codes("Pro'sKit 1PK-052DS pense, LR1130 pil, 64 GB, 64gb, M10, 3x3 küp");
        assert!(c.contains("1pk052ds") && c.contains("lr1130"));
        assert_eq!(c.len(), 2, "{c:?}");
    }
}
