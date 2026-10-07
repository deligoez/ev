//! Free-text search behind `ev find` and the UI's `/`. Every word of the query has to turn up
//! somewhere on the record — name, code, make, model, serial, tags, theme or note — in any
//! order. A word counts when it appears as written, when one of its Turkish stems starts a word
//! there (`kırmızılar` finds `kırmızı`), or when a synonym group names it. Only a word that
//! meets nothing at all that way is tried again with a typo or two (`kirmzi`), so a correct word
//! never drags in look-alikes.

use rusqlite::Connection;

use crate::Result;
use crate::fold;
use crate::model::Node;
use crate::placement::candidates;

/// How much a field says about the record: its name and code most, its note least.
const NAME: f64 = 4.0;
const CODE: f64 = 4.0;
const TAG: f64 = 2.0;
const THEME: f64 = 2.0;
const NOTE: f64 = 1.0;

/// How well a query word met a field.
const WHOLE_WORD: f64 = 1.5;
const INSIDE: f64 = 1.0;
const STEM: f64 = 0.8;
const SYNONYM: f64 = 0.7;
/// A typo of the whole word (`vdia` for `vida`) says more than one of its start (`vdia` for
/// `diamond`).
const TYPO: f64 = 0.5;
const TYPO_OF_A_START: f64 = 0.25;
/// A word this long or shorter is a whole word or a word's start, never found inside another.
const SHORT: usize = 3;

/// A query word with everything else it may be written as.
struct Word {
    text: String,
    /// The word as a code compares (spec/codes.md): `S5_13`, `S5-013` and `S5-13` are one.
    code: String,
    stems: Vec<String>,
    synonyms: Vec<String>,
}

/// The query's words. Spaces and commas separate them; hyphens stay, so codes like
/// `K4x4-07-Ü` are one word.
pub(super) struct Query {
    folded: String,
    words: Vec<Word>,
}

impl Query {
    pub(super) fn parse(conn: &Connection, text: &str) -> Result<Query> {
        let folded = fold(text);
        let groups = synonym_groups(conn)?;
        let words = folded
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|w| !w.is_empty())
            .map(|w| {
                let stems: Vec<String> = candidates(w).into_iter().filter(|s| s != w).collect();
                let synonyms = groups
                    .iter()
                    .filter(|g| g.iter().any(|p| p == w || stems.contains(p)))
                    .flatten()
                    .filter(|p| *p != w && !stems.contains(p))
                    .cloned()
                    .collect();
                Word {
                    text: w.to_string(),
                    code: crate::fold::fold_code(w),
                    stems,
                    synonyms,
                }
            })
            .collect();
        Ok(Query { folded, words })
    }

    pub(super) fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// The nodes every query word meets, best first; ties keep the given order. A word that
    /// meets no node as written, by stem or by synonym is retried allowing typos.
    pub(super) fn rank(&self, nodes: &[Node]) -> Vec<(i64, f64)> {
        let fields: Vec<Vec<Field>> = nodes.iter().map(fields).collect();
        let typos: Vec<bool> = self
            .words
            .iter()
            .map(|w| !fields.iter().any(|f| meet(w, f, false).is_some()))
            .collect();
        let mut hits: Vec<(i64, f64)> = nodes
            .iter()
            .zip(&fields)
            .filter_map(|(n, f)| {
                let mut score = 0.0;
                for (w, &typo) in self.words.iter().zip(&typos) {
                    score += meet(w, f, typo)?;
                }
                if f[0].1.contains(&self.folded) {
                    score += NAME;
                }
                Some((n.id, score))
            })
            .collect();
        hits.sort_by(|a, b| b.1.total_cmp(&a.1));
        hits
    }
}

/// A searchable field: its weight, its folded text, and whether it is the record's code.
type Field = (f64, String, bool);

/// The record's searchable text, folded, with each field's weight; the name comes first. The
/// code is folded as a code, so its separator and the padding of its numbers do not count.
fn fields(n: &Node) -> Vec<Field> {
    let mut out = vec![(NAME, fold(&n.name), false)];
    out.extend(
        n.code
            .iter()
            .map(|c| (CODE, crate::fold::fold_code(c), true)),
    );
    // Make, model and serial are read off the label: as telling as the code.
    out.extend(
        [&n.make, &n.model, &n.serial]
            .into_iter()
            .flatten()
            .map(|t| (CODE, fold(t), false)),
    );
    out.extend(n.tags.iter().map(|t| (TAG, fold(t), false)));
    out.extend(n.theme.iter().map(|t| (THEME, fold(t), false)));
    out.extend(n.note.iter().map(|t| (NOTE, fold(t), false)));
    out
}

/// The best score a query word gets on any of the record's fields, if it meets one at all. A
/// code is met by the word as a code compares (`S5-13` meets `S5_13`), then as any field.
fn meet(w: &Word, fields: &[Field], typos: bool) -> Option<f64> {
    fields
        .iter()
        .filter_map(|(weight, text, code)| {
            let as_code = (*code && text.contains(&w.code))
                .then(|| if *text == w.code { WHOLE_WORD } else { INSIDE });
            as_code
                .or_else(|| quality(w, text, typos))
                .map(|q| weight * q)
        })
        .max_by(f64::total_cmp)
}

/// Whether `part` is in `text`: anywhere, or for a short part only at the start of a word, so
/// `ir` is the IR sensor and not every `bir`, and `ble` is not inside `mixable`
/// (spec/find-any.md).
fn contains(text: &str, part: &str) -> bool {
    if part.chars().count() <= SHORT {
        words(text).any(|t| t.starts_with(part))
    } else {
        text.contains(part)
    }
}

/// Whether `text` names a synonym: the words of a phrase in a row, each a whole word of the
/// text or its Turkish stem (`ekranlı` names `ekran`), never the start of another word
/// (`displayport` does not name `display`).
fn names(text: &str, phrase: &str) -> bool {
    let want: Vec<&str> = words(phrase).collect();
    let have: Vec<&str> = words(text).collect();
    if want.is_empty() || have.len() < want.len() {
        return false;
    }
    let is = |t: &str, s: &str| t == s || candidates(t).iter().any(|c| c == s);
    have.windows(want.len())
        .any(|run| run.iter().zip(&want).all(|(t, s)| is(t, s)))
}

fn quality(w: &Word, text: &str, typos: bool) -> Option<f64> {
    if contains(text, &w.text) {
        let whole = words(text).any(|t| t == w.text);
        return Some(if whole { WHOLE_WORD } else { INSIDE });
    }
    // A stem of a letter or two starts too many words to say anything.
    if w.stems
        .iter()
        .filter(|s| s.chars().count() > 2)
        .any(|s| words(text).any(|t| t.starts_with(s.as_str())))
    {
        return Some(STEM);
    }
    if w.synonyms.iter().any(|s| names(text, s)) {
        return Some(SYNONYM);
    }
    let allowed = match w.text.chars().count() {
        0..4 => return None,
        4..8 => 1,
        _ => 2,
    };
    if !typos {
        return None;
    }
    words(text)
        .filter_map(|t| typo(&w.text, t, allowed))
        .max_by(f64::total_cmp)
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
}

/// Whether `word` is within `allowed` edits (insert, delete, change, swap of neighbours) of
/// `target` or of a start of it, so a typo in a word written with an ending still counts.
#[cfg(test)]
fn near(word: &str, target: &str, allowed: usize) -> bool {
    typo(word, target, allowed).is_some()
}

/// How well `word` meets `target` with a typo: `TYPO` within `allowed` edits (insert, delete,
/// change, swap of neighbours) of the whole of it, `TYPO_OF_A_START` of a start of it only (a
/// typo in a word written with an ending, or a word that only looks like another's start).
fn typo(word: &str, target: &str, allowed: usize) -> Option<f64> {
    let a: Vec<char> = word.chars().collect();
    let b: Vec<char> = target.chars().collect();
    if b.len() + allowed < a.len() {
        return None;
    }
    // Optimal string alignment distance, keeping the last three rows.
    let mut prev2 = vec![0; b.len() + 1];
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut row = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            row[j] = (prev[j] + 1).min(row[j - 1] + 1).min(prev[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                row[j] = row[j].min(prev2[j - 2] + 1);
            }
        }
        prev2 = std::mem::replace(&mut prev, row);
    }
    // The last row holds the distance from the whole word to every start of the target; a
    // start much shorter than the word is not a match.
    if prev[b.len()] <= allowed {
        return Some(TYPO);
    }
    prev[a.len().saturating_sub(allowed).min(b.len())..]
        .iter()
        .any(|&d| d <= allowed)
        .then_some(TYPO_OF_A_START)
}

/// Synonym groups as folded phrases.
fn synonym_groups(conn: &Connection) -> Result<Vec<Vec<String>>> {
    let mut stmt = conn.prepare("SELECT words FROM synonyms ORDER BY id")?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .iter()
        .map(|w| w.split(',').map(fold).filter(|p| !p.is_empty()).collect())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::near;

    #[test]
    fn a_typo_is_near_the_word_and_a_start_of_it_but_a_different_word_is_not() {
        assert!(near("kirmzi", "kirmizi", 1));
        assert!(near("kimrizi", "kirmizi", 1));
        assert!(near("kirmzi", "kirmizilar", 1));
        assert!(!near("kirmzi", "kir", 1));
        assert!(!near("yesil", "kirmizi", 1));
    }
}
