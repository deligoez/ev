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
use crate::purchases::purchase_row;
use crate::store::{Inventory, ids, load, resolve};

/// Below this a line is not offered when a thing is recorded. Measured on one shop's 158 lines
/// against 451 records: every known right match scored above it except one part of a set whose
/// sibling did, and one record in about thirty got a wrong first offer.
pub const OFFER_AT: f64 = 15.0;

const ALIAS: f64 = 80.0;
const EXACT_KEY: f64 = 60.0;
const CODE: f64 = 25.0;
const BRAND: f64 = 12.0;
/// The brand when the line shares nothing of what the thing is: a Pro'sKit pliers line for a
/// Pro'sKit wire stripper.
const BRAND_ASIDE: f64 = 5.0;
const WORDS_CAP: f64 = 30.0;
const CONFLICT: f64 = -40.0;
/// The most words alone can give when none of them is in what the thing is (the head of its
/// name): below `OFFER_AT`, so such a line is ranked but never offered on words alone.
const ASIDE_CAP: f64 = 10.0;

/// Letters and digits only, lowercased: `GSB 13-RE` and `gsb13re` read alike.
fn squash(s: &str) -> String {
    fold(s).chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Whether `phrase` (squashed) is one to four whole words of `text` run together, or starts a
/// word when it is long enough to mean something on its own: `Pro's Kit` is in "Kombine pense,
/// Pro'sKit", `Ecotag` in "ecotagPLUS", `HP` is not in "siyah plastik".
fn has_phrase(text: &str, phrase: &str) -> bool {
    has_words(&tokens(text), phrase, 4)
}

/// The folded words of `text`, split at anything not a letter or digit.
fn tokens(text: &str) -> Vec<String> {
    fold(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether `phrase` (squashed) is one to four of `words` run together, or starts a word when it
/// is at least `prefix_at` characters long.
fn has_words(words: &[String], phrase: &str, prefix_at: usize) -> bool {
    (0..words.len()).any(|i| {
        phrase.chars().count() >= prefix_at && words[i].starts_with(phrase)
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
        // A lone `A` is an ampere only when it is written as one: against the number (`3A`) or
        // after a decimal (`2,5 A`). `Pi 3 A+` is a model.
        let ampere_like = unit_start == i
            || number.contains([',', '.']) && chars.get(j).is_none_or(|c| *c != '+');
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
            "a" if ampere_like => Some(("current", 1000.0)),
            "ma" => Some(("current", 1.0)),
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

/// What a thing is, from its name: the part before the first comma or dash, without what is
/// in brackets. "Kombine pense, Pro'sKit (yeşil-gri saplı)" is a `kombine pense`; "TP4056 Li-ion
/// şarj modülü, USB-C girişli" is a `şarj modülü`, whatever it is for or made of.
fn head(name: &str) -> String {
    let mut plain = String::new();
    let mut depth = 0;
    for c in name.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => plain.push(c),
            _ => {}
        }
    }
    let cut = [",", " — ", " – ", " - ", ";"]
        .iter()
        .filter_map(|s| plain.find(s))
        .min()
        .unwrap_or(plain.len());
    plain[..cut].to_string()
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
    /// The name's folded words, where a model or serial is looked for whole.
    tokens: Vec<String>,
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
            tokens: tokens(&name),
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
    /// The words of what it is, the head of its name, each with its stems.
    head: Vec<(String, Vec<String>)>,
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
            // Codes from what names it, never from the note: a note says what a module is used
            // with ("for an ESP32"), which is not what it is.
            codes: codes(&format!(
                "{} {} {} {}",
                n.name,
                n.make.as_deref().unwrap_or_default(),
                n.model.as_deref().unwrap_or_default(),
                n.serial.as_deref().unwrap_or_default()
            )),
            words: words(&text),
            head: words(&head(&n.name)),
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
    // Whether anything stronger than words says it: an alias, a key, a code, the brand.
    let mut strong = false;
    // 1. A product linked before to a thing of the same name.
    if let Some((a, _)) = p
        .aliases
        .iter()
        .find(|(a, name)| *a != n.node.id && *name == n.folded_name)
    {
        add(format!("bought before for #{a}"), ALIAS);
        strong = true;
    }
    // 2. The thing's model or serial written in the line, as whole words: a short model (`561`)
    //    sits inside many other codes (`6002561`).
    for (field, v) in &n.keys {
        if has_words(&p.tokens, v, 6) {
            add(format!("{field} {v}"), EXACT_KEY);
            strong = true;
        }
    }
    // 3. Model codes both carry.
    let mut shared: Vec<&String> = n.codes.intersection(&p.codes).collect();
    shared.sort();
    for c in shared.into_iter().take(2) {
        add(format!("code {c}"), CODE);
        strong = true;
    }
    // Whether the line shares any word of what the thing is (the head of its name).
    let shares_head = n.head.iter().any(|(_, stems)| {
        p.words
            .iter()
            .any(|(_, ls)| ls.iter().any(|s| stems.contains(s)))
    });
    let brand = if shares_head { BRAND } else { BRAND_ASIDE };
    // 4. The brand, compared without spaces or marks (`Pro's Kit` is `Pro'sKit`), in the
    //    thing's name or make only (a note says "for Arduino" of every module), never the
    //    shop's own name (a shop's own service names it as the brand).
    if let Some(b) = p.brand.as_ref().filter(|b| has_phrase(&n.own, b)) {
        add(format!("brand {b}"), brand);
        strong |= shares_head;
    } else if let Some(m) = n.make.as_ref().filter(|m| has_phrase(&p.name, m)) {
        add(format!("brand {m}"), brand);
        strong |= shares_head;
    }
    // 5. Words, weighted by how rare they are among the lines. On their own they must name
    //    what the thing is: words shared only with what it is for, made of or kept with ("USB-C
    //    girişli", "banyodaki dolabı için") stay below the bar.
    let shares = |stems: &[String]| {
        p.words
            .iter()
            .any(|(_, ls)| ls.iter().any(|s| stems.contains(s)))
    };
    let mut points = 0.0;
    let mut matched: Vec<&str> = Vec::new();
    for (w, stems) in &n.words {
        if shares(stems) && !matched.contains(&w.as_str()) {
            points += corpus.weight(stems);
            matched.push(w);
        }
    }
    // The line names what the thing is when it carries more than half of the head of its name,
    // each word weighted by how rare it is: "RFID okuyucu kartı" is not named by a card reader
    // hub that shares only "okuyucu kartı".
    let (named, all) = n.head.iter().fold((0.0, 0.0), |(named, all), (_, stems)| {
        let w = corpus.weight(stems);
        (named + if shares(stems) { w } else { 0.0 }, all + w)
    });
    let names_it = all > 0.0 && named * 2.0 > all;
    if !matched.is_empty() {
        let (cap, what) = if strong || names_it {
            (WORDS_CAP, "words")
        } else {
            (ASIDE_CAP, "words aside")
        };
        add(format!("{what} {}", matched.join(", ")), points.min(cap));
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
        // A thing kept in several places is linked when any of its records is.
        let mut things: Vec<(&Node, Vec<i64>)> = Vec::new();
        for n in nodes.iter().filter(|n| toured(n.id)) {
            let members = crate::portions::members(conn, n)?;
            if !members.iter().any(|m| linked.contains(m)) {
                things.push((n, members));
            }
        }
        let matcher = Matcher::new(conn, &nodes)?;
        let mut out = Vec::new();
        for (n, members) in &things {
            if let Some(c) = matcher.rank(n, members, OFFER_AT, 1).into_iter().next() {
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
    /// `(line, thing)` pairs the person said are not the same purchase.
    declined: HashSet<(i64, i64)>,
}

impl Matcher {
    fn new(conn: &Connection, nodes: &[Node]) -> Result<Matcher> {
        // A line's own fields and links are all scoring reads: not its attachments or documents.
        let all = ids(conn, "SELECT id FROM purchases ORDER BY id", [])?
            .into_iter()
            .map(|p| purchase_row(conn, p))
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
        let mut stmt = conn.prepare("SELECT purchase_id, node_id FROM purchase_declines")?;
        let declined = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<HashSet<_>>>()?;
        Ok(Matcher {
            lines,
            corpus,
            declined,
        })
    }

    /// The lines scoring above `at` for `n`, at most `limit`, best first, and any linked to it.
    /// `members` are the records of `n`'s thing (spec/portions.md §6): a line linked to any of
    /// them is linked to it, and a "not this one" said of any of them holds for it.
    fn rank(&self, n: &Node, members: &[i64], at: f64, limit: usize) -> Vec<Value> {
        let thing = Thing::new(n);
        let mut out = Vec::new();
        for line in &self.lines {
            let p = &line.value;
            let linked_here = p["linked"].as_array().is_some_and(|l| {
                l.iter().any(|x| {
                    x["node"]["id"]
                        .as_i64()
                        .is_some_and(|i| members.contains(&i))
                })
            });
            let declined = p["id"]
                .as_i64()
                .is_some_and(|id| members.iter().any(|m| self.declined.contains(&(id, *m))));
            if !(open(p) || linked_here) || declined && !linked_here {
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
    let members = crate::portions::members(conn, &n)?;
    Ok(Matcher::new(conn, &nodes)?.rank(&n, &members, at, limit))
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
        assert_eq!(
            conflicts(
                "Raspberry Pi resmi micro-USB güç adaptörü (5,1 V 2,5 A, sabit kablo)",
                "Raspberry Pi 4 Model B, USB-C, 5.1V, 3A için resmi güç kaynağı"
            ),
            ["current"]
        );
        assert!(conflicts("şarj 2 A", "2000 mA şarj").is_empty());
        assert_eq!(
            measures("Pi 3 A+ için, çıkış 2,5 A ve 3A"),
            vec![("current", 2500.0), ("current", 3000.0)]
        );
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
    fn a_model_is_found_as_whole_words_never_inside_another_code() {
        let words = super::tokens("Harici disk 2 TB, ürün kodu 6002561");
        assert!(!super::has_words(&words, "561", 6));
        let words = super::tokens("Bosch GSB 13 RE darbeli matkap, 4250-6/128 set");
        assert!(super::has_words(&words, "gsb13re", 6));
        assert!(super::has_words(&words, "4250", 6));
        // A long model may start a longer code: a suffix names a colour or a region.
        let words = super::tokens("WD40EFRX68N32N0 4 TB disk");
        assert!(super::has_words(&words, "wd40efrx", 6));
    }

    #[test]
    fn codes_mix_letters_and_digits_and_drop_hyphens() {
        let c = codes("Pro'sKit 1PK-052DS pense, LR1130 pil, 64 GB, 64gb, M10, 3x3 küp");
        assert!(c.contains("1pk052ds") && c.contains("lr1130"));
        assert_eq!(c.len(), 2, "{c:?}");
    }
}
