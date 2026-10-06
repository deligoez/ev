//! Words and stems, the holders things can go into, the placement rules, and `audit`.

use super::*;
use crate::error::{not_found, usage};

// ---------- placement: rules, suggest, audit (spec §14) ----------

/// Words too common to say two items are alike.
pub(crate) const STOPWORDS: &[&str] = &[
    "icin",
    "ile",
    "veya",
    "gibi",
    "olan",
    "adet",
    "kutu",
    "kutusu",
    "plastik",
    "beyaz",
    "siyah",
    "mavi",
    "sari",
    "turuncu",
    "kucuk",
    "buyuk",
    "uzun",
    "kisa",
    "the",
    "and",
    "for",
    "with",
    "birkac",
    "metal",
    "olabilir",
    "seffaf",
    "diger",
    "kirmizi",
    "yesil",
    "gri",
    "mor",
    "mini",
    "renkli",
    "cesitli",
    "karisik",
    "tane",
    "kutulu",
    "uzerinde",
    "poset",
    "posette",
    "posetli",
    "posetlerde",
    "eski",
    "yeni",
    "iki",
    "tek",
    "muhtemelen",
    "belirsiz",
    "net",
    "degil",
    "parca",
    "parcalar",
    "parcasi",
    "gorunumlu",
    "benzeri",
    "turu",
    "tipi",
    "set",
    "seti",
    "bir",
    "cok",
    "az",
    "icerik",
    "fotografta",
    "fotograftan",
    "sayida",
    "bordo",
    "erkek",
];

fn words(text: &str) -> Vec<String> {
    fold(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3 && !STOPWORDS.contains(w))
        .map(str::to_string)
        .collect()
}

/// Stems a folded Turkish word might have, longest first: the word itself, the word without a
/// possessive ending ("-sı/-su" after a vowel, "-ı/-u" after a consonant, or "-ları/-leri"),
/// and that without a plural ("-lar/-ler"). Every stem keeps at least three letters.
pub(crate) fn stem_candidates(word: &str) -> Vec<String> {
    let is_vowel = |c: Option<char>| c.is_some_and(|c| "aeiou".contains(c));
    let cut = |w: &str, s: &str| {
        (w.ends_with(s) && w.chars().count() >= s.chars().count() + 3)
            .then(|| w[..w.len() - s.len()].to_string())
    };
    let mut out = vec![word.to_string()];
    let possessive = ["lari", "leri"]
        .iter()
        .find_map(|s| cut(word, s))
        .or_else(|| {
            ["si", "su"]
                .iter()
                .find_map(|s| cut(word, s).filter(|w| is_vowel(w.chars().last())))
        })
        .or_else(|| {
            ["i", "u"]
                .iter()
                .find_map(|s| cut(word, s).filter(|w| !is_vowel(w.chars().last())))
        });
    let base = possessive.clone().unwrap_or_else(|| word.to_string());
    out.extend(possessive);
    out.extend(["lar", "ler"].iter().find_map(|s| cut(&base, s)));
    out.dedup();
    out
}

/// Groups word forms for `audit`. A word's key is its shortest candidate stem that is either a
/// word seen on its own or a stem shared by two different words, so "vidası" joins "vida",
/// "kablolar" and "kablosu" meet at "kablo", but "kutu" stays "kutu" and "kartuşu" (folded
/// "kartusu") never collapses into "kart". The key is only for grouping, never shown.
fn stem_keys(vocab: &BTreeSet<String>) -> HashMap<String, String> {
    let mut sources: HashMap<String, BTreeSet<&str>> = HashMap::new();
    for w in vocab {
        for c in stem_candidates(w) {
            sources.entry(c).or_default().insert(w);
        }
    }
    vocab
        .iter()
        .map(|w| {
            let key = stem_candidates(w)
                .into_iter()
                .rev()
                .find(|c| vocab.contains(c) || sources.get(c).is_some_and(|s| s.len() >= 2))
                .unwrap_or_else(|| w.clone());
            (w.clone(), key)
        })
        .collect()
}

pub(crate) fn live_nodes(conn: &Connection) -> Result<Vec<Node>> {
    super::load_live(conn)
}

/// The parking place (`temporary`) that `id` is, or stands inside, if any: the nearest one up
/// its chain of holders.
pub(crate) fn parking_of(by_id: &HashMap<i64, &Node>, id: i64) -> Option<i64> {
    let mut cur = Some(id);
    for _ in 0..MAX_DEPTH {
        let n = by_id.get(&cur?)?;
        if n.temporary {
            return Some(n.id);
        }
        cur = n.parent_id;
    }
    None
}

/// Anything something can be put into: every node that is not a home and is either not an
/// item or already holds something, and that stays: not set aside to leave, nor lost.
pub(crate) fn is_holder(n: &Node, has_children: &std::collections::HashSet<i64>) -> bool {
    n.kind != Kind::Home
        && (n.kind != Kind::Item || has_children.contains(&n.id))
        && n.state == crate::model::State::Active
        && !n.lost
}

pub(crate) fn holder_json(conn: &Connection, n: &Node, all: &[Node]) -> Result<Value> {
    let segments = path(conn, n.id)?;
    let inside: Vec<&str> = all
        .iter()
        .filter(|c| c.parent_id == Some(n.id) && c.kind == Kind::Item)
        .map(|c| c.name.as_str())
        .collect();
    let mut v = json!({
        "id": n.id,
        "code": n.code,
        "name": n.name,
        "kind": n.kind,
        "path_text": path_text(&segments),
        "items": item_total(conn, n.id)?,
        "sample": inside.iter().take(6).collect::<Vec<_>>(),
    });
    for (k, val) in [("theme", &n.theme), ("note", &n.note)] {
        if let Some(x) = val {
            v[k] = json!(x);
        }
    }
    if let Some(f) = n.fill {
        v["fill"] = json!(f);
    }
    if n.lost {
        v["lost"] = json!(true);
    }
    if n.temporary {
        v["temporary"] = json!(true);
    }
    // A grid holder says how many cells are still free, and which: room for a new box.
    if let Some(g) = crate::grid::grid_json(conn, n.id)? {
        v["grid"] = json!({ "cols": g["cols"], "rows": g["rows"], "free": g["free"] });
    }
    // A box in a grid says where in it.
    if let Some(c) = crate::grid::cells_of(conn, n.id)? {
        v["cells"] = json!(c.name());
    }
    Ok(v)
}

pub(crate) fn rules_json(conn: &Connection) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare("SELECT id, text FROM rules ORDER BY id")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(json!({ "id": r.get::<_, i64>(0)?, "text": r.get::<_, String>(1)? }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

impl Inventory {
    pub fn rule_add(&mut self, text: &str) -> Result<Value> {
        let t = text.trim();
        if t.is_empty() {
            return Err(usage("audit_rule_empty", Value::Null));
        }
        self.conn.execute(
            "INSERT INTO rules (text, created_at) VALUES (?1, ?2)",
            params![t, now()],
        )?;
        Ok(json!({ "rules": rules_json(&self.conn)? }))
    }

    pub fn rule_list(&self) -> Result<Value> {
        Ok(json!({ "rules": rules_json(&self.conn)? }))
    }

    pub fn rule_remove(&mut self, id: i64) -> Result<Value> {
        if self.conn.execute("DELETE FROM rules WHERE id = ?1", [id])? == 0 {
            return Err(not_found("audit_no_rule", json!({ "id": id })));
        }
        Ok(json!({ "rules": rules_json(&self.conn)? }))
    }

    /// Where the inventory could be tidier: alike things split across places, holders without
    /// a theme, and items lying directly in a room or on furniture.
    pub fn audit(&self) -> Result<Value> {
        let all = live_nodes(&self.conn)?;
        let by_id: HashMap<i64, &Node> = all.iter().map(|n| (n.id, n)).collect();
        let has_children: std::collections::HashSet<i64> =
            all.iter().filter_map(|n| n.parent_id).collect();
        // Grouped by stem, so "vida", "vidası" and "vidalar" are one row; the row is named by
        // its shortest surface form and lists every form it merged.
        type Places = std::collections::BTreeMap<i64, Vec<String>>;
        let mut spread: std::collections::BTreeMap<String, (Places, BTreeSet<String>)> =
            Default::default();
        // A thing kept in several places is spread on purpose (spec/portions.md §6): it counts
        // in one place only, its first portion's.
        let mut things = std::collections::HashSet::new();
        let item_words: Vec<(i64, &str, Vec<String>)> = all
            .iter()
            .filter(|n| n.kind == Kind::Item)
            .filter(|n| n.thing.is_none_or(|t| things.insert(t)))
            .filter_map(|n| {
                let p = n.parent_id?;
                let mut ws = words(&n.name);
                ws.extend(n.tags.iter().flat_map(|t| words(t)));
                ws.retain(|w| w.chars().count() >= 4);
                Some((p, n.name.as_str(), ws))
            })
            .collect();
        let vocab: BTreeSet<String> = item_words
            .iter()
            .flat_map(|(_, _, ws)| ws.iter().cloned())
            .collect();
        let keys = stem_keys(&vocab);
        for (p, name, ws) in item_words {
            let mut seen = BTreeSet::new();
            for w in ws {
                let key = keys.get(&w).cloned().unwrap_or_else(|| w.clone());
                let entry = spread.entry(key.clone()).or_default();
                entry.1.insert(w);
                if seen.insert(key) {
                    entry.0.entry(p).or_default().push(name.to_string());
                }
            }
        }
        let mut spread: Vec<Value> = spread
            .into_values()
            .filter(|(places, _)| (2..=8).contains(&places.len()))
            .map(|(places, forms)| {
                let list = places
                    .iter()
                    .map(|(p, names)| {
                        let segs = path(&self.conn, *p)?;
                        Ok(json!({ "path_text": path_text(&segs), "items": names }))
                    })
                    .collect::<Result<Vec<_>>>()?;
                let word = forms
                    .iter()
                    .min_by_key(|f| (f.chars().count(), (*f).clone()))
                    .cloned()
                    .unwrap_or_default();
                Ok(json!({ "word": word, "forms": forms, "places": list }))
            })
            .collect::<Result<Vec<_>>>()?;
        spread.sort_by_key(|v| std::cmp::Reverse(v["places"].as_array().map_or(0, Vec::len)));
        spread.truncate(40);
        let no_theme = all
            .iter()
            .filter(|n| {
                is_holder(n, &has_children)
                    && n.kind != Kind::Room
                    && n.theme.is_none()
                    && n.state != State::Candidate
            })
            .filter(|n| {
                all.iter()
                    .any(|c| c.parent_id == Some(n.id) && c.kind == Kind::Item)
            })
            .map(|n| brief_json(&self.conn, n.id))
            .collect::<Result<Vec<_>>>()?;
        let loose = all
            .iter()
            .filter(|n| n.kind == Kind::Item)
            .filter(|n| {
                n.parent_id
                    .and_then(|p| by_id.get(&p))
                    .is_some_and(|p| matches!(p.kind, Kind::Home | Kind::Room | Kind::Furniture))
            })
            .map(|n| brief_json(&self.conn, n.id))
            .collect::<Result<Vec<_>>>()?;
        // A holder's name that says its size while the field does not (or says another): the
        // name is for people, the field is what crops and bigger-box offers read.
        let size_drift = all
            .iter()
            .filter(|n| n.kind == Kind::Container)
            .filter_map(|n| {
                let named = size_in_name(&n.name)?;
                (n.size.as_deref() != Some(named.as_str())).then_some((n, named))
            })
            .map(|(n, named)| {
                let mut b = brief_json(&self.conn, n.id)?;
                b["name_size"] = json!(named);
                b["size"] = json!(n.size);
                Ok(b)
            })
            .collect::<Result<Vec<_>>>()?;
        // The same name in more than one place, not already one thing: maybe one thing recorded
        // twice, which `ev join` makes one (spec/portions.md §6). Each name once, with its records.
        let mut by_name: std::collections::BTreeMap<String, Vec<&Node>> = Default::default();
        for n in all
            .iter()
            .filter(|n| n.kind == Kind::Item && n.parent_id.is_some())
        {
            by_name.entry(fold(&n.name)).or_default().push(n);
        }
        let same_name = by_name
            .into_values()
            .filter(|ns| {
                let places: BTreeSet<i64> = ns.iter().filter_map(|n| n.parent_id).collect();
                let things: BTreeSet<Option<i64>> = ns.iter().map(|n| n.thing).collect();
                places.len() > 1 && !(things.len() == 1 && ns[0].thing.is_some())
            })
            .map(|ns| {
                let nodes = ns
                    .iter()
                    .map(|n| brief_json(&self.conn, n.id))
                    .collect::<Result<Vec<_>>>()?;
                Ok(json!({ "name": ns[0].name, "nodes": nodes }))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({
            "spread": spread, "no_theme": no_theme, "loose": loose,
            "size_drift": size_drift, "same_name": same_name,
        }))
    }
}
