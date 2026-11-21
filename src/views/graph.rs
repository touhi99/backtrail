use crate::config::cfg;
use crate::db::{rows, s};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

pub fn trail(c: &Connection) -> Value {
    json!({
        "threads": rows(c, "SELECT name, state, summary FROM thread_stat WHERE state!='unsorted'", &[]),
        "sources": rows(
            c,
            "SELECT id, thread, kind, title, substr(CASE WHEN kind='commit' THEN last_ts ELSE first_ts END,1,10) AS d
             FROM sources
             WHERE kind IN ('session','artifact','commit') AND thread IS NOT NULL AND first_ts IS NOT NULL
             ORDER BY d",
            &[],
        ),
        "items": rows(
            c,
            "SELECT thread, type, title, substr(coalesce(updated, created),1,10) AS d FROM item
             WHERE (type='step' AND state IN ('done','looks_done'))
                OR (type='decision' AND state='settled')
                OR (type='question' AND state IN ('answered','done'))",
            &[],
        ),
        "links": rows(
            c,
            "SELECT type, text, substr(asserted_at,1,10) AS d FROM fact
             WHERE type IN ('relation:built on','relation:spun out of','relation:forked from') AND status!='retracted'",
            &[],
        ),
    })
}

pub(super) fn thread_edges(c: &Connection) -> Vec<Value> {
    static CACHE: std::sync::Mutex<Option<(String, Vec<Value>)>> = std::sync::Mutex::new(None);
    const MIN_MENTIONS: i64 = 12;
    let version = format!(
        "{}-{}",
        crate::db::count(c, "PRAGMA data_version", &[]),
        c.total_changes()
    );
    let mut cache = CACHE.lock().expect("lock poisoned");
    if let Some((cached, edges)) = cache.as_ref()
        && *cached == version
    {
        return edges.clone();
    }
    let mut mentions: BTreeMap<(String, String), i64> = BTreeMap::new();
    for name in crate::db::strings(c, "SELECT name FROM threads WHERE name!='Unsorted'", &[]) {
        let mut names = vec![name.to_lowercase()];
        names.extend(cfg().aliases.get(&name).into_iter().flatten().cloned());
        let query = names
            .iter()
            .map(|a| format!("\"{}\"", a.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" OR ");
        let hits = rows(
            c,
            "SELECT s.thread AS other, COUNT(DISTINCT s.id) AS n
             FROM fts JOIN chunks ch ON ch.id=fts.rowid JOIN sources s ON s.id=ch.source_id
             WHERE fts MATCH ?1 AND s.thread IS NOT NULL AND s.thread NOT IN ('Unsorted', ?2)
             GROUP BY s.thread",
            &[&query, &name],
        );
        for h in hits {
            let other = s(&h, "other").to_string();
            let pair = if name <= other {
                (name.clone(), other)
            } else {
                (other, name.clone())
            };
            *mentions.entry(pair).or_insert(0) += h["n"].as_i64().unwrap_or(0);
        }
    }
    let mut edges: Vec<Value> = mentions
        .into_iter()
        .filter(|(_, n)| *n >= MIN_MENTIONS)
        .map(|((a, b), n)| json!({"a": format!("T:{a}"), "b": format!("T:{b}"), "kind": "mentions", "w": n}))
        .collect();
    edges.extend(rows(
        c,
        "SELECT 'T:'||ea.name AS a, 'T:'||eb.name AS b, substr(f.type, 10) AS kind, 1 AS w, f.quote, f.asserted_at AS ts,
                CASE f.status WHEN 'proposed' THEN 'proposed' END AS state, f.chunk_id, f.source, f.source_title, f.source_id
         FROM fact f
         JOIN claim_about sa ON sa.claim_id=f.id AND sa.role='subject' JOIN entities ea ON ea.id=sa.entity_id
         JOIN claim_about ob ON ob.claim_id=f.id AND ob.role='object' JOIN entities eb ON eb.id=ob.entity_id
         WHERE f.type IN ('relation:spun out of','relation:forked from','relation:built on','relation:replaced by')
           AND f.status!='retracted'",
        &[],
    ));
    *cache = Some((version, edges.clone()));
    edges
}

pub fn graph(c: &Connection) -> Value {
    let mut nodes: Vec<Value> = vec![];
    let mut edges = thread_edges(c);
    for t in rows(c, "SELECT * FROM thread_stat WHERE name!='Unsorted'", &[]) {
        nodes.push(json!({
            "id": format!("T:{}", s(&t, "name")), "kind": "thread", "label": t["name"], "state": t["state"],
            "n": t["sources"], "q": t["open_questions"], "area": t["area"],
            "first": t["session_first"], "last": t["session_last"],
        }));
    }
    for i in rows(
        c,
        "SELECT id, superseded_by FROM item WHERE superseded_by IS NOT NULL",
        &[],
    ) {
        edges.push(json!({
            "a": i["id"], "b": i["superseded_by"], "kind": "replaced by", "w": 1, "quote": null, "ts": null,
            "state": null, "chunk_id": null, "source": null, "source_title": null, "source_id": null,
        }));
    }
    let mut cited = HashSet::new();
    for i in rows(
        c,
        "SELECT id, type, title, state, thread, source_id FROM item WHERE state!='rejected' AND thread!='Unsorted'",
        &[],
    ) {
        let id = format!("I:{}", s(&i, "id"));
        nodes.push(json!({"id": id, "kind": i["type"], "label": i["title"], "state": i["state"], "thread": i["thread"]}));
        edges.push(
            json!({"a": id, "b": format!("T:{}", s(&i, "thread")), "kind": "belongs to", "w": 1}),
        );
        if let Some(source) = i["source_id"].as_i64() {
            cited.insert(source);
            edges.push(json!({"a": id, "b": format!("S:{source}"), "kind": "comes from", "w": 1}));
        }
    }
    for src in rows(
        c,
        "SELECT id, kind, title, thread FROM sources WHERE kind!='owner' AND ifnull(thread,'')!='Unsorted'",
        &[],
    ) {
        let id = src["id"].as_i64().unwrap_or(0);
        nodes.push(json!({
            "id": format!("S:{id}"), "kind": src["kind"], "label": src["title"], "thread": src["thread"],
            "cited": cited.contains(&id),
        }));
        if let Some(thread) = src["thread"].as_str() {
            edges.push(json!({"a": format!("S:{id}"), "b": format!("T:{thread}"), "kind": "belongs to", "w": 1}));
        }
    }
    json!({"nodes": nodes, "edges": edges})
}
