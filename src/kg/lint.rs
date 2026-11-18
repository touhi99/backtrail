//! Graph checks, printed as part of `bt check`.
use super::clip;
use crate::db::{fxhash, norm};
use rusqlite::{Connection, params};

/// Graph checks for `backtrail check`. Precision needs judgment, so a fixed sample is printed instead of a score.
pub fn lint(c: &Connection) {
    let n = |sql: &str| -> i64 { c.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };
    println!(
        "graph: {} entities, {} claims",
        n("SELECT COUNT(*) FROM entities"),
        n("SELECT COUNT(*) FROM claims")
    );
    let empty: Vec<String> = c.prepare("SELECT name FROM entities e WHERE NOT EXISTS (SELECT 1 FROM claim_about a WHERE a.entity_id=e.id)").expect("prepare query").query_map([], |r| r.get(0)).expect("run query").map(|row| row.expect("decode query row")).collect();
    println!(
        "entities with no claims: {}{}",
        empty.len(),
        if empty.is_empty() {
            String::new()
        } else {
            format!(" ({})", empty.join(", "))
        }
    );
    println!(
        "claims with no evidence: {}",
        n(
            "SELECT COUNT(*) FROM claims c WHERE c.quote IS NULL AND c.origin!='owner' AND (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%')"
        )
    );
    println!(
        "claims about no entity: {}",
        n(
            "SELECT COUNT(*) FROM claims c WHERE (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%') AND NOT EXISTS (SELECT 1 FROM claim_about a WHERE a.claim_id=c.id)"
        )
    );
    println!(
        "relations without a subject and an object: {}",
        n(
            "SELECT COUNT(*) FROM claims c WHERE c.type LIKE 'relation:%' AND c.state NOT IN ('rejected','superseded') AND (NOT EXISTS (SELECT 1 FROM claim_about a WHERE a.claim_id=c.id AND a.role='subject') OR NOT EXISTS (SELECT 1 FROM claim_about a WHERE a.claim_id=c.id AND a.role='object'))"
        )
    );
    let mut lost = 0;
    let mut st = c.prepare("SELECT c.id, c.quote, k.text FROM claims c LEFT JOIN chunks k ON k.id=c.chunk_id WHERE c.quote IS NOT NULL").expect("prepare query");
    for (id, q, t) in st
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })
        .expect("run query")
        .map(|row| row.expect("decode query row"))
    {
        if t.is_none_or(|t| !norm(&t).contains(&norm(&q))) {
            lost += 1;
            if lost <= 5 {
                println!("  quote no longer found: {id}");
            }
        }
    }
    println!("claims whose quote is no longer in its passage: {lost}");
    let tok = |s: &str| -> std::collections::HashSet<String> {
        s.to_lowercase()
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|w| w.len() > 3)
            .map(String::from)
            .collect()
    };
    let mut st = c.prepare("SELECT c.id, c.title, ( SELECT group_concat(entity_id) FROM ( SELECT entity_id FROM claim_about WHERE claim_id=c.id ORDER BY entity_id ) ) FROM claims c WHERE c.state NOT IN ('rejected','superseded') AND (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%')").expect("prepare query");
    let all: Vec<(String, String, String)> = st
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get::<_, Option<String>>(2)?.unwrap_or_default(),
            ))
        })
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .collect();
    let toks: Vec<_> = all.iter().map(|x| tok(&x.1)).collect();
    let mut dups = 0;
    for i in 0..all.len() {
        for j in i + 1..all.len() {
            if all[i].2 != all[j].2 || all[i].2.is_empty() {
                continue;
            }
            let inter = toks[i].intersection(&toks[j]).count() as f64;
            let uni = toks[i].union(&toks[j]).count() as f64;
            if uni > 0.0 && inter / uni >= 0.6 {
                dups += 1;
                if dups <= 20 {
                    println!(
                        "  probable duplicate: \"{}\" / \"{}\"",
                        clip(&all[i].1, 60),
                        clip(&all[j].1, 60)
                    );
                }
            }
        }
    }
    println!("probable duplicate claim pairs: {dups}");
    println!("sample for review (fixed order, judge each against its quote):");
    let mut sample = all;
    sample.sort_by_key(|x| fxhash(&x.0));
    for (id, t, _) in sample.iter().take(5) {
        let q: String = c
            .query_row(
                "SELECT ifnull(quote,'') FROM claims WHERE id=?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap_or_default();
        println!("  \"{}\"  <-  \"{}\"", clip(t, 70), clip(&q, 70));
    }
}
