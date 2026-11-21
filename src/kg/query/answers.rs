use super::page::{page, trace};
use super::{Row, as_of, claim_rows, excl, in_force, line};
use crate::kg::{clip, day, entities};
use rusqlite::{Connection, params};

fn stem(w: &str) -> String {
    let w = w.to_lowercase();
    for suf in ["ing", "ed", "es", "ly", "s"] {
        if w.len() > suf.len() + 3 && w.ends_with(suf) {
            return w[..w.len() - suf.len()].to_string();
        }
    }
    w
}

const STOP: &[&str] = &[
    "the", "and", "for", "why", "what", "did", "does", "how", "was", "were", "with", "that",
    "this", "from", "have", "has", "had", "our", "you", "your", "are", "not", "but", "about",
    "when", "who", "which", "should", "would", "could", "there", "their", "been", "also", "still",
    "just", "dont", "want", "wanna", "thing", "like", "use", "need", "make", "owner", "can", "its",
    "isnt", "yourself", "yes", "okay",
];

pub fn why(c: &Connection, q: &str) -> bool {
    let mut ws: Vec<String> = q
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3 && !STOP.contains(&w.to_lowercase().as_str()))
        .map(|w| format!("% {}%", stem(w)))
        .collect();
    ws.sort();
    ws.dedup();
    ws.truncate(10);
    if ws.is_empty() {
        println!("no record: query has no content words");
        return false;
    }
    let hit = |i: usize| {
        format!(
            "(' '||lower(c.text||' '||ifnull(c.body,'')||' '||ifnull(c.quote,'')) LIKE ?{})",
            i + 1
        )
    };
    let score = (0..ws.len())
        .map(hit)
        .map(|h| format!("(CASE WHEN {h} THEN 1 ELSE 0 END)"))
        .collect::<Vec<_>>()
        .join("+");
    let need = if ws.len() < 3 {
        ws.len()
    } else {
        ws.len().div_ceil(3).max(2)
    };
    let args: Vec<&dyn rusqlite::ToSql> = ws.iter().map(|w| w as &dyn rusqlite::ToSql).collect();
    let sql = format!(
        "SELECT c.id FROM claims c WHERE {} AND (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%'){} AND ({score})>={need} ORDER BY (CASE c.status WHEN 'active' THEN 0 WHEN 'proposed' THEN 1 ELSE 2 END), ({score}) DESC, c.asserted_at DESC LIMIT 5",
        in_force(),
        excl()
    );
    let ids: Vec<String> = c
        .prepare(&sql)
        .expect("prepare query")
        .query_map(&args[..], |r| r.get::<_, String>(0))
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .collect();
    if ids.is_empty() {
        println!("no record in claims");
        return false;
    }
    let rows: Vec<Row> = ids
        .iter()
        .flat_map(|id| claim_rows(c, "c.id=?1", &[id], 1))
        .collect();
    print_with_notes(c, &rows);
    true
}

fn print_with_notes(c: &Connection, rows: &[Row]) {
    for r in rows {
        println!("{}\n    about: {}", line(r, 160), r.ents);
    }
    if as_of().is_some() {
        return;
    }
    let mut names: Vec<&str> = rows
        .iter()
        .flat_map(|r| r.ents.split(", "))
        .filter(|e| !e.is_empty())
        .collect();
    names.sort_unstable();
    names.dedup();
    let mut said = 0;
    for name in names {
        let shown = rows
            .iter()
            .filter(|r| r.ents.split(", ").any(|e| e == name))
            .filter_map(|r| r.ts.as_deref())
            .max()
            .unwrap_or("");
        let latest: Option<String> = c
            .query_row(
                "SELECT MAX(c.asserted_at) FROM claims c JOIN claim_about a ON a.claim_id=c.id JOIN entities e ON e.id=a.entity_id
                 WHERE e.name=?1 AND c.state!='rejected' AND (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%')",
                params![name],
                |x| x.get(0),
            )
            .ok()
            .flatten();
        if let Some(l) = latest
            && said < 3
            && l.get(..10) > shown.get(..10)
        {
            println!(
                "[newer claims about {name} exist, latest {}: `kg trace \"{name}\"`]",
                day(Some(&l))
            );
            said += 1;
        }
    }
}

pub(super) fn links(c: &Connection, fid: &str) -> Vec<(String, String, i64, i64)> {
    let mut st = c
        .prepare(
            &"SELECT o.id, o.name, COUNT(DISTINCT a.claim_id), (SELECT COUNT(*) FROM claim_about z WHERE z.entity_id=o.id)
             FROM claim_about a JOIN claims c ON c.id=a.claim_id AND {}
               AND (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%')
             JOIN claim_about b ON b.claim_id=a.claim_id AND b.entity_id!=a.entity_id
             JOIN entities o ON o.id=b.entity_id WHERE a.entity_id=?1 GROUP BY o.id"
                .replace("{}", &in_force()),
        )
        .expect("prepare query");
    st.query_map(params![fid], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
    })
    .expect("run query")
    .map(|row| row.expect("decode query row"))
    .collect()
}

pub(super) fn strength(n: &(String, String, i64, i64)) -> f64 {
    n.2 as f64 / (n.3.max(1) as f64).sqrt()
}

pub fn ask(c: &Connection, q: &str) -> bool {
    let r = crate::search::search(
        c,
        q,
        &crate::search::Filters::default(),
        crate::search::Mode::Semantic,
    );
    let skip = match std::env::var("BACKTRAIL_EXCLUDE_SESSION") {
        Ok(r) if !r.is_empty() => r,
        _ => String::new(),
    };
    let hits: Vec<(i64, String, String)> = r["results"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter(|x| skip.is_empty() || !x["ref"].as_str().unwrap_or("").starts_with(&skip))
        .take(15)
        .map(|x| {
            (
                x["chunk_id"].as_i64().unwrap_or(0),
                x["ref"].as_str().unwrap_or("").to_string(),
                x["snip"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    let hits: Vec<(i64, String, String)> = match as_of() {
        None => hits,
        Some(d) => hits
            .into_iter()
            .filter(|(cid, _, _)| {
                c.query_row(
                    "SELECT ifnull(substr(ts,1,10),'') FROM chunks WHERE id=?1",
                    params![cid],
                    |x| x.get::<_, String>(0),
                )
                .map_or(true, |t| t.as_str() <= d.as_str())
            })
            .collect(),
    };
    if hits.is_empty() {
        println!("no record");
        return false;
    }
    let mode = r["mode"].as_str().unwrap_or("text");
    let mut seen = std::collections::HashSet::new();
    let mut found: Vec<Row> = vec![];
    for (cid, _, _) in &hits {
        for row in claim_rows(c, "c.chunk_id=?1 AND c.status!='retracted'", &[cid], 3) {
            if found.len() < 5 && seen.insert(row.text.clone()) {
                found.push(row);
            }
        }
    }
    if !found.is_empty() {
        println!("claims citing the best passages ({mode} search):");
        print_with_notes(c, &found);
        return true;
    }
    println!("no claim cites the best passages ({mode} search); passages:");
    for (cid, rf, snip) in hits.iter().take(5) {
        let (role, ts): (String, Option<String>) = c
            .query_row(
                "SELECT ifnull(role,''), ts FROM chunks WHERE id=?1",
                params![cid],
                |x| Ok((x.get(0)?, x.get(1)?)),
            )
            .unwrap_or_default();
        let snip = snip
            .replace("<mark>", "[")
            .replace("</mark>", "]")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">");
        println!(
            "{} {rf} {} | {}",
            day(ts.as_deref()),
            if role == "user" { "owner" } else { "assistant" },
            clip(&snip, 170)
        );
    }
    true
}

pub fn find(c: &Connection, q: &str) {
    let low = q.to_lowercase();
    let ents = entities::mentions(c, q);
    let lineage = [
        "lineage",
        "came from",
        "come from",
        "where did",
        "built on",
        "spun out",
        "history of",
        "how did",
        "started from",
    ]
    .iter()
    .any(|k| low.contains(k));
    let overview = [
        "status",
        "remaining",
        "what is",
        "what's",
        "tell me about",
        "overview",
        "where are we",
    ]
    .iter()
    .any(|k| low.contains(k));
    if let Some((_, name)) = ents.first() {
        if lineage && ents.len() >= 2 {
            let ids = ents
                .iter()
                .map(|e| format!("'{}'", e.0))
                .collect::<Vec<_>>()
                .join(",");
            let mut rows = claim_rows(
                c,
                &format!(
                    "c.type LIKE 'relation:%' AND c.status!='retracted' AND (SELECT COUNT(DISTINCT entity_id) FROM claim_about x WHERE x.claim_id=c.id AND x.entity_id IN ({ids}))>=2"
                ),
                &[],
                30,
            );
            rows.reverse();
            if !rows.is_empty() {
                println!(
                    "lineage among {}, oldest first:",
                    ents.iter()
                        .map(|e| e.1.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                for r in &rows {
                    println!("{}", line(r, 110));
                }
                return;
            }
        }
        if lineage {
            trace(c, name);
            return;
        }
        if overview {
            page(c, name, 700);
            return;
        }
    }
    if !why(c, q) {
        ask(c, q);
    }
    if let Some((_, name)) = ents.first() {
        let n = crate::kg::extract::backlog(c, name);
        if n > 0 {
            println!(
                "[{n} sessions that mention {name} have no extracted claims yet: skill/extract.sh --entity \"{name}\"]"
            );
        }
    }
}
