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

