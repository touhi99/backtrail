//! History questions over the facts: an entity's page, its lineage, claims found by their own words, neighbours, and
//! `find`, which routes a question to one of them and falls back to the passage search.
use super::{clip, day, entities};
use rusqlite::{Connection, params};
use std::cell::RefCell;

thread_local! {
    /// `--as-of DATE` (YYYY-MM-DD): answer from the claims in force on that day.
    static AS_OF: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Set the day to answer as of; false when it is not a YYYY-MM-DD date.
pub fn set_as_of(d: Option<&str>) -> bool {
    let Some(d) = d else { return true };
    let ok = d.len() == 10
        && d.char_indices()
            .all(|(i, ch)| if i == 4 || i == 7 { ch == '-' } else { ch.is_ascii_digit() });
    if ok {
        AS_OF.with(|a| *a.borrow_mut() = Some(d.to_string()));
    }
    ok
}

fn as_of() -> Option<String> {
    AS_OF.with(|a| a.borrow().clone())
}

/// SQL on a claims row `c` for "in force on the day": said by then and not ended yet. A claim ends on the earliest of its
/// `valid_to`, the day the claim that replaced it (`superseded_by`) was said, and its `retire` log entry (`retire` sets
/// no `valid_to`, and a clean-up can retire a claim long after it was replaced). A claim marked superseded with none of
/// those ends on the day it was last updated (proposals parked after 14 days) or, when only the resolver's `stale` verdict
/// says so, on the day of that check: both are the day the system noticed, not the day the world changed. Rejected suggestions with no end were
/// never in force. Without `--as-of` this is the usual "not retracted".
pub(super) fn in_force() -> String {
    match as_of() {
        None => "c.status!='retracted'".to_string(),
        Some(d) => {
            let end = "MIN(COALESCE(NULLIF(substr(c.valid_to,1,10),''),'9999'), COALESCE((SELECT substr(s.asserted_at,1,10) FROM claims s WHERE s.id=c.superseded_by),'9999'), COALESCE((SELECT MIN(substr(l.ts,1,10)) FROM log l WHERE l.action='retire' AND l.target=c.id),'9999'), COALESCE(CASE WHEN c.state='superseded' THEN substr(c.updated,1,10) WHEN c.stale LIKE 'superseded%' THEN (SELECT MIN(substr(l.ts,1,10)) FROM log l WHERE l.action='flag' AND l.target=c.id) END,'9999'))";
            format!(
                "(substr(ifnull(c.asserted_at,c.created),1,10)<='{d}' AND {end}>'{d}' AND (c.state!='rejected' OR {end}<'9999'))"
            )
        }
    }
}

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
    let where_ = &format!(
        "{}{}",
        where_.replace("c.status!='retracted'", &in_force()),
        excl()
    );
    let sql = "SELECT c.text, c.type, ifnull(c.speaker,''), ifnull(c.asserted_at,c.created), c.status, ifnull(c.quote,''), c.stated, ifnull((SELECT group_concat(o.name, ', ') FROM claim_about x JOIN entities o ON o.id=x.entity_id WHERE x.claim_id=c.id),'') FROM claims c WHERE (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%') AND {where} ORDER BY c.asserted_at DESC LIMIT {limit}"
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
    .map(|mut r: Row| {
        if as_of().is_some() && matches!(r.status.as_str(), "superseded" | "retracted") {
            r.status = "in force then".into();
        }
        r
    })
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
        "SELECT c.id FROM claims c WHERE {} AND (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%'){} AND ({score})>={need} ORDER BY (CASE c.status WHEN 'active' THEN 0 WHEN 'proposed' THEN 1 ELSE 2 END), ({score}) DESC, c.asserted_at DESC LIMIT 5",
        in_force(),
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
    if as_of().is_some() {
        return; // "newer claims exist" is the point of view of today, not of that day
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

/// Entities linked to `fid` by at least one claim in force (a claim naming both): (id, name, shared claims, entity size).
fn links(c: &Connection, fid: &str) -> Vec<(String, String, i64, i64)> {
    let mut st = c
        .prepare(
            &"SELECT o.id, o.name, COUNT(DISTINCT a.claim_id), (SELECT COUNT(*) FROM claim_about z WHERE z.entity_id=o.id)
             FROM claim_about a JOIN claims c ON c.id=a.claim_id AND {}
               AND (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%')
             JOIN claim_about b ON b.claim_id=a.claim_id AND b.entity_id!=a.entity_id
             JOIN entities o ON o.id=b.entity_id WHERE a.entity_id=?1 GROUP BY o.id"
                .replace("{}", &in_force()),
        )
        .unwrap();
    st.query_map(params![fid], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
    })
    .unwrap()
    .flatten()
    .collect()
}

/// Strength of a link: shared claims relative to the size of the entity reached, so a hub does not win by volume.
fn strength(n: &(String, String, i64, i64)) -> f64 {
    n.2 as f64 / (n.3.max(1) as f64).sqrt()
}

/// Entities within `depth` links of this one. Two entities are linked when a claim names both. Each hop keeps the
/// entities with the most shared claims relative to their size (so a hub such as GitHub does not crowd out specific
/// ones), at most `FANOUT[hop]` per entity; what was cut is counted. Each line carries the newest claim behind the link.
const FANOUT: [usize; 3] = [12, 5, 3];
const NEIGHBOR_BUDGET: usize = 2500;

pub fn neighbors(c: &Connection, name: &str, depth: usize) {
    let Some((id, ename)) = entities::find(c, name) else {
        println!("no record: no entity named '{name}'");
        return;
    };
    let depth = depth.clamp(1, 3);
    let mut seen: std::collections::HashSet<String> = [id.clone()].into();
    let mut frontier: Vec<(String, String)> = vec![(id, ename.clone())];
    let mut out = format!("{ename}, links up to {depth} hop(s) away (linked = a claim names both):\n");
    let mut cut_total = 0;
    let mut budget_hit = false;
    let mut found = 0;
    for hop in 0..depth {
        let mut next: Vec<(String, String)> = vec![];
        let mut hop_out = String::new();
        for (fid, fname) in &frontier {
            let mut nb = links(c, fid);
            nb.retain(|n| !seen.contains(&n.0));
            nb.sort_by(|x, y| strength(y).partial_cmp(&strength(x)).unwrap());
            cut_total += nb.len().saturating_sub(FANOUT[hop]);
            for (oid, oname, k, _) in nb.into_iter().take(FANOUT[hop]) {
                if !seen.insert(oid.clone()) {
                    continue;
                }
                found += 1;
                let ev = claim_rows(
                    c,
                    "c.id IN (SELECT x.claim_id FROM claim_about x JOIN claim_about y ON y.claim_id=x.claim_id WHERE x.entity_id=?1 AND y.entity_id=?2) AND c.status!='retracted'",
                    &[fid, &oid],
                    1,
                );
                let mut l = format!("{}{} ({k} shared, via {fname})\n", "  ".repeat(hop), oname);
                if let Some(r) = ev.first() {
                    l.push_str(&format!(
                        "{}  {}\n",
                        "  ".repeat(hop),
                        line(r, 90).replace("\n    ", &format!("\n{}    ", "  ".repeat(hop)))
                    ));
                }
                if tokens(&out) + tokens(&hop_out) + tokens(&l) > NEIGHBOR_BUDGET {
                    budget_hit = true;
                    cut_total += 1;
                    continue;
                }
                hop_out.push_str(&l);
                next.push((oid, oname));
            }
        }
        out.push_str(&hop_out);
        frontier = next;
    }
    if found == 0 {
        println!("no record: {ename} shares no claims with another entity");
        return;
    }
    print!("{out}");
    if cut_total > 0 {
        println!(
            "[{cut_total} more entities not shown{}; lowest shared-per-size first]",
            if budget_hit { " (token cap)" } else { "" }
        );
    }
    println!("[{} tokens]", tokens(&out));
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
    let hits: Vec<(i64, String, String)> = match as_of() {
        None => hits,
        Some(d) => hits
            .into_iter()
            .filter(|(cid, _, _)| {
                c.query_row("SELECT ifnull(substr(ts,1,10),'') FROM chunks WHERE id=?1", params![cid], |x| {
                    x.get::<_, String>(0)
                })
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

const PATH_MAX_HOPS: usize = 5;

/// Shortest chain of claims between two entities: breadth-first over "a claim names both", the strongest link first
/// when several chains are equally short. Each link is printed with its newest claim (date, speaker, quote).
pub fn path(c: &Connection, a: &str, b: &str) {
    let (Some((aid, aname)), Some((bid, bname))) = (entities::find(c, a), entities::find(c, b)) else {
        println!("no record: no entity named '{}'", if entities::find(c, a).is_none() { a } else { b });
        return;
    };
    if aid == bid {
        println!("{aname} and {bname} are the same entity");
        return;
    }
    // id -> (parent id, shared claims on the link, name, path strength so far)
    let mut seen: std::collections::HashMap<String, (String, i64, String, f64)> = Default::default();
    seen.insert(aid.clone(), (String::new(), 0, aname.clone(), 0.0));
    let mut frontier = vec![aid.clone()];
    let mut expanded = 0;
    for _ in 0..PATH_MAX_HOPS {
        let mut next: std::collections::HashMap<String, (String, i64, String, f64)> = Default::default();
        for fid in &frontier {
            expanded += 1;
            let base = seen[fid].3;
            for n in links(c, fid) {
                if seen.contains_key(&n.0) {
                    continue;
                }
                let sc = base + strength(&n);
                if next.get(&n.0).map_or(true, |x| sc > x.3) {
                    next.insert(n.0.clone(), (fid.clone(), n.2, n.1.clone(), sc));
                }
            }
        }
        if next.is_empty() {
            break;
        }
        let found = next.contains_key(&bid);
        frontier = next.keys().cloned().collect();
        seen.extend(next);
        if found {
            let mut chain = vec![bid.clone()];
            while let Some(p) = seen.get(chain.last().unwrap()).map(|x| x.0.clone()) {
                if p.is_empty() {
                    break;
                }
                chain.push(p);
            }
            chain.reverse();
            println!("{aname} to {bname}, {} link(s):", chain.len() - 1);
            for w in chain.windows(2) {
                let (from, to) = (&seen[&w[0]].2, &seen[&w[1]].2);
                println!("{from} - {to} ({} shared)", seen[&w[1]].1);
                for r in claim_rows(
                    c,
                    "c.id IN (SELECT x.claim_id FROM claim_about x JOIN claim_about y ON y.claim_id=x.claim_id WHERE x.entity_id=?1 AND y.entity_id=?2) AND c.status!='retracted'",
                    &[&w[0], &w[1]],
                    1,
                ) {
                    println!("  {}", line(&r, 110).replace("\n    ", "\n      "));
                }
            }
            return;
        }
    }
    println!(
        "no record: no chain of claims from {aname} to {bname} within {PATH_MAX_HOPS} links ({expanded} entities searched)"
    );
}
