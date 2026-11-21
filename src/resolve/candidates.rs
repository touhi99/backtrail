use crate::db::{rows, s};
use crate::items::COMMON_WORDS;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

use super::clip;

const LIVE: &str = "'settled','done','answered','looks_done','open','doing'";

const STOP: &[&str] = &[
    "the", "and", "for", "with", "from", "that", "this", "not", "are", "was", "but", "its", "has",
    "have", "owner", "asked", "wants", "want", "should", "would", "could", "which", "what", "how",
    "when", "only", "into", "than", "then", "them", "they", "will", "can", "use", "new", "chose",
    "decided", "said", "also", "does", "did", "all", "one", "any", "out", "off", "get", "set",
    "make", "made", "keep", "now", "for", "who", "why", "where",
];

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

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the overlap score is a fraction in 0..=1, scaled to whole points"
)]
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
            let shared = usize::try_from(o["shared"].as_i64().unwrap_or(0)).unwrap_or(0);
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

pub fn print_candidates(c: &Connection, id: &str) {
    let list = candidates(c, id, 6);
    if list.is_empty() {
        return;
    }
    println!(
        "existing items that may already settle {id} (if one does: `backtrail resolve {id} --by <id> --reason \"...\"`):"
    );
    for k in list {
        println!(
            "  {} [{}] {} {}: {}",
            k.id,
            k.thread,
            k.kind,
            k.state,
            clip(&k.title, 100)
        );
    }
}
