use super::gist;
use super::graph::thread_edges;
use crate::db::{rows, s};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

const FOCUS_NOTES: usize = 14;

fn wikilinks(c: &Connection) -> HashMap<i64, HashSet<i64>> {
    let by_title: HashMap<String, i64> = rows(
        c,
        "SELECT id, title FROM sources WHERE kind IN ('note','artifact')",
        &[],
    )
    .iter()
    .map(|r| (s(r, "title").to_lowercase(), r["id"].as_i64().unwrap_or(0)))
    .collect();
    let link = regex::Regex::new(r"\[\[([^\]|#]+)").expect("regex literal is valid");
    let mut links: HashMap<i64, HashSet<i64>> = HashMap::new();
    let passages = rows(
        c,
        "SELECT ch.source_id, ch.text FROM chunks ch JOIN sources s ON s.id=ch.source_id
         WHERE s.kind='note' AND ch.text LIKE '%[[%'",
        &[],
    );
    for p in passages {
        let from = p["source_id"].as_i64().unwrap_or(0);
        for m in link.captures_iter(s(&p, "text")) {
            let title = m[1].trim().rsplit('/').next().unwrap_or("").to_lowercase();
            if let Some(&to) = by_title.get(&title)
                && to != from
            {
                links.entry(from).or_default().insert(to);
            }
        }
    }
    links
}

pub fn focus(c: &Connection, name: &str) -> Value {
    let thread = rows(
        c,
        "SELECT *, sources AS n FROM thread_stat WHERE name=?1",
        &[&name],
    );
    let items = rows(
        c,
        "SELECT * FROM item WHERE thread=?1 AND state!='rejected' ORDER BY created DESC",
        &[&name],
    );
    let documents = rows(
        c,
        "SELECT * FROM source_stat WHERE thread=?1 AND kind IN ('note','artifact') ORDER BY cited DESC, last_ts DESC",
        &[&name],
    );
    let links = wikilinks(c);
    let shown: Vec<i64> = documents
        .iter()
        .take(FOCUS_NOTES)
        .filter_map(|d| d["id"].as_i64())
        .collect();
    let mut notes = vec![];
    for d in documents.iter().take(FOCUS_NOTES) {
        let id = d["id"].as_i64().unwrap_or(0);
        let gist = crate::db::strings(
            c,
            "SELECT text FROM chunks WHERE source_id=?1 ORDER BY seq LIMIT 5",
            &[&id],
        )
        .iter()
        .map(|text| gist(text))
        .find(|g| g.chars().count() >= 40)
        .unwrap_or_default();
        let outline = rows(
            c,
            "SELECT locator AS heading, MIN(id) AS chunk_id FROM chunks
             WHERE source_id=?1 AND locator NOT IN ('#top','§top') GROUP BY locator ORDER BY MIN(seq) LIMIT 9",
            &[&id],
        );
        let links_out = links.get(&id).map_or(0, std::collections::HashSet::len);
        let links_in = links
            .iter()
            .filter(|(from, to)| **from != id && to.contains(&id))
            .count();
        notes.push(json!({
            "id": id, "kind": d["kind"], "title": d["title"], "gist": gist, "outline": outline, "cited": d["cited"],
            "last_ts": d["last_ts"], "passages": d["nchunks"], "links": links_out + links_in,
        }));
    }
    let mut note_edges = vec![];
    for a in &shown {
        let mut targets: Vec<&i64> = links
            .get(a)
            .into_iter()
            .flatten()
            .filter(|b| shown.contains(b))
            .collect();
        targets.sort();
        note_edges.extend(targets.into_iter().map(|b| json!({"a": a, "b": b})));
    }
    let me = format!("T:{name}");
    let mut linked = vec![];
    for e in thread_edges(c) {
        let (a, b, kind) = (s(&e, "a"), s(&e, "b"), s(&e, "kind"));
        if (kind == "mentions" || kind == "built on") && (a == me || b == me) {
            let other = if a == me { b } else { a };
            linked.push(json!({
                "name": other.trim_start_matches("T:"), "kind": kind, "w": e["w"],
                "dir": if a == me { "out" } else { "in" },
            }));
        }
    }
    json!({
        "thread": thread.first(),
        "items": items,
        "notes": notes,
        "note_edges": note_edges,
        "wikilinks_resolved": links.values().map(std::collections::HashSet::len).sum::<usize>(),
        "counts": rows(c, "SELECT kind, COUNT(*) AS n FROM sources WHERE thread=?1 GROUP BY kind", &[&name]),
        "sessions": rows(
            c,
            "SELECT id, title, last_ts, nchunks FROM sources WHERE thread=?1 AND kind='session' ORDER BY last_ts DESC LIMIT 6",
            &[&name],
        ),
        "linked": linked,
        "hidden_notes": documents.len().saturating_sub(FOCUS_NOTES),
    })
}
