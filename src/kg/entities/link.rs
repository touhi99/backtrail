use crate::db::norm;
use crate::kg::has_word;
use rusqlite::{Connection, params};

pub fn link_claim(c: &Connection, id: &str, thread: &str, hay: &str) -> usize {
    let ents: Vec<(String, String, Vec<String>)> = {
        let mut st = c
            .prepare("SELECT e.id, e.name, ( SELECT group_concat(alias, char(31)) FROM entity_aliases a WHERE a.entity_id=e.id ) FROM entities e")
            .expect("prepare query");
        st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?.unwrap_or_default(),
            ))
        })
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .map(|(i, n, a)| (i, n, a.split('\u{1f}').map(String::from).collect()))
        .collect()
    };
    let hay = norm(hay);
    let mut pairs: Vec<(&str, &str)> = ents
        .iter()
        .flat_map(|(eid, _, al)| {
            al.iter()
                .filter(|a| !a.is_empty())
                .map(move |a| (a.as_str(), eid.as_str()))
        })
        .collect();
    pairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then(a.0.cmp(b.0)));
    let mut masked = hay;
    let mut by_alias: std::collections::HashSet<&str> = std::collections::HashSet::default();
    let mut i = 0;
    while i < pairs.len() {
        let alias = pairs[i].0;
        let hit = has_word(&masked, alias);
        while i < pairs.len() && pairs[i].0 == alias {
            if hit {
                by_alias.insert(pairs[i].1);
            }
            i += 1;
        }
        if hit {
            masked = mask_word(&masked, alias);
        }
    }
    let mut n = 0;
    for (eid, name, _) in &ents {
        if norm(name) == norm(thread) || by_alias.contains(eid.as_str()) {
            c.execute(
                "INSERT OR IGNORE INTO claim_about(claim_id,entity_id,role) VALUES(?1,?2,'subject')",
                params![id, eid],
            )
            .expect("write to database");
            n += 1;
        }
    }
    n
}

fn mask_word(text: &str, word: &str) -> String {
    let mut out = String::new();
    let mut from = 0;
    while let Some(i) = text[from..].find(word) {
        let (start, end) = (from + i, from + i + word.len());
        let before = text[..start]
            .chars()
            .next_back()
            .is_none_or(|ch| !ch.is_alphanumeric());
        let after = text[end..]
            .chars()
            .next()
            .is_none_or(|ch| !ch.is_alphanumeric());
        out.push_str(&text[from..start]);
        if before && after {
            out.push_str(&" ".repeat(word.len()));
        } else {
            out.push_str(&text[start..end]);
        }
        from = end;
    }
    out.push_str(&text[from..]);
    out
}

pub fn relink_all(c: &Connection) -> usize {
    let rows: Vec<(String, String, String)> = c
        .prepare("SELECT id, ifnull(thread,''), title||' '||ifnull(body,'')||' '||ifnull(quote,'') FROM claims")
        .expect("prepare query")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .collect();
    rows.iter()
        .map(|(id, th, hay)| link_claim(c, id, th, hay))
        .sum()
}
