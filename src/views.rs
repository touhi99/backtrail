macro_rules! say {
    ($out:expr, $($arg:tt)*) => {
        let _ = writeln!($out, $($arg)*);
    };
}

use crate::clock::{days_ago, now};
use crate::config::cfg;
use crate::db::{rows, s};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;

const OPEN_QUESTIONS: &str = "SELECT * FROM item WHERE type='question' AND state='open'";

fn waiting_on_owner(c: &Connection) -> Vec<Value> {
    rows(
        c,
        &format!("{OPEN_QUESTIONS} AND origin!='owner' ORDER BY created DESC"),
        &[],
    )
}

fn waiting_on_claude(c: &Connection) -> Vec<Value> {
    rows(
        c,
        &format!("{OPEN_QUESTIONS} AND origin='owner' ORDER BY created DESC"),
        &[],
    )
}

fn stale(c: &Connection) -> Vec<Value> {
    let mut out = rows(
        c,
        "SELECT i.id, i.title, i.type, i.thread, i.updated AS since, 'doing' AS why
         FROM item i JOIN thread_stat t ON t.name=i.thread
         WHERE i.type='step' AND i.state='doing'
           AND ifnull(t.session_last,'0') < strftime('%Y-%m-%dT%H:%M:%SZ','now','-7 days')",
        &[],
    );
    let open = rows(
        c,
        "SELECT i.id, i.title, i.type, i.thread, i.created AS since, 'open' AS why
         FROM item i JOIN threads t ON t.name=i.thread
         WHERE i.type IN ('step','question') AND i.state='open' AND t.state!='dropped'
         ORDER BY i.created",
        &[],
    );
    out.extend(
        open.into_iter()
            .filter(|r| days_ago(s(r, "since")) >= 7)
            .take(8),
    );
    out
}

fn threads(c: &Connection) -> Vec<Value> {
    rows(c, "SELECT * FROM thread_stat ORDER BY last DESC", &[])
}

pub fn today(c: &Connection) -> Value {
    let suggested = rows(
        c,
        "SELECT * FROM item WHERE origin='suggested' AND state='proposed' ORDER BY created DESC",
        &[],
    );
    let looks_done = rows(
        c,
        "SELECT * FROM item WHERE type='step' AND state='looks_done'",
        &[],
    );
    let recent = rows(
        c,
        "SELECT id, ref, title, kind, thread, last_ts FROM sources
         WHERE kind='session' AND thread IS NOT NULL ORDER BY last_ts DESC LIMIT 400",
        &[],
    );
    let mut moved: Vec<Value> = vec![];
    let mut seen = HashSet::new();
    for r in recent
        .iter()
        .filter(|r| !s(r, "title").starts_with("Untitled"))
    {
        if days_ago(s(r, "last_ts")) > 3 || moved.len() >= 6 {
            break;
        }
        if seen.insert(s(r, "thread").to_string()) {
            moved.push(r.clone());
        }
    }
    let totals = rows(
        c,
        "SELECT (SELECT COUNT(*) FROM sources WHERE kind='session') AS sessions,
                (SELECT COUNT(*) FROM sources WHERE kind='note') AS notes,
                (SELECT COUNT(*) FROM sources WHERE kind='artifact') AS artifacts,
                (SELECT COUNT(*) FROM chunks) AS chunks,
                (SELECT COUNT(*) FROM item) AS items",
        &[],
    );
    let todo = rows(
        c,
        "SELECT i.* FROM item i JOIN threads t ON t.name=i.thread
         WHERE i.type='step' AND i.state IN ('open','doing') AND i.origin!='suggested' AND t.state!='dropped'
         ORDER BY i.created DESC LIMIT 60",
        &[],
    );
    let pending: Vec<Value> = rows(
        c,
        "SELECT s.id, s.ref, s.title, s.thread, s.last_ts, s.nchunks FROM source_stat s
         WHERE s.kind='session' AND s.thread!='Unsorted' AND s.nchunks>=6 AND s.cited=0 AND s.reviewed=0
         ORDER BY s.last_ts DESC LIMIT 200",
        &[],
    )
    .into_iter()
    .filter(|r| days_ago(s(r, "last_ts")) <= 3)
    .collect::<Vec<_>>()
    .into_iter()
    .filter({
        let mut seen = HashSet::new();
        move |r| seen.insert((s(r, "title").to_string(), s(r, "thread").to_string()))
    })
    .collect();
    json!({
        "todo": todo, "pending": pending,
        "waiting": waiting_on_owner(c), "asks": waiting_on_claude(c), "suggested": suggested, "looks": looks_done,
        "threads": threads(c), "moved": moved, "stale": stale(c), "totals": totals[0], "date": now(),
    })
}

pub fn decided(c: &Connection, thread: &str, limit: i64) -> Vec<Value> {
    let today = now()[..10].to_string();
    let mut found = rows(
        c,
        "SELECT c.id, c.title, c.review, k.role AS role, c.source_id, c.chunk_id,
                substr(COALESCE(NULLIF(c.asserted_at,''), s.first_ts, ''), 1, 10) AS day
         FROM claims c LEFT JOIN chunks k ON k.id=c.chunk_id LEFT JOIN sources s ON s.id=c.source_id
         WHERE c.type='decision' AND c.state='settled' AND c.thread=?1
           AND IFNULL(c.stale,'') NOT LIKE 'superseded%' AND IFNULL(c.superseded_by,'')=''
           AND (IFNULL(c.valid_to,'')='' OR c.valid_to>=?2)
         ORDER BY day DESC, c.id LIMIT ?3",
        &[&thread, &today, &limit],
    );
    for d in &mut found {
        let how = if s(d, "review") == "auto" {
            "owner-stated"
        } else if matches!(s(d, "role"), "note" | "artifact") {
            "from a note"
        } else {
            "owner-accepted"
        };
        let day = Some(s(d, "day"))
            .filter(|x| !x.is_empty())
            .unwrap_or("undated")
            .to_string();
        d["how"] = json!(how);
        d["day"] = json!(day);
    }
    found
}

fn brief_short(c: &Connection) -> String {
    let mut out = String::new();
    say!(out, "Backtrail · {}", &now()[..10]);
    say!(out, "{}", crate::coverage::one_line(c));
    let waiting = waiting_on_owner(c);
    let count = |list: &[Value], name: &str| list.iter().filter(|r| s(r, "thread") == name).count();
    say!(
        out,
        "Projects in motion (detail for one: `bt brief --thread \"<name>\"`, everything: `bt brief --all`):"
    );
    for t in threads(c).iter().filter(|t| s(t, "state") == "moving") {
        let name = s(t, "name");
        let mut line = format!("  - {name}: {} open, {} done", t["open_s"], t["done_s"]);
        let n = count(&waiting, name);
        if n > 0 {
            let _ = write!(line, ", {n} waiting on the owner");
        }
        let next = rows(
            c,
            "SELECT title FROM item WHERE type='step' AND state='open' AND thread=?1 ORDER BY created LIMIT 1",
            &[&name],
        );
        if let Some(step) = next.first() {
            let _ = write!(line, " · next: {}", s(step, "title"));
        }
        say!(out, "{line}");
    }
    let moving: HashSet<String> = threads(c)
        .iter()
        .filter(|t| s(t, "state") == "moving")
        .map(|t| s(t, "name").to_string())
        .collect();
    let mut elsewhere: BTreeMap<String, usize> = BTreeMap::new();
    for q in waiting.iter().filter(|q| !moving.contains(s(q, "thread"))) {
        *elsewhere.entry(s(q, "thread").to_string()).or_default() += 1;
    }
    if !elsewhere.is_empty() {
        let list: Vec<String> = elsewhere.iter().map(|(n, k)| format!("{n} {k}")).collect();
        say!(
            out,
            "Also waiting on the owner, in projects not in motion: {}",
            list.join(", ")
        );
    }
    let asks = waiting_on_claude(c);
    if !asks.is_empty() {
        say!(
            out,
            "\nAsked by the owner, waiting on Claude ({}): answer each with `backtrail reply <id> --text ...`",
            asks.len()
        );
        for q in &asks {
            say!(
                out,
                "  - {} [{}] {} {}",
                s(q, "id"),
                s(q, "thread"),
                s(q, "title"),
                s(q, "body")
            );
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    Short,
    Full,
}

pub fn brief(c: &Connection, only: Option<&str>, detail: Detail) -> String {
    let mut out = String::new();
    if only.is_none() && detail == Detail::Short {
        return brief_short(c);
    }
    let wanted = |row: &Value, key: &str| only.is_none_or(|thread| s(row, key) == thread);
    say!(out, "Backtrail brief · {}", &now()[..10]);

    let waiting: Vec<Value> = waiting_on_owner(c)
        .into_iter()
        .filter(|q| wanted(q, "thread"))
        .collect();
    say!(
        out,
        "\nClaude asked the owner, waiting on the owner ({}): ask when it fits the work, then record the reply with `backtrail answer <id> --text ...`. Unanswered ones are parked after 14 days.",
        waiting.len()
    );
    for q in &waiting {
        say!(
            out,
            "  - {} [{}] {}",
            s(q, "id"),
            s(q, "thread"),
            s(q, "title")
        );
    }

    let asks: Vec<Value> = waiting_on_claude(c)
        .into_iter()
        .filter(|q| wanted(q, "thread"))
        .collect();
    say!(
        out,
        "\nAsked by the owner, waiting on Claude ({}): answer each with `backtrail reply <id> --text ...`",
        asks.len()
    );
    for q in &asks {
        say!(
            out,
            "  - {} [{}] {} {}",
            s(q, "id"),
            s(q, "thread"),
            s(q, "title"),
            s(q, "body")
        );
    }

    say!(out, "\nThreads in motion:");
    for t in threads(c)
        .iter()
        .filter(|t| s(t, "state") == "moving" && wanted(t, "name"))
    {
        let name = s(t, "name");
        say!(
            out,
            "  - {name}: {} open steps, {} done; last activity {}",
            t["open_s"],
            t["done_s"],
            s(t, "last").get(..10).unwrap_or("?")
        );
        let next = rows(
            c,
            "SELECT title FROM item WHERE type='step' AND state='open' AND thread=?1 ORDER BY created LIMIT 1",
            &[&name],
        );
        if let Some(step) = next.first() {
            say!(out, "      next: {}", s(step, "title"));
        }
        let answered = rows(
            c,
            "SELECT title, answer, origin FROM item WHERE type='question' AND state='answered' AND thread=?1
             ORDER BY updated DESC LIMIT 2",
            &[&name],
        );
        for q in &answered {
            let who = if s(q, "origin") == "owner" {
                "owner asked, Claude answered"
            } else {
                "owner answered"
            };
            say!(out, "      {who} \"{}\": {}", s(q, "title"), s(q, "answer"));
        }
        let decided = decided(c, name, 5);
        if !decided.is_empty() {
            say!(out, "      decided, latest settled:");
        }
        for d in &decided {
            say!(
                out,
                "        {} · {} ({})",
                s(d, "day"),
                s(d, "title"),
                s(d, "how")
            );
        }
    }

    say!(
        out,
        "\nThe store is the source of truth: backtrail serve (http://127.0.0.1:{}). Sources are read only; notes carry no status.",
        cfg().port
    );
    out
}

pub fn thread(c: &Connection, name: &str) -> Value {
    let thread = rows(c, "SELECT * FROM thread_stat WHERE name=?1", &[&name]);
    let items = rows(
        c,
        "SELECT * FROM item WHERE thread=?1 ORDER BY created",
        &[&name],
    );
    let sources = rows(
        c,
        "SELECT * FROM source_stat WHERE thread=?1 ORDER BY last_ts DESC",
        &[&name],
    );
    let claims = rows(
        c,
        "SELECT f.id, f.type, f.status, f.speaker, substr(f.asserted_at,1,10) AS d, f.text, f.quote AS q,
                (SELECT title FROM claims WHERE id=f.superseded_by) AS newer
         FROM fact f
         WHERE f.type NOT LIKE 'relation:%' AND f.status!='retracted'
           AND f.id IN (SELECT a.claim_id FROM claim_about a JOIN entities e ON e.id=a.entity_id WHERE e.name=?1)
           AND f.source_id IN (SELECT id FROM sources WHERE thread=?1)
         ORDER BY f.asserted_at",
        &[&name],
    );
    json!({"thread": thread.first(), "items": items, "sources": sources, "claims": claims, "decided": decided(c, name, 8),
           "artifacts": thread_artifacts(c, name), "agent": agent_start(name)})
}

fn agent_start(thread: &str) -> Value {
    let brief = format!(
        "\"$(backtrail brief --thread '{}')\"",
        thread.replace('\'', "'\\''")
    );
    let (name, start) = match cfg().agent.as_str() {
        "gemini" => ("Gemini CLI", format!("gemini -i {brief}")),
        "codex" => ("Codex", format!("codex {brief}")),
        "antigravity" => (
            "Antigravity",
            format!("backtrail brief --thread '{thread}'"),
        ),
        "command" => (
            "the coding agent",
            format!("backtrail brief --thread '{thread}'"),
        ),
        _ => ("Claude Code", format!("claude {brief}")),
    };
    json!({"name": name, "start": start})
}

fn thread_artifacts(c: &Connection, name: &str) -> Vec<Value> {
    let link = regex::Regex::new(r"claude\.ai/artifact/([A-Za-z0-9]{20,24})")
        .expect("regex literal is valid");
    let mut found: HashMap<String, (String, String, Option<i64>)> = HashMap::new();
    for r in rows(
        c,
        "SELECT id, substr(ref, 10) AS aid, title, last_ts FROM sources WHERE kind='artifact' AND thread=?1",
        &[&name],
    ) {
        found.insert(
            s(&r, "aid").to_string(),
            (
                s(&r, "title").to_string(),
                s(&r, "last_ts").to_string(),
                r["id"].as_i64(),
            ),
        );
    }
    for r in rows(
        c,
        "SELECT ch.text AS text, s.last_ts AS ts FROM chunks ch JOIN sources s ON s.id=ch.source_id
         WHERE s.thread=?1 AND s.kind!='artifact' AND ch.text LIKE '%claude.ai/artifact/%'",
        &[&name],
    ) {
        for m in link.captures_iter(s(&r, "text")) {
            let e = found
                .entry(m[1].to_string())
                .or_insert((String::new(), String::new(), None));
            if s(&r, "ts") > e.1.as_str() && e.2.is_none() {
                e.1 = s(&r, "ts").to_string();
            }
        }
    }
    let mut out: Vec<Value> = found
        .into_iter()
        .map(|(id, (title, ts, source_id))| {
            let meta = rows(
                c,
                "SELECT title, updated FROM artifact_meta WHERE id=?1",
                &[&id],
            );
            let (title, ts, source_id) = if source_id.is_some() {
                (title, ts, source_id)
            } else {
                let copy = rows(
                    c,
                    "SELECT id, title, last_ts FROM sources WHERE kind='artifact' AND ref=?1",
                    &[&format!("artifact:{id}")],
                );
                match copy.first() {
                    Some(r) => (
                        s(r, "title").to_string(),
                        s(r, "last_ts").to_string(),
                        r["id"].as_i64(),
                    ),
                    None => (title, ts, None),
                }
            };
            let title = if title.is_empty() {
                meta.first()
                    .map(|m| s(m, "title").to_string())
                    .unwrap_or_default()
            } else {
                title
            };
            let ts = meta
                .first()
                .map(|m| s(m, "updated").to_string())
                .filter(|u| !u.is_empty())
                .unwrap_or(ts);
            let about = source_id
                .map(|sid| {
                    crate::db::strings(
                        c,
                        "SELECT text FROM chunks WHERE source_id=?1 ORDER BY seq LIMIT 5",
                        &[&sid],
                    )
                    .iter()
                    .map(|text| gist(text))
                    .find(|g| g.chars().count() >= 40)
                    .unwrap_or_default()
                })
                .unwrap_or_default();
            json!({"id": id, "title": title, "ts": ts, "source_id": source_id, "about": about,
                   "url": format!("https://claude.ai/artifact/{id}")})
        })
        .collect();
    out.sort_by(|a, b| s(b, "ts").cmp(s(a, "ts")));
    out
}

pub fn items(c: &Connection, kind: &str, state: &str, thread: &str, thread_kind: &str) -> Value {
    json!(rows(
        c,
        "SELECT i.* FROM item i JOIN threads t ON t.name=i.thread
         WHERE (?1='' OR i.type=?1) AND (?2='' OR i.state=?2) AND (?3='' OR i.thread=?3) AND (?4='' OR t.kind=?4)
         ORDER BY i.created DESC LIMIT 300",
        &[&kind, &state, &thread, &thread_kind],
    ))
}

pub fn source(c: &Connection, id: i64) -> Value {
    let source = rows(c, "SELECT * FROM source_stat WHERE id=?1", &[&id]);
    let chunks = rows(
        c,
        "SELECT id, locator, role, ts, text FROM chunks WHERE source_id=?1 ORDER BY seq LIMIT 600",
        &[&id],
    );
    let items = rows(c, "SELECT * FROM item WHERE source_id=?1", &[&id]);
    json!({"source": source.first(), "chunks": chunks, "items": items})
}

pub fn source_file(c: &Connection, id: i64) -> Value {
    let found = rows(
        c,
        "SELECT kind, ifnull(path,'') AS path FROM sources WHERE id=?1",
        &[&id],
    );
    let (kind, path) = found
        .first()
        .map_or(("", ""), |r| (s(r, "kind"), s(r, "path")));
    let text = if kind == "note" || kind == "artifact" {
        std::fs::read_to_string(path).unwrap_or_default()
    } else {
        String::new()
    };
    json!({"kind": kind, "text": text})
}

pub fn trail(c: &Connection) -> Value {
    json!({
        "threads": rows(c, "SELECT name, state, summary FROM thread_stat WHERE state!='unsorted'", &[]),
        "sources": rows(
            c,
            "SELECT id, thread, kind, title, substr(CASE WHEN kind='commit' THEN last_ts ELSE first_ts END,1,10) AS d
             FROM sources
             WHERE kind IN ('session','artifact','commit') AND thread IS NOT NULL AND first_ts IS NOT NULL
             ORDER BY d",
            &[],
        ),
        "items": rows(
            c,
            "SELECT thread, type, title, substr(coalesce(updated, created),1,10) AS d FROM item
             WHERE (type='step' AND state IN ('done','looks_done'))
                OR (type='decision' AND state='settled')
                OR (type='question' AND state IN ('answered','done'))",
            &[],
        ),
        "links": rows(
            c,
            "SELECT type, text, substr(asserted_at,1,10) AS d FROM fact
             WHERE type IN ('relation:built on','relation:spun out of','relation:forked from') AND status!='retracted'",
            &[],
        ),
    })
}

fn thread_edges(c: &Connection) -> Vec<Value> {
    static CACHE: std::sync::Mutex<Option<(String, Vec<Value>)>> = std::sync::Mutex::new(None);
    const MIN_MENTIONS: i64 = 12;
    let version = format!(
        "{}-{}",
        crate::db::count(c, "PRAGMA data_version", &[]),
        c.total_changes()
    );
    let mut cache = CACHE.lock().expect("lock poisoned");
    if let Some((cached, edges)) = cache.as_ref()
        && *cached == version
    {
        return edges.clone();
    }
    let mut mentions: BTreeMap<(String, String), i64> = BTreeMap::new();
    for name in crate::db::strings(c, "SELECT name FROM threads WHERE name!='Unsorted'", &[]) {
        let mut names = vec![name.to_lowercase()];
        names.extend(cfg().aliases.get(&name).into_iter().flatten().cloned());
        let query = names
            .iter()
            .map(|a| format!("\"{}\"", a.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" OR ");
        let hits = rows(
            c,
            "SELECT s.thread AS other, COUNT(DISTINCT s.id) AS n
             FROM fts JOIN chunks ch ON ch.id=fts.rowid JOIN sources s ON s.id=ch.source_id
             WHERE fts MATCH ?1 AND s.thread IS NOT NULL AND s.thread NOT IN ('Unsorted', ?2)
             GROUP BY s.thread",
            &[&query, &name],
        );
        for h in hits {
            let other = s(&h, "other").to_string();
            let pair = if name <= other {
                (name.clone(), other)
            } else {
                (other, name.clone())
            };
            *mentions.entry(pair).or_insert(0) += h["n"].as_i64().unwrap_or(0);
        }
    }
    let mut edges: Vec<Value> = mentions
        .into_iter()
        .filter(|(_, n)| *n >= MIN_MENTIONS)
        .map(|((a, b), n)| json!({"a": format!("T:{a}"), "b": format!("T:{b}"), "kind": "mentions", "w": n}))
        .collect();
    edges.extend(rows(
        c,
        "SELECT 'T:'||ea.name AS a, 'T:'||eb.name AS b, substr(f.type, 10) AS kind, 1 AS w, f.quote, f.asserted_at AS ts,
                CASE f.status WHEN 'proposed' THEN 'proposed' END AS state, f.chunk_id, f.source, f.source_title, f.source_id
         FROM fact f
         JOIN claim_about sa ON sa.claim_id=f.id AND sa.role='subject' JOIN entities ea ON ea.id=sa.entity_id
         JOIN claim_about ob ON ob.claim_id=f.id AND ob.role='object' JOIN entities eb ON eb.id=ob.entity_id
         WHERE f.type IN ('relation:spun out of','relation:forked from','relation:built on','relation:replaced by')
           AND f.status!='retracted'",
        &[],
    ));
    *cache = Some((version, edges.clone()));
    edges
}

pub fn graph(c: &Connection) -> Value {
    let mut nodes: Vec<Value> = vec![];
    let mut edges = thread_edges(c);
    for t in rows(c, "SELECT * FROM thread_stat WHERE name!='Unsorted'", &[]) {
        nodes.push(json!({
            "id": format!("T:{}", s(&t, "name")), "kind": "thread", "label": t["name"], "state": t["state"],
            "n": t["sources"], "q": t["open_questions"], "area": t["area"],
            "first": t["session_first"], "last": t["session_last"],
        }));
    }
    for i in rows(
        c,
        "SELECT id, superseded_by FROM item WHERE superseded_by IS NOT NULL",
        &[],
    ) {
        edges.push(json!({
            "a": i["id"], "b": i["superseded_by"], "kind": "replaced by", "w": 1, "quote": null, "ts": null,
            "state": null, "chunk_id": null, "source": null, "source_title": null, "source_id": null,
        }));
    }
    let mut cited = HashSet::new();
    for i in rows(
        c,
        "SELECT id, type, title, state, thread, source_id FROM item WHERE state!='rejected' AND thread!='Unsorted'",
        &[],
    ) {
        let id = format!("I:{}", s(&i, "id"));
        nodes.push(json!({"id": id, "kind": i["type"], "label": i["title"], "state": i["state"], "thread": i["thread"]}));
        edges.push(
            json!({"a": id, "b": format!("T:{}", s(&i, "thread")), "kind": "belongs to", "w": 1}),
        );
        if let Some(source) = i["source_id"].as_i64() {
            cited.insert(source);
            edges.push(json!({"a": id, "b": format!("S:{source}"), "kind": "comes from", "w": 1}));
        }
    }
    for src in rows(
        c,
        "SELECT id, kind, title, thread FROM sources WHERE kind!='owner' AND ifnull(thread,'')!='Unsorted'",
        &[],
    ) {
        let id = src["id"].as_i64().unwrap_or(0);
        nodes.push(json!({
            "id": format!("S:{id}"), "kind": src["kind"], "label": src["title"], "thread": src["thread"],
            "cited": cited.contains(&id),
        }));
        if let Some(thread) = src["thread"].as_str() {
            edges.push(json!({"a": format!("S:{id}"), "b": format!("T:{thread}"), "kind": "belongs to", "w": 1}));
        }
    }
    json!({"nodes": nodes, "edges": edges})
}

const FOCUS_NOTES: usize = 14;

fn gist(text: &str) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    if lines.first().map(|l| l.trim()) == Some("---") {
        let mut end = 0;
        for (i, l) in lines.iter().enumerate().skip(1) {
            if l.trim() == "---" {
                end = i;
                break;
            }
            if let Some(d) = l.trim().strip_prefix("description:") {
                return d.trim().trim_matches('"').chars().take(170).collect();
            }
        }
        lines = lines.split_off((end + 1).min(lines.len()));
    }
    let is_prose = |l: &&str| {
        l.chars().count() >= 30
            && !["#", "|", "---", "<"]
                .iter()
                .any(|mark| l.starts_with(mark))
            && !l.contains('›')
    };
    let prose = lines
        .iter()
        .map(|l| l.trim())
        .filter(is_prose)
        .take(4)
        .collect::<Vec<_>>()
        .join(" ");
    let plain = prose
        .replace("**", "")
        .replace('`', "")
        .replace("[[", "")
        .replace("]]", "");
    plain
        .split_inclusive(". ")
        .next()
        .unwrap_or("")
        .chars()
        .take(170)
        .collect()
}

fn wikilinks(c: &Connection) -> HashMap<i64, HashSet<i64>> {
    let by_title: HashMap<String, i64> = rows(
        c,
        "SELECT id, title FROM sources WHERE kind IN ('note','artifact')",
        &[],
    )
    .iter()
    .map(|r| (s(r, "title").to_lowercase(), r["id"].as_i64().unwrap_or(0)))
    .collect();
    let link = regex::Regex::new(r"\[\[([^\]|#]+)").expect("regex literal is valid");
    let mut links: HashMap<i64, HashSet<i64>> = HashMap::new();
    let passages = rows(
        c,
        "SELECT ch.source_id, ch.text FROM chunks ch JOIN sources s ON s.id=ch.source_id
         WHERE s.kind='note' AND ch.text LIKE '%[[%'",
        &[],
    );
    for p in passages {
        let from = p["source_id"].as_i64().unwrap_or(0);
        for m in link.captures_iter(s(&p, "text")) {
            let title = m[1].trim().rsplit('/').next().unwrap_or("").to_lowercase();
            if let Some(&to) = by_title.get(&title)
                && to != from
            {
                links.entry(from).or_default().insert(to);
            }
        }
    }
    links
}

pub fn focus(c: &Connection, name: &str) -> Value {
    let thread = rows(
        c,
        "SELECT *, sources AS n FROM thread_stat WHERE name=?1",
        &[&name],
    );
    let items = rows(
        c,
        "SELECT * FROM item WHERE thread=?1 AND state!='rejected' ORDER BY created DESC",
        &[&name],
    );
    let documents = rows(
        c,
        "SELECT * FROM source_stat WHERE thread=?1 AND kind IN ('note','artifact') ORDER BY cited DESC, last_ts DESC",
        &[&name],
    );
    let links = wikilinks(c);
    let shown: Vec<i64> = documents
        .iter()
        .take(FOCUS_NOTES)
        .filter_map(|d| d["id"].as_i64())
        .collect();
    let mut notes = vec![];
    for d in documents.iter().take(FOCUS_NOTES) {
        let id = d["id"].as_i64().unwrap_or(0);
        let gist = crate::db::strings(
            c,
            "SELECT text FROM chunks WHERE source_id=?1 ORDER BY seq LIMIT 5",
            &[&id],
        )
        .iter()
        .map(|text| gist(text))
        .find(|g| g.chars().count() >= 40)
        .unwrap_or_default();
        let outline = rows(
            c,
            "SELECT locator AS heading, MIN(id) AS chunk_id FROM chunks
             WHERE source_id=?1 AND locator NOT IN ('#top','§top') GROUP BY locator ORDER BY MIN(seq) LIMIT 9",
            &[&id],
        );
        let links_out = links.get(&id).map_or(0, std::collections::HashSet::len);
        let links_in = links
            .iter()
            .filter(|(from, to)| **from != id && to.contains(&id))
            .count();
        notes.push(json!({
            "id": id, "kind": d["kind"], "title": d["title"], "gist": gist, "outline": outline, "cited": d["cited"],
            "last_ts": d["last_ts"], "passages": d["nchunks"], "links": links_out + links_in,
        }));
    }
    let mut note_edges = vec![];
    for a in &shown {
        let mut targets: Vec<&i64> = links
            .get(a)
            .into_iter()
            .flatten()
            .filter(|b| shown.contains(b))
            .collect();
        targets.sort();
        note_edges.extend(targets.into_iter().map(|b| json!({"a": a, "b": b})));
    }
    let me = format!("T:{name}");
    let mut linked = vec![];
    for e in thread_edges(c) {
        let (a, b, kind) = (s(&e, "a"), s(&e, "b"), s(&e, "kind"));
        if (kind == "mentions" || kind == "built on") && (a == me || b == me) {
            let other = if a == me { b } else { a };
            linked.push(json!({
                "name": other.trim_start_matches("T:"), "kind": kind, "w": e["w"],
                "dir": if a == me { "out" } else { "in" },
            }));
        }
    }
    json!({
        "thread": thread.first(),
        "items": items,
        "notes": notes,
        "note_edges": note_edges,
        "wikilinks_resolved": links.values().map(std::collections::HashSet::len).sum::<usize>(),
        "counts": rows(c, "SELECT kind, COUNT(*) AS n FROM sources WHERE thread=?1 GROUP BY kind", &[&name]),
        "sessions": rows(
            c,
            "SELECT id, title, last_ts, nchunks FROM sources WHERE thread=?1 AND kind='session' ORDER BY last_ts DESC LIMIT 6",
            &[&name],
        ),
        "linked": linked,
        "hidden_notes": documents.len().saturating_sub(FOCUS_NOTES),
    })
}
