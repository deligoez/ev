//! The scoring engine: words of a text, their stems against the inventory's own vocabulary,
//! and every holder as a weighted bag of words scored BM25F-style.

use super::*;

/// Words that never say what a thing is. Shorter than the audit list on purpose: colours and
/// sizes do tell LED boxes apart, and IDF already quiets words that are everywhere.
pub(super) const FILLER: &[&str] = &[
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

pub(super) fn weighted(ts: Vec<Term>, w: f64) -> Vec<Term> {
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
        "ndan", "nden", "dan", "den", "tan", "ten", "nin", "nun", "yla", "yle", "la", "le", "nda",
        "nde", "da", "de", "ta", "te", "in", "un", "ya", "ye", "yi", "yu", "na", "ne", "ni", "nu",
        "a", "e", "i", "u",
    ],
    &["lari", "leri", "si", "su", "i", "u"],
    &["lar", "ler"],
    &["li", "lu"],
];

/// Whether `stem` can carry ending `e` as Turkish writes it, on folded letters: an ending
/// starting with `t` follows only a voiceless consonant and one starting with `d` never does
/// (`kitaptan`, `kutuda`; not `civa`+`ta`, `var`+`ta`, `uni`+`te`), and the ending's high
/// vowel follows the stem's last vowel (`i` after a/e/i, `u` after o/u; not `sab`+`un`).
/// Folding merges ı/i, u/ü, o/ö and ç/c, so only what survives it is checked: a stem ending
/// in `c` may take either consonant, and the a/e of an ending is not checked at all (loans
/// such as `saatler` break it). The `-ki` and `-ları` endings carry their own vowels.
fn fits(stem: &str, e: &str) -> bool {
    let last = stem.chars().last();
    if e.starts_with('t') && !last.is_some_and(|c| "fstkhpc".contains(c)) {
        return false;
    }
    if e.starts_with('d') && last.is_some_and(|c| "fstkhp".contains(c)) {
        return false;
    }
    if e.ends_with("ki") || e.starts_with("lar") || e.starts_with("ler") {
        return true;
    }
    let Some(high) = e.chars().rev().find(|c| matches!(c, 'i' | 'u')) else {
        return true;
    };
    match stem.chars().rev().find(|c| "aeiou".contains(*c)) {
        Some('a' | 'e' | 'i') => high == 'i',
        Some(_) => high == 'u',
        None => true,
    }
}

/// Every stem a folded word might have: the word, and what is left after taking off any run of
/// the endings above, each time keeping at least three letters and only where the stem can
/// carry the ending (`fits`). A stem left by an ending that starts with a vowel also appears
/// with its last consonant hardened back (`kitabı`→`kitap`, `ışığı`→`ışık`, `rengi`→`renk`).
/// Plain-ASCII words also lose English plurals; `-es` leaves a three-letter stem only after a
/// sibilant, where English writes it (`boxes`→`box`, but not `güneş`→`gün`; `modules` still
/// meets `modül`).
pub(crate) fn candidates(word: &str) -> Vec<String> {
    let mut out = vec![word.to_string()];
    let mut frontier = vec![word.to_string()];
    for layer in ENDINGS {
        let mut next = frontier.clone();
        for w in &frontier {
            for e in *layer {
                let Some(stem) = w.strip_suffix(e) else {
                    continue;
                };
                if stem.chars().count() < 3 || !fits(stem, e) {
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
                let sibilant =
                    stem.ends_with(['s', 'x', 'z']) || stem.ends_with("ch") || stem.ends_with("sh");
                if end == "es" && !sibilant && stem.chars().count() < 4 {
                    continue;
                }
                let s = format!("{stem}{repl}");
                if s.chars().count() >= 3 && !out.contains(&s) {
                    out.push(s);
                }
            }
        }
    }
    out
}

/// The single endings a word may end in (`-ki`, case, possessive), each leaving a stem of at
/// least three letters that can carry it.
fn last_endings(word: &str) -> Vec<&'static str> {
    ENDINGS[..3]
        .iter()
        .flat_map(|layer| layer.iter().copied())
        .filter(|e| {
            word.strip_suffix(e)
                .is_some_and(|stem| stem.chars().count() >= 3 && fits(stem, e))
        })
        .collect()
}

/// Whether ending `then` can follow ending `first` on one word: after a possessive only a
/// case with the pronominal `n` or `-(y)la` (`arkasında`, `sapıyla`), after a case nothing
/// on this list, after `-ki` anything.
fn may_follow(first: &str, then: &str) -> bool {
    const POSSESSIVE: &[&str] = &["lari", "leri", "si", "su", "i", "u"];
    const AFTER_POSSESSIVE: &[&str] = &[
        "nda", "nde", "ndan", "nden", "na", "ne", "ni", "nu", "nin", "nun", "yla", "yle",
    ];
    if POSSESSIVE.contains(&first) {
        AFTER_POSSESSIVE.contains(&then)
    } else {
        !ENDINGS[1].contains(&first)
    }
}

pub(super) const COLORS: &[&str] = &[
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
    "bordo",
    "bej",
    "krem",
    "gumus",
    "haki",
    "lila",
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
pub(super) fn is_compound(a: &Term, b: &Term) -> bool {
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
/// suffix stripping from cutting too deep. In order: the word itself if the inventory shows it
/// is a root (see `roots`: `altın` stays, though `alt` is written too); the longest shorter
/// form that some word carries a plural ending on (`bağları` makes `bağ` a noun, so
/// `bağı`→`bağ` rather than the hardened `bak`; `bacaklarında`→`bacak`); the shortest shorter
/// form that is written somewhere on its own (`kutuda`→`kutu`, `kitabı`→`kitap`,
/// `modules`→`modül`); the word itself if it is written on its own, so a base form is never cut
/// further (`kutu` stays, though it could read as `kut`+`u`); the longest shorter form two
/// different words lead to (`vidası` with `vidaları`→`vida`); the word itself. The known gaps:
/// two forms whose base is written nowhere, both themselves written on their own, stay apart;
/// and a root the inventory never inflects is still cut when its letters read as a shorter
/// written word plus an ending (`kapı`→`kap`, `veri`→`ver`), which only a dictionary could tell.
pub(crate) struct Lexicon {
    words: HashSet<String>,
    shared: HashMap<String, usize>,
    /// Stems written somewhere with `-lar`/`-ler` after them, and attested beyond that one
    /// word (written on their own, or a stem of two words), so `controller` makes no `control`
    /// noun of its own.
    plural_stems: HashSet<String>,
    /// Written words that are roots of their own though they read as a shorter word plus an
    /// ending: another written word carries an ending on them that cannot follow that ending
    /// (`üniteleri` is not `ünit`+`e`+`leri`, so `ünite` is no dative; `pensesi`, `altında`
    /// with no `altı` written), and that word
    /// cannot be read through some other written word instead (`dolabının` is `dolabı`+`nın`,
    /// so it says nothing about `dolabın`).
    roots: HashSet<String>,
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
        let mut plural_stems = HashSet::new();
        for w in &words {
            for c in candidates(w).into_iter().skip(1) {
                let plural = w
                    .strip_prefix(c.as_str())
                    .is_some_and(|rest| rest.starts_with("lar") || rest.starts_with("ler"));
                let attested = words.contains(&c) || shared.get(&c).is_some_and(|n| *n >= 2);
                if plural && attested {
                    plural_stems.insert(c);
                }
            }
        }
        let all: Vec<&str> = ENDINGS.iter().flat_map(|l| l.iter().copied()).collect();
        // `v` read as another written word plus an ending that may follow that word's own.
        let other_reading = |v: &str, w: &str| {
            all.iter().any(|then| {
                v.strip_suffix(then).is_some_and(|u| {
                    u != w && u.chars().count() >= 3 && words.contains(u) && {
                        let firsts = last_endings(u);
                        firsts.is_empty() || firsts.iter().any(|f| may_follow(f, then))
                    }
                })
            })
        };
        let roots = words
            .iter()
            .filter(|w| {
                let firsts = last_endings(w);
                !firsts.is_empty()
                    && all.iter().any(|then| {
                        let v = format!("{w}{then}");
                        words.contains(&v)
                            && firsts.iter().all(|f| !may_follow(f, then))
                            && !other_reading(&v, w)
                    })
            })
            .cloned()
            .collect();
        Lexicon {
            words,
            shared,
            plural_stems,
            roots,
        }
    }

    pub(crate) fn key(&self, word: &str) -> String {
        if word.chars().any(|c| c.is_ascii_digit()) || self.roots.contains(word) {
            return word.to_string();
        }
        let cands = candidates(word);
        // The longest noun stem, the first of equals in candidate order.
        let noun = cands
            .iter()
            .skip(1)
            .filter(|c| self.plural_stems.contains(*c))
            .fold(None::<&String>, |best, c| match best {
                Some(b) if b.chars().count() >= c.chars().count() => Some(b),
                _ => Some(c),
            });
        if let Some(c) = noun {
            return c.clone();
        }
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

    /// Words stemmed against this vocabulary, plus the two-word terms that carry meaning the
    /// words alone do not: an indefinite noun compound (`hesap makinesi`, `kablo bağı`: a bare
    /// noun, then a noun whose only ending is the 3rd-person possessive) and a colour with the
    /// noun it describes (`yeşil LED`). A colour on its own counts `COLOR_WEIGHT` in a query,
    /// so a green heat gun does not land in the green LED box.
    pub(crate) fn keyed(&self, ts: Vec<Term>) -> Vec<Term> {
        let words: Vec<Term> = ts
            .into_iter()
            .map(|mut t| {
                t.key = self.key(&t.surface);
                if COLORS.contains(&t.key.as_str()) {
                    t.weight *= COLOR_WEIGHT;
                }
                t
            })
            .collect();
        let mut out = words.clone();
        for pair in words.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if COLORS.contains(&a.key.as_str()) || is_compound(a, b) {
                out.push(Term {
                    key: format!("{}+{}", a.key, b.key),
                    surface: format!("{} {}", a.surface, b.surface),
                    weight: a.weight.max(b.weight),
                });
            }
        }
        out
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
    pub(super) lex: Lexicon,
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

    pub(super) fn idf(&self, key: &str) -> f64 {
        let n = self.docs.len() as f64;
        let df = *self.df.get(key).unwrap_or(&0) as f64;
        (1.0 + (n - df + 0.5) / (df + 0.5)).ln()
    }

    /// Whether a word is rare enough here to say what a thing is.
    /// A two-word term is rare by construction, so it never counts here on its own: it adds to
    /// a score, but a move is only flagged on a word that says what the thing is.
    pub(super) fn specific(&self, key: &str) -> bool {
        !key.contains('+')
            && (self.idf(key) >= SPECIFIC || self.df.get(key).is_some_and(|d| *d <= SPECIFIC_DF))
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
