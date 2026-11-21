use crate::clock::now;
use crate::db::{count, logw, rows, s, strings};
use anyhow::Context;
use rusqlite::{Connection, params};
use serde_json::Value;

use super::candidates::candidates;
use super::clip;
use super::lifecycle::link;
use super::when;

pub fn reverse_candidates(c: &Connection, id: &str, limit: usize) -> Vec<Value> {
    let said = when(c, id);
    if said.is_empty() {
        return vec![];
    }
    let older = |o: &str| o != id && when(c, o) <= said;
    let mut ids: Vec<String> = candidates(c, id, 60)
        .into_iter()
        .filter(|k| k.kind == "decision" && k.state == "settled" && older(&k.id))
        .map(|k| k.id)
        .take(8)
        .collect();
    let by_entity = strings(
        c,
        "SELECT o.id FROM claims o
           JOIN claim_about b ON b.claim_id=o.id
           JOIN claim_about a ON a.entity_id=b.entity_id AND a.claim_id=?1
          WHERE o.id!=?1 AND o.type='decision' AND o.state='settled'
          GROUP BY o.id
          ORDER BY MIN((SELECT COUNT(*) FROM claim_about x WHERE x.entity_id=a.entity_id)), o.id",
        &[&id],
    );
    for o in by_entity {
        if ids.len() >= limit {
            break;
        }
        if !ids.contains(&o) && older(&o) {
            ids.push(o);
        }
    }
    ids.truncate(limit);
    ids.iter()
        .flat_map(|o| {
            rows(
                c,
                "SELECT id, ifnull(thread,'') AS thread, title, ifnull(quote,'') AS quote FROM claims WHERE id=?1",
                &[o],
            )
            .into_iter()
            .map(|mut r| {
                r["day"] = Value::from(when(c, o).chars().take(10).collect::<String>());
                r
            })
        })
        .collect()
}

pub fn reverse_packet(c: &Connection, id: &str) {
    let me = rows(
        c,
        "SELECT ifnull(thread,'') AS thread, title, ifnull(body,'') AS body, ifnull(quote,'') AS quote
         FROM claims WHERE id=?1 AND type='decision' AND state='settled'",
        &[&id],
    );
    let Some(me) = me.first() else {
        eprintln!("not a settled decision");
        std::process::exit(1)
    };
    println!("BACKTRAIL-REVERSE\nITEM {id}\nTODAY {}", &now()[..10]);
    println!(
        "THREAD {}\nDATE {}\nTITLE {}\nBODY {}\nQUOTE {}",
        s(me, "thread"),
        when(c, id).chars().take(10).collect::<String>(),
        s(me, "title"),
        s(me, "body"),
        s(me, "quote")
    );
    println!("\nOLDER SETTLED DECISIONS:");
    for k in reverse_candidates(c, id, 24) {
        println!(
            "- {} [{}] {}: {} | said: {}",
            s(&k, "id"),
            s(&k, "thread"),
            s(&k, "day"),
            clip(s(&k, "title"), 160),
            clip(s(&k, "quote"), 140)
        );
    }
    println!("\nREASONS SAID IN THE SAME SESSION:");
    for r in rows(
        c,
        "SELECT r.id, r.title, ifnull(r.quote,'') AS quote FROM claims r JOIN claims d ON d.id=?1
         WHERE r.type='rationale' AND r.source_id=d.source_id AND r.state!='rejected'",
        &[&id],
    ) {
        println!(
            "- {}: {} | said: {}",
            s(&r, "id"),
            clip(s(&r, "title"), 160),
            clip(s(&r, "quote"), 140)
        );
    }
}

pub fn reverse_queue(c: &Connection, limit: usize) {
    let mut listed = 0;
    for id in strings(
        c,
        "SELECT id FROM claims WHERE type='decision' AND state='settled'
           AND NOT EXISTS (SELECT 1 FROM log l WHERE l.target=claims.id AND l.action='reverse-checked')
         ORDER BY COALESCE(NULLIF(asserted_at,''), created) DESC, id",
        &[],
    ) {
        if listed >= limit {
            break;
        }
        if reverse_candidates(c, &id, 1).is_empty() {
            logw(c, "resolver", "reverse-checked", &id, "nothing older to compare");
        } else {
            println!("{id}");
            listed += 1;
        }
    }
}

pub(super) fn apply_reverse(c: &Connection, v: &Value) -> anyhow::Result<String> {
    let id = v["id"].as_str().context("no id")?;
    let reason = clip(v["reason"].as_str().unwrap_or("").trim(), 320);
    let settled = count(
        c,
        "SELECT COUNT(*) FROM claims WHERE id=?1 AND type='decision' AND state IN ('settled','superseded')",
        &[&id],
    );
    if settled == 0 {
        anyhow::bail!("the item is not a settled or replaced decision");
    }
    let said = when(c, id);
    let mut done = vec![];
    for old in v["replaces"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if reason.is_empty() {
            anyhow::bail!("a reversal needs a reason");
        }
        let ok = old != id
            && count(
                c,
                "SELECT COUNT(*) FROM claims WHERE id=?1 AND type='decision' AND state='settled'",
                &[&old],
            ) == 1
            && !said.is_empty()
            && when(c, old) <= said;
        if !ok {
            continue;
        }
        let ts = now();
        c.execute(
            "UPDATE claims SET state='superseded', answer=?2, superseded_by=?3, valid_to=?4, updated=?4 WHERE id=?1",
            params![old, reason, id, ts],
        )
        ?;
        logw(c, "resolver", "reverse", old, &format!("by {id}: {reason}"));
        link(c, id, old, "changed", &reason);
        done.push(format!("replaces {old}"));
    }
    let ids = |key: &str| -> Vec<String> {
        v[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect()
    };
    for old in ids("restates") {
        let ok = old != id
            && count(
                c,
                "SELECT COUNT(*) FROM claims WHERE id=?1 AND type='decision' AND state IN ('settled','superseded')",
                &[&old],
            ) == 1
            && when(c, &old) <= said;
        if ok {
            link(c, id, &old, "restates", "");
            done.push(format!("restates {old}"));
        }
    }
    for why in ids("because") {
        if count(
            c,
            "SELECT COUNT(*) FROM claims WHERE id=?1 AND type='rationale' AND state!='rejected'",
            &[&why],
        ) == 1
        {
            link(c, id, &why, "because", "");
            done.push(format!("because {why}"));
        }
    }
    logw(c, "resolver", "reverse-checked", id, &done.join("; "));
    Ok(done.join("; "))
}
