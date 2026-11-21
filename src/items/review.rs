use super::{MAX_BODY, clip, update};
use crate::clock::now;
use crate::db::logw;
use rusqlite::Connection;

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
