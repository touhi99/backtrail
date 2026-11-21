use super::{MAX_BODY, clip, update};
use crate::clock::now;
use crate::db::{logw, rows, thread_exists};
use rusqlite::{Connection, params};

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
