use crate::db::*;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::io::Read;
use std::time::Duration;
use tiny_http::{Header, Method, Response, Server};

#[derive(rust_embed::RustEmbed)]
#[folder = "app/dist/"]
struct Assets;

fn asset(path: &str) -> Option<Response<std::io::Cursor<Vec<u8>>>> {
    let p = path.trim_start_matches('/');
    let p = if p.is_empty() { "index.html" } else { p };
    let f = Assets::get(p).or_else(|| {
        if p.starts_with("assets/") {
            None
        } else {
            Assets::get("index.html")
        }
    })?;
    let mime = match p.rsplit('.').next().unwrap_or("") {
        "js" => "text/javascript",
        "css" => "text/css",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "woff2" => "font/woff2",
        "json" => "application/json",
        _ => "text/html; charset=utf-8",
    };
    Some(
        Response::from_data(f.data.to_vec())
            .with_header(Header::from_bytes("Content-Type", mime).unwrap()),
    )
}

/// One readable line about a note or artifact: frontmatter description, else the first real sentence.
fn gist(text: &str, kind: &str) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    if kind == "artifact" && !lines.is_empty() {
        lines.remove(0);
    }
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
    let joined: String = lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| {
            l.chars().count() >= 30
                && !l.starts_with('#')
                && !l.starts_with('|')
                && !l.starts_with("---")
                && !l.starts_with("<")
                && !l.contains('›')
        })
        .take(4)
        .collect::<Vec<_>>()
        .join(" ");
    let clean = joined
        .replace("**", "")
        .replace('`', "")
        .replace("[[", "")
        .replace("]]", "");
    let first: String = clean
        .split_inclusive(". ")
        .next()
        .unwrap_or("")
        .chars()
        .take(170)
        .collect();
    first
}

/// The focus view: one thread with the notes and artifacts it holds (each with a gist and outline), its items,
/// the threads it touches, and note-to-note links found as [[wikilinks]].
fn focus(c: &Connection, name: &str) -> Value {
    let t = rows(c, include_str!("sql/server_focus.sql"), &[&name]);
    let items = rows(c, include_str!("sql/server_focus_2.sql"), &[&name]);
    let src = rows(c, include_str!("sql/server_focus_3.sql"), &[&name]);
    // all note titles for wikilink resolution
    let all = rows(
        c,
        "SELECT id, title FROM sources WHERE kind IN ('note','artifact')",
        &[],
    );
    let mut by_title: std::collections::HashMap<String, i64> = Default::default();
    for a in &all {
        by_title.insert(
            a["title"].as_str().unwrap_or("").to_lowercase(),
            a["id"].as_i64().unwrap_or(0),
        );
    }
    let wl = regex::Regex::new(r"\[\[([^\]|#]+)").unwrap();
    let mut links: std::collections::HashMap<i64, std::collections::HashSet<i64>> =
        Default::default();
    for ch in rows(c, include_str!("sql/server_focus_4.sql"), &[]) {
        let sid = ch["sid"].as_i64().unwrap_or(0);
        for m in wl.captures_iter(ch["text"].as_str().unwrap_or("")) {
            let key = m[1].trim().rsplit('/').next().unwrap_or("").to_lowercase();
            if let Some(&to) = by_title.get(&key) {
                if to != sid {
                    links.entry(sid).or_default().insert(to);
                }
            }
        }
    }
    let shown: Vec<i64> = src
        .iter()
        .take(14)
        .filter_map(|s| s["id"].as_i64())
        .collect();
    let mut notes = vec![];
    for s in src.iter().take(14) {
        let id = s["id"].as_i64().unwrap_or(0);
        let first = rows(
            c,
            "SELECT text FROM chunks WHERE source_id=?1 ORDER BY seq LIMIT 5",
            &[&id],
        );
        let g = first
            .iter()
            .map(|x| gist(x["text"].as_str().unwrap_or(""), ""))
            .find(|g| g.chars().count() >= 40)
            .unwrap_or_default();
        let outline = rows(c, include_str!("sql/server_focus_5.sql"), &[&id]);
        let out_n = links.get(&id).map(|x| x.len()).unwrap_or(0);
        let in_n = links
            .iter()
            .filter(|(k, v)| **k != id && v.contains(&id))
            .count();
        notes.push(json!({"id": id, "kind": s["kind"], "title": s["title"], "gist": g, "outline": outline, "cited": s["cited"], "last_ts": s["last_ts"], "passages": s["nchunks"], "links": out_n + in_n}));
    }
    let mut edges = vec![];
    for a in &shown {
        if let Some(set) = links.get(a) {
            for b in set {
                if shown.contains(b) {
                    edges.push(json!({"a": a, "b": b}));
                }
            }
        }
    }
    let resolved: usize = links.values().map(|v| v.len()).sum();
    let counts = rows(
        c,
        "SELECT s.kind, COUNT(*) AS n FROM sources s WHERE s.thread=?1 GROUP BY s.kind",
        &[&name],
    );
    let sessions = rows(c, include_str!("sql/server_focus_6.sql"), &[&name]);
    let g = graph(c);
    let me = format!("T:{name}");
    let mut linked = vec![];
    for e in g["edges"].as_array().unwrap() {
        let (a, b, k) = (
            e["a"].as_str().unwrap_or(""),
            e["b"].as_str().unwrap_or(""),
            e["kind"].as_str().unwrap_or(""),
        );
        if (k == "mentions" || k == "built on") && (a == me || b == me) {
            let o = if a == me { b } else { a };
            linked.push(json!({"name": o.trim_start_matches("T:"), "kind": k, "w": e["w"], "dir": if a == me { "out" } else { "in" }}));
        }
    }
    json!({"thread": t.get(0), "items": items, "notes": notes, "note_edges": edges, "wikilinks_resolved": resolved, "counts": counts, "sessions": sessions, "linked": linked, "hidden_notes": src.len().saturating_sub(14)})
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn rows(c: &Connection, sql: &str, p: &[&dyn rusqlite::ToSql]) -> Vec<Value> {
    let mut st = c.prepare(sql).unwrap();
    let n = st.column_count();
    let names: Vec<String> = (0..n)
        .map(|i| st.column_name(i).unwrap().to_string())
        .collect();
    st.query_map(p, |r| {
        let mut o = serde_json::Map::new();
        for i in 0..n {
            let v: rusqlite::types::Value = r.get(i)?;
            o.insert(
                names[i].clone(),
                match v {
                    rusqlite::types::Value::Null => Value::Null,
                    rusqlite::types::Value::Integer(i) => json!(i),
                    rusqlite::types::Value::Real(f) => json!(f),
                    rusqlite::types::Value::Text(t) => json!(t),
                    rusqlite::types::Value::Blob(_) => Value::Null,
                },
            );
        }
        Ok(Value::Object(o))
    })
    .unwrap()
    .flatten()
    .collect()
}

pub fn days_ago(ts: &str) -> i64 {
    let now = chrono_now();
    let d = |s: &str| -> i64 {
        let out = std::process::Command::new("date")
            .args([
                "-u",
                "-j",
                "-f",
                "%Y-%m-%d",
                s.get(..10).unwrap_or("1970-01-01"),
                "+%s",
            ])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or(0)
    };
    (d(&now) - d(ts)) / 86400
}

fn today(c: &Connection) -> Value {
    let waiting = rows(c, include_str!("sql/server_today.sql"), &[]);
    let asks = rows(c, include_str!("sql/server_today_2.sql"), &[]);
    let suggested = rows(c, include_str!("sql/server_today_3.sql"), &[]);
    let looks = rows(c, include_str!("sql/server_today_4.sql"), &[]);
    let threads = rows(c, include_str!("sql/server_today_5.sql"), &[]);
    let recent = rows(c, include_str!("sql/server_today_6.sql"), &[]);
    // moved: per thread, newest session in last 3 days
    let mut moved: Vec<Value> = vec![];
    let mut seen = std::collections::HashSet::new();
    for r in &recent {
        let th = r["thread"].as_str().unwrap_or("");
        if r["title"].as_str().unwrap_or("").starts_with("Untitled") {
            continue;
        }
        if days_ago(r["last_ts"].as_str().unwrap_or("")) > 3 {
            break;
        }
        if seen.insert(th.to_string()) {
            moved.push(r.clone());
        }
        if moved.len() >= 6 {
            break;
        }
    }
    // stale: open steps/questions older than 7 days in non-dropped threads
    let mut stale: Vec<Value> = rows(c, include_str!("sql/server_today_7.sql"), &[]);
    stale.extend(
        rows(c, include_str!("sql/server_today_8.sql"), &[])
            .into_iter()
            .filter(|r| days_ago(r["since"].as_str().unwrap_or("")) >= 7)
            .take(8),
    );
    let totals = rows(c, include_str!("sql/server_today_9.sql"), &[]);
    json!({"waiting": waiting, "asks": asks, "suggested": suggested, "looks": looks, "threads": threads, "moved": moved, "stale": stale, "totals": totals[0], "date": chrono_now()})
}

fn thread(c: &Connection, name: &str) -> Value {
    let t = rows(c, include_str!("sql/server_thread.sql"), &[&name]);
    let items = rows(c, include_str!("sql/server_thread_2.sql"), &[&name]);
    let sources = rows(c, include_str!("sql/server_thread_3.sql"), &[&name]);
    let claims = rows(c, include_str!("sql/server_thread_claims.sql"), &[&name]);
    json!({"thread": t.get(0), "items": items, "sources": sources, "claims": claims})
}

fn source(c: &Connection, id: i64) -> Value {
    let s = rows(c, include_str!("sql/server_source.sql"), &[&id]);
    let ch = rows(
        c,
        "SELECT id, locator, role, ts, text FROM chunks WHERE source_id=?1 ORDER BY seq LIMIT 600",
        &[&id],
    );
    let items = rows(
        c,
        "SELECT id, type, title, thread, chunk_id FROM claims WHERE source_id=?1 AND type IN ('decision','step','question','turn')",
        &[&id],
    );
    json!({"source": s.get(0), "chunks": ch, "items": items})
}

/// Names that mean one thing: a thread's name and its aliases from config.json (an old name goes there too).
fn alias_groups() -> Vec<Vec<String>> {
    crate::ingest::cfg()
        .aliases
        .iter()
        .map(|(n, a)| {
            let mut g = vec![n.to_lowercase()];
            g.extend(a.iter().map(|x| x.to_lowercase()));
            g
        })
        .collect()
}

/// FTS expression for a query. Each word also matches the other names in its alias group.
fn fts_query(q: &str) -> String {
    let groups = alias_groups();
    q.split(|ch: char| !ch.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| {
            let tl = t.to_lowercase();
            let mut alts = vec![format!("\"{t}\"*")];
            for g in groups.iter().filter(|g| g.contains(&tl)) {
                for x in g.iter().filter(|x| **x != tl) {
                    alts.push(format!("\"{}\"", x.replace('"', "")));
                }
            }
            if alts.len() == 1 {
                alts.remove(0)
            } else {
                format!("({})", alts.join(" OR "))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Default)]
pub struct Filters {
    pub thread: String,
    pub kind: String,
    pub state: String,
    pub from: String,
    pub to: String,
}

/// Item states that mean the owner kept the item, and states that mean it no longer holds.
const KEPT: &[&str] = &["settled", "done", "open", "answered", "looks_done", "doing"];
fn dead(state: &str, stale: &str) -> bool {
    state == "rejected" || stale.starts_with("superseded")
}

/// Item index over title and body, rebuilt in a per-connection temp table when the claims change.
fn item_index(c: &Connection) {
    static SIG: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
    let sig: String = c.query_row("SELECT COUNT(*)||'-'||IFNULL(MAX(updated),'')||'-'||IFNULL(MAX(created),'') FROM claims", [], |r| r.get(0)).unwrap_or_default();
    let have = c
        .query_row(
            "SELECT COUNT(*) FROM sqlite_temp_master WHERE name='nfts'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;
    let mut g = SIG.lock().unwrap();
    if have && *g == sig {
        return;
    }
    c.execute_batch(include_str!("sql/server_item_index.sql"))
        .ok();
    *g = sig;
}

const PASSAGE_SELECT: &str = include_str!("sql/server_item_index_2.sql");
const PASSAGE_FILTER: &str = "(?2='' OR s.thread=?2) AND (?3='' OR s.kind=?3) AND (?4='' OR substr(s.last_ts,1,10)>=?4) AND (?5='' OR substr(s.last_ts,1,10)<=?5)
    AND (?6='' OR EXISTS(SELECT 1 FROM claims n WHERE n.source_id=s.id AND n.type IN ('decision','step','question','turn') AND n.state=?6))";

pub fn search(c: &Connection, q: &str, f: &Filters, semantic: bool) -> Value {
    let fq = fts_query(q);
    let (cov_have, cov_all) = crate::embed::coverage(c);
    let mut res = json!({"results": [], "items": [], "mode": "text", "note": "", "embedded": cov_have, "passages": cov_all});
    if fq.is_empty() {
        return res;
    }
    let p: [&dyn rusqlite::ToSql; 6] = [&fq, &f.thread, &f.kind, &f.from, &f.to, &f.state];

    // 1. candidates: keyword (BM25) and, when available, vector
    let kw = format!("{PASSAGE_SELECT}, snippet(fts, 0, char(1), char(2), '…', 28) AS snip, bm25(fts) AS score
        FROM fts JOIN chunks ch ON ch.id=fts.rowid JOIN sources s ON s.id=ch.source_id WHERE fts MATCH ?1 AND {PASSAGE_FILTER} ORDER BY score LIMIT 200");
    let mut bm = rows(c, &kw, &p);
    if bm.is_empty() && fq.contains(' ') {
        // no passage holds every word (a question typed as a sentence): rank passages that hold some of them
        let any = q
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|t| t.chars().count() >= 3)
            .map(|t| format!("\"{t}\"*"))
            .collect::<Vec<_>>()
            .join(" OR ");
        if !any.is_empty() {
            let p2: [&dyn rusqlite::ToSql; 6] =
                [&any, &f.thread, &f.kind, &f.from, &f.to, &f.state];
            bm = rows(c, &kw, &p2);
        }
    }
    let mut cand: std::collections::HashMap<i64, Value> = Default::default();
    let mut rrf: std::collections::HashMap<i64, f64> = Default::default();
    for (i, r) in bm.into_iter().enumerate() {
        let id = r["chunk_id"].as_i64().unwrap();
        *rrf.entry(id).or_insert(0.0) += 1.0 / (60.0 + i as f64 + 1.0);
        cand.insert(id, r);
    }
    if !semantic {
        res["note"] = json!("keyword search only (requested)");
    } else if !crate::embed::enabled() {
        res["note"] = json!("keyword search only: embeddings are turned off in config.json");
    } else {
        match crate::embed::nearest(c, q, 200) {
            Err(e) => {
                res["note"] = json!(format!(
                    "keyword search only: the embedding server did not answer ({e}). Start Ollama and run `ollama pull embeddinggemma-2:270m` once."
                ))
            }
            Ok(near) => {
                res["mode"] = json!("hybrid");
                if cov_have < cov_all {
                    res["note"] =
                        json!(format!("{cov_have} of {cov_all} passages embedded so far"));
                }
                let ids = near
                    .iter()
                    .map(|x| x.0.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                let have = rows(
                    c,
                    &format!(
                        "{PASSAGE_SELECT}, '' AS snip FROM chunks ch JOIN sources s ON s.id=ch.source_id WHERE ch.id IN ({ids}) AND ?1!='' AND {PASSAGE_FILTER}"
                    ),
                    &p,
                );
                let by: std::collections::HashMap<i64, Value> = have
                    .into_iter()
                    .map(|r| (r["chunk_id"].as_i64().unwrap(), r))
                    .collect();
                let mut rank = 0;
                for (id, _) in near {
                    if let Some(r) = by.get(&id) {
                        rank += 1;
                        *rrf.entry(id).or_insert(0.0) += 1.0 / (60.0 + rank as f64);
                        cand.entry(id).or_insert_with(|| r.clone());
                    }
                }
            }
        }
    }

    // 2. graph context: thread state and the filed items that cite each passage or its source
    let tstate: std::collections::HashMap<String, String> = rows(
        c,
        "SELECT name AS thread, state FROM threads",
        &[],
    )
    .into_iter()
    .map(|r| {
        (
            r["thread"].as_str().unwrap_or("").to_string(),
            r["state"].as_str().unwrap_or("").to_string(),
        )
    })
    .collect();
    let sids = cand
        .values()
        .filter_map(|r| r["source_id"].as_i64())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let cites = if sids.is_empty() {
        vec![]
    } else {
        rows(
            c,
            &format!(
                "SELECT id, type, title, state, IFNULL(stale,'') AS stale, source_id, chunk_id FROM claims WHERE type IN ('decision','step','question','turn') AND source_id IN ({sids})"
            ),
            &[],
        )
    };
    let mut out: Vec<(f64, Value)> = vec![];
    for (id, mut r) in cand {
        let sid = r["source_id"].as_i64().unwrap_or(0);
        let mine: Vec<&Value> = cites
            .iter()
            .filter(|n| n["source_id"].as_i64() == Some(sid))
            .collect();
        let direct: Vec<&&Value> = mine
            .iter()
            .filter(|n| n["chunk_id"].as_i64() == Some(id))
            .collect();
        let live = |n: &Value| {
            !dead(
                n["state"].as_str().unwrap_or(""),
                n["stale"].as_str().unwrap_or(""),
            )
        };
        let kept = |n: &Value| live(n) && KEPT.contains(&n["state"].as_str().unwrap_or(""));
        let mut m = 1.0;
        let mut why: Vec<String> = vec![];
        let ts = r["thread"]
            .as_str()
            .and_then(|t| tstate.get(t))
            .cloned()
            .unwrap_or_default();
        if ts == "dropped" {
            m *= 0.6;
            why.push("thread dropped".into());
        } else if ts == "parked" {
            m *= 0.85;
            why.push("thread parked".into());
        }
        if direct.iter().any(|n| kept(n)) {
            m *= 1.3;
            why.push("cited by a kept item".into());
        } else if !direct.is_empty() && direct.iter().all(|n| !live(n)) {
            m *= 0.5;
            why.push("cited only by rejected or replaced items".into());
        } else if !direct.is_empty() {
            m *= 1.1;
            why.push("cited by a suggestion".into());
        } else if mine.iter().any(|n| kept(n)) {
            m *= 1.1;
            why.push("same source has a kept item".into());
        }
        if matches!(r["kind"].as_str(), Some("note" | "artifact")) {
            m *= 1.15;
        }
        let ctx: Vec<Value> = direct.iter().map(|n| (**n).clone()).chain(mine.iter().filter(|n| n["chunk_id"].as_i64() != Some(id)).map(|n| (**n).clone())).take(3)
            .map(|n| json!({"id": n["id"], "type": n["type"], "title": n["title"], "state": n["state"], "dead": !live(&n)})).collect();
        let snip = r["snip"].as_str().unwrap_or("");
        let sn = if snip.is_empty() {
            esc(&r["body"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(240)
                .collect::<String>())
        } else {
            esc(snip)
                .replace('\u{1}', "<mark>")
                .replace('\u{2}', "</mark>")
        };
        r["snip"] = json!(sn);
        r["thread_state"] = json!(ts);
        r["why"] = json!(why);
        r["items"] = json!(ctx);
        r.as_object_mut().unwrap().remove("body");
        out.push((rrf.get(&id).copied().unwrap_or(0.0) * m, r));
    }
    out.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    res["results"] = json!(out.into_iter().take(40).map(|x| x.1).collect::<Vec<_>>());

    // 3. filed items, searched in their own index
    item_index(c);
    res["items"] = json!(rows(c, include_str!("sql/server_search.sql"), &p));
    res
}

/// Daily copy of the store (answers, ticks and accepts exist nowhere else). Keeps the newest 3, in `<data dir>/backups`.
pub fn backup(c: &Connection) -> Option<String> {
    let dir = home().join("backups");
    // Older versions wrote to a `BacktrailBackups` folder next to the data folder; move it once.
    if std::env::var("BACKTRAIL_HOME").is_err() && !dir.exists() {
        if let Some(old) = home().parent().map(|p| p.join("BacktrailBackups")) {
            std::fs::rename(old, &dir).ok();
        }
    }
    std::fs::create_dir_all(&dir).ok()?;
    let name = format!("backtrail-{}.db", &chrono_now()[..10]);
    let path = dir.join(&name);
    if path.exists() {
        return None;
    }
    c.execute("VACUUM INTO ?1", params![path.to_string_lossy()])
        .ok()?;
    private(&path, 0o600);
    let mut all: Vec<_> = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |x| x == "db"))
        .collect();
    all.sort();
    while all.len() > 3 {
        std::fs::remove_file(all.remove(0)).ok();
    }
    logw(c, "backup", "backup", &name, "");
    Some(path.display().to_string())
}

/// Incremental scan used by `brief` and the server's timer: only new or changed sources are read.
pub fn rescan(c: &Connection) {
    crate::ingest::seed_threads(c);
    crate::ingest::ingest_sessions(c);
    crate::ingest::ingest_notes(c);
    crate::ingest::ingest_artifacts(c);
    crate::ingest::ingest_git(c);
    crate::ingest::rethread(c);
    crate::ingest::relink(c);
    match_inner(c, false);
    if !home().join(".demo").exists() {
        backup(c);
    }
}

/// Candidate contradictions for a Claude session to judge: items of one thread that share several words but differ in state or type.
pub fn pairs(c: &Connection) {
    let items = rows(c, include_str!("sql/server_pairs.sql"), &[]);
    let words = |t: &str| -> std::collections::HashSet<String> {
        t.to_lowercase()
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|w| w.len() >= 5 && !STOP.contains(w))
            .map(|w| w.to_string())
            .collect()
    };
    let mut n = 0;
    for (i, a) in items.iter().enumerate() {
        for b in items.iter().skip(i + 1) {
            if a["thread"] != b["thread"] {
                continue;
            }
            let (wa, wb) = (
                words(a["title"].as_str().unwrap_or("")),
                words(b["title"].as_str().unwrap_or("")),
            );
            let shared: Vec<_> = wa.intersection(&wb).collect();
            if shared.len() >= 2 && (a["state"] != b["state"] || a["type"] != b["type"]) {
                n += 1;
                println!(
                    "[{}] {} ({} {})  <>  {} ({} {})  shared: {:?}",
                    a["thread"].as_str().unwrap_or(""),
                    a["title"].as_str().unwrap_or(""),
                    a["type"].as_str().unwrap_or(""),
                    a["state"].as_str().unwrap_or(""),
                    b["title"].as_str().unwrap_or(""),
                    b["type"].as_str().unwrap_or(""),
                    b["state"].as_str().unwrap_or(""),
                    shared
                );
            }
        }
    }
    println!(
        "{n} candidate pairs. Judge each: does one item contradict or supersede the other? Record the answer as a turn or decision."
    );
}

/// Health report: items whose quote left their source, and open work that has gone quiet.
pub fn check(c: &Connection) {
    let (ok, lost) = crate::ingest::relink(c);
    println!("claims traced to their quote: {ok}; quote no longer in source: {lost}");
    let pending = crate::ingest::unindexed_sessions(c);
    if !pending.is_empty() {
        println!(
            "session files not indexed yet: {} (oldest {} days; Claude Code removes transcripts after 30 days by default). Run `bt ingest`.",
            pending.len(),
            pending[0].1
        );
    }
    for r in rows(c, include_str!("sql/server_check_4.sql"), &[]) {
        println!(
            "  lost: [{}] {} ({})",
            r["thread"].as_str().unwrap_or(""),
            r["title"].as_str().unwrap_or(""),
            r["id"].as_str().unwrap_or("")
        );
    }
    let q = rows(c, include_str!("sql/server_check.sql"), &[]);
    for r in q {
        println!(
            "  open item in a dropped thread: [{}] {}",
            r["thread"].as_str().unwrap_or(""),
            r["title"].as_str().unwrap_or("")
        );
    }
    let un = rows(
        c,
        "SELECT COUNT(*) AS n FROM sources WHERE thread='Unsorted'",
        &[],
    );
    println!("sources in Unsorted: {}", un[0]["n"]);
    let old = rows(c, include_str!("sql/server_check_5.sql"), &[])
        .into_iter()
        .filter(|r| days_ago(r["created"].as_str().unwrap_or("")) >= 14)
        .count();
    println!("questions open for 14+ days: {old}");
    let empty: Vec<Value> = rows(c, include_str!("sql/server_check_2.sql"), &[]);
    println!("threads with no filed items (orphans): {}", empty.len());
    for e in empty.iter().take(12) {
        println!("  {}", e["thread"].as_str().unwrap_or(""));
    }
    let uncited = rows(c, include_str!("sql/server_check_6.sql"), &[]);
    println!("notes and artifacts cited by no item: {}", uncited[0]["n"]);
    for r in rows(c, include_str!("sql/server_check_3.sql"), &[]) {
        println!(
            "  more than one plan source in [{}] ({}): {}",
            r["thread"].as_str().unwrap_or(""),
            r["n"],
            r["t"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(150)
                .collect::<String>()
        );
    }
    let ld = rows(
        c,
        "SELECT COUNT(*) AS n FROM claims WHERE state='looks_done'",
        &[],
    );
    println!(
        "steps waiting on the owner as 'looks done?': {}",
        ld[0]["n"]
    );
    crate::kg::lint(c);
}

/// Step 5 input: recent real sessions in live threads that no item cites yet.
pub fn pending(c: &Connection, days: i64) {
    let list = rows(c, include_str!("sql/server_pending.sql"), &[]);
    let mut n = 0;
    for r in list {
        if days_ago(r["last_ts"].as_str().unwrap_or("")) > days {
            continue;
        }
        n += 1;
        println!(
            "{} | {} | {} | {} passages\n    {}",
            r["ref"].as_str().unwrap_or(""),
            r["thread"].as_str().unwrap_or(""),
            r["title"].as_str().unwrap_or(""),
            r["nchunks"],
            r["first"].as_str().unwrap_or("")
        );
    }
    println!(
        "{n} uncompiled sessions in the last {days} days. Read one with `backtrail show <ref>`, file items with `backtrail add ... --origin suggested --state proposed`."
    );
}

pub fn show(c: &Connection, r: &str) {
    let r = resolve_ref(c, r);
    for ch in rows(c, include_str!("sql/server_show.sql"), &[&r]) {
        println!(
            "[{} · {}] {}\n",
            ch["locator"].as_str().unwrap_or(""),
            ch["role"].as_str().unwrap_or(""),
            ch["t"].as_str().unwrap_or("")
        );
    }
}

/// Step 6: a store of invented content for screenshots and trying the app. Refuses to touch the default store.
pub fn demo(c: &Connection) {
    if std::env::var("BACKTRAIL_HOME").is_err() {
        eprintln!(
            "set BACKTRAIL_HOME to an empty folder first; demo never writes to the default store"
        );
        std::process::exit(1);
    }
    let h = home();
    std::fs::write(h.join(".demo"), "demo store\n").ok();
    std::fs::write(h.join("threads.json"), r#"[
 {"area":"Web","name":"Recipe site","state":"moving","summary":"Static site for sharing family recipes.","regex":"recipe|site"},
 {"area":"Web","name":"Photo archive","state":"parked","summary":"Scan, tag and search old prints.","regex":"photo|scan"},
 {"area":"Tools","name":"Budget tracker","state":"built","summary":"CSV import and monthly summary.","regex":"budget|csv"},
 {"area":"Ideas, no repo","name":"Garden planner","state":"open","summary":"Planting calendar by zone.","regex":"garden"},
 {"area":"Tools","name":"Old backup script","state":"dropped","summary":"Replaced by the system backup.","regex":"backup"}]"#).ok();
    crate::ingest::seed_threads(c);
    let docs: &[(&str, &str, &str, &str)] = &[
        (
            "note:demo/recipe-site plan.md",
            "recipe-site plan",
            "Recipe site",
            "# Plan\nThe site is static: one Markdown file per recipe, built into HTML. Search runs in the browser, so there is no server to host.\n# Open\nWhich host to use is not decided. A custom domain costs about 12 euros a year.",
        ),
        (
            "note:demo/photo archive notes.md",
            "photo archive notes",
            "Photo archive",
            "# Scanning\nA flatbed scan at 600 dpi takes about 40 seconds per print. Tags come from the album name written on the back.\n# Parked\nThe batch of 300 prints is on hold until the new scanner arrives.\n# Export\nTag lists are exported as CSV, reusing the budget importer.",
        ),
        (
            "note:demo/budget csv import.md",
            "budget csv import",
            "Budget tracker",
            "# Import\nThe bank export is a semicolon CSV with German dates. The importer maps columns by header name and rejects rows with an unknown date format.\n# Done\nMonthly summary shows totals per category.",
        ),
        (
            "note:demo/garden ideas.md",
            "garden ideas",
            "Garden planner",
            "# Idea\nA calendar that shows what to sow in which week for a given frost date. Data source could be a public seed table.\n# Reuse\nThe planner can be built on the recipe site's Markdown pipeline.",
        ),
        (
            "note:demo/old backup script.md",
            "old backup script",
            "Old backup script",
            "# Status\nThe script is replaced by the system backup, which also covers the photo folder.",
        ),
    ];
    for (n, (r, t, th, body)) in docs.iter().enumerate() {
        c.execute(include_str!("sql/server_demo.sql"), params![r, t, th])
            .unwrap();
        c.execute(
            "UPDATE sources SET last_ts=?2 WHERE ref=?1",
            params![r, format!("2026-11-{:02}T12:00:00Z", 3 + n * 2)],
        )
        .unwrap();
        let sid: i64 = c
            .query_row("SELECT id FROM sources WHERE ref=?1", params![r], |x| {
                x.get(0)
            })
            .unwrap();
        for (i, part) in body.split("\n# ").enumerate() {
            let text = part.trim_start_matches("# ").to_string();
            c.execute(
                "INSERT INTO chunks(source_id,seq,locator,role,text,ts) VALUES(?1,?2,?3,'note',?4,?5)",
                params![
                    sid,
                    i as i64,
                    format!("#part {}", i + 1),
                    text,
                    format!("2026-11-{:02}T12:00:00Z", 3 + n * 2)
                ],
            )
            .unwrap();
            let cid = c.last_insert_rowid();
            c.execute(
                "INSERT INTO fts(rowid,text) VALUES(?1,?2)",
                params![cid, text],
            )
            .unwrap();
        }
        c.execute("UPDATE sources SET nchunks=(SELECT COUNT(*) FROM chunks WHERE source_id=?1) WHERE id=?1", params![sid]).unwrap();
    }
    let items = [
        (
            "decision",
            "Recipe site",
            "Static site, no server",
            "One Markdown file per recipe, built into HTML.",
            "note:demo/recipe-site plan.md",
            "The site is static",
            "settled",
            "compiled",
        ),
        (
            "question",
            "Recipe site",
            "Which host to use?",
            "Not decided; a domain is about 12 euros a year.",
            "note:demo/recipe-site plan.md",
            "Which host to use is not decided",
            "open",
            "compiled",
        ),
        (
            "step",
            "Recipe site",
            "Build search in the browser",
            "",
            "note:demo/recipe-site plan.md",
            "Search runs in the browser",
            "open",
            "compiled",
        ),
        (
            "decision",
            "Photo archive",
            "Tags come from the album name on the back",
            "",
            "note:demo/photo archive notes.md",
            "Tags come from the album name",
            "settled",
            "compiled",
        ),
        (
            "question",
            "Photo archive",
            "Scan the 300 prints before the new scanner?",
            "On hold until it arrives.",
            "note:demo/photo archive notes.md",
            "on hold until the new scanner arrives",
            "open",
            "compiled",
        ),
        (
            "decision",
            "Budget tracker",
            "Importer rejects unknown date formats",
            "",
            "note:demo/budget csv import.md",
            "rejects rows with an unknown date format",
            "settled",
            "compiled",
        ),
        (
            "step",
            "Budget tracker",
            "Monthly summary per category",
            "",
            "note:demo/budget csv import.md",
            "Monthly summary shows totals per category",
            "done",
            "compiled",
        ),
        (
            "decision",
            "Recipe site",
            "Search runs in the browser",
            "No server; the Search index ships with the HTML.",
            "note:demo/recipe-site plan.md",
            "Search runs in the browser",
            "settled",
            "compiled",
        ),
        (
            "decision",
            "Recipe site",
            "Keep hosting under 12 euros a year",
            "",
            "note:demo/recipe-site plan.md",
            "A custom domain costs about 12 euros a year",
            "settled",
            "compiled",
        ),
        (
            "decision",
            "Photo archive",
            "Scan at 600 dpi",
            "Scanner setting for every print.",
            "note:demo/photo archive notes.md",
            "A flatbed scan at 600 dpi",
            "settled",
            "compiled",
        ),
        (
            "decision",
            "Photo archive",
            "Wait for the new scanner",
            "The batch of 300 prints stays on hold.",
            "note:demo/photo archive notes.md",
            "on hold until the new scanner arrives",
            "proposed",
            "suggested",
        ),
        (
            "decision",
            "Budget tracker",
            "Map CSV columns by header name",
            "",
            "note:demo/budget csv import.md",
            "maps columns by header name",
            "settled",
            "compiled",
        ),
        (
            "decision",
            "Budget tracker",
            "Bank CSV uses German dates",
            "",
            "note:demo/budget csv import.md",
            "semicolon CSV with German dates",
            "settled",
            "compiled",
        ),
        (
            "decision",
            "Garden planner",
            "Sow calendar keyed to the frost date",
            "",
            "note:demo/garden ideas.md",
            "for a given frost date",
            "proposed",
            "suggested",
        ),
        (
            "decision",
            "Photo archive",
            "Export tag lists as CSV",
            "Reuses the import code from the budget tracker.",
            "note:demo/photo archive notes.md",
            "exported as CSV",
            "settled",
            "compiled",
        ),
        (
            "decision",
            "Garden planner",
            "Build the planner on the Markdown pipeline",
            "",
            "note:demo/garden ideas.md",
            "built on the recipe site",
            "proposed",
            "suggested",
        ),
        (
            "decision",
            "Old backup script",
            "Retire the script",
            "The system backup also covers the photo folder.",
            "note:demo/old backup script.md",
            "replaced by the system backup",
            "settled",
            "compiled",
        ),
        (
            "step",
            "Garden planner",
            "Find a public seed table",
            "Suggested while reading the idea note.",
            "note:demo/garden ideas.md",
            "public seed table",
            "proposed",
            "suggested",
        ),
    ];
    for (ty, th, ti, bo, so, qu, st, or) in items {
        let it = crate::db::Item {
            typ: ty.into(),
            thread: th.into(),
            title: ti.into(),
            body: bo.into(),
            source: so.into(),
            quote: qu.into(),
            state: Some(st.into()),
            origin: or.into(),
            date: Some("2026-11-09T12:00:00Z".into()),
        };
        if let Err(e) = crate::db::add_item(c, &it) {
            eprintln!("demo item rejected: {e}");
        }
    }
    c.execute(include_str!("sql/server_demo_2.sql"), []).ok();
    for (name, kind) in [
        ("Recipe site", "project"),
        ("Photo archive", "project"),
        ("Budget tracker", "project"),
        ("Garden planner", "project"),
        ("Old backup script", "project"),
        ("Markdown", "topic"),
        ("HTML", "topic"),
        ("CSV", "topic"),
        ("Search", "topic"),
        ("Scanner", "tool"),
    ] {
        crate::kg::entity_add(c, name, kind, &[name.to_lowercase()]);
    }
    for (a, b, quote, src) in [
        (
            "Photo archive",
            "Budget tracker",
            "reusing the budget importer",
            "note:demo/photo archive notes.md",
        ),
        (
            "Garden planner",
            "Recipe site",
            "built on the recipe site's Markdown pipeline",
            "note:demo/garden ideas.md",
        ),
    ] {
        if let Err(e) = crate::compile::relate(c, a, b, "built on", src, quote, "proposed") {
            eprintln!("demo relation rejected: {e}");
        }
    }
    println!(
        "demo store written to {}; run BACKTRAIL_HOME={} backtrail serve --port 8800",
        h.display(),
        h.display()
    );
}

pub const STOP: &[&str] = &[
    "about", "after", "before", "their", "there", "which", "would", "first", "later", "again",
    "build", "built", "write", "files", "check", "start",
];

/// Signal 3: open steps whose words appear in a later commit of the thread's repo turn into "looks done?". Unproven heuristic.
pub fn match_evidence(c: &Connection) {
    match_inner(c, true)
}

fn match_inner(c: &Connection, say: bool) {
    let steps = rows(
        c,
        "SELECT n.id, n.title, n.thread, n.created FROM claims n WHERE n.type='step' AND n.state='open'",
        &[],
    );
    let (mut hit, mut seen) = (0, 0);
    for st in steps {
        let th = st["thread"].as_str().unwrap_or("");
        let id = st["id"].as_str().unwrap_or("");
        let dismissed: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM log WHERE action='not-yet' AND target=?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if dismissed > 0 {
            continue;
        }
        let toks: Vec<String> = st["title"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(|w| w.to_string())
            .collect();
        let phrases: Vec<String> = toks
            .windows(2)
            .filter(|w| {
                w[0].len() >= 4
                    && w[1].len() >= 4
                    && !STOP.contains(&w[0].as_str())
                    && !STOP.contains(&w[1].as_str())
            })
            .map(|w| format!("{} {}", w[0], w[1]))
            .collect();
        if phrases.is_empty() {
            continue;
        }
        seen += 1;
        let created = st["created"]
            .as_str()
            .unwrap_or("")
            .get(..10)
            .unwrap_or("")
            .to_string();
        let commits = rows(c, include_str!("sql/server_match_inner.sql"), &[&th]);
        for cm in commits {
            if cm["ts"].as_str().unwrap_or("").get(..10).unwrap_or("") < created.as_str() {
                continue;
            }
            let t = cm["text"].as_str().unwrap_or("").to_lowercase();
            if phrases.iter().any(|p| t.contains(p.as_str())) {
                c.execute("UPDATE claims SET state='looks_done', done_by='evidence?', answer=?2, updated=?3 WHERE id=?1", params![id, cm["text"].as_str().unwrap_or(""), chrono_now()]).unwrap();
                logw(
                    c,
                    "check",
                    "looks-done",
                    id,
                    cm["text"].as_str().unwrap_or(""),
                );
                if say {
                    println!(
                        "looks done? [{}] {}  <-  {}",
                        th,
                        st["title"].as_str().unwrap_or(""),
                        cm["text"].as_str().unwrap_or("")
                    );
                }
                hit += 1;
                break;
            }
        }
    }
    if say {
        println!(
            "{seen} open steps compared with commits; {hit} marked 'looks done?' (an ordered two-word phrase from the step title must appear in a later commit; the owner confirms)"
        );
    }
}

/// Reads a listing `[{id,title,updated}]` (from the Artifact tool's list action), records publish dates, and reports
/// which artifacts have no snapshot or a snapshot older than the published version.
fn artifact_manifest(c: &Connection, file: &str) {
    let raw = std::fs::read_to_string(file).unwrap_or_default();
    let v: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    let (mut missing, mut stale, mut fresh) = (vec![], vec![], 0);
    for a in v.as_array().cloned().unwrap_or_default() {
        let (id, title, upd) = (
            a["id"].as_str().unwrap_or(""),
            a["title"].as_str().unwrap_or(""),
            a["updated"].as_str().unwrap_or(""),
        );
        if id.is_empty() {
            continue;
        }
        c.execute(
            include_str!("sql/server_artifact_manifest.sql"),
            params![id, title, upd],
        )
        .unwrap();
        let snap = rows(
            c,
            "SELECT mtime FROM sources WHERE ref=?1",
            &[&format!("artifact:{id}")],
        );
        match snap.first() {
            None => missing.push(format!("{id}  {title}")),
            Some(r) => {
                let snap_day = rows(
                    c,
                    "SELECT date(?1,'unixepoch') AS d",
                    &[&r["mtime"].as_i64().unwrap_or(0)],
                )[0]["d"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                if upd > snap_day.as_str() {
                    stale.push(format!(
                        "{id}  {title} (published {upd}, snapshot {snap_day})"
                    ));
                } else {
                    fresh += 1;
                }
                c.execute(
                    "UPDATE sources SET first_ts=?2, last_ts=?2 WHERE ref=?1",
                    params![format!("artifact:{id}"), format!("{upd}T12:00:00Z")],
                )
                .ok();
            }
        }
    }
    println!(
        "manifest: {fresh} up to date, {} stale, {} without a snapshot",
        stale.len(),
        missing.len()
    );
    for m in &stale {
        println!("  stale   {m}");
    }
    for m in &missing {
        println!("  missing {m}");
    }
    println!(
        "Re-read these with the Artifact tool (action read, path index.html, out_dir <data dir>/artifacts/<id>), then run `backtrail ingest`."
    );
}

/// Claude artifact links found in sessions and notes that have no snapshot yet.
pub fn artifacts(c: &Connection, manifest: Option<String>) {
    if let Some(f) = manifest {
        artifact_manifest(c, &f);
    }
    let re = regex::Regex::new(r"claude\.ai/artifact/([A-Za-z0-9]{20,24})").unwrap();
    let mut seen: std::collections::BTreeMap<String, i64> = Default::default();
    let mut st = c
        .prepare("SELECT text FROM chunks WHERE text LIKE '%claude.ai/artifact/%'")
        .unwrap();
    for t in st
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .flatten()
    {
        for m in re.captures_iter(&t) {
            *seen.entry(m[1].to_string()).or_insert(0) += 1;
        }
    }
    let have: std::collections::HashSet<String> =
        rows(c, "SELECT ref FROM sources WHERE kind='artifact'", &[])
            .iter()
            .map(|r| {
                r["ref"]
                    .as_str()
                    .unwrap_or("")
                    .trim_start_matches("artifact:")
                    .to_string()
            })
            .collect();
    let missing: Vec<_> = seen.iter().filter(|(k, _)| !have.contains(*k)).collect();
    println!(
        "{} artifact links mentioned, {} already snapshotted, {} missing:",
        seen.len(),
        seen.len() - missing.len(),
        missing.len()
    );
    for (k, n) in missing {
        println!("  https://claude.ai/artifact/{k}  ({n} mentions)");
    }
}

pub fn search_cli(c: &Connection, q: &str, f: &Filters, semantic: bool) {
    let r = search(c, q, f, semantic);
    println!(
        "mode: {}{}",
        r["mode"].as_str().unwrap_or(""),
        r["note"]
            .as_str()
            .filter(|n| !n.is_empty())
            .map(|n| format!(" ({n})"))
            .unwrap_or_default()
    );
    for i in r["items"].as_array().unwrap() {
        println!(
            "ITEM [{}] {} {}: {}",
            i["thread"].as_str().unwrap_or(""),
            i["type"].as_str().unwrap_or(""),
            i["state"].as_str().unwrap_or(""),
            i["title"].as_str().unwrap_or("")
        );
    }
    for x in r["results"].as_array().unwrap().iter().take(10) {
        let sn = x["snip"]
            .as_str()
            .unwrap_or("")
            .replace("<mark>", "[")
            .replace("</mark>", "]")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">");
        let why = x["why"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|w| w.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_default();
        println!(
            "{} | {} | {}{}\n    {}",
            x["ref"].as_str().unwrap_or(""),
            x["locator"].as_str().unwrap_or(""),
            x["thread"].as_str().unwrap_or("-"),
            if why.is_empty() {
                String::new()
            } else {
                format!(" | {why}")
            },
            sn.replace('\n', " ")
        );
    }
}

pub fn log_cli(c: &Connection, n: i64) {
    for r in rows(
        c,
        "SELECT ts, actor, action, target, detail FROM log ORDER BY id DESC LIMIT ?1",
        &[&n],
    )
    .iter()
    .rev()
    {
        println!(
            "## [{}] {} | {} {} | {}",
            r["ts"].as_str().unwrap_or("").get(..16).unwrap_or(""),
            r["actor"].as_str().unwrap_or(""),
            r["action"].as_str().unwrap_or(""),
            r["target"].as_str().unwrap_or(""),
            r["detail"].as_str().unwrap_or("")
        );
    }
}

/// The graph is rebuilt only when the store changed (any connection's write moves data_version or total_changes).
fn graph_cached(c: &Connection) -> Value {
    static G: std::sync::Mutex<Option<(String, Value)>> = std::sync::Mutex::new(None);
    let dv: i64 = c
        .query_row("PRAGMA data_version", [], |r| r.get(0))
        .unwrap_or(0);
    let sig = format!("{dv}-{}", c.total_changes());
    let mut g = G.lock().unwrap();
    if let Some((k, v)) = g.as_ref() {
        if *k == sig {
            return v.clone();
        }
    }
    let v = graph(c);
    *g = Some((sig, v.clone()));
    v
}

/// The context graph: threads, items and sources as nodes; belongs-to, comes-from, built-on and mention edges.
fn graph(c: &Connection) -> Value {
    let threads = rows(c, include_str!("sql/server_graph.sql"), &[]);
    let mut nodes: Vec<Value> = vec![];
    let mut edges: Vec<Value> = vec![];
    for t in &threads {
        nodes.push(json!({"id": format!("T:{}", t["name"].as_str().unwrap()), "kind": "thread", "label": t["name"], "state": t["state"], "n": t["n"], "q": t["q"], "area": t["area"], "first": t["first"], "last": t["last"]}));
    }
    // mention edges: sources of thread A that talk about thread B
    let mut w: std::collections::HashMap<(String, String), i64> = Default::default();
    for t in &threads {
        let name = t["name"].as_str().unwrap().to_string();
        if name == "Unsorted" {
            continue;
        }
        let mut al: Vec<String> = vec![name.to_lowercase()];
        if let Some(v) = crate::ingest::cfg().aliases.get(&name) {
            al.extend(v.iter().cloned());
        }
        let q = al
            .iter()
            .map(|a| format!("\"{}\"", a.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" OR ");
        for r in rows(c, include_str!("sql/server_graph_2.sql"), &[&q, &name]) {
            let other = r["th"].as_str().unwrap_or("").to_string();
            let mut k = (name.clone(), other);
            if k.0 > k.1 {
                k = (k.1, k.0);
            }
            *w.entry(k).or_insert(0) += r["n"].as_i64().unwrap_or(0);
        }
    }
    for ((a, b), n) in w {
        if n >= 12 {
            edges.push(
                json!({"a": format!("T:{a}"), "b": format!("T:{b}"), "kind": "mentions", "w": n}),
            );
        }
    }
    // thread lineage is drawn from relation claims; a replaced item points at the item that replaced it
    for l in rows(c, include_str!("sql/server_graph_3.sql"), &[]) {
        edges.push(json!({"a": l["a"], "b": l["b"], "kind": l["kind"], "w": 1, "quote": l["quote"], "ts": l["ts"], "state": l["state"], "chunk_id": l["chunk_id"], "source": l["source"], "source_title": l["source_title"], "source_id": l["source_id"]}));
    }
    for l in rows(c, include_str!("sql/server_graph_4.sql"), &[]) {
        edges.push(json!({"a": l["a"], "b": l["b"], "kind": "replaced by", "w": 1, "quote": null, "ts": null, "state": null, "chunk_id": null, "source": null, "source_title": null, "source_id": null}));
    }
    // items and the sources they come from
    let mut cited = std::collections::HashSet::new();
    for i in rows(c, include_str!("sql/server_graph_5.sql"), &[]) {
        let id = format!("I:{}", i["id"].as_str().unwrap());
        nodes.push(json!({"id": id, "kind": i["type"], "label": i["title"], "state": i["state"], "thread": i["thread"]}));
        edges.push(json!({"a": id, "b": format!("T:{}", i["thread"].as_str().unwrap_or("")), "kind": "belongs to", "w": 1}));
        if let Some(sid) = i["source_id"].as_i64() {
            cited.insert(sid);
            edges.push(json!({"a": id, "b": format!("S:{sid}"), "kind": "comes from", "w": 1}));
        }
    }
    for s in rows(
        c,
        "SELECT id, kind, title, thread FROM sources WHERE kind!='owner' AND ifnull(thread,'')!='Unsorted'",
        &[],
    ) {
        let sid = s["id"].as_i64().unwrap();
        nodes.push(json!({"id": format!("S:{sid}"), "kind": s["kind"], "label": s["title"], "thread": s["thread"], "cited": cited.contains(&sid)}));
        if let Some(t) = s["thread"].as_str() {
            edges.push(json!({"a": format!("S:{sid}"), "b": format!("T:{t}"), "kind": "belongs to", "w": 1}));
        }
    }
    json!({"nodes": nodes, "edges": edges})
}

fn items_list(c: &Connection, ty: &str, st: &str, th: &str, kind: &str) -> Value {
    let like = |s: &str| {
        if s.is_empty() {
            "%".to_string()
        } else {
            s.to_string()
        }
    };
    json!(rows(
        c,
        include_str!("sql/server_items_list.sql"),
        &[&like(ty), &like(st), &like(th), &like(kind)]
    ))
}

pub fn brief(c: &Connection, only: Option<String>) {
    let t = today(c);
    println!(
        "Backtrail brief · {}",
        &t["date"].as_str().unwrap_or("")[..10]
    );
    let w: Vec<&Value> = t["waiting"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| only.as_deref().map_or(true, |o| x["thread"] == o))
        .collect();
    println!(
        "\nClaude asked the owner, waiting on the owner ({}):",
        w.len()
    );
    for x in w {
        println!(
            "  - [{}] {}",
            x["thread"].as_str().unwrap_or(""),
            x["title"].as_str().unwrap_or("")
        );
    }
    let asks: Vec<&Value> = t["asks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| only.as_deref().map_or(true, |o| x["thread"] == o))
        .collect();
    println!(
        "\nAsked by the owner, waiting on Claude ({}): answer each with `backtrail reply <id> --text ...`",
        asks.len()
    );
    for x in asks {
        println!(
            "  - {} [{}] {} {}",
            x["id"].as_str().unwrap_or(""),
            x["thread"].as_str().unwrap_or(""),
            x["title"].as_str().unwrap_or(""),
            x["body"].as_str().unwrap_or("")
        );
    }
    println!("\nThreads in motion:");
    for th in t["threads"].as_array().unwrap() {
        if th["state"] == "moving" && only.as_deref().map_or(true, |o| th["name"] == o) {
            println!(
                "  - {}: {} open steps, {} done; last activity {}",
                th["name"].as_str().unwrap(),
                th["open_s"],
                th["done_s"],
                th["last"].as_str().unwrap_or("?").get(..10).unwrap_or("?")
            );
            let nm = th["name"].as_str().unwrap().to_string();
            if let Some(ns) = rows(c, "SELECT title FROM claims WHERE type='step' AND state='open' AND thread=?1 ORDER BY created LIMIT 1", &[&nm]).first() { println!("      next: {}", ns["title"].as_str().unwrap_or("")); }
            for a in rows(c, include_str!("sql/server_brief.sql"), &[&nm]) {
                println!(
                    "      owner answered \"{}\": {}",
                    a["title"].as_str().unwrap_or(""),
                    a["answer"].as_str().unwrap_or("")
                );
            }
        }
    }
    if !t["stale"].as_array().unwrap().is_empty() {
        println!("\nGone stale:");
        for x in t["stale"].as_array().unwrap() {
            println!(
                "  - [{}] {} (since {})",
                x["thread"].as_str().unwrap(),
                x["title"].as_str().unwrap(),
                x["since"].as_str().unwrap_or("").get(..10).unwrap_or("")
            );
        }
    }
    println!(
        "\nThe store is the source of truth: backtrail serve (http://127.0.0.1:{}). Sources are read only; notes carry no status.",
        crate::ingest::cfg().port
    );
}

fn json_resp(v: Value) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(v.to_string())
        .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
}

fn qparam(url: &str, key: &str) -> String {
    let q = url.split_once('?').map(|x| x.1).unwrap_or("");
    for kv in q.split('&') {
        if let Some((k, v)) = kv.split_once('=') {
            if k == key {
                return urldecode(v);
            }
        }
    }
    String::new()
}

fn urldecode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() + 0 && i + 2 <= b.len() - 1 + 0 => {
                if let Ok(x) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(x);
                    i += 2;
                } else {
                    out.push(b'%');
                }
            }
            x => out.push(x),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// True when a Host or Origin header value names this machine (`127.0.0.1`, `localhost` or `[::1]`, any port).
fn loopback(v: &str) -> bool {
    let h = v.strip_prefix("http://").unwrap_or(v);
    let host = match h.strip_prefix('[') {
        Some(r) => r.split(']').next(),
        None => h.split(':').next(),
    };
    matches!(host, Some("127.0.0.1" | "localhost" | "::1"))
}

pub fn serve(c: Connection, port: u16) {
    let srv = Server::http(format!("127.0.0.1:{}", port)).expect("bind");
    println!("backtrail on http://127.0.0.1:{}", port);
    if !home().join(".demo").exists() {
        std::thread::spawn(|| {
            let mut warned = false;
            loop {
                let c = open();
                rescan(&c);
                if crate::embed::enabled() {
                    match crate::embed::run(&c, crate::ingest::cfg().emb.4) {
                        Err(e) if !warned => {
                            warned = true;
                            eprintln!("embed: {e} (further errors are not printed)");
                        }
                        Ok(_) => warned = false,
                        _ => {}
                    }
                }
                std::thread::sleep(Duration::from_secs(300));
            }
        });
    }
    for mut rq in srv.incoming_requests() {
        // Only this machine's browser may talk to the store. A page on another site reaches a loopback server
        // through DNS rebinding (its own name in Host) or a cross-site POST (its own Origin); both are refused.
        let hdr = |n: &'static str| {
            rq.headers()
                .iter()
                .find(|h| h.field.equiv(n))
                .map(|h| h.value.as_str().to_string())
        };
        if !hdr("Host").is_some_and(|h| loopback(&h)) || hdr("Origin").is_some_and(|o| !loopback(&o)) {
            rq.respond(Response::from_string("{\"error\":\"forbidden\"}").with_status_code(403))
                .ok();
            continue;
        }
        let url = rq.url().to_string();
        let path = url.split('?').next().unwrap_or("/").to_string();
        let mut body = String::new();
        if rq.method() == &Method::Post {
            rq.as_reader().read_to_string(&mut body).ok();
        }
        let b: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        let now = chrono_now();
        let resp = match (rq.method(), path.as_str()) {
            (Method::Get, "/api/focus") => json_resp(focus(&c, &qparam(&url, "thread"))),
            (Method::Get, p) if !p.starts_with("/api/") && asset(p).is_some() => asset(p).unwrap(),
            (Method::Get, "/api/today") => json_resp(today(&c)),
            (Method::Get, "/api/thread") => json_resp(thread(&c, &qparam(&url, "name"))),
            (Method::Get, "/api/source") => {
                json_resp(source(&c, qparam(&url, "id").parse().unwrap_or(0)))
            }
            (Method::Get, "/api/source_raw") => {
                // the original file for notes (markdown) and artifacts (html); sessions have no single file
                let id: i64 = qparam(&url, "id").parse().unwrap_or(0);
                let r = rows(&c, "SELECT kind, path FROM sources WHERE id=?1", &[&id]);
                let (kind, path) = r
                    .first()
                    .map(|x| {
                        (
                            x["kind"].as_str().unwrap_or("").to_string(),
                            x["path"].as_str().unwrap_or("").to_string(),
                        )
                    })
                    .unwrap_or_default();
                let text = if kind == "note" || kind == "artifact" {
                    std::fs::read_to_string(&path).unwrap_or_default()
                } else {
                    String::new()
                };
                json_resp(json!({"kind": kind, "text": text}))
            }
            (Method::Get, "/api/graph") => json_resp(graph_cached(&c)),
            (Method::Get, "/api/trail") => json_resp(json!({
                "threads": rows(&c, include_str!("sql/server_trail.sql"), &[]),
                "sources": rows(&c, include_str!("sql/server_trail_2.sql"), &[]),
                "items": rows(&c, include_str!("sql/server_trail_3.sql"), &[]),
                "links": rows(&c, include_str!("sql/server_trail_4.sql"), &[]),
            })),
            (Method::Get, "/api/kg") => {
                json_resp(json!(rows(&c, include_str!("sql/server_api_kg.sql"), &[])))
            }
            (Method::Get, "/api/kg/graph") => json_resp(json!({
                "nodes": rows(&c, include_str!("sql/server_api_kg_graph.sql"), &[]),
                "edges": rows(&c, include_str!("sql/server_api_kg_graph_2.sql"), &[]),
                "none": rows(&c, include_str!("sql/server_api_kg_graph_3.sql"), &[]),
            })),
            (Method::Get, "/api/kg/growth") => json_resp(json!(rows(
                &c,
                include_str!("sql/server_api_kg_growth.sql"),
                &[]
            ))),
            (Method::Get, "/api/kg/suggest") => {
                let (al, ca) = crate::kg::suggest_data(&c);
                json_resp(
                    json!({"aliases": al.iter().map(|(w, k, n)| json!({"alias": w, "entity": k, "turns": n})).collect::<Vec<_>>(),
                                 "entities": ca.iter().map(|(w, n, s)| json!({"term": w, "turns": n, "sessions": s})).collect::<Vec<_>>()}),
                )
            }
            (Method::Post, "/api/kg/entity") => {
                let v = &b;
                let (name, kind, alias_of) = (
                    v["name"].as_str().unwrap_or("").trim().to_string(),
                    v["kind"].as_str().unwrap_or("project").to_string(),
                    v["alias_of"].as_str().unwrap_or("").to_string(),
                );
                if name.is_empty() {
                    json_resp(json!({"ok": false}))
                } else if kind == "ignore" {
                    crate::kg::ignore_term(&c, &name);
                    json_resp(json!({"ok": true}))
                } else {
                    let ok = if alias_of.is_empty() {
                        crate::kg::entity_add(&c, &name, &kind, &[name.to_lowercase()]);
                        true
                    } else {
                        crate::kg::alias_add(&c, &alias_of, &name)
                    };
                    json_resp(json!({"ok": ok}))
                }
            }
            (Method::Get, "/api/kg/lineage") => json_resp(json!(rows(
                &c,
                include_str!("sql/server_api_kg_lineage.sql"),
                &[]
            ))),
            (Method::Get, "/api/kg/claims") => {
                let (ent, q, st, sp) = (
                    qparam(&url, "entity"),
                    qparam(&url, "q"),
                    qparam(&url, "status"),
                    qparam(&url, "speaker"),
                );
                // Shared WHERE clause: ?1 entity, ?2 text, ?3 status, ?4 speaker.
                let where_ = include_str!("sql/server_api_kg_claims_filter.sql");
                let total = rows(
                    &c,
                    &format!("SELECT COUNT(*) AS n FROM claims c WHERE {where_}"),
                    &[&ent, &q, &st, &sp],
                );
                let claims = rows(
                    &c,
                    &include_str!("sql/server_api_kg_claims.sql").replace("{where}", where_),
                    &[&ent, &q, &st, &sp],
                );
                let ids: Vec<String> = claims
                    .iter()
                    .filter_map(|x| x["id"].as_str())
                    .filter(|i| i.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-'))
                    .map(|i| format!("'{i}'"))
                    .collect();
                let ev = if ids.is_empty() {
                    vec![]
                } else {
                    rows(
                        &c,
                        &include_str!("sql/server_api_kg_claims_evidence.sql")
                            .replace("{ids}", &ids.join(",")),
                        &[],
                    )
                };
                json_resp(
                    json!({"total": total.get(0).map(|t| t["n"].clone()), "claims": claims, "evidence": ev}),
                )
            }
            (Method::Get, "/api/items") => json_resp(items_list(
                &c,
                &qparam(&url, "type"),
                &qparam(&url, "state"),
                &qparam(&url, "thread"),
                &qparam(&url, "kind"),
            )),
            (Method::Get, "/api/log") => json_resp(json!(rows(
                &c,
                "SELECT ts, actor, action, target, detail FROM log ORDER BY id DESC LIMIT 80",
                &[]
            ))),
            (Method::Post, "/api/doing") => {
                let id = b["id"].as_str().unwrap_or("");
                let n = c
                    .execute(include_str!("sql/server_api_doing.sql"), params![id, now])
                    .unwrap_or(0);
                if n == 1 {
                    logw(&c, "owner", "doing", id, "");
                }
                json_resp(json!({"ok": n == 1}))
            }
            (Method::Post, "/api/notyet") => {
                let id = b["id"].as_str().unwrap_or("");
                let n = c
                    .execute(include_str!("sql/server_api_notyet.sql"), params![id, now])
                    .unwrap_or(0);
                if n == 1 {
                    logw(&c, "owner", "not-yet", id, "dismissed looks-done");
                }
                json_resp(json!({"ok": n == 1}))
            }
            (Method::Get, "/api/source_id") => {
                let id: i64 = c
                    .query_row(
                        "SELECT id FROM sources WHERE ref=?1",
                        params![resolve_ref(&c, &qparam(&url, "ref"))],
                        |x| x.get(0),
                    )
                    .unwrap_or(0);
                json_resp(json!({"id": id}))
            }
            (Method::Get, "/api/search") => json_resp(search(
                &c,
                &qparam(&url, "q"),
                &Filters {
                    thread: qparam(&url, "thread"),
                    kind: qparam(&url, "kind"),
                    state: qparam(&url, "state"),
                    from: qparam(&url, "from"),
                    to: qparam(&url, "to"),
                },
                qparam(&url, "mode") != "text",
            )),
            (Method::Post, "/api/tick") => {
                let n = c
                    .execute(
                        include_str!("sql/server_api_tick.sql"),
                        params![b["id"].as_str().unwrap_or(""), now],
                    )
                    .unwrap_or(0);
                logw(&c, "owner", "tick", b["id"].as_str().unwrap_or(""), "");
                json_resp(json!({"ok": n == 1}))
            }
            (Method::Post, "/api/answer") => {
                let a = b["text"].as_str().unwrap_or("").trim().to_string();
                if a.is_empty() || a.chars().count() > MAX_BODY {
                    json_resp(json!({"ok": false, "error": "answer must be 1..320 chars"}))
                } else {
                    let n = c.execute("UPDATE claims SET state='answered', answer=?2, updated=?3 WHERE id=?1 AND type='question'", params![b["id"].as_str().unwrap_or(""), a, now]).unwrap_or(0);
                    logw(&c, "owner", "answer", b["id"].as_str().unwrap_or(""), &a);
                    json_resp(json!({"ok": n == 1}))
                }
            }
            (Method::Post, "/api/add") => {
                // owner capture: a todo, bug, question or decision typed on a thread page. The typed text is its own source.
                let th = b["thread"].as_str().unwrap_or("").to_string();
                let ty = b["type"].as_str().unwrap_or("step").to_string();
                let tag = b["tag"].as_str().unwrap_or("").to_string();
                let t = b["text"].as_str().unwrap_or("").trim().to_string();
                let body = b["body"].as_str().unwrap_or("").trim().to_string();
                let known: i64 = c
                    .query_row(
                        "SELECT COUNT(*) FROM threads WHERE name=?1",
                        params![th],
                        |x| x.get(0),
                    )
                    .unwrap_or(0);
                if !["step", "question", "decision", "turn"].contains(&ty.as_str()) || known == 0 {
                    json_resp(json!({"ok": false, "error": "unknown thread or type"}))
                } else if t.is_empty()
                    || t.chars().count() > MAX_TITLE
                    || body.chars().count() > MAX_BODY
                {
                    json_resp(
                        json!({"ok": false, "error": format!("title 1..{} and detail up to {} characters", MAX_TITLE, MAX_BODY)}),
                    )
                } else {
                    let r = format!("owner:{}", now);
                    c.execute(
                        include_str!("sql/server_api_add_2.sql"),
                        params![r, th, now],
                    )
                    .unwrap();
                    let sid: i64 = c
                        .query_row("SELECT id FROM sources WHERE ref=?1", params![r], |x| {
                            x.get(0)
                        })
                        .unwrap();
                    let full = if body.is_empty() {
                        t.clone()
                    } else {
                        format!("{}\n{}", t, body)
                    };
                    c.execute("INSERT INTO chunks(source_id,seq,locator,role,ts,text) VALUES(?1,0,'typed','owner',?2,?3)", params![sid, now, full]).unwrap();
                    let cid = c.last_insert_rowid();
                    c.execute(
                        "INSERT INTO fts(rowid,text) VALUES(?1,?2)",
                        params![cid, full],
                    )
                    .unwrap();
                    let id = format!("{}-{}", &ty[..1], id64(&format!("{}{}{}", th, t, now)));
                    let st = if ty == "step" || ty == "question" {
                        "open"
                    } else {
                        "settled"
                    };
                    c.execute(
                        include_str!("sql/server_api_add.sql"),
                        params![id, ty, th, t, body, st, tag, sid, cid, now],
                    )
                    .unwrap();
                    logw(&c, "owner", "add", &id, &t);
                    json_resp(json!({"ok": true, "id": id}))
                }
            }
            (Method::Post, "/api/idea") => {
                // an idea becomes an open thread of its own (no repo, no session), with the typed line as its source
                let t = b["text"].as_str().unwrap_or("").trim().to_string();
                if t.is_empty() || t.chars().count() > MAX_TITLE {
                    json_resp(
                        json!({"ok": false, "error": format!("idea must be 1..{} chars", MAX_TITLE)}),
                    )
                } else {
                    let exists: i64 = c
                        .query_row(
                            "SELECT COUNT(*) FROM threads WHERE name=?1",
                            params![t],
                            |x| x.get(0),
                        )
                        .unwrap_or(0);
                    let name = if exists > 0 {
                        format!("{} ({})", t, &now[..10])
                    } else {
                        t.clone()
                    };
                    let r = format!("owner:{}", now);
                    c.execute(
                        include_str!("sql/server_api_idea_2.sql"),
                        params![r, name, now],
                    )
                    .unwrap();
                    let sid: i64 = c
                        .query_row("SELECT id FROM sources WHERE ref=?1", params![r], |x| {
                            x.get(0)
                        })
                        .unwrap();
                    c.execute("INSERT INTO chunks(source_id,seq,locator,role,ts,text) VALUES(?1,0,'typed','owner',?2,?3)", params![sid, now, t]).unwrap();
                    let cid = c.last_insert_rowid();
                    c.execute("INSERT INTO fts(rowid,text) VALUES(?1,?2)", params![cid, t])
                        .unwrap();
                    c.execute(
                        include_str!("sql/server_api_idea.sql"),
                        params![name, now],
                    )
                    .unwrap();
                    logw(&c, "owner", "idea", &name, "");
                    json_resp(json!({"ok": true, "thread": name}))
                }
            }
            (Method::Post, "/api/review") => {
                let acc = b["accept"].as_bool() == Some(true);
                let reason = b["reason"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(MAX_BODY)
                    .collect::<String>();
                let n = c
                    .execute(
                        include_str!("sql/server_api_review.sql"),
                        params![
                            b["id"].as_str().unwrap_or(""),
                            acc as i64,
                            now,
                            reason,
                            (b["park"].as_bool() == Some(true)) as i64
                        ],
                    )
                    .unwrap_or(0);
                if n == 1 {
                    logw(
                        &c,
                        "owner",
                        if acc {
                            "accept"
                        } else if b["park"].as_bool() == Some(true) {
                            "park"
                        } else {
                            "reject"
                        },
                        b["id"].as_str().unwrap_or(""),
                        &reason,
                    );
                }
                json_resp(json!({"ok": n == 1}))
            }
            (Method::Post, "/api/relation") => {
                let (a, bb, k) = (
                    b["a"].as_str().unwrap_or(""),
                    b["b"].as_str().unwrap_or(""),
                    b["kind"].as_str().unwrap_or(""),
                );
                let st = if b["accept"].as_bool() == Some(true) {
                    "accepted"
                } else {
                    "rejected"
                };
                let n = c.execute(
                    "UPDATE claims SET state=?4, updated=?5 WHERE type=?3 AND state='proposed' AND id IN (
                       SELECT s.claim_id FROM claim_about s
                       JOIN entities es ON es.id=s.entity_id
                       JOIN claim_about o ON o.claim_id=s.claim_id AND o.role='object'
                       JOIN entities eo ON eo.id=o.entity_id
                       WHERE s.role='subject' AND es.name=?1 AND eo.name=?2)",
                    params![a.trim_start_matches("T:"), bb.trim_start_matches("T:"), format!("relation:{k}"), if st == "accepted" { "settled" } else { "rejected" }, now],
                ).unwrap_or(0);
                if n == 1 {
                    logw(
                        &c,
                        "owner",
                        if st == "accepted" { "accept" } else { "reject" },
                        &format!("{a} {k} {bb}"),
                        "",
                    );
                }
                json_resp(json!({"ok": n == 1}))
            }
            (Method::Post, "/api/move") => {
                // re-file an item under another project (typed in the wrong one)
                let th = b["thread"].as_str().unwrap_or("");
                let known: i64 = c
                    .query_row(
                        "SELECT COUNT(*) FROM threads WHERE name=?1",
                        params![th],
                        |x| x.get(0),
                    )
                    .unwrap_or(0);
                let id = b["id"].as_str().unwrap_or("");
                let n = if known == 0 {
                    0
                } else {
                    c.execute(
                        "UPDATE claims SET thread=?2, updated=?3 WHERE id=?1 AND type IN ('decision','step','question','turn')",
                        params![id, th, now],
                    )
                    .unwrap_or(0)
                };
                if n == 1 {
                    c.execute("UPDATE sources SET thread=?2 WHERE kind='owner' AND id=(SELECT source_id FROM claims WHERE id=?1)", params![id, th]).ok();
                    logw(&c, "owner", "move", id, th);
                }
                json_resp(json!({"ok": n == 1}))
            }
            (Method::Post, "/api/supersede") => {
                // a kept decision that no longer holds; stays on file, drops out of the live list
                let reason = b["reason"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(200)
                    .collect::<String>();
                let n = c.execute("UPDATE claims SET stale=?2, updated=?3 WHERE id=?1 AND type='decision' AND state='settled'", params![b["id"].as_str().unwrap_or(""), format!("superseded|{}", if reason.trim().is_empty() { "marked replaced in the app" } else { reason.trim() }), now]).unwrap_or(0);
                if n == 1 {
                    logw(
                        &c,
                        "owner",
                        "supersede",
                        b["id"].as_str().unwrap_or(""),
                        &reason,
                    );
                }
                json_resp(json!({"ok": n == 1}))
            }
            _ => Response::from_string("{\"error\":\"not found\"}").with_status_code(404),
        };
        rq.respond(resp).ok();
    }
}
