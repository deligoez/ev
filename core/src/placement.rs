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

