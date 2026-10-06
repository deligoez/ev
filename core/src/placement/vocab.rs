//! The words the inventory keeps for placing: facets that keep kinds apart, the themes read
//! from a holder's contents, and synonym groups.

use super::*;

/// Facets (spec §26): kinds of things kept apart, such as modules and bare parts or novels and
/// technical books. A holder is in a facet by carrying the facet's name as a tag (or by being
/// inside one that does); a thing is in a facet by its own tag, else by a facet word in its
/// name, else by where it is. Placement never proposes a holder of another facet.
pub(crate) struct Facets {
    /// (folded name, name as written, the stems that name it)
    list: Vec<(String, String, HashSet<String>)>,
}

impl Facets {
    pub(crate) fn load(conn: &Connection, index: &Index) -> Result<Facets> {
        let mut stmt = conn.prepare("SELECT name, words FROM facets ORDER BY name")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let list = rows
            .into_iter()
            .map(|(name, words)| {
                // Both the stems and the words as written, folded, so a word matches whether
                // the inventory stems it or not.
                let keys = index
                    .keyed(terms(&format!("{name}, {words}")))
                    .into_iter()
                    .filter(|t| !t.key.contains('+'))
                    .flat_map(|t| [t.key, t.surface])
                    .collect();
                (fold(&name), name, keys)
            })
            .collect();
        Ok(Facets { list })
    }

    fn tagged(&self, n: &Node) -> HashSet<String> {
        n.tags
            .iter()
            .map(|t| fold(t))
            .filter(|t| self.list.iter().any(|f| &f.0 == t))
            .collect()
    }

    /// Facets a word names: its stem, or any form it could be cut to, is a facet word. Every
    /// candidate counts, not only the chosen stem, so `modülü` finds `modül` even in an
    /// inventory that never writes the bare form.
    pub(super) fn worded(&self, words: &[Term]) -> HashSet<String> {
        self.list
            .iter()
            .filter(|f| {
                words.iter().any(|t| {
                    f.2.contains(&t.key) || candidates(&t.surface).iter().any(|c| f.2.contains(c))
                })
            })
            .map(|f| f.0.clone())
            .collect()
    }

    /// A holder's facets: its own tags, else its nearest ancestor's.
    pub(super) fn of_holder(&self, by_id: &HashMap<i64, &Node>, id: i64) -> HashSet<String> {
        let mut cur = Some(id);
        while let Some(n) = cur.and_then(|i| by_id.get(&i)) {
            let f = self.tagged(n);
            if !f.is_empty() {
                return f;
            }
            cur = n.parent_id;
        }
        HashSet::new()
    }

    /// A thing's facets: its own tags, else the facet words in its name, else where it is.
    pub(super) fn of_thing(
        &self,
        index: &Index,
        by_id: &HashMap<i64, &Node>,
        n: &Node,
    ) -> HashSet<String> {
        let own = self.tagged(n);
        if !own.is_empty() {
            return own;
        }
        let worded = self.worded(&index.keyed(terms(&n.name)));
        if !worded.is_empty() {
            return worded;
        }
        n.parent_id
            .map(|p| self.of_holder(by_id, p))
            .unwrap_or_default()
    }

    /// A thing of one facet does not go into a holder of another; either side without a facet
    /// is free.
    pub(super) fn clash(thing: &HashSet<String>, holder: &HashSet<String>) -> bool {
        !thing.is_empty() && !holder.is_empty() && thing.is_disjoint(holder)
    }

    pub(super) fn names(&self, set: &HashSet<String>) -> Vec<String> {
        self.list
            .iter()
            .filter(|f| set.contains(&f.0))
            .map(|f| f.1.clone())
            .collect()
    }
}

/// How many of a holder's words and contents `ev themes` shows.
const THEME_WORDS: usize = 6;
const THEME_SAMPLE: usize = 8;

impl Inventory {
    /// Containers and furniture with things in them and no theme, each with what a theme
    /// could be read from: its contents, the words those share (rarer words first), and the
    /// themed holder they read most like. Writing the theme is a judgement, the agent's with
    /// the person; this only gathers the evidence, the way a table of contents is summarised
    /// from its sections.
    pub fn themes(&self, reference: Option<&str>) -> Result<Value> {
        let all = live_nodes(&self.conn)?;
        let has_children: HashSet<i64> = all.iter().filter_map(|n| n.parent_id).collect();
        let scope: HashSet<i64> = match reference {
            Some(r) => subtree(&all, resolve(&self.conn, r, false)?),
            None => all.iter().map(|n| n.id).collect(),
        };
        let index = self.word_index(&all)?;
        let unthemed = |n: &Node| n.theme.as_deref().is_none_or(|t| t.trim().is_empty());
        let mut out = Vec::new();
        for h in all.iter().filter(|n| {
            scope.contains(&n.id)
                && is_holder(n, &has_children)
                // A kit recorded as an item is named for what it is; only places get themes.
                && matches!(n.kind, Kind::Container | Kind::Furniture)
                && unthemed(n)
                && n.state != crate::model::State::Candidate
        }) {
            let things: Vec<&Node> = all
                .iter()
                .filter(|c| c.parent_id == Some(h.id) && c.kind == Kind::Item)
                .collect();
            if things.is_empty() {
                continue;
            }
            // Per stem: how many things name it, and the form it is written in most.
            let mut words: BTreeMap<String, (usize, BTreeMap<String, usize>)> = BTreeMap::new();
            for c in &things {
                let mut seen = HashSet::new();
                for token in c.name.split(|ch: char| !ch.is_alphanumeric()) {
                    let folded = fold(token);
                    if folded.chars().count() < 3
                        || folded.chars().any(|ch| ch.is_ascii_digit())
                        || FILLER.contains(&folded.as_str())
                    {
                        continue;
                    }
                    let key = index.lex.key(&folded);
                    if COLORS.contains(&key.as_str()) {
                        continue;
                    }
                    let entry = words.entry(key.clone()).or_default();
                    if seen.insert(key) {
                        entry.0 += 1;
                    }
                    *entry.1.entry(token.to_string()).or_default() += 1;
                }
            }
            let mut ranked: Vec<(String, usize, f64, String)> = words
                .into_iter()
                .map(|(key, (count, forms))| {
                    let form = forms
                        .into_iter()
                        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
                        .map_or_else(|| key.clone(), |f| f.0);
                    let score = count as f64 * index.idf(&key);
                    (key, count, score, form)
                })
                .collect();
            ranked.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.0.cmp(&b.0)));
            ranked.truncate(THEME_WORDS);

            // The themed holder these words read most like, outside this holder.
            let query: Vec<Term> = ranked
                .iter()
                .map(|(key, count, _, form)| Term {
                    key: key.clone(),
                    surface: form.clone(),
                    weight: *count as f64,
                })
                .collect();
            let mut skip = subtree(&all, h.id);
            skip.extend(all.iter().filter(|n| unthemed(n)).map(|n| n.id));
            let like = match index.score(&query, &HashSet::new(), &skip).first() {
                Some(s) if s.score >= CLEAR => {
                    let mut v = brief_json(&self.conn, s.id)?;
                    v["theme"] = json!(
                        all.iter()
                            .find(|n| n.id == s.id)
                            .and_then(|n| n.theme.clone())
                    );
                    v["score"] = json!(round(s.score));
                    v
                }
                _ => Value::Null,
            };
            out.push(json!({
                "holder": brief_json(&self.conn, h.id)?,
                "things": things.len(),
                "words": ranked
                    .iter()
                    .map(|(_, count, _, form)| json!({ "word": form, "things": count }))
                    .collect::<Vec<_>>(),
                "contents": things.iter().take(THEME_SAMPLE).map(|c| c.name.as_str()).collect::<Vec<_>>(),
                "like": like,
            }));
        }
        out.sort_by(|a, b| {
            b["things"]
                .as_u64()
                .cmp(&a["things"].as_u64())
                .then(a["holder"]["id"].as_i64().cmp(&b["holder"]["id"].as_i64()))
        });
        Ok(json!({ "themes": out }))
    }
}

impl Inventory {
    /// Adds a facet, or replaces its words: a kind of thing kept apart from the others.
    /// Holders join it by carrying `name` as a tag; `words` (comma-separated) also tell a
    /// thing's facet from its name, the name itself always among them.
    pub fn facet_add(&mut self, name: &str, words: Option<&str>) -> Result<Value> {
        let name = name.trim().to_lowercase();
        if terms(&name).is_empty() {
            return Err(usage("vocab_facet_name_bad", Value::Null));
        }
        let words: Vec<&str> = words
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|w| !w.is_empty())
            .collect();
        self.conn.execute(
            "INSERT INTO facets (name, words, created_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(name) DO UPDATE SET words = excluded.words",
            rusqlite::params![name, words.join(", "), crate::store::now()],
        )?;
        self.facet_list()
    }

    /// Every facet, its words, and the holders tagged with it.
    pub fn facet_list(&self) -> Result<Value> {
        let all = live_nodes(&self.conn)?;
        let mut stmt = self
            .conn
            .prepare("SELECT name, words FROM facets ORDER BY name")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let list = rows
            .into_iter()
            .map(|(name, words)| {
                let key = fold(&name);
                let holders = all
                    .iter()
                    .filter(|n| n.tags.iter().any(|t| fold(t) == key))
                    .map(|n| brief_json(&self.conn, n.id))
                    .collect::<Result<Vec<_>>>()?;
                Ok(json!({ "name": name, "words": words, "holders": holders }))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({ "facets": list }))
    }

    /// Removes a facet; the tags stay on the holders, they just stop keeping things apart.
    pub fn facet_remove(&mut self, name: &str) -> Result<Value> {
        let name = name.trim().to_lowercase();
        if self
            .conn
            .execute("DELETE FROM facets WHERE name = ?1", [&name])?
            == 0
        {
            return Err(not_found("vocab_no_facet", json!({ "name": name })));
        }
        self.facet_list()
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
            return Err(usage("vocab_synonyms_too_few", Value::Null));
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
            return Err(not_found("vocab_no_synonym_group", json!({ "id": id })));
        }
        self.synonym_list()
    }
}
