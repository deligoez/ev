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

