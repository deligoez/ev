//! Where a thing should go (spec §23): every holder scored against a description with a
//! BM25F-style formula, and regrouping hints built on the same score.
//!
//! A holder is described by its own theme, name and note and by the names, notes and tags of
//! the things directly inside it, each field with its own weight. Rare words count for more
//! than common ones (a part number beats "sensor"), Turkish endings are cut off, and codes such
//! as `KY-018` are kept whole. Everything here is deterministic: the same data and the same
//! query give the same ranking, ties broken by id.

use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::Connection;
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::fold;
use crate::model::{Kind, Node};
use crate::store::{
    Inventory, brief_json, holder_json, is_holder, live_nodes, parse_size, resolve, rules_json,
};

/// How much a match in each field counts.
const THEME: f64 = 3.0;
const NAME: f64 = 2.5;
const ITEM_NAME: f64 = 2.0;
const TAG: f64 = 1.5;
const NOTE: f64 = 1.0;
const ITEM_NOTE: f64 = 1.0;
/// BM25 saturation and length normalization.
const K1: f64 = 1.2;
const B: f64 = 0.5;

/// Fill thresholds, in percent: below `ROOM` there is room, from `FULL` on there is none.
const ROOM: i64 = 70;
const FULL: i64 = 90;
const SPARSE: i64 = 25;
/// A thing is flagged as better off elsewhere when another holder scores at least this many
/// times its own, and at least `CLEAR` in absolute terms.
const ELSEWHERE: f64 = 1.5;
const CLEAR: f64 = 3.0;

/// A match on a word this rare (df of about 1 in 20 holders or fewer, or at most
/// `SPECIFIC_DF` holders in a small inventory) says what the thing is; a match on commoner
/// words ("module", "sensor") only says what family it is in.
const SPECIFIC: f64 = 3.0;
const SPECIFIC_DF: usize = 2;
/// How much a word of an existing node counts in a query, by where it was written: a long
/// note carries more incidental words than a name.
const Q_NAME: f64 = 1.0;
const Q_TAG: f64 = 0.8;
const Q_NOTE: f64 = 0.4;
const Q_SYNONYM: f64 = 0.8;
/// Below this share of the query matched by the best holder, the thing has no group yet.
const GROUP_COVERAGE: f64 = 0.5;

/// Words that never say what a thing is. Shorter than the audit list on purpose: colours and
/// sizes do tell LED boxes apart, and IDF already quiets words that are everywhere.
const FILLER: &[&str] = &[
    "icin",
    "ile",
    "veya",
    "gibi",
    "olan",
    "adet",
    "tane",
    "the",
    "and",
    "for",
    "with",
    "bir",
    "cok",
    "olabilir",
    "muhtemelen",
    "belirsiz",
    "net",
    "degil",
    "benzeri",
    "turu",
    "tipi",
    "icerik",
    "fotografta",
    "fotograftan",
    "sayida",
    "kullanici",
    "dogrulandi",
    "sonra",
    "once",
    "gore",
    "kadar",
    "hepsi",
    "bunlar",
    "this",
    "that",
    "from",
    "into",
];

/// One searchable word: the key it is matched by, the form it was written in, and how much it
/// counts in a query.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Term {
    pub key: String,
    pub surface: String,
    pub weight: f64,
}

fn weighted(ts: Vec<Term>, w: f64) -> Vec<Term> {
    ts.into_iter()
        .map(|mut t| {
            t.weight = w;
            t
        })
        .collect()
}

/// The words of a text worth matching, keyed by their written form for now (the index turns
/// words into stems with `Lexicon`). Letters-and-digits runs joined by hyphens that mix
/// letters and digits (`KY-018`, `HC-SR04`) are kept whole as codes as well as split; plain
/// numbers are dropped (dates, counts).
pub(crate) fn terms(text: &str) -> Vec<Term> {
    let folded = fold(text);
    let mut out = Vec::new();
    let mut push = |w: &str| {
        if w.is_empty() || w.chars().all(|c| c.is_ascii_digit()) {
            return;
        }
        let has_digit = w.chars().any(|c| c.is_ascii_digit());
        let keep = if has_digit {
            w.chars().count() >= 2
        } else {
            w.chars().count() >= 3 && !FILLER.contains(&w)
        };
        if keep {
            out.push(Term {
                key: w.to_string(),
                surface: w.to_string(),
                weight: 1.0,
            });
        }
    };
    for chunk in folded.split(|c: char| !(c.is_alphanumeric() || c == '-')) {
        let chunk = chunk.trim_matches('-');
        if chunk.is_empty() {
            continue;
        }
        let digits = chunk.chars().any(|c| c.is_ascii_digit());
        let letters = chunk.chars().any(|c| c.is_alphabetic());
        if chunk.contains('-') && digits && letters {
            push(&chunk.replace('-', ""));
        }
        for part in chunk.split('-') {
            push(part);
        }
    }
    out
}

/// Turkish noun endings in the order they stack, outermost first, as they read after folding
/// (ı→i, ü→u, ö→o, ğ→g): relative `-ki` after a locative, case, possessive, plural, and
/// "with" (`-lı`). `-sız` ("without") is deliberately not an ending here: `modülsüz` must not
/// match `modül`. Derivations such as `-lık` stay too: `sıcaklık` is not `sıcak`.
const ENDINGS: &[&[&str]] = &[
    &["daki", "deki", "taki", "teki"],
    &[
        "ndan", "nden", "dan", "den", "tan", "ten", "nin", "nun", "yla", "yle", "nda", "nde", "da",
        "de", "ta", "te", "in", "un", "ya", "ye", "yi", "yu", "na", "ne", "ni", "nu", "a", "e",
        "i", "u",
    ],
    &["lari", "leri", "si", "su", "i", "u"],
    &["lar", "ler"],
    &["li", "lu"],
];

/// Every stem a folded word might have: the word, and what is left after taking off any run of
/// the endings above, each time keeping at least three letters. A stem left by an ending that
/// starts with a vowel also appears with its last consonant hardened back (`kitabı`→`kitap`,
/// `ışığı`→`ışık`, `rengi`→`renk`). Plain-ASCII words also lose English plurals.
fn candidates(word: &str) -> Vec<String> {
    let mut out = vec![word.to_string()];
    let mut frontier = vec![word.to_string()];
    for layer in ENDINGS {
        let mut next = frontier.clone();
        for w in &frontier {
            for e in *layer {
                let Some(stem) = w.strip_suffix(e) else {
                    continue;
                };
                if stem.chars().count() < 3 {
                    continue;
                }
                next.push(stem.to_string());
                if e.starts_with(['a', 'e', 'i', 'u', 'o']) {
                    let hard = match stem.chars().last() {
                        Some('b') => Some('p'),
                        Some('d') => Some('t'),
                        Some('g') => Some('k'),
                        _ => None,
                    };
                    if let Some(h) = hard {
                        let mut s = stem.to_string();
                        s.pop();
                        s.push(h);
                        next.push(s);
                    }
                }
            }
        }
        next.dedup();
        for n in &next {
            if !out.contains(n) {
                out.push(n.clone());
            }
        }
        frontier = next;
    }
    if word.is_ascii() {
        for (end, repl) in [("ies", "y"), ("es", ""), ("s", "")] {
            if let Some(stem) = word.strip_suffix(end) {
                let s = format!("{stem}{repl}");
                if s.chars().count() >= 3 && !out.contains(&s) {
                    out.push(s);
                }
            }
        }
    }
    out
}

const COLORS: &[&str] = &[
    "kirmizi",
    "yesil",
    "mavi",
    "sari",
    "siyah",
    "beyaz",
    "gri",
    "turuncu",
    "mor",
    "pembe",
    "kahverengi",
    "lacivert",
    "red",
    "green",
    "blue",
    "yellow",
    "black",
    "white",
];
const COLOR_WEIGHT: f64 = 0.3;

/// Whether `a b` reads as an indefinite noun compound: `a` bare (neither an ending cut off nor
/// a code nor a colour), `b` carrying only the 3rd-person possessive (-sI / -I), on a soft stem
/// too (`kablo bağı`, `ahşap vidası`, `kontrol kalemi`). A guess from endings alone: it cannot
/// tell a noun from an adjective, which a dictionary could.
fn is_compound(a: &Term, b: &Term) -> bool {
    let bare = a.surface == a.key
        && !a.surface.chars().any(|c| c.is_ascii_digit())
        && !COLORS.contains(&a.key.as_str());
    // The stem with its last letter hardened back, the way `candidates` does (kalem-i, bağ-ı
    // → bak, kitab-ı → kitap), so `b`'s key can be compared with it.
    let hard = |stem: &str| {
        let mut s = stem.to_string();
        match s.pop() {
            Some('b') => s.push('p'),
            Some('d') => s.push('t'),
            Some('g') => s.push('k'),
            Some(c) => s.push(c),
            None => {}
        }
        s
    };
    bare && ["si", "su", "i", "u"].iter().any(|e| {
        b.surface
            .strip_suffix(e)
            .is_some_and(|stem| stem == b.key || hard(stem) == b.key)
    })
}

/// Chooses a word's stem by looking at the inventory's own words, which is what keeps plain
/// suffix stripping from cutting too deep. In order: the shortest shorter form that is written
/// somewhere on its own (`kutuda`→`kutu`, `kitabı`→`kitap`, `modules`→`modül`); the word
/// itself if it is written on its own, so a base form is never cut further (`kutu` stays,
/// though it could read as `kut`+`u`); the longest shorter form two different words lead to
/// (`vidası` with `vidaları`→`vida`); the word itself. The known gap: two forms whose base is
/// written nowhere, both themselves written on their own, stay apart.
pub(crate) struct Lexicon {
    words: HashSet<String>,
    shared: HashMap<String, usize>,
}

impl Lexicon {
    pub(crate) fn new<'a>(surfaces: impl Iterator<Item = &'a str>) -> Lexicon {
        let words: HashSet<String> = surfaces
            .filter(|w| !w.chars().any(|c| c.is_ascii_digit()))
            .map(str::to_string)
            .collect();
        let mut shared: HashMap<String, usize> = HashMap::new();
        for w in &words {
            for c in candidates(w).into_iter().skip(1) {
                *shared.entry(c).or_default() += 1;
            }
        }
        Lexicon { words, shared }
    }

    pub(crate) fn key(&self, word: &str) -> String {
        if word.chars().any(|c| c.is_ascii_digit()) {
            return word.to_string();
        }
        let cands = candidates(word);
        if let Some(c) = cands
            .iter()
            .skip(1)
            .filter(|c| self.words.contains(*c))
            .min_by(|a, b| a.chars().count().cmp(&b.chars().count()).then(a.cmp(b)))
        {
            return c.clone();
        }
        if self.words.contains(word) {
            return word.to_string();
        }
        cands
            .iter()
            .skip(1)
            .filter(|c| self.shared.get(*c).is_some_and(|n| *n >= 2))
            .max_by(|a, b| a.chars().count().cmp(&b.chars().count()).then(b.cmp(a)))
            .cloned()
            .unwrap_or_else(|| word.to_string())
    }

    pub(crate) fn keyed(&self, ts: Vec<Term>) -> Vec<Term> {
        ts.into_iter()
            .map(|mut t| {
                t.key = self.key(&t.surface);
                t
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
struct Hit {
    weight: f64,
    /// The node the words came from: the holder itself or a thing inside it.
    source: i64,
    field: &'static str,
}

#[derive(Default)]
struct Doc {
    hits: HashMap<String, Vec<Hit>>,
    len: usize,
}

/// Every holder as a weighted bag of words, with document frequencies for IDF.
pub(crate) struct Index {
    lex: Lexicon,
    docs: BTreeMap<i64, Doc>,
    df: HashMap<String, usize>,
    avglen: f64,
    names: HashMap<i64, String>,
}

/// One holder's score for a query, with what matched and where.
#[derive(Debug, Clone)]
pub(crate) struct Scored {
    pub id: i64,
    pub score: f64,
    /// The share of the query's weight (each word's weight × its IDF) this holder matched.
    pub coverage: f64,
    pub matched: Vec<Value>,
    pub items: Vec<String>,
}

impl Index {
    pub(crate) fn build(all: &[Node]) -> Index {
        let has_children: HashSet<i64> = all.iter().filter_map(|n| n.parent_id).collect();
        let mut docs: BTreeMap<i64, Doc> = BTreeMap::new();
        let mut names = HashMap::new();
        for n in all {
            names.insert(n.id, n.name.clone());
        }
        // The inventory's own vocabulary decides how far words are stemmed.
        let mut surfaces: Vec<String> = Vec::new();
        for n in all {
            let texts = [Some(&n.name), n.theme.as_ref(), n.note.as_ref()];
            for t in texts.into_iter().flatten().chain(n.tags.iter()) {
                surfaces.extend(terms(t).into_iter().map(|t| t.surface));
            }
        }
        let lex = Lexicon::new(surfaces.iter().map(String::as_str));
        let add = |doc: &mut Doc, text: &str, weight: f64, source: i64, field: &'static str| {
            for t in lex.keyed(terms(text)) {
                doc.len += 1;
                doc.hits.entry(t.key).or_default().push(Hit {
                    weight,
                    source,
                    field,
                });
            }
        };
        for h in all.iter().filter(|n| is_holder(n, &has_children)) {
            let mut doc = Doc::default();
            if let Some(t) = &h.theme {
                add(&mut doc, t, THEME, h.id, "theme");
            }
            add(&mut doc, &h.name, NAME, h.id, "name");
            if let Some(t) = &h.note {
                add(&mut doc, t, NOTE, h.id, "note");
            }
            for c in all
                .iter()
                .filter(|c| c.parent_id == Some(h.id) && c.kind == Kind::Item)
            {
                add(&mut doc, &c.name, ITEM_NAME, c.id, "item");
                if let Some(t) = &c.note {
                    add(&mut doc, t, ITEM_NOTE, c.id, "item note");
                }
                for tag in &c.tags {
                    add(&mut doc, tag, TAG, c.id, "item tag");
                }
            }
            docs.insert(h.id, doc);
        }
        let mut df: HashMap<String, usize> = HashMap::new();
        for d in docs.values() {
            for k in d.hits.keys() {
                *df.entry(k.clone()).or_default() += 1;
            }
        }
        let avglen = if docs.is_empty() {
            1.0
        } else {
            (docs.values().map(|d| d.len).sum::<usize>() as f64 / docs.len() as f64).max(1.0)
        };
        Index {
            lex,
            docs,
            df,
            avglen,
            names,
        }
    }

    /// Words stemmed the way this index stems them.
    pub(crate) fn keyed(&self, ts: Vec<Term>) -> Vec<Term> {
        self.lex.keyed(ts)
    }

    fn idf(&self, key: &str) -> f64 {
        let n = self.docs.len() as f64;
        let df = *self.df.get(key).unwrap_or(&0) as f64;
        (1.0 + (n - df + 0.5) / (df + 0.5)).ln()
    }

    /// Whether a word is rare enough here to say what a thing is.
    fn specific(&self, key: &str) -> bool {
        self.idf(key) >= SPECIFIC || self.df.get(key).is_some_and(|d| *d <= SPECIFIC_DF)
    }

    /// Scores every holder not in `skip`, ignoring words that came from nodes in `exclude`
    /// (the thing being placed, so it does not vote for where it already is). Best first.
    pub(crate) fn score(
        &self,
        query: &[Term],
        exclude: &HashSet<i64>,
        skip: &HashSet<i64>,
    ) -> Vec<Scored> {
        // One entry per word, at the highest weight it was given.
        let mut keys: Vec<&Term> = Vec::new();
        for t in query {
            match keys.iter().position(|k| k.key == t.key) {
                Some(i) if keys[i].weight < t.weight => keys[i] = t,
                Some(_) => {}
                None => keys.push(t),
            }
        }
        let total: f64 = keys.iter().map(|t| t.weight * self.idf(&t.key)).sum();
        let mut out = Vec::new();
        for (id, doc) in &self.docs {
            if skip.contains(id) {
                continue;
            }
            let mut covered = 0.0;
            let norm = K1 * (1.0 - B + B * doc.len as f64 / self.avglen);
            let mut score = 0.0;
            let mut matched = Vec::new();
            let mut items: Vec<String> = Vec::new();
            for t in &keys {
                let Some(hits) = doc.hits.get(&t.key) else {
                    continue;
                };
                let live: Vec<&Hit> = hits
                    .iter()
                    .filter(|h| !exclude.contains(&h.source))
                    .collect();
                let Some(best) = live
                    .iter()
                    .max_by(|a, b| a.weight.total_cmp(&b.weight).then(b.source.cmp(&a.source)))
                else {
                    continue;
                };
                let tf = live.len() as f64;
                let idf = self.idf(&t.key);
                let part = t.weight * idf * best.weight * tf * (K1 + 1.0) / (tf + norm);
                score += part;
                covered += t.weight * idf;
                let from = if best.source == *id {
                    best.field.to_string()
                } else {
                    format!(
                        "{}: {}",
                        best.field,
                        self.names.get(&best.source).cloned().unwrap_or_default()
                    )
                };
                matched.push(json!({
                    "term": t.surface,
                    "points": round(part),
                    "from": from,
                    "specific": self.specific(&t.key),
                }));
                for h in &live {
                    if h.source != *id {
                        let name = self.names.get(&h.source).cloned().unwrap_or_default();
                        if !items.contains(&name) {
                            items.push(name);
                        }
                    }
                }
            }
            if score > 0.0 {
                matched.sort_by(|a, b| {
                    b["points"]
                        .as_f64()
                        .unwrap_or(0.0)
                        .total_cmp(&a["points"].as_f64().unwrap_or(0.0))
                });
                out.push(Scored {
                    id: *id,
                    score: round(score),
                    coverage: if total > 0.0 {
                        round(covered / total)
                    } else {
                        0.0
                    },
                    matched,
                    items,
                });
            }
        }
        out.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.id.cmp(&b.id)));
        out
    }
}

fn round(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// How full a holder is, and whether that is still known: a fill estimate is stale once the
/// contents changed after it was given.
pub(crate) fn room(conn: &Connection, n: &Node) -> Result<Value> {
    let Some(fill) = n.fill else {
        return Ok(json!({ "room": "unknown" }));
    };
    let set_at: Option<String> = conn.query_row(
        "SELECT COALESCE(
             (SELECT MAX(at) FROM events WHERE node_id = ?1 AND type = 'edit'
                AND json_extract(data, '$.fill') IS NOT NULL),
             (SELECT created_at FROM nodes WHERE id = ?1))",
        [n.id],
        |r| r.get(0),
    )?;
    let changed = crate::marks::contents_changed_at(conn, n.id)?;
    let stale = matches!((&set_at, &changed), (Some(s), Some(c)) if c > s);
    let room = if fill >= FULL {
        "none"
    } else if fill >= ROOM {
        "little"
    } else {
        "yes"
    };
    Ok(json!({ "room": room, "fill": fill, "fill_at": set_at, "stale": stale }))
}

fn volume(size: &str) -> Option<f64> {
    parse_size(size).ok().map(|p| p.iter().product())
}

/// Every node below `id`, and `id` itself.
fn subtree(all: &[Node], id: i64) -> HashSet<i64> {
    let mut out = HashSet::from([id]);
    let mut grew = true;
    while grew {
        grew = false;
        for n in all {
            if let Some(p) = n.parent_id
                && out.contains(&p)
                && out.insert(n.id)
            {
                grew = true;
            }
        }
    }
    out
}

/// The query words for an existing node: its name, then its tags and note at lower weights.
fn node_terms(n: &Node) -> Vec<Term> {
    let mut q = weighted(terms(&n.name), Q_NAME);
    for tag in &n.tags {
        q.extend(weighted(terms(tag), Q_TAG));
    }
    if let Some(t) = &n.note {
        q.extend(weighted(terms(t), Q_NOTE));
    }
    q
}

/// Whether a scored holder matched on at least one word rare enough to say what the thing is.
fn is_specific(s: &Scored) -> bool {
    s.matched.iter().any(|m| m["specific"] == true)
}

/// The words of a holder's theme, as a set: holders with the same set are one group split
/// over several boxes ("MQ sensors (1)", "MQ sensors (2)").
fn theme_key(index: &Index, n: &Node) -> Vec<String> {
    let mut k: Vec<String> = n
        .theme
        .iter()
        .flat_map(|t| index.keyed(terms(t)))
        .map(|t| t.key)
        .collect();
    k.sort();
    k.dedup();
    k
}

/// Groups of words that mean the same thing here (`ldr, ışık sensörü, fotodirenç`), each
/// phrase split into its search words.
fn synonym_groups(conn: &Connection) -> Result<Vec<(i64, Vec<Term>)>> {
    let mut stmt = conn.prepare("SELECT id, words FROM synonyms ORDER BY id")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().map(|(id, w)| (id, terms(&w))).collect())
}

/// The query with every synonym group it touches added in, and the words that were added.
fn expand(query: Vec<Term>, groups: &[(i64, Vec<Term>)]) -> (Vec<Term>, Vec<String>) {
    let mut out = query;
    let mut added = Vec::new();
    for (_, group) in groups {
        if group.iter().any(|g| out.iter().any(|q| q.key == g.key)) {
            for g in group {
                if !out.iter().any(|q| q.key == g.key) {
                    added.push(g.surface.clone());
                    out.push(Term {
                        weight: Q_SYNONYM,
                        ..g.clone()
                    });
                }
            }
        }
    }
    (out, added)
}

const CONSIDERED: &str = "each holder is scored on its own theme (×3), name (×2.5) and note (×1), \
and on the names (×2), tags (×1.5) and notes (×1) of the things directly inside it; rare words \
weigh more than common ones (IDF), repeats count less and less (BM25, k1=1.2, b=0.5), Turkish \
endings are cut, codes like KY-018 are kept whole; ties go to the lower id";

impl Inventory {
    /// Where could this go: holders ranked by how well they match the description (or an
    /// existing node's own words, with `for_ref`), the rules, and every holder in the tree.
    pub fn suggest_with(
        &self,
        text: &str,
        tag: Option<&str>,
        for_ref: Option<&str>,
    ) -> Result<Value> {
        let all = live_nodes(&self.conn)?;
        let mut query = terms(text);
        if let Some(t) = tag {
            query.extend(terms(t));
        }
        let mut exclude = HashSet::new();
        let mut skip = HashSet::new();
        let mut for_node = Value::Null;
        if let Some(r) = for_ref {
            let id = resolve(&self.conn, r, false)?;
            let n = all
                .iter()
                .find(|n| n.id == id)
                .ok_or_else(|| Error::NotFound(format!("no live node {r}")))?;
            query.extend(node_terms(n));
            exclude.insert(id);
            // A box cannot go into itself or anything inside it.
            skip = subtree(&all, id);
            for_node = brief_json(&self.conn, id)?;
        }
        if query.is_empty() {
            return Err(Error::Usage(
                "describe the thing to place (a word of 3+ letters or a part code), or give --for"
                    .into(),
            ));
        }
        let index = Index::build(&all);
        let groups: Vec<(i64, Vec<Term>)> = synonym_groups(&self.conn)?
            .into_iter()
            .map(|(id, ts)| (id, index.keyed(ts)))
            .collect();
        let (query, synonyms) = expand(index.keyed(query), &groups);
        let has_children: HashSet<i64> = all.iter().filter_map(|n| n.parent_id).collect();
        let by_id: HashMap<i64, &Node> = all.iter().map(|n| (n.id, n)).collect();
        let ranked = index.score(&query, &exclude, &skip);
        let similar = ranked
            .iter()
            .take(12)
            .map(|s| {
                let n = by_id[&s.id];
                let mut c = holder_json(&self.conn, n, &all)?;
                c["room"] = room(&self.conn, n)?;
                Ok(json!({
                    "container": c,
                    "score": round(s.score),
                    "coverage": round(s.coverage),
                    "specific": is_specific(s),
                    "matched": s.matched,
                    "count": s.items.len(),
                    "matches": s.items,
                }))
            })
            .collect::<Result<Vec<_>>>()?;
        let holders = all
            .iter()
            .filter(|n| is_holder(n, &has_children) && !skip.contains(&n.id))
            .map(|n| {
                let mut c = holder_json(&self.conn, n, &all)?;
                c["room"] = room(&self.conn, n)?;
                Ok(c)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut words: Vec<&str> = Vec::new();
        for t in &query {
            if !words.contains(&t.surface.as_str()) {
                words.push(&t.surface);
            }
        }
        Ok(json!({
            "query": text,
            "for": for_node,
            "words": words,
            "synonyms_added": synonyms,
            // No holder matched on a word that says what the thing is: it has no group yet.
            "new_group_likely": ranked.first().is_none_or(|s| s.coverage < GROUP_COVERAGE),
            "considered": CONSIDERED,
            "rules": rules_json(&self.conn)?,
            "similar": similar,
            "containers": holders,
            "complete": { "containers": holders.len(), "note": "every place in the tree that can hold something is listed" },
        }))
    }

    /// Kept for callers without `--for`.
    pub fn suggest(&self, text: &str, tag: Option<&str>) -> Result<Value> {
        self.suggest_with(text, tag, None)
    }

    /// Regrouping hints for the holders under `reference` (or everywhere), all from the same
    /// score as `suggest`: things that would fit better in another holder here, strays whose
    /// kind has a themed home, full holders with a bigger spare box that would fit, sparse
    /// holders that could merge, mixed holders, and holders whose fill is unknown or stale.
    pub fn regroup(&self, reference: Option<&str>) -> Result<Value> {
        let all = live_nodes(&self.conn)?;
        let has_children: HashSet<i64> = all.iter().filter_map(|n| n.parent_id).collect();
        let by_id: HashMap<i64, &Node> = all.iter().map(|n| (n.id, n)).collect();
        let (root, scope) = match reference {
            Some(r) => {
                let id = resolve(&self.conn, r, false)?;
                (Some(id), subtree(&all, id))
            }
            None => (None, all.iter().map(|n| n.id).collect()),
        };
        let holders: Vec<&Node> = all
            .iter()
            .filter(|n| scope.contains(&n.id) && is_holder(n, &has_children))
            .collect();
        let holder_ids: HashSet<i64> = holders.iter().map(|h| h.id).collect();
        // Candidates are the holders in scope; the rest of the house is not a regroup.
        let outside: HashSet<i64> = all
            .iter()
            .filter(|n| !holder_ids.contains(&n.id))
            .map(|n| n.id)
            .collect();
        let index = Index::build(&all);
        let groups: Vec<(i64, Vec<Term>)> = synonym_groups(&self.conn)?
            .into_iter()
            .map(|(id, ts)| (id, index.keyed(ts)))
            .collect();
        let node_terms = |n: &Node| expand(index.keyed(node_terms(n)), &groups).0;
        let theme_key = |n: &Node| theme_key(&index, n);
        let ancestors = |id: i64| {
            let mut out = HashSet::new();
            let mut cur = by_id.get(&id).and_then(|n| n.parent_id);
            while let Some(p) = cur {
                out.insert(p);
                cur = by_id.get(&p).and_then(|n| n.parent_id);
            }
            out
        };

        // Things that would fit better elsewhere, and how often a thing's best place is
        // where it already is.
        let mut elsewhere = Vec::new();
        let mut checked = 0;
        let mut at_home = 0;
        let mut elsewhere_by_holder: BTreeMap<i64, Vec<String>> = BTreeMap::new();
        let mut best_of: HashMap<i64, i64> = HashMap::new();
        let mut flagged: HashSet<i64> = HashSet::new();
        for item in all.iter().filter(|n| n.kind == Kind::Item) {
            let Some(parent) = item.parent_id else {
                continue;
            };
            if !holder_ids.contains(&parent) {
                continue;
            }
            let q = node_terms(item);
            if q.is_empty() {
                continue;
            }
            let mut skip = outside.clone();
            // Moving a thing out of its box into the drawer around it is not a regroup.
            skip.extend(ancestors(parent));
            let ranked = index.score(&q, &HashSet::from([item.id]), &skip);
            let Some(best) = ranked.first() else {
                continue;
            };
            checked += 1;
            best_of.insert(item.id, best.id);
            // A holder with the same theme as this one is the same group split over boxes.
            let same_group = |id: i64| {
                id == parent
                    || (!theme_key(by_id[&parent]).is_empty()
                        && theme_key(by_id[&id]) == theme_key(by_id[&parent]))
            };
            if same_group(best.id) {
                at_home += 1;
                continue;
            }
            let here = ranked
                .iter()
                .find(|s| s.id == parent)
                .map_or(0.0, |s| s.score);
            elsewhere_by_holder
                .entry(parent)
                .or_default()
                .push(item.name.clone());
            // Only a match on a word that says what the thing is makes another holder better.
            if best.score >= CLEAR && best.score >= ELSEWHERE * here && is_specific(best) {
                flagged.insert(item.id);
                elsewhere.push(json!({
                    "item": brief_json(&self.conn, item.id)?,
                    "now": { "holder": brief_json(&self.conn, parent)?, "score": round(here) },
                    "better": { "holder": brief_json(&self.conn, best.id)?, "score": round(best.score), "matched": best.matched },
                }));
            }
        }

        // Strays: things named for another holder's theme, whose best score is that holder too,
        // but by too small a margin to be flagged above: worth a look, not a move.
        let mut strays = Vec::new();
        let mut seen_items = HashSet::new();
        for h in &holders {
            let themed: Vec<Term> = h
                .theme
                .iter()
                .flat_map(|t| index.keyed(terms(t)))
                .filter(|t| index.specific(&t.key))
                .collect();
            for t in themed {
                for item in all.iter().filter(|n| {
                    n.kind == Kind::Item
                        && n.parent_id != Some(h.id)
                        && n.parent_id.is_some_and(|p| holder_ids.contains(&p))
                        // A holder the thing is already inside is not somewhere else.
                        && !ancestors(n.id).contains(&h.id)
                        && best_of.get(&n.id) == Some(&h.id)
                        && !flagged.contains(&n.id)
                }) {
                    let parent = item.parent_id.unwrap_or_default();
                    let parent_theme = by_id[&parent].theme.clone().unwrap_or_default();
                    let in_theme = index
                        .keyed(terms(&parent_theme))
                        .iter()
                        .any(|x| x.key == t.key);
                    let named = index
                        .keyed(terms(&item.name))
                        .iter()
                        .any(|x| x.key == t.key);
                    if named && !in_theme && seen_items.insert((item.id, h.id)) {
                        strays.push(json!({
                            "term": t.surface,
                            "item": brief_json(&self.conn, item.id)?,
                            "now": brief_json(&self.conn, parent)?,
                            "home": brief_json(&self.conn, h.id)?,
                        }));
                    }
                }
            }
        }

        // Spare boxes: things tagged "boş kap" (or "spare box") with a size.
        let spares: Vec<&Node> = all
            .iter()
            .filter(|n| {
                n.size.is_some()
                    && n.tags
                        .iter()
                        .any(|t| matches!(fold(t).as_str(), "bos kap" | "spare box"))
            })
            .collect();

        let mut full = Vec::new();
        let mut sparse = Vec::new();
        let mut unknown_fill = Vec::new();
        for h in &holders {
            if !matches!(h.kind, Kind::Container | Kind::Item) {
                continue;
            }
            let r = room(&self.conn, h)?;
            let has_items = all.iter().any(|c| c.parent_id == Some(h.id));
            if !has_items {
                continue;
            }
            if r["room"] == "unknown" || r["stale"] == true {
                unknown_fill.push(json!({
                    "holder": brief_json(&self.conn, h.id)?,
                    "fill": r["fill"],
                    "stale": r["stale"] == true,
                }));
                continue;
            }
            let fill = r["fill"].as_i64().unwrap_or(0);
            if fill >= FULL {
                let own = h.size.as_deref().and_then(volume);
                let bigger: Vec<Value> = spares
                    .iter()
                    .filter(|s| {
                        let v = s.size.as_deref().and_then(volume);
                        matches!((own, v), (Some(o), Some(v)) if v > o) || own.is_none()
                    })
                    .map(|s| {
                        let mut v = brief_json(&self.conn, s.id)?;
                        v["size"] = json!(s.size);
                        v["qty"] = json!(s.qty);
                        v["fits_at"] = json!(fits_at(&self.conn, h, s)?);
                        Ok(v)
                    })
                    .collect::<Result<_>>()?;
                full.push(json!({
                    "holder": brief_json(&self.conn, h.id)?,
                    "fill": fill,
                    "size": h.size,
                    "bigger_spares": bigger,
                }));
            } else if fill <= SPARSE {
                // The sibling that best matches this holder's own words, with room.
                let mut q = node_terms(h);
                for c in all.iter().filter(|c| c.parent_id == Some(h.id)) {
                    q.extend(terms(&c.name));
                }
                let mut skip: HashSet<i64> = all
                    .iter()
                    .filter(|n| n.parent_id != h.parent_id || n.id == h.id)
                    .map(|n| n.id)
                    .collect();
                skip.extend(outside.iter().copied());
                let own_items: HashSet<i64> = all
                    .iter()
                    .filter(|c| c.parent_id == Some(h.id))
                    .map(|c| c.id)
                    .chain([h.id])
                    .collect();
                let ranked = index.score(&q, &own_items, &skip);
                let mut into = Value::Null;
                for s in ranked.iter().filter(|s| s.score >= CLEAR) {
                    let r = room(&self.conn, by_id[&s.id])?;
                    if r["room"] == "yes" {
                        into = json!({ "holder": brief_json(&self.conn, s.id)?, "score": round(s.score), "room": r });
                        break;
                    }
                }
                sparse.push(json!({
                    "holder": brief_json(&self.conn, h.id)?,
                    "fill": fill,
                    "merge_into": into,
                }));
            }
        }

        // Mixed: at least three things, and for half or more of them another holder here
        // scores higher than this one.
        let mut mixed = Vec::new();
        for (h, names) in &elsewhere_by_holder {
            let count = all
                .iter()
                .filter(|c| c.parent_id == Some(*h) && c.kind == Kind::Item)
                .count();
            if count >= 3 && names.len() * 2 >= count {
                mixed.push(json!({
                    "holder": brief_json(&self.conn, *h)?,
                    "items": count,
                    "better_elsewhere": names,
                }));
            }
        }

        Ok(json!({
            "scope": root.map(|r| brief_json(&self.conn, r)).transpose()?,
            "considered": CONSIDERED,
            "checked": { "items": checked, "best_where_they_are": at_home },
            "elsewhere": elsewhere,
            "strays": strays,
            "full": full,
            "sparse": sparse,
            "mixed": mixed,
            "unknown_fill": unknown_fill,
        }))
    }
}

impl Inventory {
    /// Adds a group of words that mean the same thing for placing (`ldr, ışık sensörü`): a
    /// query with one of them also looks for the others.
    pub fn synonym_add(&mut self, words: &str) -> Result<Value> {
        let phrases: Vec<&str> = words
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        if phrases.len() < 2 || phrases.iter().any(|p| terms(p).is_empty()) {
            return Err(Error::Usage(
                "give two or more comma-separated words or phrases, each with a searchable word"
                    .into(),
            ));
        }
        self.conn.execute(
            "INSERT INTO synonyms (words, created_at) VALUES (?1, ?2)",
            rusqlite::params![phrases.join(", "), crate::store::now()],
        )?;
        self.synonym_list()
    }

    pub fn synonym_list(&self) -> Result<Value> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, words FROM synonyms ORDER BY id")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(json!({ "id": r.get::<_, i64>(0)?, "words": r.get::<_, String>(1)? }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(json!({ "synonyms": rows }))
    }

    pub fn synonym_remove(&mut self, id: i64) -> Result<Value> {
        if self
            .conn
            .execute("DELETE FROM synonyms WHERE id = ?1", [id])?
            == 0
        {
            return Err(Error::NotFound(format!("no synonym group {id}")));
        }
        self.synonym_list()
    }
}

/// Where in `h`'s grid a spare box of `spare`'s footprint would fit, counting `h`'s own cells
/// as free since the spare replaces it. Up to three back-left cells.
fn fits_at(conn: &Connection, h: &Node, spare: &Node) -> Result<Vec<String>> {
    let (Some(parent), Some(size)) = (h.parent_id, spare.size.as_deref()) else {
        return Ok(Vec::new());
    };
    let Some(grid) = crate::grid::grid_json(conn, parent)? else {
        return Ok(Vec::new());
    };
    let dims = parse_size(size)?;
    let (w, d) = (dims[0].round() as i64, dims[1].round() as i64);
    let cols = grid["cols"].as_i64().unwrap_or(0);
    let rows = grid["rows"].as_i64().unwrap_or(0);
    let map = &grid["map"];
    let free = |c: i64, r: i64| {
        let cell = &map[r as usize][c as usize];
        cell.is_null() || cell.as_i64() == Some(h.id)
    };
    let mut out = Vec::new();
    for (ww, dd) in [(w, d), (d, w)] {
        for r in 0..rows {
            for c in 0..cols {
                if c + ww > cols || r + dd > rows {
                    continue;
                }
                if (r..r + dd).all(|rr| (c..c + ww).all(|cc| free(cc, rr))) {
                    let name = format!("{}{}", (b'A' + c as u8) as char, r + 1);
                    if !out.contains(&name) {
                        out.push(name);
                    }
                    if out.len() == 3 {
                        return Ok(out);
                    }
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{Lexicon, terms};

    /// A vocabulary like the inventory's: base forms written somewhere on their own.
    fn lexicon() -> Lexicon {
        let text = "sensör modül kart kablo kutu vida pil renk kitap düğme sıcaklık sıcak ışık \
                    anahtar tornavida lehim mıknatıs çekmece modülsüz kar battery cable";
        let words: Vec<String> = terms(text).into_iter().map(|t| t.surface).collect();
        Lexicon::new(words.iter().map(String::as_str))
    }

    fn key(lex: &Lexicon, w: &str) -> String {
        lex.key(&terms(w)[0].surface)
    }

    #[test]
    fn turkish_word_forms_meet_at_the_inventorys_own_base_form() {
        let lex = lexicon();
        for (forms, base) in [
            (
                "sensörü sensörler sensörleri sensörlerin sensörlü",
                "sensor",
            ),
            ("modülü modülleri modüllerin modülden", "modul"),
            ("kartı kartlar kartları kartlı", "kart"),
            ("kablosu kabloları kablolu", "kablo"),
            ("kutuda kutudaki kutular kutusu", "kutu"),
            ("vidası vidaları", "vida"),
            ("pili pilleri pilli", "pil"),
            ("rengi renkli renkler", "renk"),
            ("kitabı kitaplar", "kitap"),
            ("düğmesi düğmeli düğmeler", "dugme"),
            ("sıcaklığı sıcaklıklar", "sicaklik"),
            ("ışığı ışıklı", "isik"),
            ("anahtarı anahtarlar", "anahtar"),
            ("tornavidası", "tornavida"),
            ("lehimi lehimli", "lehim"),
            ("mıknatısı mıknatıslı", "miknatis"),
            ("çekmecede çekmecedeki", "cekmece"),
            ("modules", "modul"),
            ("batteries", "battery"),
            ("cables", "cable"),
        ] {
            for f in forms.split(' ') {
                assert_eq!(key(&lex, f), base, "{f}");
            }
        }
        // Not cut too deep, and "without" stays apart from the thing itself.
        assert_eq!(key(&lex, "kart"), "kart");
        assert_eq!(key(&lex, "lehim"), "lehim");
        assert_eq!(key(&lex, "tornavida"), "tornavida");
        assert_eq!(key(&lex, "modülsüz"), "modulsuz");
        assert_eq!(key(&lex, "sıcaklık"), "sicaklik");
        // A base form is never cut further, even when it could read as stem + ending.
        assert_eq!(key(&lex, "kutu"), "kutu");
        assert_eq!(key(&lex, "düğme"), "dugme");
        // Without the base written anywhere, a query form still finds the stem two of the
        // inventory's forms share.
        let lex = Lexicon::new(["vidasi", "vidalari"].into_iter());
        assert_eq!(lex.key("vidalar"), "vida");
    }

    #[test]
    fn codes_stay_whole_and_numbers_are_not_words() {
        let surfaces: Vec<String> = terms("KY-018 LDR, 2026-09-29 DS18B20 HC-SR04 5 mm")
            .into_iter()
            .map(|t| t.surface)
            .collect();
        for code in ["ky018", "ldr", "ds18b20", "hcsr04"] {
            assert!(surfaces.contains(&code.to_string()), "{surfaces:?}");
        }
        assert!(
            !surfaces.iter().any(|k| k.starts_with("2026")),
            "{surfaces:?}"
        );
        let lex = lexicon();
        assert_eq!(lex.key("ky018"), "ky018");
    }
}
