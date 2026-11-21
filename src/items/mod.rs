mod curate;
mod review;
mod state;

pub use curate::{keep, keep_all, skip_session};
pub use review::{RelationVerdict, Verdict, flag, review, review_relation};
pub use state::{
    answer, built, done, move_to, not_yet, rehome, reply, retire, supersede, tick, toggle_doing,
};

use crate::clock::now;
use crate::db::{count, find_quote, id64, logw, source_id, thread_exists};
use anyhow::Context;
use rusqlite::{Connection, params};
use std::collections::HashSet;

pub const TYPES: &[&str] = &["decision", "step", "question", "turn"];
pub const MAX_TITLE: usize = 110;
pub const MAX_BODY: usize = 320;

pub const COMMON_WORDS: &[&str] = &[
    "about", "after", "before", "their", "there", "which", "would", "first", "later", "again",
    "build", "built", "write", "files", "check", "start",
];

pub fn content_words(title: &str) -> HashSet<String> {
    title
        .to_lowercase()
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|w| w.len() >= 5 && !COMMON_WORDS.contains(w))
        .map(String::from)
        .collect()
}

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn update(c: &Connection, sql: &str, args: &[&dyn rusqlite::ToSql]) -> bool {
    c.execute(sql, args).unwrap_or(0) == 1
}

#[derive(Clone)]
pub struct NewItem {
    pub kind: String,
    pub thread: String,
    pub title: String,
    pub body: String,
    pub source: String,
    pub quote: String,
    pub state: Option<String>,
    pub origin: String,
    pub date: Option<String>,
    pub entities: Vec<String>,
    pub valid_to: Option<String>,
    pub check_how: Option<String>,
}

pub fn add(c: &Connection, it: &NewItem) -> anyhow::Result<String> {
    if !TYPES.contains(&it.kind.as_str()) {
        anyhow::bail!(
            "type must be one of decision, step, question, turn (got {})",
            it.kind
        );
    }
    if it.title.trim().is_empty() || it.title.chars().count() > MAX_TITLE {
        anyhow::bail!("title must be 1..{MAX_TITLE} chars");
    }
    if it.body.chars().count() > MAX_BODY {
        anyhow::bail!("body over {MAX_BODY} chars");
    }
    if !thread_exists(c, &it.thread) {
        anyhow::bail!("unknown thread '{}'", it.thread);
    }
    let source = source_id(c, &it.source).with_context(|| {
        format!(
            "source '{}' not found: every item must cite an indexed source",
            it.source
        )
    })?;
    if it.quote.trim().len() < 8 {
        anyhow::bail!("quote of at least 8 chars from the source is required");
    }
    let passage = find_quote(c, source, &it.quote)
        .with_context(|| format!("quote not found in {}: \"{}\"", it.source, it.quote))?;
    let (subjects, thread) = subjects(c, it);
    if subjects.is_empty() {
        anyhow::bail!(
            "name at least one entity the item is about (--entities \"A,B\"); `bt kg entity` lists them"
        );
    }
    let it = &NewItem {
        thread,
        ..it.clone()
    };

    let existing: Option<String> = c
        .query_row(
            "SELECT id FROM claims WHERE thread=?1 AND type=?2 AND title=?3",
            params![it.thread, it.kind, it.title],
            |r| r.get(0),
        )
        .ok();
    let is_new = existing.is_none();
    let id = existing.unwrap_or_else(|| {
        format!(
            "{}-{}",
            &it.kind[..1],
            id64(&format!("{}{}{}", it.thread, it.kind, it.title))
        )
    });
    if is_new && count(c, "SELECT COUNT(*) FROM claims WHERE id=?1", &[&id]) > 0 {
        anyhow::bail!("id {id} is already used by another item");
    }
    let state = it
        .state
        .clone()
        .unwrap_or_else(|| default_state(&it.kind).into());
    let filed = it.date.clone().unwrap_or_else(now);
    c.execute(
        "INSERT INTO claims(id,type,thread,title,body,state,origin,source_id,chunk_id,quote,created,updated,speaker,asserted_at,valid_to,check_how)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,?12,coalesce(?13,?11),?14,?15)
         ON CONFLICT(id) DO UPDATE SET body=?5, quote=?10, chunk_id=?9, source_id=?8, valid_to=coalesce(?14,valid_to), check_how=coalesce(?15,check_how)",
        params![
            id, it.kind, it.thread, it.title, it.body, state, it.origin, source, passage.chunk_id, it.quote, filed,
            passage.speaker, passage.ts, it.valid_to, it.check_how
        ],
    )
    ?;
    for (eid, _) in &subjects {
        c.execute(
            "INSERT OR IGNORE INTO claim_about(claim_id,entity_id,role) VALUES(?1,?2,'subject')",
            params![id, eid],
        )
        .ok();
    }
    if it.kind == "decision" {
        let text = format!("{} {} {}", it.title, it.body, it.quote);
        crate::kg::entities::link_claim(c, &id, &it.thread, &text);
    }
    let actor = if it.origin == "suggested" {
        "claude-suggest"
    } else {
        "claude"
    };
    logw(c, actor, "add", &id, &it.title);
    if is_new && state == "proposed" {
        keep(c, &id);
    }
    Ok(id)
}

fn subjects(c: &Connection, it: &NewItem) -> (Vec<(String, String)>, String) {
    let named: Vec<(String, String)> = it
        .entities
        .iter()
        .filter_map(|n| crate::kg::entities::find(c, n))
        .collect();
    let mut all = named.clone();
    for m in crate::kg::entities::mentions(c, &format!("{} {}", it.title, it.body)) {
        if !all.iter().any(|a| a.0 == m.0) {
            all.push(m);
        }
    }
    let has_thread = |n: &str| thread_exists(c, n);
    let is_project = |n: &str| {
        count(
            c,
            "SELECT COUNT(*) FROM entities WHERE name=?1 AND kind='project'",
            &[&n],
        ) > 0
    };
    let mut thread = it.thread.clone();
    if !all.iter().any(|a| a.1 == thread) {
        if let Some(n) = named.iter().find(|n| has_thread(&n.1) && is_project(&n.1)) {
            thread.clone_from(&n.1);
        } else if !is_project(&thread) {
            let cands: Vec<_> = all
                .iter()
                .filter(|a| has_thread(&a.1) && is_project(&a.1))
                .collect();
            if cands.len() == 1 {
                thread.clone_from(&cands[0].1);
            }
        }
    }
    (all, thread)
}

fn default_state(kind: &str) -> &'static str {
    if kind == "step" || kind == "question" {
        "open"
    } else {
        "settled"
    }
}

pub fn capture(
    c: &Connection,
    thread: &str,
    kind: &str,
    tag: &str,
    title: &str,
    body: &str,
) -> anyhow::Result<String> {
    if !TYPES.contains(&kind) || !thread_exists(c, thread) {
        anyhow::bail!("unknown thread or type");
    }
    if title.is_empty() || title.chars().count() > MAX_TITLE || body.chars().count() > MAX_BODY {
        anyhow::bail!("title 1..{MAX_TITLE} and detail up to {MAX_BODY} characters");
    }
    let ts = now();
    let text = if body.is_empty() {
        title.to_string()
    } else {
        format!("{title}\n{body}")
    };
    let (source, chunk) = typed_source(c, thread, &text, &ts);
    let id = format!("{}-{}", &kind[..1], id64(&format!("{thread}{title}{ts}")));
    c.execute(
        "INSERT INTO claims(id,type,thread,title,body,state,origin,tag,source_id,chunk_id,quote,created,updated,speaker,asserted_at)
         VALUES(?1,?2,?3,?4,?5,?6,'owner',?7,?8,?9,?4,?10,?10,'owner',?10)",
        params![id, kind, thread, title, body, default_state(kind), tag, source, chunk, ts],
    )
    ?;
    logw(c, "owner", "add", &id, title);
    Ok(id)
}

pub fn idea(c: &Connection, text: &str) -> anyhow::Result<String> {
    if text.is_empty() || text.chars().count() > MAX_TITLE {
        anyhow::bail!("idea must be 1..{MAX_TITLE} chars");
    }
    let ts = now();
    let name = if thread_exists(c, text) {
        format!("{text} ({})", &ts[..10])
    } else {
        text.to_string()
    };
    typed_source(c, &name, text, &ts);
    c.execute(
        "INSERT INTO threads(name,state,area,body,origin,kind,created,updated)
         VALUES(?1,'open','Ideas, no repo','Idea typed in the app.','owner','idea',?2,?2)",
        params![name, ts],
    )?;
    logw(c, "owner", "idea", &name, "");
    Ok(name)
}

fn typed_source(c: &Connection, thread: &str, text: &str, ts: &str) -> (i64, i64) {
    let source_ref = format!("owner:{ts}");
    c.execute(
        "INSERT OR IGNORE INTO sources(ref,kind,title,thread,first_ts,last_ts,mtime,nchunks)
         VALUES(?1,'owner','Typed in the app',?2,?3,?3,0,1)",
        params![source_ref, thread, ts],
    )
    .expect("write to database");
    let source = source_id(c, &source_ref).expect("source row was just inserted");
    let chunk = crate::ingest::add_chunk(c, source, 0, "typed", "owner", ts, text);
    (source, chunk)
}
