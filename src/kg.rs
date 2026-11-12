//! Knowledge-graph tables (plan: notes/backtrail/knowledge-graph-plan.md). Entities with aliases, claims, and
//! one-to-many evidence. `links` stays in place; `migrate_links` copies its evidenced thread relations as claims.
use crate::db::{fxhash, logw, norm};
use rusqlite::{Connection, params};

pub fn ensure(c: &Connection) {
    c.execute_batch(include_str!("sql/kg_schema.sql"))
        .expect("kg schema");
}

fn entity_for(c: &Connection, name: &str, kind: &str, now: &str) -> String {
    let id = format!("e-{:x}", fxhash(&norm(name)));
    c.execute(
        "INSERT OR IGNORE INTO entities(id,name,kind,created) VALUES(?1,?2,?3,?4)",
        params![id, name, kind, now],
    )
    .unwrap();
    c.execute(
        "INSERT OR IGNORE INTO entity_aliases(alias,entity_id) VALUES(?1,?2)",
        params![norm(name), id],
    )
    .unwrap();
    id
}

/// Copy thread-to-thread links that carry a source and quote into claims with evidence. Idempotent; `links` is not changed.
/// Item-to-item links and links without a quote are counted and left out.
pub fn migrate_links(c: &Connection) -> (usize, usize) {
    let now = crate::db::chrono_now();
    let rows: Vec<(
        String,
        String,
        String,
        Option<i64>,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = {
        let mut st = c
            .prepare("SELECT a,b,kind,source_id,chunk_id,quote,ts,state FROM links")
            .unwrap();
        st.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        })
        .unwrap()
        .flatten()
        .collect()
    };
    let (mut done, mut skipped) = (0, 0);
    for (a, b, kind, sid, cid, quote, ts, state) in rows {
        let (Some(an), Some(bn)) = (a.strip_prefix("T:"), b.strip_prefix("T:")) else {
            skipped += 1;
            continue;
        };
        let (Some(sid), Some(cid), Some(q)) = (sid, cid, quote) else {
            skipped += 1;
            continue;
        };
        let (ea, eb) = (
            entity_for(c, an, "project", &now),
            entity_for(c, bn, "project", &now),
        );
        let id = format!("c-{:x}", fxhash(&format!("{ea}{kind}{eb}")));
        let role: Option<String> = c
            .query_row("SELECT role FROM chunks WHERE id=?1", params![cid], |r| {
                r.get(0)
            })
            .ok();
        let speaker = match role.as_deref() {
            Some("user") => "owner",
            Some("assistant") => "assistant",
            _ => "unknown",
        };
        let status = match state.as_deref() {
            Some("proposed") | None => "proposed",
            Some("rejected") => "retracted",
            _ => "active",
        };
        c.execute(
            include_str!("sql/kg_migrate_links.sql"),
            params![
                id,
                format!("relation:{kind}"),
                format!("{an} {kind} {bn}"),
                speaker,
                ts,
                status,
                now
            ],
        )
        .unwrap();
        c.execute(
            "INSERT OR IGNORE INTO claim_about(claim_id,entity_id,role) VALUES(?1,?2,'subject')",
            params![id, ea],
        )
        .unwrap();
        c.execute(
            "INSERT OR IGNORE INTO claim_about(claim_id,entity_id,role) VALUES(?1,?2,'object')",
            params![id, eb],
        )
        .unwrap();
        c.execute("INSERT OR IGNORE INTO claim_evidence(claim_id,source_id,chunk_id,quote,ts) VALUES(?1,?2,?3,?4,?5)", params![id, sid, cid, q, ts]).unwrap();
        done += 1;
    }
    (done, skipped)
}

pub fn stats(c: &Connection) {
    let n = |sql: &str| -> i64 { c.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };
    println!(
        "entities {} (aliases {}), claims {} (evidence rows {}), overrides {}",
        n("SELECT COUNT(*) FROM entities"),
        n("SELECT COUNT(*) FROM entity_aliases"),
        n("SELECT COUNT(*) FROM claims"),
        n("SELECT COUNT(*) FROM claim_evidence"),
        n("SELECT COUNT(*) FROM kg_overrides")
    );
    println!(
        "claims without evidence: {}",
        n(include_str!("sql/kg_stats.sql"))
    );
}

/// Hand-seeded entities from `config.json` key `entities`: [{name, kind, aliases}]. Aliases match whole words, lower-case.
fn seeds() -> Vec<(String, String, Vec<String>)> {
    let raw = std::fs::read_to_string(crate::db::home().join("config.json")).unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    v["entities"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|e| {
                    (
                        e["name"].as_str().unwrap_or("").to_string(),
                        e["kind"].as_str().unwrap_or("").to_string(),
                        e["aliases"]
                            .as_array()
                            .map(|x| x.iter().filter_map(|s| s.as_str().map(norm)).collect())
                            .unwrap_or_default(),
                    )
                })
                .filter(|e| !e.0.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn has_word(text: &str, w: &str) -> bool {
    let mut from = 0;
    while let Some(i) = text[from..].find(w) {
        let (s, e) = (from + i, from + i + w.len());
        let before = text[..s]
            .chars()
            .next_back()
            .map_or(true, |c| !c.is_alphanumeric());
        let after = text[e..]
            .chars()
            .next()
            .map_or(true, |c| !c.is_alphanumeric());
        if before && after {
            return true;
        }
        from = e;
    }
    false
}

/// Technologies and concepts offered as `topic` entities. One is created only when owner turns in at least
/// `TOPIC_MIN_SESSIONS` sessions mention it, so the list can be long without filling the graph. Ambiguous words
/// (go, swift, bun, node) are left out; aliases are matched as whole words.
const TOPIC_MIN_SESSIONS: i64 = 3;
const TOPICS: &[(&str, &[&str])] = &[
    ("Rust", &[]),
    ("Python", &[]),
    ("TypeScript", &[]),
    ("JavaScript", &["js"]),
    ("Bash", &[]),
    ("HTML", &[]),
    ("CSS", &[]),
    ("SQL", &[]),
    ("React", &[]),
    ("Next.js", &["nextjs"]),
    ("Vite", &[]),
    ("Tailwind", &["tailwindcss"]),
    ("Tauri", &[]),
    ("Electron", &[]),
    ("SwiftUI", &[]),
    ("FastAPI", &[]),
    ("Flask", &[]),
    ("Django", &[]),
    ("Node.js", &["nodejs"]),
    ("Svelte", &[]),
    ("Vue", &[]),
    ("SQLite", &[]),
    ("Postgres", &["postgresql"]),
    ("Redis", &[]),
    ("DuckDB", &[]),
    ("Supabase", &[]),
    ("Firebase", &[]),
    ("Embeddings", &["embedding"]),
    ("RAG", &[]),
    ("LLM", &["llms"]),
    ("MCP", &[]),
    ("Knowledge graph", &["knowledge graphs"]),
    ("Docker", &[]),
    ("Git", &[]),
    ("GitHub", &[]),
    ("Cloudflare", &[]),
    ("AWS", &[]),
    ("Homebrew", &[]),
    ("Playwright", &[]),
    ("Puppeteer", &[]),
    ("npm", &[]),
    ("Cargo", &[]),
    ("launchd", &[]),
    ("Xcode", &[]),
    ("Figma", &[]),
    ("Stripe", &[]),
    ("Telegram", &[]),
    ("OpenAI", &[]),
    ("Anthropic", &[]),
    ("Gemini", &[]),
    ("Claude Code", &[]),
    ("Whisper", &[]),
    ("ElevenLabs", &[]),
    ("iOS", &[]),
    ("Android", &[]),
    ("macOS", &[]),
    ("Chrome extension", &["chrome extensions"]),
];

/// Create the built-in topics that the owner's turns mention often enough.
fn seed_topics(c: &Connection) {
    let now = crate::db::chrono_now();
    for (name, aliases) in TOPICS {
        let id = format!("e-{:x}", fxhash(&norm(name)));
        if c.query_row(
            "SELECT 1 FROM entities WHERE id=?1",
            params![id],
            |_| Ok(()),
        )
        .is_ok()
        {
            continue;
        }
        let terms: Vec<String> = std::iter::once(norm(name))
            .chain(aliases.iter().map(|a| norm(a)))
            .collect();
        let phrase = |t: &str| {
            format!(
                "\"{}\"",
                t.chars()
                    .map(|ch| if ch.is_alphanumeric() { ch } else { ' ' })
                    .collect::<String>()
                    .trim()
            )
        };
        let m = terms
            .iter()
            .map(|t| phrase(t))
            .collect::<Vec<_>>()
            .join(" OR ");
        let n: i64 = c
            .query_row(include_str!("sql/kg_seed_topics.sql"), params![m], |r| {
                r.get(0)
            })
            .unwrap_or(0);
        if n < TOPIC_MIN_SESSIONS {
            continue;
        }
        let id = entity_for(c, name, "topic", &now);
        for a in &terms {
            c.execute(
                "INSERT OR IGNORE INTO entity_aliases(alias,entity_id) VALUES(?1,?2)",
                params![a, id],
            )
            .unwrap();
        }
    }
}

/// Create the configured entities and their aliases.
pub fn sync_entities(c: &Connection) -> usize {
    let now = crate::db::chrono_now();
    seed_topics(c);
    let list = seeds();
    for (name, kind, aliases) in &list {
        let id = entity_for(c, name, kind, &now);
        c.execute("UPDATE entities SET kind=?2 WHERE id=?1", params![id, kind])
            .unwrap();
        for a in aliases {
            c.execute(
                "INSERT OR IGNORE INTO entity_aliases(alias,entity_id) VALUES(?1,?2)",
                params![a, id],
            )
            .unwrap();
        }
    }
    list.len()
}

/// Copy decisions (with a source and quote) into claims. The entities a claim is about are those whose name or alias
/// appears as a whole word in its title, body or quote, plus the entity named like the decision's thread.
/// Idempotent; `nodes` is not changed. Returns (claims, linked to an entity, without an entity).
pub fn import_decisions(c: &Connection) -> (usize, usize, usize) {
    sync_entities(c);
    let now = crate::db::chrono_now();
    let ents: Vec<(String, String, Vec<String>)> = {
        let mut st = c
            .prepare(include_str!("sql/kg_import_decisions_3.sql"))
            .unwrap();
        st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?.unwrap_or_default(),
            ))
        })
        .unwrap()
        .flatten()
        .map(|(i, n, a)| (i, n, a.split('\u{1f}').map(String::from).collect()))
        .collect()
    };
    let rows: Vec<(
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        i64,
        i64,
        String,
        Option<String>,
        Option<String>,
    )> = {
        let mut st = c
            .prepare(include_str!("sql/kg_import_decisions.sql"))
            .unwrap();
        st.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
            ))
        })
        .unwrap()
        .flatten()
        .collect()
    };
    let (mut n, mut linked, mut bare) = (0, 0, 0);
    for (nid, title, body, thread, state, stale, sid, cid, quote, role, ts) in rows {
        let id = format!("c-{nid}");
        let speaker = match role.as_deref() {
            Some("user") => "owner",
            Some("assistant") => "assistant",
            _ => "unknown",
        };
        let superseded = stale
            .as_deref()
            .map_or(false, |s| s.starts_with("superseded"));
        let status = match state.as_str() {
            "rejected" => "retracted",
            _ if superseded => "superseded",
            "proposed" => "proposed",
            _ => "active",
        };
        c.execute(
            include_str!("sql/kg_import_decisions_2.sql"),
            params![id, title, body, speaker, ts, status, now],
        )
        .unwrap();
        c.execute("INSERT OR IGNORE INTO claim_evidence(claim_id,source_id,chunk_id,quote,ts) VALUES(?1,?2,?3,?4,?5)", params![id, sid, cid, quote, ts]).unwrap();
        let hay = norm(&format!("{title} {body} {quote}"));
        let mut any = false;
        for (eid, name, aliases) in &ents {
            if norm(name) == norm(&thread)
                || aliases.iter().any(|a| !a.is_empty() && has_word(&hay, a))
            {
                c.execute("INSERT OR IGNORE INTO claim_about(claim_id,entity_id,role) VALUES(?1,?2,'subject')", params![id, eid]).unwrap();
                any = true;
            }
        }
        n += 1;
        if any { linked += 1 } else { bare += 1 }
    }
    (n, linked, bare)
}

// ---------------------------------------------------------------- helpers

fn clip(s: &str, n: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= n {
        s
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}
fn tokens(s: &str) -> usize {
    s.chars().count() / 4
}
fn day(ts: &Option<String>) -> String {
    ts.as_deref()
        .and_then(|t| t.get(..10))
        .unwrap_or("?")
        .to_string()
}

/// Entities whose name or alias appears as a whole word in `text`.
pub fn mentions(c: &Connection, text: &str) -> Vec<(String, String)> {
    let hay = norm(text);
    let mut st = c.prepare("SELECT e.id, e.name, a.alias FROM entities e JOIN entity_aliases a ON a.entity_id=e.id").unwrap();
    let mut out: Vec<(String, String)> = vec![];
    for (id, name, alias) in st
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .flatten()
    {
        if !alias.is_empty() && has_word(&hay, &alias) && !out.iter().any(|o| o.0 == id) {
            out.push((id, name));
        }
    }
    out
}

/// An entity by exact name or alias (case-insensitive).
pub fn find_entity(c: &Connection, name: &str) -> Option<(String, String)> {
    c.query_row("SELECT e.id, e.name FROM entity_aliases a JOIN entities e ON e.id=a.entity_id WHERE a.alias=?1", params![norm(name)], |r| Ok((r.get(0)?, r.get(1)?))).ok()
}

// ---------------------------------------------------------------- KG-11 tier 0: state from activity

/// Thread state suggested by session activity alone (no model). Prints suggestions; changes nothing.
pub fn state(c: &Connection) {
    let days = |ts: &str| -> i64 { crate::server::days_ago(ts) };
    let mut st = c.prepare(include_str!("sql/kg_state.sql")).unwrap();
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

// ---------------------------------------------------------------- KG-13 gated claim writer and extraction inputs

/// The write rule for a claim: the source exists, the quote is found verbatim in one of its chunks, every entity exists.
/// The speaker comes from the chunk role and the date from the chunk, never from the model.
pub fn add_claim(
    c: &Connection,
    ctype: &str,
    text: &str,
    body: &str,
    source: &str,
    quote: &str,
    ents: &[String],
    stated: &str,
    origin: &str,
) -> Result<String, String> {
    if text.trim().is_empty() || text.chars().count() > 160 {
        return Err("text must be 1..160 chars".into());
    }
    if body.chars().count() > 320 {
        return Err("body over 320 chars".into());
    }
    if quote.trim().chars().count() < 8 {
        return Err("quote of at least 8 chars is required".into());
    }
    if ents.is_empty() {
        return Err("at least one entity is required".into());
    }
    let sid: i64 = c
        .query_row(
            "SELECT id FROM sources WHERE ref=?1",
            params![source],
            |r| r.get(0),
        )
        .map_err(|_| format!("source '{source}' not found"))?;
    let q = norm(quote);
    let mut st = c
        .prepare("SELECT id, role, ts, text FROM chunks WHERE source_id=?1 ORDER BY seq")
        .unwrap();
    let hit = st
        .query_map(params![sid], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .unwrap()
        .flatten()
        .find(|r| norm(&r.3).contains(&q))
        .ok_or_else(|| format!("quote not found in {source}: \"{}\"", clip(quote, 60)))?;
    let mut ids = vec![];
    for e in ents {
        ids.push(
            find_entity(c, e)
                .ok_or_else(|| format!("unknown entity '{e}'"))?
                .0,
        );
    }
    ids.sort();
    ids.dedup();
    let id = format!(
        "c-{:x}",
        fxhash(&format!("{}{}{}", ids.join(","), ctype, norm(text)))
    );
    let speaker = match hit.1.as_deref() {
        Some("user") => "owner",
        Some("assistant") => "assistant",
        _ => "unknown",
    };
    let now = crate::db::chrono_now();
    c.execute(
        include_str!("sql/kg_add_claim.sql"),
        params![id, ctype, text, body, speaker, hit.2, origin, now],
    )
    .map_err(|e| e.to_string())?;
    for e in &ids {
        c.execute(
            "INSERT OR IGNORE INTO claim_about(claim_id,entity_id,role) VALUES(?1,?2,'subject')",
            params![id, e],
        )
        .unwrap();
    }
    c.execute(
        include_str!("sql/kg_add_claim_2.sql"),
        params![id, sid, hit.0, quote, stated, hit.2],
    )
    .unwrap();
    Ok(id)
}

/// `kg claims -` : a JSON array of {type,text,body,source,quote,entities:[..],stated} through the write rule.
pub fn import_claims(c: &Connection, raw: &str) -> (usize, usize) {
    let v: serde_json::Value = serde_json::from_str(raw).unwrap_or_default();
    let (mut ok, mut bad) = (0, 0);
    for x in v.as_array().cloned().unwrap_or_default() {
        let g = |k: &str| x[k].as_str().unwrap_or("").to_string();
        let ents: Vec<String> = x["entities"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let stated = if g("stated") == "inferred" {
            "inferred"
        } else {
            "stated"
        };
        match add_claim(
            c,
            &g("type"),
            &g("text"),
            &g("body"),
            &g("source"),
            &g("quote"),
            &ents,
            stated,
            "extracted",
        ) {
            Ok(_) => ok += 1,
            Err(e) => {
                bad += 1;
                eprintln!("rejected [{}]: {e}", clip(&g("text"), 50));
            }
        }
    }
    (ok, bad)
}

/// Sessions to extract claims from: enough owner turns, not automated probes, not yet extracted, newest first,
/// within the daily cap. `entity` restricts to sessions whose owner turns mention that entity (tier 2, lazy).
pub fn extract_queue(
    c: &Connection,
    limit: usize,
    entity: Option<&str>,
    session_id: Option<&str>,
) -> Vec<String> {
    let today = crate::db::chrono_now()[..10].to_string();
    let used: i64 = c.query_row("SELECT COUNT(*) FROM log WHERE action IN ('kg-extract','kg-extract-failed') AND ts LIKE ?1", params![format!("{today}%")], |x| x.get(0)).unwrap_or(0);
    let room = (8 - used).max(0) as usize;
    let only = session_id.map(|s| format!("session:{}", s.chars().take(8).collect::<String>()));
    let mut st = c.prepare(include_str!("sql/kg_extract_queue.sql")).unwrap();
    let want: Option<Vec<String>> = entity.and_then(|e| find_entity(c, e)).map(|(id, _)| {
        c.prepare("SELECT alias FROM entity_aliases WHERE entity_id=?1")
            .unwrap()
            .query_map(params![id], |r| r.get::<_, String>(0))
            .unwrap()
            .flatten()
            .collect()
    });
    let mut out = vec![];
    for (r, sid) in st
        .query_map([], |x| Ok((x.get::<_, String>(0)?, x.get::<_, i64>(1)?)))
        .unwrap()
        .flatten()
    {
        if out.len() >= limit.min(room) {
            break;
        }
        if only.as_ref().map_or(false, |o| *o != r) {
            continue;
        }
        if let Some(al) = &want {
            let txt: String = c
                .prepare(
                    "SELECT group_concat(text, ' ') FROM chunks WHERE source_id=?1 AND role='user'",
                )
                .unwrap()
                .query_row(params![sid], |x| x.get::<_, Option<String>>(0))
                .unwrap_or_default()
                .unwrap_or_default();
            let t = norm(&txt);
            if !al.iter().any(|a| has_word(&t, a)) {
                continue;
            }
        }
        out.push(r);
    }
    out
}

/// The text a small model reads: entity names, owner turns in full, assistant turns clipped.
pub fn extract_input(c: &Connection, r: &str) {
    let Ok((sid, thread, ts)) = c.query_row("SELECT id, ifnull(thread,''), ifnull(last_ts,'') FROM sources WHERE ref=?1 AND kind='session'", params![r], |x| Ok((x.get::<_, i64>(0)?, x.get::<_, String>(1)?, x.get::<_, String>(2)?))) else { eprintln!("no such session"); std::process::exit(1) };
    println!(
        "BACKTRAIL-KG\nSESSION {r}\nTHREAD {thread}\nDATE {}\n",
        ts.get(..10).unwrap_or("")
    );
    println!("ENTITIES (use these names exactly):");
    for (n, k) in c
        .prepare("SELECT name, ifnull(kind,'') FROM entities ORDER BY name")
        .unwrap()
        .query_map([], |x| Ok((x.get::<_, String>(0)?, x.get::<_, String>(1)?)))
        .unwrap()
        .flatten()
    {
        println!("- {n} ({k})");
    }
    println!("\nTURNS:");
    let mut used = 0usize;
    let mut q = c
        .prepare("SELECT locator, role, text FROM chunks WHERE source_id=?1 ORDER BY seq")
        .unwrap();
    for (l, role, t) in q
        .query_map(params![sid], |x| {
            Ok((
                x.get::<_, String>(0)?,
                x.get::<_, String>(1)?,
                x.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .flatten()
    {
        let user = role == "user";
        let s = clip(&t, if user { 900 } else { 220 });
        if used + s.len() > 22000 && !user {
            continue;
        }
        used += s.len();
        println!(
            "[{} · {}] {}",
            l,
            if user { "owner" } else { "assistant" },
            s
        );
    }
}

// ---------------------------------------------------------------- KG-14 entity discovery (suggestions only)

/// Terms that look like names but are not entities or aliases yet, with counts; and likely alias spellings of known names.
/// Nothing is written: the owner adds an entity with `kg entity add`.
pub fn suggest_data(c: &Connection) -> (Vec<(String, String, usize)>, Vec<(String, usize, usize)>) {
    let words: std::collections::HashSet<String> = std::fs::read_to_string("/usr/share/dict/words")
        .unwrap_or_default()
        .lines()
        .map(|w| w.trim().to_lowercase())
        .collect();
    let known: std::collections::HashSet<String> = c
        .prepare("SELECT alias FROM entity_aliases")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .flatten()
        .collect();
    let mut st = c.prepare(include_str!("sql/kg_suggest_data.sql")).unwrap();
    let mut df: std::collections::HashMap<String, (usize, std::collections::HashSet<i64>, usize)> =
        Default::default();
    for (t, sid) in st
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .unwrap()
        .flatten()
    {
        let mut seen = std::collections::HashSet::new();
        let mut prev_end = true; // the previous word ended a sentence
        for w in t.split_whitespace() {
            let ends = w.ends_with(['.', '!', '?', ':']);
            let core = w.trim_matches(|ch: char| !(ch.is_alphanumeric()));
            let lead_end = prev_end;
            prev_end = ends;
            if core.chars().count() < 4
                || !core.chars().next().map_or(false, |ch| ch.is_alphabetic())
                || core.contains("http")
                || w.contains(['\'', '’', '/'])
            {
                continue;
            }
            let l = core.to_lowercase();
            if !seen.insert(l.clone()) {
                continue;
            }
            let e = df.entry(l).or_default();
            e.0 += 1;
            e.1.insert(sid);
            if core.chars().next().unwrap().is_uppercase() && !lead_end {
                e.2 += 1;
            }
        }
    }
    let squash = |s: &str| s.replace(['-', '_', ' ', '.'], "");
    let known_sq: std::collections::HashMap<String, String> =
        known.iter().map(|k| (squash(k), k.clone())).collect();
    // Terms the owner dismissed from the suggestions (kept in kg_overrides, so nothing is hard-coded).
    let ignored: std::collections::HashSet<String> = c
        .prepare("SELECT target FROM kg_overrides WHERE field='entity_ignore'")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .flatten()
        .collect();
    let in_dict = |w: &str| -> bool {
        let stems = [
            w.to_string(),
            w.trim_end_matches('s').to_string(),
            w.trim_end_matches("es").to_string(),
            w.trim_end_matches("ed").to_string(),
            w.trim_end_matches('d').to_string(),
            w.trim_end_matches("ing").to_string(),
            format!("{}e", w.trim_end_matches("ing")),
            w.trim_end_matches("ly").to_string(),
            w.trim_end_matches("er").to_string(),
        ];
        stems.iter().any(|x| words.contains(x))
    };
    let mut alias_hits = vec![];
    let mut cands = vec![];
    for (w, (n, ss, caps)) in &df {
        if *n < 5 || ss.len() < 3 || known.contains(w) || ignored.contains(w) {
            continue;
        }
        let sq = squash(w);
        let trimmed = sq.trim_end_matches('s').to_string();
        if let Some(k) = known_sq.get(&sq).or_else(|| known_sq.get(&trimmed)) {
            alias_hits.push((w.clone(), k.clone(), *n));
            continue;
        }
        if !in_dict(w) || *caps * 2 >= *n {
            cands.push((w.clone(), *n, ss.len(), *caps));
        }
    }
    cands.sort_by(|a, b| b.1.cmp(&a.1));
    alias_hits.sort_by(|a, b| b.2.cmp(&a.2));
    // A name is written capitalised in the middle of a sentence ("GitHub", "SQLite"); a common word or a typo is not.
    // The assistant's turns are the reference, since the owner rarely capitalises.
    cands.truncate(150);
    let want: std::collections::HashSet<&str> = cands.iter().map(|x| x.0.as_str()).collect();
    let mut seen: std::collections::HashMap<String, (usize, usize)> = Default::default();
    let mut st = c
        .prepare(include_str!("sql/kg_suggest_data_2.sql"))
        .unwrap();
    for t in st
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .flatten()
    {
        let mut prev_end = true;
        for w in t.split_whitespace() {
            let ends = w.ends_with(['.', '!', '?', ':']);
            let lead = prev_end;
            prev_end = ends;
            let core = w.trim_matches(|ch: char| !ch.is_alphanumeric());
            let l = core.to_lowercase();
            if lead || !want.contains(l.as_str()) {
                continue;
            }
            let e = seen.entry(l).or_default();
            e.0 += 1;
            if core.chars().next().is_some_and(|ch| ch.is_uppercase()) {
                e.1 += 1;
            }
        }
    }
    cands.retain(|(w, ..)| {
        seen.get(w)
            .is_some_and(|&(n, caps)| n >= 3 && caps * 2 >= n)
    });
    alias_hits.truncate(15);
    cands.truncate(30);
    (
        alias_hits,
        cands.into_iter().map(|(w, n, s, _)| (w, n, s)).collect(),
    )
}

pub fn suggest(c: &Connection) {
    let (alias_hits, cands) = suggest_data(c);
    println!(
        "likely aliases of known names (same letters, different spacing or plural); add with `kg entity alias`:"
    );
    for (w, k, n) in &alias_hits {
        println!("  {w} -> {k}  ({n} turns)");
    }
    println!(
        "\nterms that may be new entities (not in the dictionary, or mostly capitalised); add with `kg entity add`:"
    );
    for (w, n, s) in &cands {
        println!("  {w}  ({n} turns, {s} sessions)");
    }
    println!(
        "\nnot suggested on purpose: spellings within one edit of a known name collide with real words. Dismiss a term with `kg entity ignore <term>`."
    );
}

pub fn entity_add(c: &Connection, name: &str, kind: &str, aliases: &[String]) {
    let id = entity_for(c, name, kind, &crate::db::chrono_now());
    c.execute("UPDATE entities SET kind=?2 WHERE id=?1", params![id, kind])
        .unwrap();
    for a in aliases {
        c.execute(
            "INSERT OR REPLACE INTO entity_aliases(alias,entity_id) VALUES(?1,?2)",
            params![norm(a), id],
        )
        .unwrap();
    }
    logw(c, "owner", "kg-entity", name, &aliases.join(", "));
    println!("ok {id}");
}

// ---------------------------------------------------------------- KG-15/16 queries over the history store

struct Row {
    text: String,
    ctype: String,
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
        Ok(r) if r.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == ':') && !r.is_empty() => {
            format!(
                " AND NOT EXISTS (SELECT 1 FROM claim_evidence xv JOIN sources xs ON xs.id=xv.source_id WHERE xv.claim_id=c.id AND xs.ref='{r}')"
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
    let sql = include_str!("sql/kg_claim_rows.sql")
        .replace("{where}", where_)
        .replace("{limit}", &limit.to_string());
    let mut st = c.prepare(&sql).unwrap();
    st.query_map(args, |r| {
        Ok(Row {
            text: r.get(0)?,
            ctype: r.get(1)?,
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
    format!(
        "{} {} [{}{}] {}\n    \"{}\"",
        day(&r.ts),
        who(&r.speaker),
        r.status,
        if r.stated == "inferred" {
            ", inferred"
        } else {
            ""
        },
        r.text,
        clip(&r.quote, qlen)
    )
}

/// Entity page: header, newest claims in full, older claims rolled up by month, within a token budget.
pub fn page(c: &Connection, name: &str, budget: usize) {
    let Some((id, ename)) = find_entity(c, name) else {
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
    let Some((id, ename)) = find_entity(c, name) else {
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
            "(' '||lower(c.text||' '||ifnull(c.body,'')||' '||ifnull((SELECT group_concat(quote,' ') FROM claim_evidence v WHERE v.claim_id=c.id),'')) LIKE ?{})",
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
        "SELECT c.id FROM claims c WHERE c.status!='retracted'{} AND ({score})>={need} ORDER BY ({score}) DESC, c.asserted_at DESC LIMIT 5",
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
    for id in &ids {
        for r in claim_rows(c, "c.id=?1", &[id], 1) {
            println!("{}\n    about: {}", line(&r, 160), r.ents);
        }
    }
    true
}

const STOP: &[&str] = &[
    "the", "and", "for", "why", "what", "did", "does", "how", "was", "were", "with", "that",
    "this", "from", "have", "has", "had", "our", "you", "your", "are", "not", "but", "about",
    "when", "who", "which", "should", "would", "could", "there", "their", "been", "also", "still",
    "just", "dont", "want", "wanna", "thing", "like", "use", "need", "make", "owner", "can", "its",
    "isnt", "yourself", "yes", "okay",
];

/// Entities that share claims with this one, then (2nd hop) the newest few claims of the top neighbours. Capped.
pub fn neighbors(c: &Connection, name: &str) {
    let Some((id, ename)) = find_entity(c, name) else {
        println!("no record: no entity named '{name}'");
        return;
    };
    let mut st = c.prepare(include_str!("sql/kg_neighbors.sql")).unwrap();
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
pub fn passage(c: &Connection, q: &str, entity: Option<&str>, from: Option<&str>) {
    let ws: Vec<String> = q
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3 && !STOP.contains(&w.to_lowercase().as_str()))
        .map(|w| format!("\"{}\"", w.to_lowercase()))
        .collect();
    if ws.is_empty() {
        println!("no record: query has no content words");
        return;
    }
    let ent_filter = entity
        .and_then(|e| find_entity(c, e))
        .map(|(id, _)| id)
        .unwrap_or_default();
    let from = from.unwrap_or("").to_string();
    for (label, m) in [
        ("all words", ws.join(" AND ")),
        ("any word", ws.join(" OR ")),
    ] {
        let exclude = match std::env::var("BACKTRAIL_EXCLUDE_SESSION") {
            Ok(r)
                if r.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == ':') && !r.is_empty() =>
            {
                format!(" AND s.ref!='{r}'")
            }
            _ => String::new(),
        };
        let mut st = c
            .prepare(&include_str!("sql/kg_passage.sql").replace("{exclude}", &exclude))
            .unwrap();
        let rs: Vec<(String, String, String, String)> = st
            .query_map(params![m, ent_filter, from], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .unwrap()
            .flatten()
            .collect();
        if !rs.is_empty() {
            println!("passages ({label}):");
            for (r, d, role, sn) in rs {
                println!(
                    "{d} {r} {} | {}",
                    if role == "user" { "owner" } else { "assistant" },
                    clip(&sn, 170)
                );
            }
            return;
        }
    }
    println!("no record in passages");
}

/// Router for a history question: lineage questions trace the entity; questions about an entity's state or contents
/// show its page; everything else looks up claims, then passages. A miss prints two short lines.
pub fn find(c: &Connection, q: &str) {
    let low = q.to_lowercase();
    let ents = mentions(c, q);
    let lineage = [
        "lineage",
        "came from",
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
        passage(c, q, None, None);
    }
    if let Some((_, name)) = ents.first() {
        let n = backlog(c, name);
        if n > 0 {
            println!(
                "[{n} sessions that mention {name} have no extracted claims yet: skill/kg-extract.sh --entity \"{name}\"]"
            );
        }
    }
}

// ---------------------------------------------------------------- KG-19 lint

/// Graph checks for `backtrail check`. Precision needs judgment, so a fixed sample is printed instead of a score.
pub fn lint(c: &Connection) {
    let n = |sql: &str| -> i64 { c.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };
    println!(
        "graph: {} entities, {} claims",
        n("SELECT COUNT(*) FROM entities"),
        n("SELECT COUNT(*) FROM claims")
    );
    let empty: Vec<String> = c.prepare("SELECT name FROM entities e WHERE NOT EXISTS (SELECT 1 FROM claim_about a WHERE a.entity_id=e.id)").unwrap().query_map([], |r| r.get(0)).unwrap().flatten().collect();
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
        n(include_str!("sql/kg_lint_3.sql"))
    );
    println!(
        "claims about no entity: {}",
        n(
            "SELECT COUNT(*) FROM claims c WHERE NOT EXISTS (SELECT 1 FROM claim_about a WHERE a.claim_id=c.id)"
        )
    );
    let mut lost = 0;
    let mut st = c.prepare("SELECT v.claim_id, v.quote, k.text FROM claim_evidence v LEFT JOIN chunks k ON k.id=v.chunk_id").unwrap();
    for (id, q, t) in st
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })
        .unwrap()
        .flatten()
    {
        if t.map_or(true, |t| !norm(&t).contains(&norm(&q))) {
            lost += 1;
            if lost <= 5 {
                println!("  quote no longer found: {id}");
            }
        }
    }
    println!("claims whose quote is no longer in its passage: {lost}");
    let mm: Vec<(String, String, String)> = c
        .prepare(include_str!("sql/kg_lint.sql"))
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .flatten()
        .collect();
    println!(
        "claims whose state differs from their source item: {}{}",
        mm.len(),
        if mm.is_empty() {
            ""
        } else {
            " (run `kg import`)"
        }
    );
    let tok = |s: &str| -> std::collections::HashSet<String> {
        s.to_lowercase()
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|w| w.len() > 3)
            .map(String::from)
            .collect()
    };
    let mut st = c.prepare(include_str!("sql/kg_lint_2.sql")).unwrap();
    let all: Vec<(String, String, String)> = st
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get::<_, Option<String>>(2)?.unwrap_or_default(),
            ))
        })
        .unwrap()
        .flatten()
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
                if dups <= 5 {
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
    let mut sample = all.clone();
    sample.sort_by_key(|x| fxhash(&x.0));
    for (id, t, _) in sample.iter().take(5) {
        let q: String = c
            .query_row(
                "SELECT quote FROM claim_evidence WHERE claim_id=?1 LIMIT 1",
                params![id],
                |r| r.get(0),
            )
            .unwrap_or_default();
        println!("  \"{}\"  <-  \"{}\"", clip(t, 70), clip(&q, 70));
    }
}

// ---------------------------------------------------------------- KG-20 backup export and restore

/// One JSON object per line. Evidence is stored by source ref and quote, not by row id, so another machine can re-locate it.
pub fn export(c: &Connection, path: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    let q = |sql: &str, cols: &[&str]| -> Vec<serde_json::Value> {
        let mut st = c.prepare(sql).unwrap();
        st.query_map([], |r| {
            let mut o = serde_json::Map::new();
            for (i, k) in cols.iter().enumerate() {
                let v = match r.get::<_, rusqlite::types::Value>(i).unwrap() {
                    rusqlite::types::Value::Text(t) => serde_json::Value::from(t),
                    rusqlite::types::Value::Integer(n) => serde_json::Value::from(n.to_string()),
                    _ => serde_json::Value::Null,
                };
                o.insert((*k).into(), v);
            }
            Ok(serde_json::Value::Object(o))
        })
        .unwrap()
        .flatten()
        .collect()
    };
    let sets: [(&str, &str, &[&str]); 6] = [
        (
            "entity",
            "SELECT id,name,kind,created FROM entities",
            &["id", "name", "kind", "created"],
        ),
        (
            "alias",
            "SELECT alias,entity_id FROM entity_aliases",
            &["alias", "entity_id"],
        ),
        (
            "claim",
            include_str!("sql/kg_export.sql"),
            &[
                "id",
                "type",
                "text",
                "body",
                "speaker",
                "asserted_at",
                "valid_to",
                "status",
                "superseded_by",
                "origin",
                "created",
            ],
        ),
        (
            "about",
            "SELECT claim_id,entity_id,role FROM claim_about",
            &["claim_id", "entity_id", "role"],
        ),
        (
            "evidence",
            include_str!("sql/kg_export_2.sql"),
            &["claim_id", "source", "quote", "stated", "ts"],
        ),
        (
            "override",
            "SELECT target,field,value,by,ts FROM kg_overrides",
            &["target", "field", "value", "by", "ts"],
        ),
    ];
    for (t, sql, cols) in sets {
        for mut v in q(sql, cols) {
            v["t"] = t.into();
            writeln!(f, "{v}")?;
        }
    }
    Ok(())
}

/// Restore an export into this store. Evidence whose source or quote cannot be found here is counted and skipped.
pub fn restore(c: &Connection, path: &str) -> (usize, usize) {
    let raw = std::fs::read_to_string(path).unwrap_or_default();
    let (mut n, mut orphan) = (0, 0);
    for l in raw.lines() {
        let v: serde_json::Value = match serde_json::from_str(l) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let g = |k: &str| v[k].as_str().map(String::from);
        match v["t"].as_str().unwrap_or("") {
            "entity" => {
                c.execute(
                    "INSERT OR REPLACE INTO entities(id,name,kind,created) VALUES(?1,?2,?3,?4)",
                    params![g("id"), g("name"), g("kind"), g("created")],
                )
                .unwrap();
                n += 1
            }
            "alias" => {
                c.execute(
                    "INSERT OR REPLACE INTO entity_aliases(alias,entity_id) VALUES(?1,?2)",
                    params![g("alias"), g("entity_id")],
                )
                .unwrap();
                n += 1
            }
            "claim" => {
                c.execute(
                    include_str!("sql/kg_restore.sql"),
                    params![
                        g("id"),
                        g("type"),
                        g("text"),
                        g("body"),
                        g("speaker"),
                        g("asserted_at"),
                        g("valid_to"),
                        g("status"),
                        g("superseded_by"),
                        g("origin"),
                        g("created")
                    ],
                )
                .unwrap();
                n += 1
            }
            "about" => {
                c.execute(
                    "INSERT OR IGNORE INTO claim_about(claim_id,entity_id,role) VALUES(?1,?2,?3)",
                    params![g("claim_id"), g("entity_id"), g("role")],
                )
                .unwrap();
                n += 1
            }
            "override" => {
                c.execute("INSERT OR REPLACE INTO kg_overrides(target,field,value,by,ts) VALUES(?1,?2,?3,?4,?5)", params![g("target"), g("field"), g("value"), g("by"), g("ts")]).unwrap();
                n += 1
            }
            "evidence" => {
                let q = norm(&g("quote").unwrap_or_default());
                let sid: Option<i64> = c
                    .query_row(
                        "SELECT id FROM sources WHERE ref=?1",
                        params![g("source")],
                        |r| r.get(0),
                    )
                    .ok();
                let hit: Option<(i64, i64)> = sid.and_then(|sid: i64| {
                    let mut st = c
                        .prepare("SELECT id, text FROM chunks WHERE source_id=?1 ORDER BY seq")
                        .unwrap();
                    let found = st
                        .query_map(params![sid], |r| {
                            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                        })
                        .unwrap()
                        .flatten()
                        .find(|r| norm(&r.1).contains(&q))
                        .map(|r| (sid, r.0));
                    found
                });
                match hit {
                    Some((s, ch)) => {
                        c.execute(
                            include_str!("sql/kg_restore_2.sql"),
                            params![g("claim_id"), s, ch, g("quote"), g("stated"), g("ts")],
                        )
                        .unwrap();
                        n += 1
                    }
                    None => orphan += 1,
                }
            }
            _ => {}
        }
    }
    (n, orphan)
}

/// Add an alias to an existing entity (by name or alias), then relink claims. Returns false when the entity is unknown.
/// Hide a term from the entity suggestions.
pub fn ignore_term(c: &Connection, term: &str) {
    c.execute(
        "INSERT OR REPLACE INTO kg_overrides(target,field,value,by,ts) VALUES(?1,'entity_ignore','1','owner',?2)",
        params![term.to_lowercase(), crate::db::chrono_now()],
    )
    .unwrap();
}

pub fn alias_add(c: &Connection, entity: &str, alias: &str) -> bool {
    let Some((id, name)) = find_entity(c, entity) else {
        return false;
    };
    c.execute(
        "INSERT OR REPLACE INTO entity_aliases(alias,entity_id) VALUES(?1,?2)",
        params![norm(alias), id],
    )
    .unwrap();
    logw(c, "owner", "kg-alias", &name, alias);
    true
}

/// How many sessions that name the entity in an owner turn have no claims extracted yet (tier 2 backlog).
pub fn backlog(c: &Connection, entity: &str) -> usize {
    let mut n = 0;
    let saved = std::env::var("BACKTRAIL_EXCLUDE_SESSION").ok();
    let _ = saved;
    n += extract_queue_all(c, entity).len();
    n
}

fn extract_queue_all(c: &Connection, entity: &str) -> Vec<String> {
    // like extract_queue but without the daily cap, to report the size of the backlog
    let Some((id, _)) = find_entity(c, entity) else {
        return vec![];
    };
    let al: Vec<String> = c
        .prepare("SELECT alias FROM entity_aliases WHERE entity_id=?1")
        .unwrap()
        .query_map(params![id], |r| r.get::<_, String>(0))
        .unwrap()
        .flatten()
        .collect();
    let mut st = c
        .prepare(include_str!("sql/kg_extract_queue_all.sql"))
        .unwrap();
    st.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?.unwrap_or_default(),
        ))
    })
    .unwrap()
    .flatten()
    .filter(|(_, t)| {
        let t = norm(t);
        al.iter().any(|a| has_word(&t, a))
    })
    .map(|x| x.0)
    .collect()
}
