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
    Inventory, brief_json, holder_json, is_holder, live_nodes, parking_of, parse_size, resolve,
    rules_json,
};

mod index;
mod vocab;

#[cfg(test)]
use index::Lexicon;
use index::{COLORS, FILLER, weighted};
pub(crate) use index::{Index, Scored, Term, candidates, terms};

/// The word index kept between calls, with the state of the data it was built from.
pub(crate) type WordIndex = std::cell::RefCell<Option<((i64, i64), std::sync::Arc<Index>)>>;
use vocab::Facets;

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
/// How many empty boxes `suggest` offers when the thing has no group yet.
const EMPTY_OFFERED: usize = 10;

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
    /// The word index of every live holder (`all`, as `live_nodes` gives them), built once per
    /// state of the data: `suggest`, `regroup` and `themes` each built it again, stemming the
    /// whole house's vocabulary (20 ms on one household), and `ev ui` asks on every new place it
    /// shows. The state is the database's data version (writes by other connections) and this
    /// connection's own write count, so a change from anywhere builds it anew.
    pub(crate) fn word_index(&self, all: &[Node]) -> Result<std::sync::Arc<Index>> {
        let key = (self.data_version()?, self.conn.total_changes() as i64);
        if let Some((k, index)) = self.word_index.borrow().as_ref()
            && *k == key
        {
            return Ok(index.clone());
        }
        let index = std::sync::Arc::new(Index::build(all));
        *self.word_index.borrow_mut() = Some((key, index.clone()));
        Ok(index)
    }

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
        let index = self.word_index(&all)?;
        let groups: Vec<(i64, Vec<Term>)> = synonym_groups(&self.conn)?
            .into_iter()
            .map(|(id, ts)| (id, index.keyed(ts)))
            .collect();
        let (query, synonyms) = expand(index.keyed(query), &groups);
        let has_children: HashSet<i64> = all.iter().filter_map(|n| n.parent_id).collect();
        let by_id: HashMap<i64, &Node> = all.iter().map(|n| (n.id, n)).collect();
        // The thing's facet, from its words (and, with --for, from the node itself): holders of
        // another facet are kept out of the ranking and listed apart.
        let facets = Facets::load(&self.conn, &index)?;
        let mut want = facets.worded(&query);
        if let Some(n) = for_node["id"].as_i64().and_then(|id| by_id.get(&id)) {
            want.extend(facets.of_thing(&index, &by_id, n));
        }
        let (ranked, other): (Vec<Scored>, Vec<Scored>) = index
            .score(&query, &exclude, &skip)
            .into_iter()
            .partition(|s| !Facets::clash(&want, &facets.of_holder(&by_id, s.id)));
        // A parking place (or anything inside one) is where things wait, not where they belong:
        // it is listed apart, never offered as the answer.
        let (ranked, parking): (Vec<Scored>, Vec<Scored>) = ranked
            .into_iter()
            .partition(|s| parking_of(&by_id, s.id).is_none());
        let facet_names = |id: i64| facets.names(&facets.of_holder(&by_id, id));
        let similar = ranked
            .iter()
            .take(12)
            .map(|s| {
                let n = by_id[&s.id];
                let mut c = holder_json(&self.conn, n, &all)?;
                c["room"] = room(&self.conn, n)?;
                c["facet"] = json!(facet_names(s.id));
                // Whether the place has been gone through (a toured drawer covers its boxes):
                // an untoured one is a guess to check.
                c["review"] = crate::plan::review_inherited(&self.conn, s.id)?;
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
        let other_facet = other
            .iter()
            .take(5)
            .map(|s| {
                let mut c = brief_json(&self.conn, s.id)?;
                c["facet"] = json!(facet_names(s.id));
                Ok(json!({ "container": c, "score": round(s.score) }))
            })
            .collect::<Result<Vec<_>>>()?;
        let parking = parking
            .iter()
            .take(5)
            .map(|s| {
                let mut c = brief_json(&self.conn, s.id)?;
                c["temporary_in"] = json!(parking_of(&by_id, s.id));
                Ok(json!({ "container": c, "score": round(s.score) }))
            })
            .collect::<Result<Vec<_>>>()?;
        let holders = all
            .iter()
            .filter(|n| is_holder(n, &has_children) && !skip.contains(&n.id))
            .map(|n| {
                let mut c = holder_json(&self.conn, n, &all)?;
                c["room"] = room(&self.conn, n)?;
                c["facet"] = json!(facet_names(n.id));
                Ok(c)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut words: Vec<&str> = Vec::new();
        for t in &query {
            if !words.contains(&t.surface.as_str()) {
                words.push(&t.surface);
            }
        }
        // No holder matched on a word that says what the thing is: it has no group yet.
        let new_group = ranked.first().is_none_or(|s| s.coverage < GROUP_COVERAGE);
        // A new group starts in an empty box with no theme yet: those in the thing's own room
        // first. Empty is worked out from the records, never from a tag.
        let empty = if new_group {
            let near = for_node["id"].as_i64().and_then(|id| room_of(&by_id, id));
            // Only boxes known to be empty: a carton never gone through may be full. Boxes that
            // move come before the slots of furniture, which a person seldom means by "a box".
            let mut boxes: Vec<(&Node, bool)> = Vec::new();
            for n in all.iter().filter(|n| {
                n.kind == Kind::Container
                    && n.theme.is_none()
                    && !has_children.contains(&n.id)
                    && !skip.contains(&n.id)
                    && parking_of(&by_id, n.id).is_none()
            }) {
                if crate::store::known_empty(&self.conn, n.id)? {
                    boxes.push((n, crate::store::is_slot(&self.conn, n)?));
                }
            }
            let here = |n: &Node| near.is_some() && room_of(&by_id, n.id) == near;
            boxes.sort_by_key(|(n, slot)| (!here(n), *slot, n.id));
            boxes
                .iter()
                .take(EMPTY_OFFERED)
                .map(|(n, slot)| {
                    let mut c = holder_json(&self.conn, n, &all)?;
                    c["same_room"] = json!(here(n));
                    c["slot"] = json!(slot);
                    Ok(c)
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        Ok(json!({
            "query": text,
            "for": for_node,
            "words": words,
            "synonyms_added": synonyms,
            "facet": facets.names(&want),
            "new_group_likely": new_group,
            "empty": empty,
            "considered": CONSIDERED,
            "rules": rules_json(&self.conn)?,
            "similar": similar,
            "other_facet": other_facet,
            "parking": parking,
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
        // Only holders are scored, so only the holders outside the scope need skipping: a set of
        // every record outside it was copied once per thing.
        let outside: HashSet<i64> = all
            .iter()
            .filter(|n| is_holder(n, &has_children) && !holder_ids.contains(&n.id))
            .map(|n| n.id)
            .collect();
        let index = self.word_index(&all)?;
        let groups: Vec<(i64, Vec<Term>)> = synonym_groups(&self.conn)?
            .into_iter()
            .map(|(id, ts)| (id, index.keyed(ts)))
            .collect();
        let node_terms = |n: &Node| expand(index.keyed(node_terms(n)), &groups).0;
        let facets = Facets::load(&self.conn, &index)?;
        let holder_facets: Vec<(i64, HashSet<String>)> = holders
            .iter()
            .map(|h| (h.id, facets.of_holder(&by_id, h.id)))
            .collect();
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
        let mut alone = Vec::new();
        let mut checked = 0;
        let mut at_home = 0;
        let mut elsewhere_by_holder: BTreeMap<i64, Vec<String>> = BTreeMap::new();
        let mut best_of: HashMap<i64, i64> = HashMap::new();
        let mut flagged: HashSet<i64> = HashSet::new();
        // Moves the person said no to, while the thing is still where it was then.
        let declines = declines_of(&self.conn)?;
        let mut declined = Vec::new();
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
            // Nor is moving it into a holder of another facet (a module among bare parts).
            let own = facets.of_thing(&index, &by_id, item);
            if !own.is_empty() {
                skip.extend(
                    holder_facets
                        .iter()
                        .filter(|(_, f)| Facets::clash(&own, f))
                        .map(|(id, _)| *id),
                );
            }
            // A thing that holds things is a holder too, but never a better place for itself.
            if has_children.contains(&item.id) {
                skip.extend(subtree(&all, item.id));
            } else {
                skip.insert(item.id);
            }
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
            if let Some((_, why)) = declines.get(&item.id).filter(|d| d.0 == parent) {
                flagged.insert(item.id);
                declined.push(json!({
                    "item": brief_json(&self.conn, item.id)?,
                    "holder": brief_json(&self.conn, parent)?,
                    "why": why,
                }));
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
                let entry = json!({
                    "item": brief_json(&self.conn, item.id)?,
                    "now": { "holder": brief_json(&self.conn, parent)?, "score": round(here) },
                    "better": { "holder": brief_json(&self.conn, best.id)?, "score": round(best.score), "matched": best.matched },
                });
                // Nothing else where it is shares a word with it: its home says nothing either
                // way (a heat gun among other power tools), so the other holder is a guess.
                if here == 0.0 {
                    alone.push(entry);
                } else {
                    elsewhere.push(entry);
                }
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
                    // The cheap tests first: the walk up the thing's holders comes last.
                    n.kind == Kind::Item
                        && best_of.get(&n.id) == Some(&h.id)
                        && !flagged.contains(&n.id)
                        && n.parent_id != Some(h.id)
                        && n.parent_id.is_some_and(|p| holder_ids.contains(&p))
                        // A holder the thing is already inside is not somewhere else.
                        && !ancestors(n.id).contains(&h.id)
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

        // Spare boxes: boxes with a size that nothing is in, worked out from the records. The
        // old "boş kap" (or "spare box") tag still marks a spare that is not a container, but a
        // box with something in it is never one, whatever its tag says.
        let mut spares: Vec<&Node> = Vec::new();
        for n in all.iter().filter(|n| {
            n.size.is_some() && n.theme.is_none() && !all.iter().any(|c| c.parent_id == Some(n.id))
        }) {
            // Tagged by the person, or a container known to be empty (not one never counted).
            let tagged = n
                .tags
                .iter()
                .any(|t| matches!(fold(t).as_str(), "bos kap" | "spare box"));
            if tagged || n.kind == Kind::Container && crate::store::known_empty(&self.conn, n.id)? {
                spares.push(n);
            }
        }

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
                // A spare that stands inside this holder is not a bigger one for it, and a
                // holder laid out in a grid (a drawer of boxes) is not swapped for a box at all.
                let inside = |s: &Node| {
                    let mut cur = s.parent_id;
                    for _ in 0..10_000 {
                        match cur {
                            Some(p) if p == h.id => return true,
                            Some(p) => {
                                cur = all.iter().find(|n| n.id == p).and_then(|n| n.parent_id)
                            }
                            None => return false,
                        }
                    }
                    false
                };
                let swappable = crate::grid::grid_of(&self.conn, h.id)?.is_none();
                let bigger: Vec<Value> = spares
                    .iter()
                    .filter(|s| swappable && s.id != h.id && !inside(s))
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
            "alone": alone,
            "strays": strays,
            "full": full,
            "sparse": sparse,
            "mixed": mixed,
            "unknown_fill": unknown_fill,
            "declined": declined,
        }))
    }

    /// The person said no to moving `reference` out of where it is: regroup leaves it there,
    /// with the reason, until it is moved.
    pub fn regroup_decline(&mut self, reference: &str, why: Option<&str>) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let id = resolve(&tx, reference, false)?;
        let Some(holder) = crate::store::load(&tx, id)?.parent_id else {
            return Err(Error::Usage("it is in no holder to stay in".into()));
        };
        tx.execute(
            "INSERT INTO declines (node_id, holder_id, why, at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(node_id) DO UPDATE SET holder_id = excluded.holder_id,
               why = excluded.why, at = excluded.at",
            rusqlite::params![id, holder, why, crate::store::now()],
        )?;
        crate::store::event(&tx, id, "decline", json!({ "holder": holder, "why": why }))?;
        tx.commit()?;
        Ok(json!({
            "item": brief_json(&self.conn, id)?,
            "declined": { "holder": brief_json(&self.conn, holder)?, "why": why },
        }))
    }

    /// Takes a decline back: regroup may propose moving it again.
    pub fn regroup_allow(&mut self, reference: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let id = resolve(&tx, reference, false)?;
        if tx.execute("DELETE FROM declines WHERE node_id = ?1", [id])? > 0 {
            crate::store::event(&tx, id, "decline_cleared", json!({}))?;
        }
        tx.commit()?;
        Ok(json!({ "item": brief_json(&self.conn, id)?, "declined": null }))
    }
}

/// Declined moves: the thing, the holder it was to stay in, and why.
/// The room `id` is in (itself when it is one), walking up the tree.
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
fn declines_of(conn: &Connection) -> Result<HashMap<i64, (i64, Option<String>)>> {
    let mut stmt = conn.prepare("SELECT node_id, holder_id, why FROM declines")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
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

    /// A stem some word carries a plural on is a noun: it wins over a hardened reading (`bağı`
    /// is `bağ`, not `bak`) and reaches through stacked endings (`bacaklarında`), but only when
    /// something besides that one word attests it (`controller` makes no `control`).
    #[test]
    fn a_stem_written_with_a_plural_is_the_noun() {
        let words = "bak bağı bağları bağlarla bacaklarında bacağı blok bloğu controller";
        let surfaces: Vec<String> = terms(words).into_iter().map(|t| t.surface).collect();
        let lex = Lexicon::new(surfaces.iter().map(String::as_str));
        let k = |w: &str| key(&lex, w);
        assert_eq!(k("bağı"), "bag");
        assert_eq!(k("bağları"), "bag");
        assert_eq!(k("bağlarla"), "bag");
        assert_eq!(k("bacaklarında"), "bacak");
        // No plural of `blok` is written, so the written hardened form still wins.
        assert_eq!(k("bloğu"), "blok");
        assert_eq!(k("controller"), "controller");
    }

    /// A written word that the inventory inflects as a root of its own stays whole, though
    /// its letters read as a shorter written word plus an ending; endings that cannot follow
    /// the stem's sounds are never taken off; and none of it undoes the plural nouns above.
    #[test]
    fn a_word_that_is_a_root_of_its_own_stays_whole() {
        let words = "alt altın altında altta ünite üniteleri uni pens pense pensesi cıvata \
                     cıvatası cıva sap sabun var varta gün güneş türk türkiye box boxes \
                     dolap dolabı dolabın dolabının kutu kutuda kitap kitaptan \
                     bak bağı bağları bacaklarında bacağı blok bloğu";
        let surfaces: Vec<String> = terms(words).into_iter().map(|t| t.surface).collect();
        let lex = Lexicon::new(surfaces.iter().map(String::as_str));
        let k = |w: &str| key(&lex, w);
        // Roots the inventory inflects on their own: `altın`+`da`, `ünite`+`leri`,
        // `pense`+`si`, `cıvata`+`sı` cannot be a case ending followed by another ending.
        assert_eq!(k("altın"), "altin");
        assert_eq!(k("ünite"), "unite");
        assert_eq!(k("pense"), "pense");
        assert_eq!(k("cıvata"), "civata");
        // Endings that cannot follow the stem: `-ta` after a vowel or a voiced consonant,
        // `-un` after `a`, `-es` where English would write `-s`.
        assert_eq!(k("varta"), "varta");
        assert_eq!(k("sabun"), "sabun");
        assert_eq!(k("güneş"), "gunes");
        assert_eq!(k("türkiye"), "turkiye");
        assert_eq!(k("boxes"), "box");
        // Still cut: real inflections of written words.
        assert_eq!(k("altta"), "alt");
        assert_eq!(k("kutuda"), "kutu");
        assert_eq!(k("kitaptan"), "kitap");
        assert_eq!(k("dolabı"), "dolap");
        // `dolabının` reads as `dolabı`+`nın`, so it proves nothing about `dolabın`.
        assert_eq!(k("dolabın"), "dolap");
        // The plural nouns and the hardened written form keep working.
        assert_eq!(k("bağı"), "bag");
        assert_eq!(k("bağları"), "bag");
        assert_eq!(k("bacaklarında"), "bacak");
        assert_eq!(k("bloğu"), "blok");
    }

    /// Writes `word<TAB>key` for every word of `EV_WORDS` (first column), stemmed against those
    /// same words, to `EV_OUT`; `tools/measure/stems.py` scores it.
    #[test]
    #[ignore]
    fn dump_keys() {
        let words: Vec<String> = std::fs::read_to_string(std::env::var("EV_WORDS").unwrap())
            .unwrap()
            .lines()
            .filter_map(|l| l.split('\t').next().map(str::to_string))
            .collect();
        let lex = Lexicon::new(words.iter().map(String::as_str));
        let out: String = words
            .iter()
            .map(|w| format!("{w}\t{}\n", lex.key(w)))
            .collect();
        std::fs::write(std::env::var("EV_OUT").unwrap(), out).unwrap();
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
    fn compounds_and_colour_nouns_become_two_word_terms() {
        let words: Vec<String> =
            terms("hesap makine kontrol kalem ahşap vida yeşil led tabanca kutu kablo")
                .into_iter()
                .map(|t| t.surface)
                .collect();
        let lex = Lexicon::new(words.iter().map(String::as_str));
        let keys = |text: &str| -> Vec<(String, f64)> {
            lex.keyed(terms(text))
                .into_iter()
                .map(|t| (t.key, t.weight))
                .collect()
        };
        let has = |text: &str, k: &str| keys(text).iter().any(|(key, _)| key == k);
        // Indefinite noun compounds, hard and soft stems.
        assert!(has("hesap makinesi", "hesap+makine"));
        assert!(has("kontrol kalemi", "kontrol+kalem"));
        assert!(has("ahşap vidası", "ahsap+vida"));
        // A colour pairs with the noun it describes, and weighs little alone.
        assert!(has("yeşil LED", "yesil+led"));
        let lone = keys("yeşil tabanca");
        assert!(
            lone.iter().any(|(k, w)| k == "yesil" && *w < 0.5),
            "{lone:?}"
        );
        // Not a compound: the first word has an ending, or the second has more than the
        // possessive.
        assert!(!has("kutudaki kalemi", "kutu+kalem"));
        assert!(!has("kablo kutuları", "kablo+kutu"));
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
