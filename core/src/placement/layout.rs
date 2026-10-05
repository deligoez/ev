//! `ev layout`: a piece of furniture read across all its places at once, from what they hold,
//! their themes left aside (spec/reorganize.md). A theme says what a drawer holds today; this
//! looks for what a new layout would fix: kinds spread over several places, places that read
//! alike, places nearly empty or too full.

use super::*;

/// How many rows each list keeps, best first.
const LISTED: usize = 12;
/// Two places read alike from this share of their weighted words on (cosine).
const OVERLAP: f64 = 0.3;
/// A place with this many things or fewer is nearly empty.
const FEW: usize = 3;
/// A place of this many things or more is mixed when no word names this share of them.
const MIXED_THINGS: usize = 8;
const MIXED_SHARE: f64 = 0.4;
/// A word in more than this share of the places says nothing about any of them.
const COMMON_SHARE: f64 = 0.7;

/// A place of the furniture: what is in it, per word (stem) how many of its things name it,
/// and per kind how many of its things are of it.
struct Place<'a> {
    node: &'a Node,
    things: Vec<&'a Node>,
    words: BTreeMap<String, usize>,
    kinds: BTreeMap<String, usize>,
}

/// What kind of thing a name says it is: the last word of its head (the part before the first
/// comma or dash), since a Turkish name ends in what the thing is: a `şarj modülü` is a
/// module, a `şarjlı matkap` a drill, `Kablo, USB-C` a cable. Adjectives (`mini`, `şeffaf`,
/// `kutulu`) say what it is like, not what it is.
fn kind_of(index: &Index, name: &str) -> Option<(String, String)> {
    name_stems(index, &crate::purchase_match::head(name))
        .pop()
        .or_else(|| name_stems(index, name).pop())
}

/// The stems of a thing's name, as `ev themes` reads them: words of three letters or more, no
/// numbers, no filler or colour words; each with the form it is written in.
fn name_stems(index: &Index, name: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for token in name.split(|c: char| !c.is_alphanumeric()) {
        let folded = fold(token);
        if folded.chars().count() < 3
            || folded.chars().any(|c| c.is_ascii_digit())
            || FILLER.contains(&folded.as_str())
        {
            continue;
        }
        let key = index.lex.key(&folded);
        if COLORS.contains(&key.as_str()) || out.iter().any(|(k, _)| *k == key) {
            continue;
        }
        out.push((key, token.to_lowercase()));
    }
    out
}

/// The cosine of two places' word counts, each word weighted by how rare it is.
fn alike(index: &Index, a: &Place, b: &Place) -> f64 {
    let w = |p: &Place, k: &str| p.words.get(k).copied().unwrap_or(0) as f64 * index.idf(k);
    let dot: f64 = a.words.keys().map(|k| w(a, k) * w(b, k)).sum();
    let norm = |p: &Place| p.words.keys().map(|k| w(p, k).powi(2)).sum::<f64>().sqrt();
    let (na, nb) = (norm(a), norm(b));
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

impl Inventory {
    /// Every place of a piece of furniture (the places gone through one at a time below it)
    /// compared by what it holds, never by its theme: kinds of thing kept in several places
    /// (`spread`), places that read alike (`overlap`), nearly empty places and the like place
    /// with room they could join (`merge`), full or mixed places (`split`). With `propose`, a
    /// layout drafted from the contents alone: the kinds, largest first, each given the place
    /// that holds most of it, the moves that takes and a theme for each place. Nothing is moved.
    pub fn layout(&self, reference: &str, propose: bool) -> Result<Value> {
        let all = live_nodes(&self.conn)?;
        let root = resolve(&self.conn, reference, false)?;
        let scope = subtree(&all, root);
        let by_id: HashMap<i64, &Node> = all.iter().map(|n| (n.id, n)).collect();
        let mut kids: HashMap<i64, Vec<&Node>> = HashMap::new();
        for n in &all {
            if let Some(p) = n.parent_id {
                kids.entry(p).or_default().push(n);
            }
        }
        let units: Vec<i64> = crate::plan::units(&all)
            .into_iter()
            .filter(|u| *u != root && scope.contains(u))
            .collect();
        if units.len() < 2 {
            return Err(Error::Usage(format!(
                "{reference} has {} place(s) gone through on its own; a layout compares two or more",
                units.len()
            )));
        }
        let index = self.word_index(&all)?;
        let mut places: Vec<Place> = Vec::new();
        for u in &units {
            let mut things = Vec::new();
            let mut stack: Vec<&Node> = kids.get(u).cloned().unwrap_or_default();
            while let Some(n) = stack.pop() {
                if n.lost {
                    continue;
                }
                if n.kind == Kind::Item {
                    things.push(n);
                }
                stack.extend(kids.get(&n.id).cloned().unwrap_or_default());
            }
            things.sort_by_key(|n| n.id);
            let (mut words, mut kinds) = (BTreeMap::new(), BTreeMap::new());
            for t in &things {
                for (k, _) in name_stems(&index, &t.name) {
                    *words.entry(k).or_insert(0) += 1;
                }
                if let Some((k, _)) = kind_of(&index, &t.name) {
                    *kinds.entry(k).or_insert(0) += 1;
                }
            }
            places.push(Place {
                node: by_id[u],
                things,
                words,
                kinds,
            });
        }
        // A word in most places (a material, a brand of the whole shelf) tells none apart.
        let mut in_places: BTreeMap<String, usize> = BTreeMap::new();
        for p in &places {
            for k in p.words.keys() {
                *in_places.entry(k.clone()).or_insert(0) += 1;
            }
        }
        let common = |k: &str| {
            in_places.get(k).copied().unwrap_or(0) as f64 > COMMON_SHARE * places.len() as f64
        };
        // The form each word is written in most, for the person to read.
        let mut forms: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
        for p in &places {
            for t in &p.things {
                for (k, f) in name_stems(&index, &t.name) {
                    *forms.entry(k).or_default().entry(f).or_insert(0) += 1;
                }
            }
        }
        let form = |k: &str| -> String {
            forms
                .get(k)
                .and_then(|f| f.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))))
                .map_or_else(|| k.to_string(), |(f, _)| f.clone())
        };
        let brief = |n: &Node| brief_json(&self.conn, n.id);

        // Kinds of thing in several places.
        let mut kind_places: BTreeMap<&String, usize> = BTreeMap::new();
        for p in &places {
            for k in p.kinds.keys() {
                *kind_places.entry(k).or_insert(0) += 1;
            }
        }
        let mut spread = Vec::new();
        for (k, n) in &kind_places {
            if *n < 2 || common(k) {
                continue;
            }
            let mut where_ = Vec::new();
            let mut total = 0;
            for p in &places {
                if let Some(c) = p.kinds.get(*k) {
                    total += c;
                    where_.push((p, *c));
                }
            }
            if total < 3 {
                continue;
            }
            where_.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.node.id.cmp(&b.0.node.id)));
            spread.push((
                where_.len(),
                total,
                json!({
                    "word": form(k),
                    "things": total,
                    "places": where_
                        .iter()
                        .map(|(p, c)| Ok(json!({ "place": brief(p.node)?, "things": c })))
                        .collect::<Result<Vec<_>>>()?,
                }),
            ));
        }
        spread.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

        // The words two places share, rarest first.
        let shared = |a: &Place, b: &Place| -> Vec<String> {
            let mut s: Vec<(&String, f64)> = a
                .words
                .iter()
                .filter(|(k, _)| b.words.contains_key(*k) && !common(k))
                .map(|(k, c)| (k, (*c).min(b.words[k]) as f64 * index.idf(k)))
                .collect();
            s.sort_by(|x, y| y.1.total_cmp(&x.1).then(x.0.cmp(y.0)));
            s.into_iter().take(5).map(|(k, _)| form(k)).collect()
        };
        let mut overlap = Vec::new();
        for (i, a) in places.iter().enumerate() {
            for b in &places[i + 1..] {
                let s = alike(&index, a, b);
                if s >= OVERLAP {
                    overlap.push((
                        s,
                        json!({
                            "places": [brief(a.node)?, brief(b.node)?],
                            "alike": round(s),
                            "shared": shared(a, b),
                        }),
                    ));
                }
            }
        }
        overlap.sort_by(|a, b| b.0.total_cmp(&a.0));

        // Nearly empty places, and the like place with room each could join.
        let mut merge = Vec::new();
        for a in &places {
            let sparse = a.things.len() <= FEW || a.node.fill.is_some_and(|f| f <= SPARSE);
            if !sparse || a.things.is_empty() {
                continue;
            }
            let into = places
                .iter()
                .filter(|b| b.node.id != a.node.id && b.node.fill.is_none_or(|f| f < ROOM))
                .map(|b| (alike(&index, a, b), b))
                .filter(|(s, _)| *s > 0.0)
                .max_by(|x, y| x.0.total_cmp(&y.0).then(y.1.node.id.cmp(&x.1.node.id)));
            merge.push(json!({
                "place": brief(a.node)?,
                "things": a.things.len(),
                "fill": a.node.fill,
                "into": into.map(|(_, b)| brief(b.node)).transpose()?,
                "shared": into.map(|(_, b)| shared(a, b)).unwrap_or_default(),
            }));
        }

        // Full places, and mixed ones no kind makes up most of.
        let mut split = Vec::new();
        for p in &places {
            let mut groups: Vec<(&String, &usize)> = p.kinds.iter().collect();
            groups.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let top = groups.first().map_or(0, |g| *g.1);
            let full = p.node.fill.is_some_and(|f| f >= FULL);
            let mixed = p.things.len() >= MIXED_THINGS
                && (top as f64) < MIXED_SHARE * p.things.len() as f64;
            if !full && !mixed {
                continue;
            }
            split.push(json!({
                "place": brief(p.node)?,
                "things": p.things.len(),
                "fill": p.node.fill,
                "why": if full { "full" } else { "mixed" },
                "groups": groups
                    .iter()
                    .take(4)
                    .map(|(k, c)| json!({ "word": form(k), "things": c }))
                    .collect::<Vec<_>>(),
            }));
        }

        let mut out = json!({
            "furniture": brief(by_id[&root])?,
            "places": places.len(),
            "things": places.iter().map(|p| p.things.len()).sum::<usize>(),
            "spread": spread.into_iter().take(LISTED).map(|s| s.2).collect::<Vec<_>>(),
            "overlap": overlap.into_iter().take(LISTED).map(|o| o.1).collect::<Vec<_>>(),
            "merge": merge,
            "split": split,
        });
        if propose {
            out["proposal"] = self.draft_layout(&index, &places, &common, &form)?;
        }
        Ok(out)
    }

    /// A layout from the contents alone: each thing in the group of its kind, the groups
    /// largest first, each to the place holding most of it and not yet given a group
    /// (else the emptiest place left); the moves that takes, and a theme per place.
    fn draft_layout(
        &self,
        index: &Index,
        places: &[Place],
        common: &dyn Fn(&str) -> bool,
        form: &dyn Fn(&str) -> String,
    ) -> Result<Value> {
        // (place index, thing) per kind; a kind of one thing in the whole furniture is no group,
        // and stays where it is.
        let mut groups: BTreeMap<String, Vec<(usize, &Node)>> = BTreeMap::new();
        for (pi, p) in places.iter().enumerate() {
            for t in &p.things {
                if let Some((k, _)) = kind_of(index, &t.name).filter(|(k, _)| !common(k)) {
                    groups.entry(k).or_default().push((pi, t));
                }
            }
        }
        groups.retain(|_, members| members.len() >= 2);
        let mut order: Vec<(String, Vec<(usize, &Node)>)> = groups.into_iter().collect();
        order.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
        let mut taken = vec![false; places.len()];
        let (mut moves, mut themes, mut kept) = (Vec::new(), Vec::new(), Vec::new());
        for (word, members) in order {
            let mut held: BTreeMap<usize, usize> = BTreeMap::new();
            for (pi, _) in &members {
                *held.entry(*pi).or_insert(0) += 1;
            }
            let target = held
                .iter()
                .filter(|(pi, _)| !taken[**pi])
                .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
                .map(|(pi, _)| *pi)
                .or_else(|| {
                    (0..places.len())
                        .filter(|pi| !taken[*pi])
                        .min_by_key(|pi| (places[*pi].things.len(), places[*pi].node.id))
                });
            let Some(to) = target else {
                // More kinds than places: this one stays where it is.
                kept.push(json!({ "word": form(&word), "things": members.len() }));
                continue;
            };
            taken[to] = true;
            themes.push(json!({
                "place": brief_json(&self.conn, places[to].node.id)?,
                "theme": form(&word),
                "things": members.len(),
            }));
            for (pi, t) in members {
                if pi != to {
                    moves.push(json!({
                        "thing": brief_json(&self.conn, t.id)?,
                        "from": brief_json(&self.conn, places[pi].node.id)?,
                        "to": brief_json(&self.conn, places[to].node.id)?,
                    }));
                }
            }
        }
        Ok(json!({ "themes": themes, "moves": moves, "kept": kept }))
    }
}
