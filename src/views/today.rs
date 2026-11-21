use super::{stale, threads, waiting_on_claude, waiting_on_owner};
use crate::clock::{days_ago, now};
use crate::db::{rows, s};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::collections::HashSet;

pub fn today(c: &Connection) -> Value {
    let suggested = rows(
        c,
        "SELECT * FROM item WHERE origin='suggested' AND state='proposed' ORDER BY created DESC",
        &[],
    );
    let looks_done = rows(
        c,
        "SELECT * FROM item WHERE type='step' AND state='looks_done'",
        &[],
    );
    let recent = rows(
        c,
        "SELECT id, ref, title, kind, thread, last_ts FROM sources
         WHERE kind='session' AND thread IS NOT NULL ORDER BY last_ts DESC LIMIT 400",
        &[],
    );
    let mut moved: Vec<Value> = vec![];
    let mut seen = HashSet::new();
    for r in recent
        .iter()
        .filter(|r| !s(r, "title").starts_with("Untitled"))
    {
        if days_ago(s(r, "last_ts")) > 3 || moved.len() >= 6 {
            break;
        }
        if seen.insert(s(r, "thread").to_string()) {
            moved.push(r.clone());
        }
    }
    let totals = rows(
        c,
        "SELECT (SELECT COUNT(*) FROM sources WHERE kind='session') AS sessions,
                (SELECT COUNT(*) FROM sources WHERE kind='note') AS notes,
                (SELECT COUNT(*) FROM sources WHERE kind='artifact') AS artifacts,
                (SELECT COUNT(*) FROM chunks) AS chunks,
                (SELECT COUNT(*) FROM item) AS items",
        &[],
    );
    let todo = rows(
        c,
        "SELECT i.* FROM item i JOIN threads t ON t.name=i.thread
         WHERE i.type='step' AND i.state IN ('open','doing') AND i.origin!='suggested' AND t.state!='dropped'
         ORDER BY i.created DESC LIMIT 60",
        &[],
    );
    let pending: Vec<Value> = rows(
        c,
        "SELECT s.id, s.ref, s.title, s.thread, s.last_ts, s.nchunks FROM source_stat s
         WHERE s.kind='session' AND s.thread!='Unsorted' AND s.nchunks>=6 AND s.cited=0 AND s.reviewed=0
         ORDER BY s.last_ts DESC LIMIT 200",
        &[],
    )
    .into_iter()
    .filter(|r| days_ago(s(r, "last_ts")) <= 3)
    .collect::<Vec<_>>()
    .into_iter()
    .filter({
        let mut seen = HashSet::new();
        move |r| seen.insert((s(r, "title").to_string(), s(r, "thread").to_string()))
    })
    .collect();
    json!({
        "todo": todo, "pending": pending,
        "waiting": waiting_on_owner(c), "asks": waiting_on_claude(c), "suggested": suggested, "looks": looks_done,
        "threads": threads(c), "moved": moved, "stale": stale(c), "totals": totals[0], "date": now(),
    })
}

pub fn decided(c: &Connection, thread: &str, limit: i64) -> Vec<Value> {
    let today = now()[..10].to_string();
    let mut found = rows(
        c,
        "SELECT c.id, c.title, c.thread, c.review, k.role AS role, c.source_id, c.chunk_id,
                substr(COALESCE(NULLIF(c.asserted_at,''), s.first_ts, ''), 1, 10) AS day
         FROM claims c LEFT JOIN chunks k ON k.id=c.chunk_id LEFT JOIN sources s ON s.id=c.source_id
         WHERE c.type='decision' AND c.state='settled' AND c.thread=?1
           AND IFNULL(c.stale,'') NOT LIKE 'superseded%' AND IFNULL(c.superseded_by,'')=''
           AND (IFNULL(c.valid_to,'')='' OR c.valid_to>=?2)
         ORDER BY day DESC, c.id LIMIT ?3",
        &[&thread, &today, &limit],
    );
    for d in &mut found {
        let how = if s(d, "review") == "auto" {
            "owner-stated"
        } else if matches!(s(d, "role"), "note" | "artifact") {
            "from a note"
        } else {
            "owner-accepted"
        };
        let day = Some(s(d, "day"))
            .filter(|x| !x.is_empty())
            .unwrap_or("undated")
            .to_string();
        d["how"] = json!(how);
        d["day"] = json!(day);
        let id = s(d, "id").to_string();
        let also = rows(
            c,
            "SELECT b.id, b.title, b.body, b.state, k.reason
             FROM claim_edges k JOIN claims b ON b.id=k.b
             WHERE k.a=?1 AND k.kind='considered' ORDER BY b.title",
            &[&id],
        );
        if !also.is_empty() {
            d["also"] = json!(also);
        }
    }
    found
}
