//! Resolving proposed items against what the store already knows. The binary finds candidates (entity
//! neighbourhood, shared word stems, nearest passages) and records a verdict; a model call decides, either the
//! Claude session that files the item or `skill/resolve.sh`. A resolved item stays on file: a question becomes
//! `answered`, a step or decision becomes `superseded`, and `superseded_by` names the item that settled it.
use crate::clock::now;
use crate::db::{count, logw, rows, s, strings};
use crate::items::COMMON_WORDS;
use rusqlite::{Connection, params};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

const LIVE: &str = "'settled','done','answered','looks_done','open','doing'";

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

const STOP: &[&str] = &[
    "the", "and", "for", "with", "from", "that", "this", "not", "are", "was", "but", "its", "has", "have", "owner",
    "asked", "wants", "want", "should", "would", "could", "which", "what", "how", "when", "only", "into", "than",
    "then", "them", "they", "will", "can", "use", "new", "chose", "decided", "said", "also", "does", "did", "all",
    "one", "any", "out", "off", "get", "set", "make", "made", "keep", "now", "for", "who", "why", "where",
];

/// The first five letters of each word of three letters or more, stop words left out: "removed" and "remove" count
/// as one, and "menu" and "bar" still count.
fn stems(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|w| w.len() >= 3 && !STOP.contains(w) && !COMMON_WORDS.contains(w))
        .map(|w| w.chars().take(5).collect())
        .collect()
}

pub struct Candidate {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub thread: String,
    pub title: String,
    pub score: usize,
}

/// Existing items that may already settle `id`: those sharing an entity, a word stem or a nearby passage with it.
/// Best first. An item that only shares a broad entity with it, and nothing else, is not listed.
pub fn candidates(c: &Connection, id: &str, limit: usize) -> Vec<Candidate> {
    let me = rows(
        c,
        "SELECT type, title, ifnull(body,'') AS body FROM claims WHERE id=?1",
        &[&id],
    );
    let Some(me) = me.first() else { return vec![] };
    let text = format!("{} {}", s(me, "title"), s(me, "body"));
    let mine = stems(&text);
    let near: HashMap<i64, usize> = crate::embed::nearest(c, &text, 12)
        .map(|v| {
            v.into_iter()
                .enumerate()
                .map(|(i, (chunk, _))| (chunk, 12 - i))
                .collect()
        })
        .unwrap_or_default();
    let others = rows(
        c,
        &format!(
            "SELECT o.id, o.type, o.state, ifnull(o.thread,'') AS thread, o.title, ifnull(o.chunk_id,0) AS chunk,
                    (SELECT COUNT(*) FROM claim_about a JOIN claim_about b ON b.entity_id=a.entity_id
                      WHERE a.claim_id=?1 AND b.claim_id=o.id) AS shared
             FROM claims o
             WHERE o.id!=?1 AND o.type IN ('decision','step','question') AND o.state IN ({LIVE})"
        ),
        &[&id],
    )
    ;
    // A shared word counts for more when few items use it: "backtrail" says little, "menu" says a lot.
    let docs: Vec<HashSet<String>> = others.iter().map(|o| stems(s(o, "title"))).collect();
    let mut df: HashMap<&str, usize> = HashMap::new();
    for d in &docs {
        for w in d {
            *df.entry(w.as_str()).or_insert(0) += 1;
        }
    }
    let n = docs.len().max(1) as f64;
    let mut scored: Vec<Candidate> = others
    .iter()
    .zip(&docs)
    .filter_map(|(o, theirs)| {
        let overlap: f64 = mine
            .intersection(theirs)
            .map(|w| (n / df.get(w.as_str()).copied().unwrap_or(1) as f64).ln())
            .sum();
        let vec = near
            .get(&o["chunk"].as_i64().unwrap_or(0))
            .copied()
            .unwrap_or(0);
        let shared = o["shared"].as_i64().unwrap_or(0) as usize;
        if overlap < 2.0 && vec == 0 {
            return None;
        }
        Some(Candidate {
            id: s(o, "id").into(),
            kind: s(o, "type").into(),
            state: s(o, "state").into(),
            thread: s(o, "thread").into(),
            title: s(o, "title").into(),
            score: (overlap * 10.0) as usize + shared.min(2) * 5 + vec,
        })
    })
    .collect();
    scored.sort_by(|a, b| b.score.cmp(&a.score).then(a.id.cmp(&b.id)));
    scored.truncate(limit);
    scored
}

/// After a filing: the existing items that may already settle the new one, for the filer to judge.
pub fn print_candidates(c: &Connection, id: &str) {
    let list = candidates(c, id, 6);
    if list.is_empty() {
        return;
    }
    println!("existing items that may already settle {id} (if one does: `backtrail resolve {id} --by <id> --reason \"...\"`):");
    for k in list {
        println!(
            "  {} [{}] {} {}: {}",
            k.id, k.thread, k.kind, k.state, clip(&k.title, 100)
        );
    }
}

/// Settle `id` with `by`: a question is answered; a step or decision is superseded. Entity links of the settled item
/// are copied to `by`, so the graph keeps one node for the matter.
pub fn resolve(c: &Connection, id: &str, by: &str, reason: &str) -> Result<bool, String> {
    let reason = clip(reason.trim(), 320);
    if reason.is_empty() || by.is_empty() {
        return Err("--by <id> and --reason are required".into());
    }
    if id == by || count(c, "SELECT COUNT(*) FROM claims WHERE id=?1", &[&by]) == 0 {
        return Err("--by must name another existing item".into());
    }
    let kind: Option<String> = c
        .query_row(
            "SELECT type FROM claims WHERE id=?1 AND state IN ('proposed','open')
               AND type IN ('decision','step','question')",
            [id],
            |r| r.get(0),
        )
        .ok();
    let Some(kind) = kind else { return Ok(false) };
    let ts = now();
    if kind == "question" {
        c.execute(
            "UPDATE claims SET state='answered', answer=?2, superseded_by=?3, updated=?4 WHERE id=?1",
            params![id, reason, by, ts],
        )
        .map_err(|e| e.to_string())?;
    } else {
        c.execute(
            "UPDATE claims SET state='superseded', answer=?2, superseded_by=?3, valid_to=?4, updated=?4 WHERE id=?1",
            params![id, reason, by, ts],
        )
        .map_err(|e| e.to_string())?;
    }
    c.execute(
        "INSERT OR IGNORE INTO claim_about(claim_id,entity_id,role)
         SELECT ?1, entity_id, role FROM claim_about WHERE claim_id=?2",
        params![by, id],
    )
    .ok();
    logw(c, "resolver", "resolve", id, &format!("by {by}: {reason}"));
    Ok(true)
}

/// Undo a resolution: the item goes back to `proposed` with no answer, no settling item and no closing date.
pub fn unresolve(c: &Connection, id: &str) -> bool {
    let ok = c
        .execute(
            "UPDATE claims SET state='proposed', answer=NULL, superseded_by=NULL, valid_to=NULL, updated=?2
             WHERE id=?1 AND state IN ('answered','superseded')
               AND EXISTS (SELECT 1 FROM log l WHERE l.target=?1 AND l.actor='resolver' AND l.action IN ('resolve','expire'))",
            params![id, now()],
        )
        .unwrap_or(0)
        == 1;
    if ok {
        c.execute("DELETE FROM log WHERE target=?1 AND action='resolve-checked'", [id]).ok();
        logw(c, "owner", "unresolve", id, "back to proposed");
    }
    ok
}

/// Time-bound items whose `valid_to` has passed leave the queue. Returns how many.
pub fn expire(c: &Connection) -> usize {
    let today = &now()[..10];
    let ids = strings(
        c,
        "SELECT id FROM claims WHERE state IN ('proposed','open') AND type IN ('decision','step','question')
           AND valid_to IS NOT NULL AND substr(valid_to,1,10)<?1",
        &[&today],
    );
    for id in &ids {
        c.execute(
            "UPDATE claims SET state='superseded', answer='valid_to passed', updated=?2 WHERE id=?1",
            params![id, now()],
        )
        .ok();
        logw(c, "resolver", "expire", id, "valid_to passed");
    }
    ids.len()
}

/// Proposed items a model has not looked at yet, oldest first.
pub fn queue(c: &Connection, limit: usize) {
    expire(c);
    for id in strings(
        c,
        "SELECT id FROM claims WHERE state='proposed' AND type IN ('decision','step','question')
           AND NOT EXISTS (SELECT 1 FROM log l WHERE l.target=claims.id AND l.action='resolve-checked')
         ORDER BY created LIMIT ?1",
        &[&(limit as i64)],
    ) {
        println!("{id}");
    }
}

/// The text a small model reads for one item: the item, the entities it may name, the threads, the candidates.
pub fn packet(c: &Connection, id: &str) {
    let me = rows(
        c,
        "SELECT type, ifnull(thread,'') AS thread, title, ifnull(body,'') AS body, ifnull(quote,'') AS quote,
                substr(ifnull(asserted_at,created),1,10) AS day
         FROM claims WHERE id=?1",
        &[&id],
    );
    let Some(me) = me.first() else {
        eprintln!("no such item");
        std::process::exit(1)
    };
    println!("BACKTRAIL-RESOLVE\nITEM {id}\nTODAY {}", &now()[..10]);
    println!(
        "TYPE {}\nTHREAD {}\nDATE {}\nTITLE {}\nBODY {}\nQUOTE {}",
        s(me, "type"),
        s(me, "thread"),
        s(me, "day"),
        s(me, "title"),
        s(me, "body"),
        s(me, "quote")
    );
    let about = strings(
        c,
        "SELECT e.name FROM claim_about a JOIN entities e ON e.id=a.entity_id WHERE a.claim_id=?1",
        &[&id],
    );
    println!("ENTITIES NOW {}", about.join(", "));
    println!("\nENTITIES (use these names exactly):");
    for r in rows(c, "SELECT name, ifnull(kind,'') AS kind FROM entities ORDER BY name", &[]) {
        println!("- {} ({})", s(&r, "name"), s(&r, "kind"));
    }
    println!("\nTHREADS:");
    for t in strings(c, "SELECT name FROM threads ORDER BY name", &[]) {
        println!("- {t}");
    }
    println!("\nCANDIDATES:");
    for k in candidates(c, id, 8) {
        println!("- {} [{}] {} {}: {}", k.id, k.thread, k.kind, k.state, clip(&k.title, 160));
    }
}

/// Apply a model's verdict: `{"id", "entities", "thread", "valid_to", "resolves_by", "reason"}`. Entities and thread
/// are checked against the store; a resolution needs an existing item and a reason.
pub fn apply(c: &Connection, v: &Value) -> Result<String, String> {
    let id = v["id"].as_str().ok_or("no id")?;
    let kind: String = c
        .query_row("SELECT type FROM claims WHERE id=?1", [id], |r| r.get(0))
        .map_err(|_| "no such item".to_string())?;
    let mut done = vec![];
    for name in v["entities"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        if let Some((eid, _)) = crate::kg::entities::find(c, name) {
            c.execute(
                "INSERT OR IGNORE INTO claim_about(claim_id,entity_id,role) VALUES(?1,?2,'subject')",
                params![id, eid],
            )
            .ok();
            done.push(format!("about {name}"));
        }
    }
    if let Some(t) = v["thread"].as_str().filter(|t| !t.is_empty()) {
        let now_thread: String = c
            .query_row("SELECT ifnull(thread,'') FROM claims WHERE id=?1", [id], |r| r.get(0))
            .unwrap_or_default();
        // the entity is the subject and the thread follows it: a project thread moves only to the thread of the
        // item's main entity; an item on a topic thread may move to any project thread
        let main = v["entities"].as_array().and_then(|a| a.first()).and_then(Value::as_str).unwrap_or("");
        let on_project = count(c, "SELECT COUNT(*) FROM entities WHERE name=?1 AND kind='project'", &[&now_thread]) > 0;
        if t != now_thread && (!on_project || t == main) && crate::items::move_to(c, id, t) {
            done.push(format!("thread {t} (was {now_thread})"));
        }
    }
    if let Some(day) = v["valid_to"].as_str().filter(|d| d.len() >= 10) {
        c.execute("UPDATE claims SET valid_to=?2 WHERE id=?1 AND valid_to IS NULL", params![id, day]).ok();
        done.push(format!("valid_to {day}"));
        expire(c);
    }
    let by = v["resolves_by"].as_str().unwrap_or("");
    if !by.is_empty() {
        if resolve(c, id, by, v["reason"].as_str().unwrap_or(""))? {
            done.push(format!("resolved by {by}"));
        }
    }
    logw(c, "resolver", "resolve-checked", id, &format!("{kind}: {}", done.join("; ")));
    Ok(done.join("; "))
}

pub fn apply_cli(c: &Connection, file: &str) {
    let mut raw = String::new();
    if file == "-" {
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut raw).ok();
    } else {
        raw = std::fs::read_to_string(file).unwrap_or_default();
    }
    match serde_json::from_str::<Value>(&raw) {
        Ok(v) => match apply(c, &v) {
            Ok(done) => println!("{}", if done.is_empty() { "kept as is".into() } else { done }),
            Err(e) => {
                eprintln!("rejected: {e}");
                std::process::exit(1)
            }
        },
        Err(e) => {
            eprintln!("not JSON: {e}");
            std::process::exit(1)
        }
    }
}
