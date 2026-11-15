//! History questions over the facts: an entity's page, its lineage, claims found by their own words, neighbours, and
//! `find`, which routes a question to one of them and falls back to the passage search.
use super::{clip, day, entities};
use rusqlite::{Connection, params};

pub fn stats(c: &Connection) {
    let n = |sql: &str| -> i64 { c.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };
    println!(
        "entities {} (aliases {}), claims {} (with a quote {}), overrides {}",
        n("SELECT COUNT(*) FROM entities"),
        n("SELECT COUNT(*) FROM entity_aliases"),
        n("SELECT COUNT(*) FROM claims"),
        n("SELECT COUNT(*) FROM claims WHERE quote IS NOT NULL"),
        n("SELECT COUNT(*) FROM kg_overrides")
    );
    println!(
        "claims without evidence: {}",
        n("SELECT COUNT(*) FROM claims c WHERE c.quote IS NULL AND c.origin!='owner'")
    );
}

/// Thread state suggested by session activity alone (no model). Prints suggestions; changes nothing.
pub fn state(c: &Connection) {
    let days = |ts: &str| -> i64 { crate::clock::days_ago(ts) };
    let mut st = c.prepare("SELECT t.name, t.state, ( SELECT MAX(last_ts) FROM sources s WHERE s.thread=t.name AND s.kind='session' ), ( SELECT COUNT(*) FROM sources s WHERE s.thread=t.name AND s.kind='session' AND s.last_ts >= date('now','-3 day') ) FROM threads t WHERE t.name!='Unsorted'").unwrap();
    let mut n = 0;
    for (name, state, last, recent) in st
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })
        .unwrap()
        .flatten()
    {
        let Some(last) = last else { continue };
        let idle = days(&last);
        if state == "moving" && idle >= 14 {
            println!("{name}: marked moving, no session for {idle} days -> quiet?");
            n += 1;
        } else if matches!(state.as_str(), "built" | "parked" | "dropped") && recent >= 2 {
            println!(
                "{name}: marked {state}, {recent} sessions in the last 3 days -> active again?"
            );
            n += 1;
        } else if state == "open" && idle >= 10 {
            println!("{name}: open idea, idle {idle} days");
            n += 1;
        }
    }
    if n == 0 {
        println!("no state changes suggested");
    }
}

struct Row {
    text: String,
    speaker: String,
    ts: Option<String>,
    status: String,
    quote: String,
    stated: String,
    ents: String,
}

/// Evaluation only: BACKTRAIL_EXCLUDE_SESSION=session:xxxxxxxx hides claims and passages from that session, to ask a
/// question as if it were put in a fresh session.
fn excl() -> String {
    match std::env::var("BACKTRAIL_EXCLUDE_SESSION") {
        Ok(r)
            if r.chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == ':' || ch == '-')
                && !r.is_empty() =>
        {
            // a prefix is enough: `session:xxxxxxxx` matches the stored full id
            format!(
                " AND NOT EXISTS (SELECT 1 FROM sources xs WHERE xs.id=c.source_id AND substr(xs.ref,1,{})='{r}')",
                r.len()
            )
        }
        _ => String::new(),
    }
}

fn claim_rows(
    c: &Connection,
    where_: &str,
    args: &[&dyn rusqlite::ToSql],
    limit: usize,
) -> Vec<Row> {
    let where_ = &format!("{where_}{}", excl());
    let sql = "SELECT c.text, c.type, ifnull(c.speaker,''), c.asserted_at, c.status, ifnull(c.quote,''), c.stated, ifnull((SELECT group_concat(o.name, ', ') FROM claim_about x JOIN entities o ON o.id=x.entity_id WHERE x.claim_id=c.id),'') FROM claims c WHERE (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%') AND {where} ORDER BY c.asserted_at DESC LIMIT {limit}"
        .replace("{where}", where_)
        .replace("{limit}", &limit.to_string());
    let mut st = c.prepare(&sql).unwrap();
    st.query_map(args, |r| {
        Ok(Row {
            text: r.get(0)?,
            speaker: r.get(2)?,
            ts: r.get(3)?,
            status: r.get(4)?,
            quote: r.get(5)?,
            stated: r.get(6)?,
            ents: r.get(7)?,
        })
    })
    .unwrap()
    .flatten()
    .collect()
}

fn who(s: &str) -> &str {
    match s {
        "owner" => "owner",
        "assistant" => "assistant",
        _ => "?",
    }
}

fn line(r: &Row, qlen: usize) -> String {
    let head = format!(
        "{} {} [{}{}] {}",
        day(&r.ts),
        who(&r.speaker),
        r.status,
        if r.stated == "inferred" {
            ", inferred"
        } else {
            ""
        },
        r.text
    );
    if r.quote.is_empty() {
        head
    } else {
        format!("{head}\n    \"{}\"", clip(&r.quote, qlen))
    }
}

fn tokens(s: &str) -> usize {
    s.chars().count() / 4
}

/// Entity page: header, newest claims in full, older claims rolled up by month, within a token budget.
pub fn page(c: &Connection, name: &str, budget: usize) {
    let Some((id, ename)) = entities::find(c, name) else {
        println!("no record: no entity named '{name}'");
        return;
    };
    let aliases: Vec<String> = c
        .prepare("SELECT alias FROM entity_aliases WHERE entity_id=?1")
        .unwrap()
        .query_map(params![id], |r| r.get::<_, String>(0))
        .unwrap()
        .flatten()
        .collect();
    let all = claim_rows(
        c,
        "c.id IN (SELECT claim_id FROM claim_about WHERE entity_id=?1) AND c.status!='retracted'",
        &[&id],
        2000,
    );
    let mut out = format!(
        "{ename} ({} claims; also: {})\n",
        all.len(),
        aliases.join(", ")
    );
    let reserve = 220;
    let mut shown = 0;
    for r in all.iter().take(25) {
        let l = format!("{}\n", line(r, 110));
        if tokens(&out) + tokens(&l) + reserve > budget {
            break;
        }
        out.push_str(&l);
        shown += 1;
    }
    let rest = &all[shown..];
    if !rest.is_empty() {
        out.push_str("older, rolled up by month:\n");
        let mut by: Vec<(String, usize)> = vec![];
        for r in rest {
            let m =
                r.ts.as_deref()
                    .and_then(|t| t.get(..7))
                    .unwrap_or("?")
                    .to_string();
            match by.last_mut() {
                Some(l) if l.0 == m => l.1 += 1,
                _ => by.push((m, 1)),
            }
        }
        for (m, n) in by {
            let l = format!("  {m}: {n} claims\n");
            if tokens(&out) + tokens(&l) > budget {
                out.push_str("  …\n");
                break;
            }
            out.push_str(&l);
        }
    }
    print!("{out}");
    println!("[{} tokens, budget {budget}]", tokens(&out));
}

/// Dated lineage and decisions for one entity, oldest first, each with speaker and quote.
pub fn trace(c: &Connection, name: &str) {
    let Some((id, ename)) = entities::find(c, name) else {
        println!("no record: no entity named '{name}'");
        return;
    };
    let mut rows = claim_rows(
        c,
        "c.id IN (SELECT claim_id FROM claim_about WHERE entity_id=?1) AND c.status!='retracted' AND (c.type LIKE 'relation:%' OR c.type='decision')",
        &[&id],
        60,
    );
    rows.reverse();
    if rows.is_empty() {
        println!("no record: no dated claims about {ename}");
        return;
    }
    println!("{ename}, oldest first:");
    for r in rows.iter().take(30) {
        println!("{}", line(r, 100));
    }
}

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

/// Claims that hold the query's content words (stemmed) in their text, body or quote, best match first, newest first.
/// At least two words must match (all of them when the query has fewer than three); "no record" otherwise.
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
        .map(|i| format!("{}", hit(i)))
        .map(|h| format!("(CASE WHEN {h} THEN 1 ELSE 0 END)"))
        .collect::<Vec<_>>()
        .join("+");
    let need = if ws.len() < 3 {
        ws.len()
    } else {
        ((ws.len() + 2) / 3).max(2)
    };
    let args: Vec<&dyn rusqlite::ToSql> = ws.iter().map(|w| w as &dyn rusqlite::ToSql).collect();
    let sql = format!(
        "SELECT c.id FROM claims c WHERE c.status!='retracted' AND (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%'){} AND ({score})>={need} ORDER BY (CASE c.status WHEN 'active' THEN 0 WHEN 'proposed' THEN 1 ELSE 2 END), ({score}) DESC, c.asserted_at DESC LIMIT 5",
        excl()
    );
    let ids: Vec<String> = c
        .prepare(&sql)
        .unwrap()
        .query_map(&args[..], |r| r.get::<_, String>(0))
        .unwrap()
        .flatten()
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

/// Print claims, then one line per entity they are about when newer claims about that entity exist than any shown.
/// Wording alone cannot tell a changed mind from a restatement, so this only reports dates.
fn print_with_notes(c: &Connection, rows: &[Row]) {
    for r in rows {
        println!("{}\n    about: {}", line(r, 160), r.ents);
    }
    let mut names: Vec<&str> = rows
        .iter()
        .flat_map(|r| r.ents.split(", "))
        .filter(|e| !e.is_empty())
        .collect();
    names.sort();
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
        if let Some(l) = latest {
            if said < 3 && l.get(..10) > shown.get(..10) {
                println!(
                    "[newer claims about {name} exist, latest {}: `kg trace \"{name}\"`]",
                    day(&Some(l))
                );
                said += 1;
            }
        }
    }
}

/// Entities that share claims with this one, then (2nd hop) the newest few claims of the top neighbours. Capped.
pub fn neighbors(c: &Connection, name: &str) {
    let Some((id, ename)) = entities::find(c, name) else {
        println!("no record: no entity named '{name}'");
        return;
    };
    let mut st = c.prepare("SELECT o.id, o.name, COUNT(*) AS n FROM claim_about a JOIN claim_about b ON a.claim_id=b.claim_id AND b.entity_id!=a.entity_id JOIN entities o ON o.id=b.entity_id WHERE a.entity_id=?1 GROUP BY o.id ORDER BY n DESC LIMIT 12").unwrap();
    let nb: Vec<(String, String, i64)> = st
        .query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .flatten()
        .collect();
    if nb.is_empty() {
        println!("no record: {ename} shares no claims with another entity");
        return;
    }
    println!("{ename} shares claims with:");
    for (_, n, k) in &nb {
        println!("  {n} ({k})");
    }
    println!("newest claims about the top 3 neighbours (hop 2, 3 each):");
    for (nid, n, _) in nb.iter().take(3) {
        println!("  {n}:");
        for r in claim_rows(
            c,
            "c.id IN (SELECT claim_id FROM claim_about WHERE entity_id=?1) AND c.status!='retracted'",
            &[nid],
            3,
        ) {
            println!("  {}", line(&r, 70).replace("\n    ", "\n      "));
        }
    }
}

/// Full-text passages, prefiltered to the entity's sessions or a time window when given.
/// The last route of `find`: the same hybrid search that `/search` and `bt search` use (keyword and vectors,
/// merged). Claims that cite the best passages are printed first, with date, speaker and quote. When no claim
/// cites them, the passages themselves are printed. Returns false when nothing matched.
pub fn ask(c: &Connection, q: &str) -> bool {
    let r = crate::search::search(c, q, &crate::search::Filters::default(), true);
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
            day(&ts),
            if role == "user" { "owner" } else { "assistant" },
            clip(&snip, 170)
        );
    }
    true
}

/// Router for a history question: lineage questions trace the entity; questions about an entity's state or contents
/// show its page; everything else looks up claims by their own words, then searches passages by meaning and keyword.
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
        let n = super::extract::backlog(c, name);
        if n > 0 {
            println!(
                "[{n} sessions that mention {name} have no extracted claims yet: skill/extract.sh --entity \"{name}\"]"
            );
        }
    }
}
