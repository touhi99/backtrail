//! What the app's pages and the session-start brief show. Every function here only reads: it filters the `item`,
//! `thread_stat`, `source_stat` and `fact` views (sql/views.sql) and shapes the result as JSON.
use crate::clock::{days_ago, now};
use crate::config::cfg;
use crate::db::{rows, s};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

const OPEN_QUESTIONS: &str = "SELECT * FROM item WHERE type='question' AND state='open'";

/// Questions Claude asked that wait for the owner.
fn waiting_on_owner(c: &Connection) -> Vec<Value> {
    rows(
        c,
        &format!("{OPEN_QUESTIONS} AND origin!='owner' ORDER BY created DESC"),
        &[],
    )
}

/// Questions the owner typed that wait for Claude.
fn waiting_on_claude(c: &Connection) -> Vec<Value> {
    rows(
        c,
        &format!("{OPEN_QUESTIONS} AND origin='owner' ORDER BY created DESC"),
        &[],
    )
}

/// Work that has gone quiet: steps in progress on a thread with no session for 7 days, and up to 8 open steps or
/// questions older than 7 days.
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

/// The Home page.
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
    // Moved: the newest titled session of each thread, for up to six threads active in the last three days.
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
    // To do: steps noted on a live project that are open or in progress.
    let todo = rows(
        c,
        "SELECT i.* FROM item i JOIN threads t ON t.name=i.thread
         WHERE i.type='step' AND i.state IN ('open','doing') AND i.origin!='suggested' AND t.state!='dropped'
         ORDER BY i.created DESC LIMIT 60",
        &[],
    );
    // Pending sessions: real sessions in the last 3 days that no item cites, so no decision came out of them.
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
        // a resumed session has several rows with one title; show it once
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

/// The latest settled decisions of a project that are still in force: not marked superseded, not past their
/// `valid_to`. Each carries who confirmed it (`how`): the owner's own typed words (the keep rule), the owner's
/// accept in the app, or a note. The date falls back to the source's date when the claim has none.
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

/// The short session-start brief (`bt brief` with no flag): one line per project in motion, so a session in a
/// folder that holds many projects is not handed all of their detail. Questions the owner asked still print in
/// full, because Claude has to answer them.
fn brief_short(c: &Connection) {
    println!("Backtrail · {}", &now()[..10]);
    let waiting = waiting_on_owner(c);
    let stale = stale(c);
    let count = |list: &[Value], name: &str| list.iter().filter(|r| s(r, "thread") == name).count();
    println!(
        "Projects in motion (detail for one: `bt brief --thread \"<name>\"`, everything: `bt brief --all`):"
    );
    for t in threads(c).iter().filter(|t| s(t, "state") == "moving") {
        let name = s(t, "name");
        let mut line = format!("  - {name}: {} open, {} done", t["open_s"], t["done_s"]);
        for (n, what) in [
            (count(&waiting, name), "waiting on the owner"),
            (count(&stale, name), "stale"),
        ] {
            if n > 0 {
                line += &format!(", {n} {what}");
            }
        }
        let next = rows(
            c,
            "SELECT title FROM item WHERE type='step' AND state='open' AND thread=?1 ORDER BY created LIMIT 1",
            &[&name],
        );
        if let Some(step) = next.first() {
            line += &format!(" · next: {}", s(step, "title"));
        }
        println!("{line}");
    }
    // Questions waiting on the owner in projects that are not in motion.
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
        println!(
            "Also waiting on the owner, in projects not in motion: {}",
            list.join(", ")
        );
    }
    let asks = waiting_on_claude(c);
    if !asks.is_empty() {
        println!(
            "\nAsked by the owner, waiting on Claude ({}): answer each with `backtrail reply <id> --text ...`",
            asks.len()
        );
        for q in &asks {
            println!(
                "  - {} [{}] {} {}",
                s(q, "id"),
                s(q, "thread"),
                s(q, "title"),
                s(q, "body")
            );
        }
    }
}

/// The session-start brief (`bt brief`): the short form, or with `full` (or a project) open questions, threads in
/// motion and stale work, as text.
pub fn brief(c: &Connection, only: Option<&str>, full: bool) {
    if only.is_none() && !full {
        return brief_short(c);
    }
    let wanted = |row: &Value, key: &str| only.is_none_or(|thread| s(row, key) == thread);
    println!("Backtrail brief · {}", &now()[..10]);

    let waiting: Vec<Value> = waiting_on_owner(c)
        .into_iter()
        .filter(|q| wanted(q, "thread"))
        .collect();
    println!(
        "\nClaude asked the owner, waiting on the owner ({}): ask when it fits the work, then record the reply with `backtrail answer <id> --text ...`. Unanswered ones are parked after 14 days.",
        waiting.len()
    );
    for q in &waiting {
        println!("  - {} [{}] {}", s(q, "id"), s(q, "thread"), s(q, "title"));
    }

    let asks: Vec<Value> = waiting_on_claude(c)
        .into_iter()
        .filter(|q| wanted(q, "thread"))
        .collect();
    println!(
        "\nAsked by the owner, waiting on Claude ({}): answer each with `backtrail reply <id> --text ...`",
        asks.len()
    );
    for q in &asks {
        println!(
            "  - {} [{}] {} {}",
            s(q, "id"),
            s(q, "thread"),
            s(q, "title"),
            s(q, "body")
        );
    }

    println!("\nThreads in motion:");
    for t in threads(c)
        .iter()
        .filter(|t| s(t, "state") == "moving" && wanted(t, "name"))
    {
        let name = s(t, "name");
        println!(
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
            println!("      next: {}", s(step, "title"));
        }
        let answered = rows(
            c,
            "SELECT title, answer, origin FROM item WHERE type='question' AND state='answered' AND thread=?1
             ORDER BY updated DESC LIMIT 2",
            &[&name],
        );
        for q in &answered {
            // A question the owner asked is answered by Claude; one Claude asked is answered by the owner.
            let who = if s(q, "origin") == "owner" {
                "owner asked, Claude answered"
            } else {
                "owner answered"
            };
            println!("      {who} \"{}\": {}", s(q, "title"), s(q, "answer"));
        }
        let decided = decided(c, name, 5);
        if !decided.is_empty() {
            println!("      decided, latest settled:");
        }
        for d in &decided {
            println!(
                "        {} · {} ({})",
                s(d, "day"),
                s(d, "title"),
                s(d, "how")
            );
        }
    }

    let stale: Vec<Value> = stale(c)
        .into_iter()
        .filter(|x| wanted(x, "thread"))
        .collect();
    if !stale.is_empty() {
        println!("\nGone stale:");
        for x in &stale {
            println!(
                "  - [{}] {} (since {})",
                s(x, "thread"),
                s(x, "title"),
                s(x, "since").get(..10).unwrap_or("")
            );
        }
    }
    println!(
        "\nThe store is the source of truth: backtrail serve (http://127.0.0.1:{}). Sources are read only; notes carry no status.",
        cfg().port
    );
}

/// A project page: the thread, its items, its sources, and the facts about it that came from its own sources.
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
    json!({"thread": thread.first(), "items": items, "sources": sources, "claims": claims, "decided": decided(c, name, 8)})
}

/// The Items page: every item, filtered by type, state, thread and thread kind (empty means any).
pub fn items(c: &Connection, kind: &str, state: &str, thread: &str, thread_kind: &str) -> Value {
    json!(rows(
        c,
        "SELECT i.* FROM item i JOIN threads t ON t.name=i.thread
         WHERE (?1='' OR i.type=?1) AND (?2='' OR i.state=?2) AND (?3='' OR i.thread=?3) AND (?4='' OR t.kind=?4)
         ORDER BY i.created DESC LIMIT 300",
        &[&kind, &state, &thread, &thread_kind],
    ))
}

/// The source reader: one source, its passages (the first 600) and the items that cite it.
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

/// The original file of a note (Markdown) or an artifact (HTML), for the formatted view. Sessions have no single
/// file to show.
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

/// The trail: what happened on each thread, by day.
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

// ---- how threads connect

/// Edges between threads: `mentions` when the sources of one thread name another often enough, and the lineage
/// relations (built on, spun out of, ...) filed as claims. Counting mentions runs one full-text query per thread,
/// so the result is kept until the store changes.
fn thread_edges(c: &Connection) -> Vec<Value> {
    static CACHE: std::sync::Mutex<Option<(String, Vec<Value>)>> = std::sync::Mutex::new(None);
    let version = format!(
        "{}-{}",
        crate::db::count(c, "PRAGMA data_version", &[]),
        c.total_changes()
    );
    let mut cache = CACHE.lock().unwrap();
    if let Some((cached, edges)) = cache.as_ref() {
        if *cached == version {
            return edges.clone();
        }
    }
    const MIN_MENTIONS: i64 = 12;
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

/// The context graph: threads, items and sources as nodes; mention, lineage, replaced-by, belongs-to and comes-from
/// edges.
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

// ---- the focus view of one thread

/// How many notes and artifacts the focus view shows as cards.
const FOCUS_NOTES: usize = 14;

/// One readable line about a passage: the frontmatter description, else its first real sentence.
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

/// Note-to-note links written as `[[wikilinks]]`: for each note, the notes and artifacts it names.
fn wikilinks(c: &Connection) -> HashMap<i64, HashSet<i64>> {
    let by_title: HashMap<String, i64> = rows(
        c,
        "SELECT id, title FROM sources WHERE kind IN ('note','artifact')",
        &[],
    )
    .iter()
    .map(|r| (s(r, "title").to_lowercase(), r["id"].as_i64().unwrap_or(0)))
    .collect();
    let link = regex::Regex::new(r"\[\[([^\]|#]+)").unwrap();
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
            if let Some(&to) = by_title.get(&title) {
                if to != from {
                    links.entry(from).or_default().insert(to);
                }
            }
        }
    }
    links
}

/// The focus view: one thread with its notes and artifacts as cards (gist, outline, links), its items, its newest
/// sessions and the threads it touches.
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
        let links_out = links.get(&id).map_or(0, |to| to.len());
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
        "wikilinks_resolved": links.values().map(|to| to.len()).sum::<usize>(),
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
