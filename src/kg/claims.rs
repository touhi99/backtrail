use super::{clip, entities};
use crate::clock::now;
use crate::db::{find_quote, fxhash, id64, logw, norm, source_id, thread_exists};
use anyhow::Context;
use rusqlite::{Connection, params};

const THREAD_RELATIONS: &[&str] = &["spun out of", "forked from", "built on", "replaced by"];

pub struct NewClaim<'a> {
    pub kind: &'a str,
    pub text: &'a str,
    pub body: &'a str,
    pub source: &'a str,
    pub quote: &'a str,
    pub entities: &'a [String],
    pub stated: &'a str,
    pub origin: &'a str,
}

pub fn add(c: &Connection, claim: &NewClaim<'_>) -> anyhow::Result<String> {
    let relation = claim.kind.starts_with("relation:");
    if !relation && !["state", "rationale"].contains(&claim.kind) {
        anyhow::bail!(
            "type must be state, rationale or relation:<kind> (got {})",
            claim.kind
        );
    }
    if claim.text.trim().is_empty() || claim.text.chars().count() > 160 {
        anyhow::bail!("text must be 1..160 chars");
    }
    if claim.body.chars().count() > 320 {
        anyhow::bail!("body over 320 chars");
    }
    if claim.quote.trim().chars().count() < 8 {
        anyhow::bail!("quote of at least 8 chars is required");
    }
    if relation && !THREAD_RELATIONS.contains(&&claim.kind["relation:".len()..]) {
        anyhow::bail!(
            "relation kind must be one of {} (got '{}')",
            THREAD_RELATIONS.join(", "),
            &claim.kind["relation:".len()..]
        );
    }
    if relation && claim.entities.len() != 2 {
        anyhow::bail!("a relation needs two entities");
    }
    if claim.entities.is_empty() {
        anyhow::bail!("at least one entity is required");
    }
    let source = source_id(c, claim.source)
        .with_context(|| format!("source '{}' not found", claim.source))?;
    let passage = find_quote(c, source, claim.quote).with_context(|| {
        format!(
            "quote not found in {}: \"{}\"",
            claim.source,
            clip(claim.quote, 60)
        )
    })?;
    let mut about = vec![];
    for name in claim.entities {
        about.push(
            entities::find(c, name)
                .with_context(|| format!("unknown entity '{name}'"))?
                .0,
        );
    }
    if relation && about[0] == about[1] {
        anyhow::bail!("a relation needs two different entities");
    }
    if !relation {
        about.sort();
        about.dedup();
    }

    if relation {
        let live: Option<String> = c
            .query_row(
                "SELECT c.id FROM claims c
                 JOIN claim_about s ON s.claim_id=c.id AND s.role='subject' AND s.entity_id=?2
                 JOIN claim_about o ON o.claim_id=c.id AND o.role='object' AND o.entity_id=?3
                 WHERE c.type=?1 AND c.state NOT IN ('rejected','superseded') ORDER BY c.created LIMIT 1",
                params![claim.kind, about[0], about[1]],
                |r| r.get(0),
            )
            .ok();
        if let Some(id) = live {
            return Ok(id);
        }
    }

    let natural = format!("{}{}{}", about.join(","), claim.kind, norm(claim.text));
    let same_content = |id: &str| -> Option<bool> {
        c.query_row(
            "SELECT type=?2 AND lower(title)=lower(?3) FROM claims WHERE id=?1",
            params![id, claim.kind, claim.text],
            |r| r.get(0),
        )
        .ok()
    };
    let old_id = format!("c-{:x}", fxhash(&natural));
    if same_content(&old_id) == Some(true) {
        return Ok(old_id);
    }
    let id = format!("c-{}", id64(&natural));
    match same_content(&id) {
        Some(true) => return Ok(id),
        Some(false) => anyhow::bail!("id {id} is already used by another claim"),
        None => {}
    }

    let thread: Option<String> = c
        .query_row("SELECT thread FROM sources WHERE id=?1", [source], |r| {
            r.get(0)
        })
        .unwrap_or(None);
    c.execute(
        "INSERT INTO claims(id,type,thread,title,body,speaker,asserted_at,state,origin,source_id,chunk_id,quote,stated,created,updated)
         VALUES(?1,?2,?3,?4,?5,?6,?7,'proposed',?8,?9,?10,?11,?12,?13,?13)",
        params![
            id, claim.kind, thread, claim.text, claim.body, passage.speaker, passage.ts, claim.origin, source,
            passage.chunk_id, claim.quote, claim.stated, now()
        ],
    )
    ?;
    for (i, entity) in about.iter().enumerate() {
        let role = if relation && i == 1 {
            "object"
        } else {
            "subject"
        };
        link(c, &id, entity, role);
    }
    crate::items::keep(c, &id);
    Ok(id)
}

fn link(c: &Connection, claim: &str, entity: &str, role: &str) {
    c.execute(
        "INSERT OR IGNORE INTO claim_about(claim_id,entity_id,role) VALUES(?1,?2,?3)",
        params![claim, entity, role],
    )
    .expect("write to database");
}

pub fn owner_relation(c: &Connection, a: &str, b: &str, kind: &str, ts: &str) -> String {
    let subject = entities::ensure(c, a, "project", ts);
    let object = entities::ensure(c, b, "project", ts);
    let id = format!("c-{:x}", fxhash(&format!("{subject}{kind}{object}")));
    c.execute(
        "INSERT OR IGNORE INTO claims(id,type,title,state,origin,speaker,asserted_at,created,updated)
         VALUES(?1,?2,?3,'settled','owner','owner',?4,?4,?4)",
        params![id, format!("relation:{kind}"), format!("{a} {kind} {b}"), ts],
    )
    .expect("write to database");
    link(c, &id, &subject, "subject");
    link(c, &id, &object, "object");
    id
}

pub fn relate(
    c: &Connection,
    a: &str,
    b: &str,
    kind: &str,
    source: &str,
    quote: &str,
    state: &str,
) -> anyhow::Result<()> {
    if !THREAD_RELATIONS.contains(&kind) {
        anyhow::bail!(
            "kind must be spun out of, forked from, built on or replaced by (got {kind})"
        );
    }
    if let Some(unknown) = [a, b].into_iter().find(|t| !thread_exists(c, t)) {
        anyhow::bail!("unknown thread '{unknown}'");
    }
    if a == b {
        anyhow::bail!("a relation needs two different threads");
    }
    let found = source_id(c, source).with_context(|| format!("source '{source}' not found"))?;
    if quote.trim().len() < 8 {
        anyhow::bail!("quote of at least 8 chars from the source is required");
    }
    if find_quote(c, found, quote).is_none() {
        anyhow::bail!("quote not found in {source}: \"{quote}\"");
    }
    let ts = now();
    entities::ensure(c, a, "project", &ts);
    entities::ensure(c, b, "project", &ts);
    let text = format!("{a} {kind} {b}");
    let id = add(
        c,
        &NewClaim {
            kind: &format!("relation:{kind}"),
            text: &text,
            body: "",
            source,
            quote,
            entities: &[a.to_string(), b.to_string()],
            stated: "stated",
            origin: "extracted",
        },
    )?;
    if state != "proposed" {
        let state = if state == "rejected" {
            "rejected"
        } else {
            "settled"
        };
        c.execute(
            "UPDATE claims SET state=?2, updated=?3 WHERE id=?1",
            params![id, state, now()],
        )?;
    }
    let actor = if state == "proposed" {
        "claude-suggest"
    } else {
        "claude"
    };
    logw(c, actor, "relate", &text, quote);
    Ok(())
}

pub fn merge(c: &Connection, keep: &str, duplicate: &str) -> anyhow::Result<()> {
    let exists =
        |id: &str| crate::db::count(c, "SELECT COUNT(*) FROM claims WHERE id=?1", &[&id]) > 0;
    if keep == duplicate || !exists(keep) || !exists(duplicate) {
        anyhow::bail!("both ids must be existing, different claims");
    }
    c.execute(
        "INSERT OR IGNORE INTO claim_about(claim_id,entity_id,role)
         SELECT ?1, entity_id, role FROM claim_about WHERE claim_id=?2",
        params![keep, duplicate],
    )
    .expect("write to database");
    c.execute(
        "UPDATE claims SET state='superseded', superseded_by=?1, valid_to=?3 WHERE id=?2",
        params![keep, duplicate, now()],
    )
    .expect("write to database");
    logw(
        c,
        "owner",
        "kg-merge",
        duplicate,
        &format!("merged into {keep}"),
    );
    Ok(())
}
