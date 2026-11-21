use super::{clip, content_words, update};
use crate::clock::now;
use crate::db::{logw, rows};
use rusqlite::{Connection, params};

const KEEP_TYPES: &[&str] = &["decision", "rationale", "state"];

fn inserted_turn(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("This session is being continued")
        || t.starts_with('<')
        || t.starts_with("[Request interrupted")
}

pub fn keep(c: &Connection, id: &str) -> bool {
    let fact = rows(
        c,
        "SELECT c.type, ifnull(c.thread,'') AS thread, c.title, c.speaker, c.stated, ifnull(c.asserted_at,'') AS said,
                c.stale, k.role, k.text
         FROM claims c LEFT JOIN chunks k ON k.id=c.chunk_id
         WHERE c.id=?1 AND c.state='proposed'",
        &[&id],
    );
    let Some(f) = fact.first() else { return false };
    let get = |key: &str| f[key].as_str().unwrap_or("");
    let typed = get("role") == "user" && !inserted_turn(get("text"));
    if !KEEP_TYPES.contains(&get("type"))
        || get("speaker") != "owner"
        || get("stated") != "stated"
        || !typed
    {
        return false;
    }
    let mine = content_words(get("title"));
    let earlier = rows(
        c,
        "SELECT id, title FROM claims
         WHERE type=?1 AND ifnull(thread,'')=?2 AND id!=?3 AND state!='rejected' AND ifnull(asserted_at,'')<=?4",
        &[&get("type"), &get("thread"), &id, &get("said")],
    );
    let repeat = earlier.iter().find(|e| {
        mine.intersection(&content_words(e["title"].as_str().unwrap_or("")))
            .count()
            >= 3
    });
    if let Some(e) = repeat {
        let why = format!(
            "held|may repeat or change {}: {}",
            e["id"].as_str().unwrap_or(""),
            e["title"].as_str().unwrap_or("")
        );
        c.execute(
            "UPDATE claims SET review=?2 WHERE id=?1",
            params![id, clip(&why, 200)],
        )
        .ok();
        return false;
    }
    let stale = Some(get("stale")).filter(|s| s.starts_with("superseded"));
    let kept = update(
        c,
        "UPDATE claims SET state='settled', review='auto', stale=?2, updated=?3 WHERE id=?1 AND state='proposed'",
        &[&id, &stale, &now()],
    );
    if kept {
        logw(c, "auto", "keep", id, get("title"));
    }
    kept
}

pub fn keep_all(c: &Connection) -> usize {
    let ids = crate::db::strings(
        c,
        "SELECT id FROM claims WHERE state='proposed' ORDER BY ifnull(asserted_at,''), id",
        &[],
    );
    ids.iter().filter(|id| keep(c, id)).count()
}

pub fn skip_session(c: &Connection, source: i64, actor: &str) -> bool {
    let ok = c
        .execute(
            "INSERT OR IGNORE INTO source_review(source_id,ts,actor)
         SELECT o.id, ?2, ?3 FROM sources s JOIN sources o
           ON o.kind='session' AND o.title IS s.title AND o.thread IS s.thread
         WHERE s.id=?1 AND s.kind='session'",
            rusqlite::params![source, now(), actor],
        )
        .unwrap_or(0)
        > 0;
    if ok {
        logw(
            c,
            actor,
            "skip-session",
            &source.to_string(),
            "nothing to file",
        );
    }
    ok
}
