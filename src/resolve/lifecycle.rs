use crate::clock::now;
use crate::db::{count, logw, rows, s, strings};
use rusqlite::{Connection, params};

use super::clip;

pub fn resolve(c: &Connection, id: &str, by: &str, reason: &str) -> anyhow::Result<bool> {
    let reason = clip(reason.trim(), 320);
    if reason.is_empty() || by.is_empty() {
        anyhow::bail!("--by <id> and --reason are required");
    }
    if id == by || count(c, "SELECT COUNT(*) FROM claims WHERE id=?1", &[&by]) == 0 {
        anyhow::bail!("--by must name another existing item");
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
        ?;
    } else {
        c.execute(
            "UPDATE claims SET state='superseded', answer=?2, superseded_by=?3, valid_to=?4, updated=?4 WHERE id=?1",
            params![id, reason, by, ts],
        )
        ?;
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

pub fn link(c: &Connection, a: &str, b: &str, kind: &str, reason: &str) {
    c.execute(
        "INSERT OR IGNORE INTO claim_links(a,b,kind,reason,actor,created) VALUES(?1,?2,?3,?4,'resolver',?5)",
        params![a, b, kind, reason, now()],
    )
    .expect("write to database");
}

pub fn label_replaced(c: &Connection) -> usize {
    let open = rows(
        c,
        "SELECT o.id, o.superseded_by AS by,
                (SELECT l.actor||':'||l.action||':'||ifnull(l.detail,'') FROM log l
                  WHERE l.target=o.id AND l.action IN ('resolve','reverse','retire') ORDER BY l.id DESC LIMIT 1) AS how
         FROM claims o
         WHERE o.type='decision' AND ifnull(o.superseded_by,'')!=''
           AND NOT EXISTS (SELECT 1 FROM claim_links k WHERE k.b=o.id AND k.a=o.superseded_by AND k.kind IN ('changed','restates'))",
        &[],
    );
    let mut n = 0;
    for r in &open {
        let how = s(r, "how").to_lowercase();
        let kind = if how.starts_with("resolver:resolve")
            || (how.contains(":retire:")
                && ["duplicate", "repeats", "which says"]
                    .iter()
                    .any(|w| how.contains(w)))
        {
            "restates"
        } else if how.starts_with("resolver:reverse") || how.contains(":retire:") {
            "changed"
        } else {
            continue;
        };
        link(c, s(r, "by"), s(r, "id"), kind, "");
        n += 1;
    }
    n
}

pub fn unresolve(c: &Connection, id: &str) -> bool {
    let reversed = count(
        c,
        "SELECT COUNT(*) FROM claims c WHERE c.id=?1 AND c.state='superseded'
           AND EXISTS (SELECT 1 FROM log l WHERE l.target=?1 AND l.actor='resolver' AND l.action='reverse')",
        &[&id],
    ) > 0;
    if reversed {
        let ok = c
            .execute(
                "UPDATE claims SET state='settled', answer=NULL, superseded_by=NULL, valid_to=NULL, updated=?2 WHERE id=?1",
                params![id, now()],
            )
            .unwrap_or(0)
            == 1;
        if ok {
            c.execute("DELETE FROM log WHERE target=?1 AND action='reverse'", [id])
                .ok();
            c.execute(
                "DELETE FROM claim_links WHERE b=?1 AND kind='changed'",
                [id],
            )
            .ok();
            logw(c, "owner", "unresolve", id, "back to settled");
        }
        return ok;
    }
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
        c.execute(
            "DELETE FROM log WHERE target=?1 AND action='resolve-checked'",
            [id],
        )
        .ok();
        logw(c, "owner", "unresolve", id, "back to proposed");
    }
    ok
}

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
    ids.len() + park_stale(c) + park_closed_threads(c)
}

fn park_closed_threads(c: &Connection) -> usize {
    let rows = crate::db::rows(
        c,
        "SELECT c.id, t.state FROM claims c JOIN threads t ON t.name=c.thread
          WHERE c.origin!='owner' AND c.type IN ('decision','step','question') AND c.state='proposed'
            AND t.state IN ('built','dropped','parked')",
        &[],
    );
    for r in &rows {
        let (id, st) = (crate::db::s(r, "id"), crate::db::s(r, "state"));
        let why = format!("parked: project {st}");
        c.execute(
            "UPDATE claims SET state='superseded', answer=?2, updated=?3 WHERE id=?1",
            params![id, why, now()],
        )
        .ok();
        logw(c, "resolver", "expire", id, &why);
    }
    rows.len()
}

const PARK_DAYS: i64 = 14;

fn park_stale(c: &Connection) -> usize {
    let ids = strings(
        c,
        "SELECT c.id FROM claims c LEFT JOIN sources s ON s.id=c.source_id
          WHERE c.origin!='owner' AND c.type IN ('decision','step','question')
            AND (c.state='proposed' OR (c.state='open' AND c.type='question'))
            AND substr(COALESCE(NULLIF(c.asserted_at,''), s.first_ts, c.created),1,10) < date('now', ?1)",
        &[&format!("-{PARK_DAYS} days")],
    );
    for id in &ids {
        c.execute(
            "UPDATE claims SET state='superseded', answer=?2, updated=?3 WHERE id=?1",
            params![
                id,
                format!("parked: no activity for {PARK_DAYS} days"),
                now()
            ],
        )
        .ok();
        logw(c, "resolver", "expire", id, "parked: no activity");
    }
    ids.len()
}
