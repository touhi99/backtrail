//! Work items: decisions, steps, questions and turns. This module holds the write rule that files them, the keep
//! rule that settles what the owner said directly, and every change the app or the command line can make to one.
//! Items are rows of `claims`; the `item` view (sql/views.sql) is how they are read.
use crate::clock::now;
use crate::db::{count, find_quote, id64, logw, rows, source_id, thread_exists};
use rusqlite::{Connection, params};
use std::collections::HashSet;

pub const TYPES: &[&str] = &["decision", "step", "question", "turn"];
pub const MAX_TITLE: usize = 110;
pub const MAX_BODY: usize = 320;

/// Words too common in item titles to count as shared content.
pub const COMMON_WORDS: &[&str] = &[
    "about", "after", "before", "their", "there", "which", "would", "first", "later", "again",
    "build", "built", "write", "files", "check", "start",
];

/// The content words of a title: five letters or more, common words left out.
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

// ---- filing

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
}

/// The write rule: an item names a known thread, cites an indexed source, quotes text found in that source and
/// respects the length limits. Filing the same thread, type and title again updates the item. Returns its id.
pub fn add(c: &Connection, it: &NewItem) -> Result<String, String> {
    if !TYPES.contains(&it.kind.as_str()) {
        return Err(format!(
            "type must be one of decision, step, question, turn (got {})",
            it.kind
        ));
    }
    if it.title.trim().is_empty() || it.title.chars().count() > MAX_TITLE {
        return Err(format!("title must be 1..{MAX_TITLE} chars"));
    }
    if it.body.chars().count() > MAX_BODY {
        return Err(format!("body over {MAX_BODY} chars"));
    }
    if !thread_exists(c, &it.thread) {
        return Err(format!("unknown thread '{}'", it.thread));
    }
    let source = source_id(c, &it.source).ok_or_else(|| {
        format!(
            "source '{}' not found: every item must cite an indexed source",
            it.source
        )
    })?;
    if it.quote.trim().len() < 8 {
        return Err("quote of at least 8 chars from the source is required".into());
    }
    let passage = find_quote(c, source, &it.quote)
        .ok_or_else(|| format!("quote not found in {}: \"{}\"", it.source, it.quote))?;

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
        return Err(format!("id {id} is already used by another item"));
    }
    let state = it
        .state
        .clone()
        .unwrap_or_else(|| default_state(&it.kind).into());
    let filed = it.date.clone().unwrap_or_else(now);
    c.execute(
        "INSERT INTO claims(id,type,thread,title,body,state,origin,source_id,chunk_id,quote,created,updated,speaker,asserted_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,?12,coalesce(?13,?11))
         ON CONFLICT(id) DO UPDATE SET body=?5, quote=?10, chunk_id=?9, source_id=?8",
        params![
            id, it.kind, it.thread, it.title, it.body, state, it.origin, source, passage.chunk_id, it.quote, filed,
            passage.speaker, passage.ts
        ],
    )
    .map_err(|e| e.to_string())?;
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

fn default_state(kind: &str) -> &'static str {
    if kind == "step" || kind == "question" {
        "open"
    } else {
        "settled"
    }
}

/// Something the owner typed in the app on a thread page: a todo, bug, question or decision. The typed text is
/// stored as its own source, so the item cites it like any other.
pub fn capture(
    c: &Connection,
    thread: &str,
    kind: &str,
    tag: &str,
    title: &str,
    body: &str,
) -> Result<String, String> {
    if !TYPES.contains(&kind) || !thread_exists(c, thread) {
        return Err("unknown thread or type".into());
    }
    if title.is_empty() || title.chars().count() > MAX_TITLE || body.chars().count() > MAX_BODY {
        return Err(format!(
            "title 1..{MAX_TITLE} and detail up to {MAX_BODY} characters"
        ));
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
    .map_err(|e| e.to_string())?;
    logw(c, "owner", "add", &id, title);
    Ok(id)
}

/// An idea typed in the app becomes an open thread of its own, with the typed line as its source. Returns the
/// thread's name, which gets the date added when the name is taken.
pub fn idea(c: &Connection, text: &str) -> Result<String, String> {
    if text.is_empty() || text.chars().count() > MAX_TITLE {
        return Err(format!("idea must be 1..{MAX_TITLE} chars"));
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
    )
    .map_err(|e| e.to_string())?;
    logw(c, "owner", "idea", &name, "");
    Ok(name)
}

/// Store text the owner typed as a one-passage source. Returns (source id, chunk id).
fn typed_source(c: &Connection, thread: &str, text: &str, ts: &str) -> (i64, i64) {
    let source_ref = format!("owner:{ts}");
    c.execute(
        "INSERT OR IGNORE INTO sources(ref,kind,title,thread,first_ts,last_ts,mtime,nchunks)
         VALUES(?1,'owner','Typed in the app',?2,?3,?3,0,1)",
        params![source_ref, thread, ts],
    )
    .unwrap();
    let source = source_id(c, &source_ref).unwrap();
    let chunk = crate::ingest::add_chunk(c, source, 0, "typed", "owner", ts, text);
    (source, chunk)
}

// ---- the keep rule

/// Kinds of fact the keep rule may keep without review.
const KEEP_TYPES: &[&str] = &["decision", "rationale", "state"];

/// Turns stored with the user role that the owner did not type: the summary written when a session is continued,
/// command output and notifications that Claude Code inserts as user turns.
fn inserted_turn(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("This session is being continued")
        || t.starts_with('<')
        || t.starts_with("[Request interrupted")
}

/// The keep rule. A proposed fact is kept (`state='settled'`, `review='auto'`) when the owner said it in so many
/// words: it is a decision, rationale or state claim, its quote comes from a turn the owner typed, and it is marked
/// `stated`. It stays proposed with `review='held|<reason>'` when an earlier fact of the same type on the same
/// thread shares three or more content words with it, since it may repeat or change that fact. Steps, questions,
/// relations and the assistant's words always wait for the owner. Returns true when the fact was kept.
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
    // A Claude flag that only said "still live" or "already in effect" no longer matters once the fact is kept;
    // a "superseded" flag stays, so the kept fact shows as replaced.
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

/// Run the keep rule over every proposed fact, oldest first, so a fact is only compared with facts said before it.
pub fn keep_all(c: &Connection) -> usize {
    let ids = crate::db::strings(
        c,
        "SELECT id FROM claims WHERE state='proposed' ORDER BY ifnull(asserted_at,''), id",
        &[],
    );
    ids.iter().filter(|id| keep(c, id)).count()
}

// ---- changes by the owner (the app)

/// Tick a step done, or untick it.
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

/// Mark a step in progress, or back to open.
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

/// The owner says a step marked "looks done?" is not done.
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

pub fn answer(c: &Connection, id: &str, text: &str) -> Result<bool, String> {
    if text.is_empty() || text.chars().count() > MAX_BODY {
        return Err("answer must be 1..320 chars".into());
    }
    let ok = update(
        c,
        "UPDATE claims SET state='answered', answer=?2, updated=?3 WHERE id=?1 AND type='question'",
        &[&id, &text, &now()],
    );
    logw(c, "owner", "answer", id, text);
    Ok(ok)
}

pub enum Verdict {
    Accept,
    Reject,
    /// True once, no longer applies.
    Park,
}

/// The owner's verdict on a proposed item. A fact the keep rule kept (`review='auto'`) can still be rejected.
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
                     WHERE id=?1 AND (state='proposed' OR (review='auto' AND state='settled'))",
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

/// Accept or reject a proposed relation between two threads.
pub fn review_relation(c: &Connection, a: &str, b: &str, kind: &str, accept: bool) -> bool {
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

/// Move an item to another thread (it was typed in the wrong one).
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

/// A kept decision that no longer holds: it stays on file and drops out of the live list.
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

// ---- changes by Claude (the command line)

/// Close a step. `by` is owner, claude or evidence; the last two need proof.
pub fn done(c: &Connection, id: &str, by: &str, proof: &str) -> Result<bool, String> {
    if by != "owner" && proof.trim().is_empty() {
        return Err("a step closed by Claude or evidence needs --proof".into());
    }
    let ok = update(
        c,
        "UPDATE claims SET state='done', done_by=?2, answer=?3, updated=?4 WHERE id=?1 AND type='step'",
        &[&id, &by, &proof, &now()],
    );
    logw(c, by, "done", id, proof);
    Ok(ok)
}

/// Claude says a step is built; it shows as "looks done?" until the owner confirms.
pub fn built(c: &Connection, id: &str, evidence: &str) -> Result<bool, String> {
    let evidence = clip(evidence, 300);
    if evidence.trim().is_empty() {
        return Err("--evidence is required".into());
    }
    let ok = update(
        c,
        "UPDATE claims SET state='looks_done', done_by='claude', answer=?2, updated=?3
         WHERE id=?1 AND type='step' AND state IN ('open','doing')",
        &[&id, &evidence, &now()],
    );
    if ok {
        logw(c, "claude", "built", id, &evidence);
    }
    Ok(ok)
}

/// Label a proposed item after checking it against later items and the code: live, in_effect, superseded or done.
pub fn flag(c: &Connection, id: &str, label: &str, reason: &str) -> Result<bool, String> {
    if !["live", "in_effect", "superseded", "done"].contains(&label) {
        return Err("--as must be live, in_effect, superseded or done".into());
    }
    let reason = clip(reason, 200);
    if label != "live" && reason.trim().is_empty() {
        return Err("a flag other than live needs --reason".into());
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

/// Claude answers a question the owner typed in the app.
pub fn reply(c: &Connection, id: &str, text: &str) -> Result<bool, String> {
    let text = clip(text, 600);
    if text.trim().is_empty() {
        return Err("--text is required".into());
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

/// Retire an item: it stays on file as rejected with the reason; `replaced_by` names the item that replaced it.
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
            .unwrap();
        }
        let note = replaced_by
            .map(|new| format!(" (replaced by {new})"))
            .unwrap_or_default();
        logw(c, "owner", "retire", id, &format!("{reason}{note}"));
    }
    ok
}
