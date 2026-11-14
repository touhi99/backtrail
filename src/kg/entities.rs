//! Entities: the projects, tools and topics that claims are about. Each has aliases, the spellings that count as a
//! mention of it. Entities come from `config.json`, from a built-in list of topics, or are added by the owner.
use super::has_word;
use crate::clock::now;
use crate::config::cfg;
use crate::db::{fxhash, logw, norm};
use rusqlite::{Connection, params};

pub fn ensure(c: &Connection, name: &str, kind: &str, now: &str) -> String {
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

/// An entity by exact name or alias (case-insensitive).
pub fn find(c: &Connection, name: &str) -> Option<(String, String)> {
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
    let now = now();
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
            .query_row("SELECT COUNT(DISTINCT k.source_id) FROM fts JOIN chunks k ON k.id=fts.rowid WHERE fts MATCH ?1 AND k.role='user'", params![m], |r| {
                r.get(0)
            })
            .unwrap_or(0);
        if n < TOPIC_MIN_SESSIONS {
            continue;
        }
        let id = ensure(c, name, "topic", &now);
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
pub fn sync_configured(c: &Connection) -> usize {
    let now = now();
    seed_topics(c);
    let list = &cfg().entities;
    for crate::config::EntitySeed {
        name,
        kind,
        aliases,
    } in list
    {
        let id = ensure(c, name, kind, &now);
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
pub fn link_claim(c: &Connection, id: &str, thread: &str, hay: &str) -> usize {
    let ents: Vec<(String, String, Vec<String>)> = {
        let mut st = c
            .prepare("SELECT e.id, e.name, ( SELECT group_concat(alias, char(31)) FROM entity_aliases a WHERE a.entity_id=e.id ) FROM entities e")
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
        if norm(name) == norm(thread) || aliases.iter().any(|a| !a.is_empty() && has_word(&hay, a))
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
        .map(|(id, th, hay)| link_claim(c, id, th, hay))
        .sum()
}

/// Add an entity with its aliases, then link decisions that mention it. Returns its id.
pub fn add(c: &Connection, name: &str, kind: &str, aliases: &[String]) -> String {
    let id = ensure(c, name, kind, &now());
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
    id
}

/// Add an alias to an existing entity (named by name or alias), then link decisions that mention it. Returns false
/// when the entity is unknown.
pub fn add_alias(c: &Connection, entity: &str, alias: &str) -> bool {
    let Some((id, name)) = find(c, entity) else {
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

/// Hide a term from the entity suggestions.
pub fn ignore_suggestion(c: &Connection, term: &str) {
    c.execute(
        "INSERT OR REPLACE INTO kg_overrides(target,field,value,by,ts) VALUES(?1,'entity_ignore','1','owner',?2)",
        params![term.to_lowercase(), now()],
    )
    .unwrap();
}

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
    let mut st = c.prepare("SELECT c.text, c.source_id FROM chunks c JOIN sources s ON s.id=c.source_id WHERE s.kind='session' AND c.role='user' AND length(c.text)<3000 AND c.text NOT LIKE '%task-notification%' AND c.text NOT LIKE 'This session is being continued%' AND c.text NOT LIKE '<bash-%'").unwrap();
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
        .prepare("SELECT c.text FROM chunks c JOIN sources s ON s.id=c.source_id WHERE s.kind='session' AND c.role!='user'")
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
