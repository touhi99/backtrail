use crate::clock::now;
use crate::db::{count, find_quote, id64, logw, rows, source_id, thread_exists};
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

pub fn tick(c: &Connection, id: &str) -> bool {
    let ok = update(
        c,
        "UPDATE claims SET state=CASE state WHEN 'done' THEN 'open' ELSE 'done' END, done_by='owner', updated=?2
         WHERE id=?1 AND type='step'",
        &[&id, &now()],
    );
    logw(c, "owner", "tick", id, "");
    ok
}

pub fn toggle_doing(c: &Connection, id: &str) -> bool {
    let ok = update(
        c,
        "UPDATE claims SET state=CASE state WHEN 'doing' THEN 'open' ELSE 'doing' END, updated=?2
         WHERE id=?1 AND type='step' AND state IN ('open','doing')",
        &[&id, &now()],
    );
    if ok {
        logw(c, "owner", "doing", id, "");
    }
    ok
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

pub fn not_yet(c: &Connection, id: &str) -> bool {
    let ok = update(
        c,
        "UPDATE claims SET state='open', done_by=NULL, answer=NULL, updated=?2 WHERE id=?1 AND state='looks_done'",
        &[&id, &now()],
    );
    if ok {
        logw(c, "owner", "not-yet", id, "dismissed looks-done");
    }
    ok
}

pub fn answer(c: &Connection, id: &str, text: &str) -> anyhow::Result<bool> {
    if text.is_empty() || text.chars().count() > MAX_BODY {
        anyhow::bail!("answer must be 1..320 chars");
    }
    let ok = update(
        c,
        "UPDATE claims SET state='answered', answer=?2, updated=?3 WHERE id=?1 AND type='question'",
        &[&id, &text, &now()],
    );
    logw(c, "owner", "answer", id, text);
    Ok(ok)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Accept,
    Reject,
    Park,
}

pub fn review(c: &Connection, id: &str, verdict: Verdict, reason: &str) -> bool {
    let reason = clip(reason, MAX_BODY);
    let ts = now();
    let (ok, action) = match verdict {
        Verdict::Accept => (
            update(
                c,
                "UPDATE claims SET state=CASE WHEN type IN ('step','question') THEN 'open' ELSE 'settled' END,
                        review=NULL, updated=?2
                 WHERE id=?1 AND state='proposed'",
                &[&id, &ts],
            ),
            "accept",
        ),
        Verdict::Reject | Verdict::Park => {
            let (state, action) = if matches!(verdict, Verdict::Park) { ("parked", "park") } else { ("rejected", "reject") };
            (
                update(
                    c,
                    "UPDATE claims SET state=?2, answer=?3, review=NULL, updated=?4
                     WHERE id=?1 AND (state='proposed' OR (review='auto' AND state='settled')
                          OR (type='step' AND state IN ('open','doing','looks_done')))",
                    &[&id, &state, &reason, &ts],
                ),
                action,
            )
        }
    };
    if ok {
        logw(c, "owner", action, id, &reason);
    }
    ok
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationVerdict {
    Accept,
    Reject,
}

pub fn review_relation(
    c: &Connection,
    a: &str,
    b: &str,
    kind: &str,
    verdict: RelationVerdict,
) -> bool {
    let accept = verdict == RelationVerdict::Accept;
    let name = |t: &str| t.trim_start_matches("T:").to_string();
    let ok = update(
        c,
        "UPDATE claims SET state=?4, updated=?5 WHERE type=?3 AND state='proposed' AND id IN (
           SELECT s.claim_id FROM claim_about s
           JOIN entities es ON es.id=s.entity_id
           JOIN claim_about o ON o.claim_id=s.claim_id AND o.role='object'
           JOIN entities eo ON eo.id=o.entity_id
           WHERE s.role='subject' AND es.name=?1 AND eo.name=?2)",
        &[
            &name(a),
            &name(b),
            &format!("relation:{kind}"),
            &if accept { "settled" } else { "rejected" },
            &now(),
        ],
    );
    if ok {
        logw(
            c,
            "owner",
            if accept { "accept" } else { "reject" },
            &format!("{a} {kind} {b}"),
            "",
        );
    }
    ok
}

pub fn move_to(c: &Connection, id: &str, thread: &str) -> bool {
    if !thread_exists(c, thread) {
        return false;
    }
    let ok = update(
        c,
        "UPDATE claims SET thread=?2, updated=?3 WHERE id=?1 AND type IN ('decision','step','question','turn')",
        &[&id, &thread, &now()],
    );
    if ok {
        c.execute(
            "UPDATE sources SET thread=?2 WHERE kind='owner' AND id=(SELECT source_id FROM claims WHERE id=?1)",
            params![id, thread],
        )
        .ok();
        logw(c, "owner", "move", id, thread);
    }
    ok
}

pub fn rehome(c: &Connection) -> usize {
    let moves = rows(
        c,
        "SELECT c.id, min(e.name) AS target
         FROM claims c
         JOIN threads cur ON cur.name=c.thread
         JOIN claim_about a ON a.claim_id=c.id
         JOIN entities e ON e.id=a.entity_id AND e.kind='project'
         JOIN threads t ON t.name=e.name
         WHERE c.type IN ('decision','step','question','turn')
           AND c.state NOT IN ('superseded','rejected')
           AND ifnull(cur.kind,'') IN ('','discussion','idea')
           AND c.id NOT IN (SELECT target FROM log WHERE action IN ('move','rehome'))
         GROUP BY c.id
         HAVING count(DISTINCT e.id)=1",
        &[],
    );
    for m in &moves {
        let (id, target) = (
            m["id"].as_str().unwrap_or(""),
            m["target"].as_str().unwrap_or(""),
        );
        c.execute(
            "UPDATE claims SET thread=?2, updated=?3 WHERE id=?1",
            params![id, target, now()],
        )
        .expect("write to database");
        logw(c, "derive", "rehome", id, target);
    }
    moves.len()
}

pub fn supersede(c: &Connection, id: &str, reason: &str) -> bool {
    let reason = clip(reason, 200);
    let why = if reason.trim().is_empty() {
        "marked replaced in the app"
    } else {
        reason.trim()
    };
    let ok = update(
        c,
        "UPDATE claims SET stale=?2, updated=?3 WHERE id=?1 AND type='decision' AND state='settled'",
        &[&id, &format!("superseded|{why}"), &now()],
    );
    if ok {
        logw(c, "owner", "supersede", id, &reason);
    }
    ok
}

pub fn done(c: &Connection, id: &str, by: &str, proof: &str) -> anyhow::Result<bool> {
    if by != "owner" && proof.trim().is_empty() {
        anyhow::bail!("a step closed by Claude or evidence needs --proof");
    }
    let ok = update(
        c,
        "UPDATE claims SET state='done', done_by=?2, answer=?3, updated=?4 WHERE id=?1 AND type='step'",
        &[&id, &by, &proof, &now()],
    );
    logw(c, by, "done", id, proof);
    Ok(ok)
}

pub fn built(
    c: &Connection,
    id: &str,
    evidence: &str,
    check: Option<String>,
) -> anyhow::Result<bool> {
    let evidence = clip(evidence, 300);
    if evidence.trim().is_empty() {
        anyhow::bail!("--evidence is required");
    }
    let ok = update(
        c,
        "UPDATE claims SET state='looks_done', done_by='claude', answer=?2, updated=?3, check_how=coalesce(?4,check_how)
         WHERE id=?1 AND type='step' AND state IN ('open','doing')",
        &[&id, &evidence, &now(), &check.map(|t| clip(&t, 300))],
    );
    if ok {
        logw(c, "claude", "built", id, &evidence);
    }
    Ok(ok)
}

pub fn flag(c: &Connection, id: &str, label: &str, reason: &str) -> anyhow::Result<bool> {
    if !["live", "in_effect", "superseded", "done"].contains(&label) {
        anyhow::bail!("--as must be live, in_effect, superseded or done");
    }
    let reason = clip(reason, 200);
    if label != "live" && reason.trim().is_empty() {
        anyhow::bail!("a flag other than live needs --reason");
    }
    let ok = update(
        c,
        "UPDATE claims SET stale=?2 WHERE id=?1 AND state='proposed'",
        &[&id, &format!("{label}|{reason}")],
    );
    if ok {
        logw(c, "claude", "flag", id, &format!("{label}: {reason}"));
    }
    Ok(ok)
}

pub fn reply(c: &Connection, id: &str, text: &str) -> anyhow::Result<bool> {
    let text = clip(text, 600);
    if text.trim().is_empty() {
        anyhow::bail!("--text is required");
    }
    let ok = update(
        c,
        "UPDATE claims SET state='answered', answer=?2, done_by='claude', updated=?3
         WHERE id=?1 AND type='question' AND origin='owner' AND state='open'",
        &[&id, &text, &now()],
    );
    if ok {
        logw(c, "claude", "reply", id, &text);
    }
    Ok(ok)
}

pub fn retire(c: &Connection, id: &str, replaced_by: Option<&str>, reason: &str) -> bool {
    let ok = update(
        c,
        "UPDATE claims SET state='rejected', answer=?2, updated=?3
         WHERE id=?1 AND type IN ('decision','step','question','turn') AND state!='rejected'",
        &[&id, &reason, &now()],
    );
    if ok {
        if let Some(new) = replaced_by {
            c.execute(
                "UPDATE claims SET superseded_by=?2 WHERE id=?1",
                params![id, new],
            )
            .expect("write to database");
        }
        let note = replaced_by
            .map(|new| format!(" (replaced by {new})"))
            .unwrap_or_default();
        logw(c, "owner", "retire", id, &format!("{reason}{note}"));
    }
    ok
}
