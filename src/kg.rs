//! Knowledge-graph code (plan: notes/backtrail/knowledge-graph-plan.md). Entities with aliases, and the queries
//! over `claims`, which also holds the work items (see schema.sql). One quote from one source supports each claim.
use crate::db::{fxhash, logw, norm};
use rusqlite::{Connection, params};

pub fn ensure(c: &Connection) {
    c.execute_batch(include_str!("sql/kg_schema.sql"))
        .expect("kg schema");
}

pub fn entity_for(c: &Connection, name: &str, kind: &str, now: &str) -> String {
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

/// A relation between two projects that the owner asserted without a quote (`bt link`). Such claims have origin
/// `owner` and are the only claims allowed to lack evidence. Returns the claim id.
pub fn owner_relation(c: &Connection, a: &str, b: &str, kind: &str, now: &str) -> String {
    let (ea, eb) = (
        entity_for(c, a, "project", now),
        entity_for(c, b, "project", now),
    );
    let id = format!("c-{:x}", fxhash(&format!("{ea}{kind}{eb}")));
    c.execute(
        "INSERT OR IGNORE INTO claims(id,type,title,state,origin,speaker,asserted_at,created,updated)
         VALUES(?1,?2,?3,'settled','owner','owner',?4,?4,?4)",
        params![id, format!("relation:{kind}"), format!("{a} {kind} {b}"), now],
    )
    .unwrap();
    for (e, role) in [(&ea, "subject"), (&eb, "object")] {
        c.execute(
            "INSERT OR IGNORE INTO claim_about(claim_id,entity_id,role) VALUES(?1,?2,?3)",
            params![id, e, role],
        )
        .unwrap();
    }
    id
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

/// Link a decision to the entities it is about: those whose name or alias appears as a whole word in `hay`
/// (its title, body and quote), and the entity named like its thread. Returns how many were linked.
pub fn link_entities(c: &Connection, id: &str, thread: &str, hay: &str) -> usize {
    let ents: Vec<(String, String, Vec<String>)> = {
        let mut st = c
            .prepare(include_str!("sql/kg_entity_aliases.sql"))
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
    let hay = norm(hay);
    let mut n = 0;
    for (eid, name, aliases) in &ents {
        if norm(name) == norm(thread)
            || aliases.iter().any(|a| !a.is_empty() && has_word(&hay, a))
        {
            c.execute(
                "INSERT OR IGNORE INTO claim_about(claim_id,entity_id,role) VALUES(?1,?2,'subject')",
                params![id, eid],
            )
            .unwrap();
            n += 1;
        }
    }
    n
}

/// Link every decision to the entities its text names. Run after an entity or an alias is added.
pub fn relink_all(c: &Connection) -> usize {
    let rows: Vec<(String, String, String)> = c
        .prepare("SELECT id, ifnull(thread,''), title||' '||ifnull(body,'')||' '||ifnull(quote,'') FROM claims WHERE type='decision'")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .flatten()
        .collect();
    rows.iter()
        .map(|(id, th, hay)| link_entities(c, id, th, hay))
        .sum()
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
    let look = |n: &str| {
        c.query_row(
            "SELECT e.id, e.name FROM entity_aliases a JOIN entities e ON e.id=a.entity_id WHERE a.alias=?1",
            params![norm(n)],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()
    };
    // The extraction packet lists entities as "Name (kind)"; a model may copy the suffix.
    look(name).or_else(|| {
        let n = name.trim_end();
        ["project", "topic", "tool", "service", "site"]
            .iter()
            .find_map(|k| n.strip_suffix(&format!(" ({k})")))
            .and_then(look)
    })
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
    let rel = ctype.starts_with("relation:");
    if !(rel || ["state", "rationale"].contains(&ctype)) {
        return Err(format!("type must be state, rationale or relation:<kind> (got {ctype})"));
    }
    if text.trim().is_empty() || text.chars().count() > 160 {
        return Err("text must be 1..160 chars".into());
    }
    if body.chars().count() > 320 {
        return Err("body over 320 chars".into());
    }
    if quote.trim().chars().count() < 8 {
        return Err("quote of at least 8 chars is required".into());
    }
    if ents.is_empty() || (rel && ents.len() != 2) {
        return Err(if rel {
            "a relation needs two entities".into()
        } else {
            "at least one entity is required".into()
        });
    }
    let (sid, thread): (i64, Option<String>) = c
        .query_row(
            "SELECT id, thread FROM sources WHERE ref=?1",
            params![crate::db::resolve_ref(c, source)],
            |r| Ok((r.get(0)?, r.get(1)?)),
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
    if rel && ids[0] == ids[1] {
        return Err("a relation needs two different entities".into());
    }
    let mut key = ids.clone();
    if !rel {
        key.sort();
        key.dedup();
    }
    // The same entities, type and text are the same claim, whether it was filed under the older 32-bit id or the
    // 64-bit one; filing it again changes nothing. A new claim whose id is taken by different content is refused.
    let natural = format!("{}{}{}", key.join(","), ctype, norm(text));
    let same = |id: &str| -> Option<bool> {
        c.query_row(
            "SELECT type=?2 AND lower(title)=lower(?3) FROM claims WHERE id=?1",
            params![id, ctype, text],
            |r| r.get::<_, bool>(0),
        )
        .ok()
    };
    let old_id = format!("c-{:x}", fxhash(&natural));
    if same(&old_id) == Some(true) {
        return Ok(old_id);
    }
    let id = format!("c-{}", crate::db::id64(&natural));
    match same(&id) {
        Some(true) => return Ok(id),
        Some(false) => return Err(format!("id {id} is already used by another claim")),
        None => {}
    }
    let speaker = match hit.1.as_deref() {
        Some("user") => "owner",
        Some("assistant") => "assistant",
        _ => "unknown",
    };
    let now = crate::db::chrono_now();
    c.execute(
        include_str!("sql/kg_add_claim.sql"),
        params![
            id, ctype, thread, text, body, speaker, hit.2, origin, sid, hit.0, quote, stated, now
        ],
    )
    .map_err(|e| e.to_string())?;
    for (i, e) in key.iter().enumerate() {
        let role = if rel && i == 1 { "object" } else { "subject" };
        c.execute(
            "INSERT OR IGNORE INTO claim_about(claim_id,entity_id,role) VALUES(?1,?2,?3)",
            params![id, e, role],
        )
        .unwrap();
    }
    crate::db::keep(c, &id);
    Ok(id)
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
    let used: i64 = c.query_row("SELECT COUNT(*) FROM log WHERE action IN ('extract','extract-failed') AND ts LIKE ?1", params![format!("{today}%")], |x| x.get(0)).unwrap_or(0);
    let cap: i64 = crate::compile::num(&crate::compile::cfgv(), "daily_cap", 8);
    let room = (cap - used).max(0) as usize;
    // a full session id or any unique prefix of it
    let only = session_id.map(|s| crate::db::resolve_ref(c, &format!("session:{}", s.trim_start_matches("session:"))));
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

/// The text a small model reads for one session: the thread's existing items (to avoid repeats), entity names, owner
/// turns in full, assistant turns clipped.
pub fn extract_input(c: &Connection, r: &str) {
    let r = &crate::db::resolve_ref(c, r);
    let Ok((sid, thread, ts)) = c.query_row("SELECT id, ifnull(thread,''), ifnull(last_ts,'') FROM sources WHERE ref=?1 AND kind='session'", params![r], |x| Ok((x.get::<_, i64>(0)?, x.get::<_, String>(1)?, x.get::<_, String>(2)?))) else { eprintln!("no such session"); std::process::exit(1) };
    println!(
        "BACKTRAIL-EXTRACT\nSESSION {r}\nTHREAD {thread}\nDATE {}\n",
        ts.get(..10).unwrap_or("")
    );
    println!("EXISTING ITEMS ON THIS THREAD (do not repeat these):");
    for (t, title) in c
        .prepare(include_str!("sql/kg_existing_items.sql"))
        .unwrap()
        .query_map(params![thread], |x| {
            Ok((x.get::<_, String>(0)?, x.get::<_, String>(1)?))
        })
        .unwrap()
        .flatten()
    {
        println!("- {t}: {}", clip(&title, 110));
    }
    println!("\nENTITIES (use these names exactly):");
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
    relink_all(c);
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
        Ok(r) if r.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == ':' || ch == '-') && !r.is_empty() => {
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
        "SELECT c.id FROM claims c WHERE c.status!='retracted' AND (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%'){} AND ({score})>={need} ORDER BY ({score}) DESC, c.asserted_at DESC LIMIT 5",
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
                println!("[newer claims about {name} exist, latest {}: `kg trace \"{name}\"`]", day(&Some(l)));
                said += 1;
            }
        }
    }
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
/// The last route of `find`: the same hybrid search that `/search` and `bt search` use (keyword and vectors,
/// merged). Claims that cite the best passages are printed first, with date, speaker and quote. When no claim
/// cites them, the passages themselves are printed. Returns false when nothing matched.
pub fn ask(c: &Connection, q: &str) -> bool {
    let r = crate::server::search(c, q, &crate::server::Filters::default(), true);
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
            .query_row("SELECT ifnull(role,''), ts FROM chunks WHERE id=?1", params![cid], |x| {
                Ok((x.get(0)?, x.get(1)?))
            })
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
    let ents = mentions(c, q);
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
        let n = backlog(c, name);
        if n > 0 {
            println!(
                "[{n} sessions that mention {name} have no extracted claims yet: skill/extract.sh --entity \"{name}\"]"
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
            "SELECT COUNT(*) FROM claims c WHERE (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%') AND NOT EXISTS (SELECT 1 FROM claim_about a WHERE a.claim_id=c.id)"
        )
    );
    let mut lost = 0;
    let mut st = c.prepare("SELECT c.id, c.quote, k.text FROM claims c LEFT JOIN chunks k ON k.id=c.chunk_id WHERE c.quote IS NOT NULL").unwrap();
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
    let mut sample = all.clone();
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

// ---------------------------------------------------------------- KG-20 backup export and restore

/// One JSON object per line. A claim's evidence is stored by source ref and quote, not by row id, so another machine can re-locate it.
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
    let sets: [(&str, &str, &[&str]); 5] = [
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
                "id", "type", "thread", "title", "body", "state", "origin", "speaker",
                "asserted_at", "valid_to", "superseded_by", "done_by", "answer", "tag", "stale",
                "source", "quote", "stated", "created", "updated",
            ],
        ),
        (
            "about",
            "SELECT claim_id,entity_id,role FROM claim_about",
            &["claim_id", "entity_id", "role"],
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

/// The source and chunk that hold `quote`, found by source ref.
fn locate(c: &Connection, source: Option<String>, quote: &str) -> Option<(i64, i64)> {
    let q = norm(quote);
    let sid: i64 = c
        .query_row("SELECT id FROM sources WHERE ref=?1", params![crate::db::resolve_ref(c, &source?)], |x| x.get(0))
        .ok()?;
    let mut st = c
        .prepare("SELECT id, text FROM chunks WHERE source_id=?1 ORDER BY seq")
        .unwrap();
    let found = st
        .query_map(params![sid], |x| Ok((x.get::<_, i64>(0)?, x.get::<_, String>(1)?)))
        .unwrap()
        .flatten()
        .find(|x| norm(&x.1).contains(&q))
        .map(|x| (sid, x.0));
    found
}

/// Restore an export into this store. A claim whose source or quote cannot be found here is kept without a quote and counted.
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
                let hit = locate(c, g("source"), &g("quote").unwrap_or_default());
                // An export from before items and claims were merged names the fields text and status.
                let (Some(title), state) = (
                    g("title").or_else(|| g("text")),
                    g("state").or_else(|| {
                        g("status").map(|s| match s.as_str() {
                            "active" => "settled".to_string(),
                            "retracted" => "rejected".to_string(),
                            _ => s,
                        })
                    }),
                ) else {
                    continue;
                };
                if hit.is_none() && g("quote").is_some() {
                    orphan += 1;
                }
                c.execute(
                    include_str!("sql/kg_restore.sql"),
                    params![
                        g("id"),
                        g("type"),
                        g("thread"),
                        title,
                        g("body"),
                        state.unwrap_or("proposed".into()),
                        g("origin"),
                        g("speaker"),
                        g("asserted_at"),
                        g("valid_to"),
                        g("superseded_by"),
                        g("done_by"),
                        g("answer"),
                        g("tag"),
                        g("stale"),
                        hit.map(|h| h.0),
                        hit.map(|h| h.1),
                        g("quote"),
                        g("stated").unwrap_or("stated".into()),
                        g("created"),
                        g("updated")
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
                // Old exports kept evidence in its own rows; the claim now carries its quote.
                match locate(c, g("source"), &g("quote").unwrap_or_default()) {
                    Some((sid, cid)) => {
                        c.execute(
                            "UPDATE claims SET source_id=?2, chunk_id=?3, quote=?4, stated=?5 WHERE id=?1 AND quote IS NULL",
                            params![g("claim_id"), sid, cid, g("quote"), g("stated").unwrap_or("stated".into())],
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
/// Merge a duplicate claim into the one to keep: its entities move over, and it is closed as superseded.
pub fn merge(c: &Connection, keep: &str, drop: &str) -> Result<(), String> {
    let has = |id: &str| -> bool {
        c.query_row("SELECT 1 FROM claims WHERE id=?1", params![id], |_| Ok(()))
            .is_ok()
    };
    if keep == drop || !has(keep) || !has(drop) {
        return Err("both ids must be existing, different claims".into());
    }
    c.execute(
        "INSERT OR IGNORE INTO claim_about(claim_id,entity_id,role)
         SELECT ?1,entity_id,role FROM claim_about WHERE claim_id=?2",
        params![keep, drop],
    )
    .unwrap();
    c.execute(
        "UPDATE claims SET state='superseded', superseded_by=?1, valid_to=?3 WHERE id=?2",
        params![keep, drop, crate::db::chrono_now()],
    )
    .unwrap();
    logw(c, "owner", "kg-merge", drop, &format!("merged into {keep}"));
    Ok(())
}

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
    relink_all(c);
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
