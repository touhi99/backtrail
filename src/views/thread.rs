use super::{decided, gist};
use crate::config::cfg;
use crate::db::{rows, s};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

pub fn thread(c: &Connection, name: &str) -> Value {
    let thread = rows(c, "SELECT * FROM thread_stat WHERE name=?1", &[&name]);
    let mut items = rows(
        c,
        "SELECT * FROM item WHERE thread=?1 ORDER BY created",
        &[&name],
    );
    let sources = rows(
        c,
        "SELECT * FROM source_stat WHERE thread=?1 ORDER BY last_ts DESC",
        &[&name],
    );
    let claims = rows(
        c,
        "SELECT f.id, f.type, f.status, f.speaker, substr(f.asserted_at,1,10) AS d, f.text, f.quote AS q,
                (SELECT title FROM claims WHERE id=f.superseded_by) AS newer
         FROM fact f
         WHERE f.type NOT LIKE 'relation:%' AND f.status!='retracted'
           AND f.id IN (SELECT a.claim_id FROM claim_about a JOIN entities e ON e.id=a.entity_id WHERE e.name=?1)
           AND f.source_id IN (SELECT id FROM sources WHERE thread=?1)
         ORDER BY f.asserted_at",
        &[&name],
    );
    let children = rows(
        c,
        "SELECT * FROM thread_stat WHERE parent=?1 ORDER BY last DESC",
        &[&name],
    );
    let kids: Vec<String> = children.iter().map(|k| s(k, "name").to_string()).collect();
    let owned: HashSet<String> = crate::prs::homes(c)
        .into_iter()
        .filter(|h| h.thread == name || kids.contains(&h.thread))
        .map(|h| h.repo)
        .collect();
    let mut seen_prs = HashSet::new();
    let prs: Vec<Value> = rows(
        c,
        "SELECT repo, number, url, ts, source, ifnull(state,'') AS state, ifnull(merged_on,'') AS merged_on,
                ifnull(title,'') AS title, opened_here
         FROM session_pr
         ORDER BY ts DESC",
        &[],
    )
    .into_iter()
    .filter(|p| owned.contains(s(p, "repo")))
    .filter(|p| seen_prs.insert(s(p, "url").to_string()))
    .take(20)
    .collect();
    let mut also: HashMap<String, Vec<Value>> = HashMap::new();
    let mut blocked: HashMap<String, Vec<Value>> = HashMap::new();
    for e in rows(
        c,
        "SELECT k.a AS a, k.kind AS kind, k.reason AS reason, b.id AS id, b.title AS title,
                b.body AS body, b.state AS state
         FROM claim_edges k JOIN claims b ON b.id=k.b
         WHERE k.a IN (SELECT id FROM claims WHERE thread=?1)
           AND k.kind IN ('considered','depends_on')
         ORDER BY b.title",
        &[&name],
    ) {
        let a = s(&e, "a").to_string();
        let v = json!({"id": s(&e, "id"), "title": s(&e, "title"), "body": s(&e, "body"),
                       "state": s(&e, "state"), "reason": s(&e, "reason")});
        if s(&e, "kind") == "considered" {
            also.entry(a).or_default().push(v);
        } else {
            blocked.entry(a).or_default().push(v);
        }
    }
    for it in &mut items {
        let Some(id) = it["id"].as_str().map(str::to_string) else {
            continue;
        };
        if let Some(v) = also.remove(&id) {
            it["also"] = json!(v);
        }
        if let Some(v) = blocked.remove(&id) {
            it["blocked_by"] = json!(v);
        }
    }
    json!({"thread": thread.first(), "children": children, "items": items, "sources": sources, "claims": claims, "decided": decided(c, name, 8),
           "artifacts": thread_artifacts(c, name), "prs": prs, "agent": agent_start(name)})
}

fn agent_start(thread: &str) -> Value {
    let brief = format!(
        "\"$(backtrail brief --thread '{}')\"",
        thread.replace('\'', "'\\''")
    );
    let (name, start) = match cfg().agent.as_str() {
        "gemini" => ("Gemini CLI", format!("gemini -i {brief}")),
        "codex" => ("Codex", format!("codex {brief}")),
        "antigravity" => (
            "Antigravity",
            format!("backtrail brief --thread '{thread}'"),
        ),
        "command" => (
            "the coding agent",
            format!("backtrail brief --thread '{thread}'"),
        ),
        _ => ("Claude Code", format!("claude {brief}")),
    };
    json!({"name": name, "start": start})
}

fn thread_artifacts(c: &Connection, name: &str) -> Vec<Value> {
    let link = regex::Regex::new(r"claude\.ai/artifact/([A-Za-z0-9]{20,24})")
        .expect("regex literal is valid");
    let mut found: HashMap<String, (String, String, Option<i64>)> = HashMap::new();
    for r in rows(
        c,
        "SELECT id, substr(ref, 10) AS aid, title, last_ts FROM sources WHERE kind='artifact' AND thread=?1",
        &[&name],
    ) {
        found.insert(
            s(&r, "aid").to_string(),
            (
                s(&r, "title").to_string(),
                s(&r, "last_ts").to_string(),
                r["id"].as_i64(),
            ),
        );
    }
    for r in rows(
        c,
        "SELECT ch.text AS text, s.last_ts AS ts FROM chunks ch JOIN sources s ON s.id=ch.source_id
         WHERE s.thread=?1 AND s.kind!='artifact' AND ch.text LIKE '%claude.ai/artifact/%'",
        &[&name],
    ) {
        for m in link.captures_iter(s(&r, "text")) {
            let e = found
                .entry(m[1].to_string())
                .or_insert((String::new(), String::new(), None));
            if s(&r, "ts") > e.1.as_str() && e.2.is_none() {
                e.1 = s(&r, "ts").to_string();
            }
        }
    }
    let mut out: Vec<Value> = found
        .into_iter()
        .map(|(id, (title, ts, source_id))| {
            let meta = rows(
                c,
                "SELECT title, updated FROM artifact_meta WHERE id=?1",
                &[&id],
            );
            let (title, ts, source_id) = if source_id.is_some() {
                (title, ts, source_id)
            } else {
                let copy = rows(
                    c,
                    "SELECT id, title, last_ts FROM sources WHERE kind='artifact' AND ref=?1",
                    &[&format!("artifact:{id}")],
                );
                match copy.first() {
                    Some(r) => (
                        s(r, "title").to_string(),
                        s(r, "last_ts").to_string(),
                        r["id"].as_i64(),
                    ),
                    None => (title, ts, None),
                }
            };
            let title = if title.is_empty() {
                meta.first()
                    .map(|m| s(m, "title").to_string())
                    .unwrap_or_default()
            } else {
                title
            };
            let ts = meta
                .first()
                .map(|m| s(m, "updated").to_string())
                .filter(|u| !u.is_empty())
                .unwrap_or(ts);
            let about = source_id
                .map(|sid| {
                    crate::db::strings(
                        c,
                        "SELECT text FROM chunks WHERE source_id=?1 ORDER BY seq LIMIT 5",
                        &[&sid],
                    )
                    .iter()
                    .map(|text| gist(text))
                    .find(|g| g.chars().count() >= 40)
                    .unwrap_or_default()
                })
                .unwrap_or_default();
            json!({"id": id, "title": title, "ts": ts, "source_id": source_id, "about": about,
                   "url": format!("https://claude.ai/artifact/{id}")})
        })
        .collect();
    out.sort_by(|a, b| s(b, "ts").cmp(s(a, "ts")));
    out
}
