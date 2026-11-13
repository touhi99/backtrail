use crate::db::*;
use regex::Regex;
use rusqlite::{Connection, params};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub const CATCHALL: &str = "Unsorted";

/// Everything personal lives in `<data dir>/config.json`: workspace folder, note folders, thread rules, repos, aliases.
/// Without it only the "Unsorted" thread exists. An older `threads.json` (an array of threads) is still read.
pub struct Cfg {
    pub workspace: PathBuf,
    pub note_dirs: Vec<String>,
    pub screens_dir: String,
    pub threads: Vec<(String, String, String, String, String, String)>,
    pub note_rules: Vec<(String, String)>,
    pub art_rules: Vec<(String, String)>,
    pub repos: Vec<(String, String)>,
    pub aliases: std::collections::HashMap<String, Vec<String>>,
    pub generic_projects: Vec<String>,
    pub port: u16,
    /// Embeddings: (base url, model, query prefix, document prefix, passages per background run). On by default with a
    /// local Ollama server; `"embeddings": false` in config.json turns them off. Empty url = off.
    pub emb: (String, String, String, String, usize),
}

pub fn cfg() -> &'static Cfg {
    static C: std::sync::OnceLock<Cfg> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let mut c = Cfg {
            workspace: PathBuf::new(),
            note_dirs: vec![],
            screens_dir: String::new(),
            threads: vec![],
            note_rules: vec![],
            art_rules: vec![],
            repos: vec![],
            aliases: Default::default(),
            generic_projects: vec!["home".into()],
            port: 8799,
            emb: (
                "http://127.0.0.1:11434".into(),
                "nomic-embed-text".into(),
                "search_query: ".into(),
                "search_document: ".into(),
                2000,
            ),
        };
        let thread = |x: &Value| {
            let g = |k: &str| x[k].as_str().unwrap_or("").to_string();
            (
                g("area"),
                g("name"),
                if g("state").is_empty() {
                    "open".into()
                } else {
                    g("state")
                },
                g("summary"),
                g("regex"),
                g("kind"),
            )
        };
        let pairs = |v: &Value| -> Vec<(String, String)> {
            v.as_array()
                .map(|a| {
                    a.iter()
                        .map(|p| {
                            (
                                p[0].as_str().unwrap_or("").to_string(),
                                p[1].as_str().unwrap_or("").to_string(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        if let Ok(raw) = fs::read_to_string(home().join("config.json")) {
            if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                c.workspace = PathBuf::from(v["workspace"].as_str().unwrap_or(""));
                c.note_dirs = v["note_dirs"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                c.screens_dir = v["screens_dir"].as_str().unwrap_or("").to_string();
                c.threads = v["threads"]
                    .as_array()
                    .map(|a| a.iter().map(thread).collect())
                    .unwrap_or_default();
                c.note_rules = pairs(&v["note_rules"]);
                c.art_rules = pairs(&v["artifact_rules"]);
                c.repos = pairs(&v["repos"]);
                if let Some(a) = v["generic_projects"].as_array() {
                    c.generic_projects = a
                        .iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect();
                }
                if let Some(n) = v["port"].as_u64() {
                    c.port = n as u16;
                }
                if v.get("embeddings") == Some(&Value::Bool(false)) {
                    c.emb = Default::default();
                } else if let Some(e) = v["embeddings"].as_object() {
                    let g = |k: &str| e.get(k).and_then(|x| x.as_str()).map(String::from);
                    let model = g("model").unwrap_or_else(|| "nomic-embed-text".into());
                    let nomic = model.contains("nomic");
                    c.emb = (
                        g("url")
                            .unwrap_or_else(|| "http://127.0.0.1:11434".into())
                            .trim_end_matches('/')
                            .to_string(),
                        model,
                        g("query_prefix").unwrap_or(if nomic {
                            "search_query: ".into()
                        } else {
                            String::new()
                        }),
                        g("document_prefix").unwrap_or(if nomic {
                            "search_document: ".into()
                        } else {
                            String::new()
                        }),
                        e.get("batch_cap").and_then(|x| x.as_u64()).unwrap_or(2000) as usize,
                    );
                }
                if let Some(o) = v["aliases"].as_object() {
                    for (k, x) in o {
                        c.aliases.insert(
                            k.clone(),
                            x.as_array()
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|y| y.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default(),
                        );
                    }
                }
            }
        } else if let Ok(raw) = fs::read_to_string(home().join("threads.json")) {
            if let Ok(Value::Array(a)) = serde_json::from_str::<Value>(&raw) {
                c.threads = a.iter().map(thread).collect();
            }
        }
        c
    })
}

pub fn load_threads() -> Vec<(String, String, String, String, String)> {
    cfg()
        .threads
        .iter()
        .map(|t| {
            (
                t.0.clone(),
                t.1.clone(),
                t.2.clone(),
                t.3.clone(),
                t.4.clone(),
            )
        })
        .collect()
}

pub fn seed_threads(c: &Connection) {
    let now = chrono_now();
    let all = load_threads();
    let unsorted = (
        "Other".to_string(),
        CATCHALL.to_string(),
        "unsorted".to_string(),
        "Sources no rule matched.".to_string(),
        String::new(),
    );
    for (area, name, state, summary, _) in all.iter().chain(std::iter::once(&unsorted)) {
        c.execute(
            include_str!("sql/ingest_seed_threads.sql"),
            params![
                format!("t-{:x}", fxhash(name)),
                name,
                summary,
                state,
                area,
                now
            ],
        )
        .unwrap();
    }
    for t in &cfg().threads {
        if !t.5.is_empty() {
            c.execute("UPDATE nodes SET kind=?2 WHERE type='thread' AND thread=?1 AND (kind IS NULL OR kind='')", params![t.1, t.5]).ok();
        }
    }
}

fn mtime(p: &Path) -> i64 {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn upsert_source(
    c: &Connection,
    r: &str,
    kind: &str,
    title: &str,
    project: &str,
    thread: Option<&str>,
    path: &str,
    mt: i64,
) -> Option<i64> {
    let old: Option<(i64, i64)> = c
        .query_row(
            "SELECT id, mtime FROM sources WHERE ref=?1",
            params![r],
            |x| Ok((x.get(0)?, x.get(1)?)),
        )
        .ok();
    if let Some((id, m)) = old {
        if m == mt {
            return None;
        }
        c.execute(
            "DELETE FROM fts WHERE rowid IN (SELECT id FROM chunks WHERE source_id=?1)",
            params![id],
        )
        .unwrap();
        c.execute("DELETE FROM chunks WHERE source_id=?1", params![id])
            .unwrap();
        c.execute(
            "UPDATE sources SET title=?2, project=?3, thread=?4, mtime=?5, path=?6 WHERE id=?1",
            params![id, title, project, thread, mt, path],
        )
        .unwrap();
        return Some(id);
    }
    c.execute("INSERT INTO sources(ref,kind,title,project,thread,path,mtime) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![r, kind, title, project, thread, path, mt]).unwrap();
    Some(c.last_insert_rowid())
}

fn add_chunk(c: &Connection, sid: i64, seq: i64, loc: &str, role: &str, ts: &str, text: &str) {
    c.execute(
        "INSERT INTO chunks(source_id,seq,locator,role,ts,text) VALUES(?1,?2,?3,?4,?5,?6)",
        params![sid, seq, loc, role, ts, text],
    )
    .unwrap();
    let id = c.last_insert_rowid();
    c.execute(
        "INSERT INTO fts(rowid,text) VALUES(?1,?2)",
        params![id, text],
    )
    .unwrap();
}

fn finish(c: &Connection, sid: i64, first: &str, last: &str) {
    c.execute(
        include_str!("sql/ingest_finish.sql"),
        params![sid, first, last],
    )
    .unwrap();
}

fn cap(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Claude Code names a project folder after its path with every non-alphanumeric character turned into "-".
fn slug(p: &Path) -> String {
    p.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn home_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

pub fn ingest_sessions(c: &Connection) -> (usize, usize) {
    let tl = load_threads();
    let rules: Vec<Regex> = tl
        .iter()
        .map(|t| Regex::new(&format!("(?i){}", if t.4.is_empty() { "$^" } else { &t.4 })).unwrap())
        .collect();
    let root = home_dir().join(".claude/projects");
    let (mut n, mut skipped) = (0, 0);
    let Ok(dirs) = fs::read_dir(&root) else {
        return (0, 0);
    };
    // The same session id can exist in two project folders (a folder was renamed or copied). Read only the larger
    // copy; otherwise each run swaps the stored text between the two and claim evidence loses its passage.
    let mut largest: std::collections::HashMap<String, (u64, std::path::PathBuf)> =
        Default::default();
    for d in fs::read_dir(&root).into_iter().flatten().flatten() {
        for f in fs::read_dir(d.path()).into_iter().flatten().flatten() {
            let p = f.path();
            if p.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let id8: String = p
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .chars()
                .take(8)
                .collect();
            let size = f.metadata().map(|m| m.len()).unwrap_or(0);
            let e = largest.entry(id8).or_insert((size, p.clone()));
            if size > e.0 {
                *e = (size, p.clone());
            }
        }
    }
    c.execute_batch("BEGIN").unwrap();
    for d in dirs.flatten() {
        let dn = d.file_name().to_string_lossy().to_string();
        if dn.contains("private-tmp") || dn.contains("private-var") {
            continue;
        }
        let hs = slug(&home_dir());
        let rest = dn.strip_prefix(&format!("{}-", hs)).unwrap_or(&dn);
        let proj = if dn == hs {
            "home".to_string()
        } else {
            ["Desktop-", "Documents-"]
                .iter()
                .find_map(|p| rest.strip_prefix(p))
                .unwrap_or(rest)
                .to_string()
        };
        let Ok(files) = fs::read_dir(d.path()) else {
            continue;
        };
        for f in files.flatten() {
            let p = f.path();
            if p.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let id8: String = p
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .chars()
                .take(8)
                .collect();
            if largest.get(&id8).is_some_and(|(_, best)| *best != p) {
                continue;
            }
            let r = format!("session:{}", id8);
            let mt = mtime(&p);
            let known: Option<i64> = c
                .query_row("SELECT mtime FROM sources WHERE ref=?1", params![r], |x| {
                    x.get(0)
                })
                .ok()
                .or_else(|| {
                    c.query_row(
                        "SELECT mtime FROM scanned WHERE path=?1",
                        params![p.to_string_lossy()],
                        |x| x.get(0),
                    )
                    .ok()
                });
            if known == Some(mt) {
                skipped += 1;
                continue;
            }
            let Ok(raw) = fs::read_to_string(&p) else {
                continue;
            };
            let mut title = String::new();
            let mut turns: Vec<(String, String, String)> = vec![];
            for l in raw.lines() {
                let Ok(v) = serde_json::from_str::<Value>(l) else {
                    continue;
                };
                let t = v["type"].as_str().unwrap_or("");
                let ts = v["timestamp"].as_str().unwrap_or("").to_string();
                if t == "ai-title" {
                    title = v["aiTitle"].as_str().unwrap_or("").to_string();
                } else if t == "user" || t == "assistant" {
                    if v["isMeta"].as_bool() == Some(true) {
                        continue;
                    }
                    let mut text = String::new();
                    match &v["message"]["content"] {
                        Value::String(s) => text = s.clone(),
                        Value::Array(a) => {
                            for b in a {
                                if b["type"] == "text" {
                                    text.push_str(b["text"].as_str().unwrap_or(""));
                                    text.push('\n');
                                }
                            }
                        }
                        _ => {}
                    }
                    let tt = text.trim();
                    if tt.len() < 15
                        || tt.starts_with("<local-command")
                        || tt.starts_with("<command-")
                        || tt.starts_with("<system-reminder")
                        || tt.starts_with("[SYSTEM")
                        || tt.starts_with("Caveat:")
                    {
                        continue;
                    }
                    turns.push((t.to_string(), ts, cap(tt, 2400)));
                }
            }
            if turns.is_empty() {
                c.execute(
                    "INSERT OR REPLACE INTO scanned(path,mtime) VALUES(?1,?2)",
                    params![p.to_string_lossy(), mt],
                )
                .unwrap();
                continue;
            }
            let users = turns.iter().filter(|t| t.0 == "user").count();
            let st = session_thread(&tl, &rules, &proj, &title, turns.len() as i64);
            let thread: Option<&str> = Some(st.as_str());
            let disp = if title.is_empty() {
                format!("Untitled session ({} · {} turns)", proj, users)
            } else {
                title.clone()
            };
            let Some(sid) = upsert_source(
                c,
                &r,
                "session",
                &disp,
                &proj,
                thread,
                &p.to_string_lossy(),
                mt,
            ) else {
                skipped += 1;
                continue;
            };
            let first = turns[0].1.clone();
            let last = turns.last().unwrap().1.clone();
            for (i, (role, ts, text)) in turns.iter().enumerate() {
                let loc = format!("turn {} · {}", i + 1, &ts.get(..16).unwrap_or(""));
                add_chunk(c, sid, i as i64, &loc, role, ts, text);
            }
            finish(c, sid, &first, &last);
            n += 1;
        }
    }
    c.execute_batch("COMMIT").unwrap();
    (n, skipped)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, ext: &str) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let n = e.file_name().to_string_lossy().to_string();
        if n.starts_with('.') || n == "node_modules" || n == "target" || n == "venv" || n == ".venv"
        {
            continue;
        }
        if p.is_dir() {
            walk(&p, out, ext);
        } else if p.extension().and_then(|x| x.to_str()) == Some(ext) {
            out.push(p);
        }
    }
}

fn thread_for(kind: &str, key: &str) -> Option<String> {
    let k = key.to_lowercase();
    let c = cfg();
    let rules = if kind == "note" {
        &c.note_rules
    } else {
        &c.art_rules
    };
    if !rules.is_empty() {
        for (name, rx) in rules {
            if Regex::new(&format!("(?i){}", rx)).unwrap().is_match(&k) {
                return Some(name.clone());
            }
        }
    } else {
        for t in &c.threads {
            if !t.4.is_empty() && Regex::new(&format!("(?i){}", t.4)).unwrap().is_match(&k) {
                return Some(t.1.clone());
            }
        }
    }
    Some(CATCHALL.to_string())
}

pub fn ingest_notes(c: &Connection) -> (usize, usize) {
    let (a, b) = ingest_notes_inner(c);
    let (x, y) = ingest_html_screens(c);
    (a + x, b + y)
}

fn ingest_notes_inner(c: &Connection) -> (usize, usize) {
    let coding = cfg().workspace.clone();
    let mut files = vec![];
    // Claude Code memory folders: the workspace's own when one is configured, otherwise every project's.
    let mut mems: Vec<(PathBuf, String)> = vec![];
    if coding.as_os_str().is_empty() {
        let root = home_dir().join(".claude/projects");
        for d in fs::read_dir(&root).into_iter().flatten().flatten() {
            let label = format!("memory/{}", d.file_name().to_string_lossy());
            mems.push((d.path().join("memory"), label));
        }
    } else {
        for sub in &cfg().note_dirs {
            walk(&coding.join(sub), &mut files, "md");
        }
        let mem = home_dir()
            .join(".claude/projects")
            .join(slug(&coding))
            .join("memory");
        mems.push((mem, "memory".to_string()));
    }
    for (mem, _) in &mems {
        walk(mem, &mut files, "md");
    }
    let (mut n, mut skipped) = (0, 0);
    c.execute_batch("BEGIN").unwrap();
    for p in files {
        let rel = match mems.iter().find(|(m, _)| p.starts_with(m)) {
            Some((m, label)) => format!("{}/{}", label, p.strip_prefix(m).unwrap().display()),
            None => p.strip_prefix(&coding).unwrap().display().to_string(),
        };
        let r = format!("note:{}", rel);
        let mt = mtime(&p);
        let Ok(raw) = fs::read_to_string(&p) else {
            continue;
        };
        let mut title = Path::new(&rel)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();
        if title == "README" {
            title = format!(
                "{} README",
                Path::new(&rel)
                    .parent()
                    .and_then(|x| x.file_name())
                    .map(|x| x.to_string_lossy().to_string())
                    .unwrap_or_default()
            );
        }
        let thread = thread_for("note", &rel);
        let Some(sid) = upsert_source(
            c,
            &r,
            "note",
            &title,
            "",
            thread.as_deref(),
            &p.to_string_lossy(),
            mt,
        ) else {
            skipped += 1;
            continue;
        };
        // split by headings, then cap blocks
        let mut head = String::from("top");
        let mut buf = String::new();
        let mut seq = 0;
        let mut flush = |head: &str, buf: &mut String, seq: &mut i64| {
            let t = buf.trim();
            if t.len() >= 12 {
                let chars: Vec<char> = t.chars().collect();
                for part in chars.chunks(1800) {
                    let s: String = part.iter().collect();
                    add_chunk(c, sid, *seq, &format!("#{}", head), "note", "", &s);
                    *seq += 1;
                }
            }
            buf.clear();
        };
        for l in raw.lines() {
            if l.starts_with('#') {
                flush(&head, &mut buf, &mut seq);
                head = l.trim_start_matches('#').trim().to_string();
            } else {
                buf.push_str(l);
                buf.push('\n');
            }
        }
        flush(&head, &mut buf, &mut seq);
        let ts = mt_to_date(mt);
        finish(c, sid, &ts, &ts);
        n += 1;
    }
    c.execute_batch("COMMIT").unwrap();
    (n, skipped)
}

fn mt_to_date(mt: i64) -> String {
    let out = std::process::Command::new("date")
        .args(["-u", "-r", &mt.to_string(), "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn strip_html(h: &str) -> String {
    let mut s = h.to_string();
    for pat in [
        r"(?is)<style.*?</style>",
        r"(?is)<script.*?</script>",
        r"(?is)<svg.*?</svg>",
        r"(?s)<!--.*?-->",
    ] {
        s = Regex::new(pat).unwrap().replace_all(&s, " ").to_string();
    }
    s = Regex::new(r"(?i)</(h[1-6]|p|li|tr|div|section|figcaption)>|<br\s*/?>")
        .unwrap()
        .replace_all(&s, "\n")
        .to_string();
    s = Regex::new(r"(?i)<h[1-6][^>]*>")
        .unwrap()
        .replace_all(&s, "\n## ")
        .to_string();
    s = Regex::new(r"<[^>]+>")
        .unwrap()
        .replace_all(&s, " ")
        .to_string();
    for (a, b) in [
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&nbsp;", " "),
        ("&middot;", "·"),
    ] {
        s = s.replace(a, b);
    }
    Regex::new(r"[ \t]+")
        .unwrap()
        .replace_all(&s, " ")
        .to_string()
}

pub fn ingest_artifacts(c: &Connection) -> (usize, usize) {
    let dir = home().join("artifacts");
    let Ok(rd) = fs::read_dir(&dir) else {
        return (0, 0);
    };
    let (mut n, mut skipped) = (0, 0);
    c.execute_batch("BEGIN").unwrap();
    for e in rd.flatten() {
        let id = e.file_name().to_string_lossy().to_string();
        let p = e.path().join("index.html");
        let Ok(raw) = fs::read_to_string(&p) else {
            continue;
        };
        let title = Regex::new(r"(?is)<title>(.*?)</title>")
            .unwrap()
            .captures(&raw)
            .map(|c| c[1].trim().to_string())
            .unwrap_or(id.clone());
        let mt = mtime(&p);
        let thread = thread_for("artifact", &title);
        let Some(sid) = upsert_source(
            c,
            &format!("artifact:{}", id),
            "artifact",
            &title,
            "",
            thread.as_deref(),
            &p.to_string_lossy(),
            mt,
        ) else {
            skipped += 1;
            continue;
        };
        let text = strip_html(&raw);
        let (mut head, mut buf, mut seq) = (String::from("top"), String::new(), 0i64);
        let flush = |head: &str, buf: &mut String, seq: &mut i64| {
            let lines: Vec<&str> = buf
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .collect();
            let t = lines.join("\n");
            if t.len() >= 12 {
                let chars: Vec<char> = t.chars().collect();
                for part in chars.chunks(1800) {
                    add_chunk(
                        c,
                        sid,
                        *seq,
                        &format!("§{}", head),
                        "artifact",
                        "",
                        &part.iter().collect::<String>(),
                    );
                    *seq += 1;
                }
            }
            buf.clear();
        };
        for l in text.lines() {
            if let Some(h) = l.trim().strip_prefix("## ") {
                flush(&head, &mut buf, &mut seq);
                head = h.trim().to_string();
            } else {
                buf.push_str(l);
                buf.push('\n');
            }
        }
        flush(&head, &mut buf, &mut seq);
        let ts = c
            .query_row(
                "SELECT updated FROM artifact_meta WHERE id=?1",
                params![id],
                |r| r.get::<_, String>(0),
            )
            .map(|d| format!("{}T12:00:00Z", d))
            .unwrap_or_else(|_| mt_to_date(mt));
        finish(c, sid, &ts, &ts);
        n += 1;
    }
    c.execute_batch("COMMIT").unwrap();
    (n, skipped)
}

/// After a source is re-read its chunk ids change; point each item back at the chunk that still holds its quote.
pub fn relink(c: &Connection) -> (usize, usize) {
    let items: Vec<(String, i64, String)> = {
        let mut st = c.prepare("SELECT id, source_id, quote FROM nodes WHERE source_id IS NOT NULL AND quote IS NOT NULL").unwrap();
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .flatten()
            .collect()
    };
    let (mut ok, mut lost) = (0, 0);
    for (id, sid, quote) in items {
        let q = norm(&quote);
        let mut st = c
            .prepare("SELECT id, text FROM chunks WHERE source_id=?1 ORDER BY seq")
            .unwrap();
        let found = st
            .query_map(params![sid], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })
            .unwrap()
            .flatten()
            .find(|(_, t)| norm(t).contains(&q));
        match found {
            Some((cid, _)) => {
                c.execute("UPDATE nodes SET chunk_id=?2 WHERE id=?1", params![id, cid])
                    .unwrap();
                ok += 1;
            }
            None => {
                c.execute("UPDATE nodes SET chunk_id=NULL WHERE id=?1", params![id])
                    .unwrap();
                lost += 1;
            }
        }
    }
    relink_claims(c);
    (ok, lost)
}

/// Claim evidence is located by source and quote, so it follows a re-read source the same way items do.
fn relink_claims(c: &Connection) {
    let rows: Vec<(String, i64, i64, String)> = {
        let mut st = c
            .prepare(
                "SELECT v.claim_id, v.source_id, v.chunk_id, v.quote FROM claim_evidence v
                 LEFT JOIN chunks k ON k.id=v.chunk_id WHERE k.id IS NULL",
            )
            .unwrap();
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .flatten()
            .collect()
    };
    let mut by_source: std::collections::HashMap<i64, Vec<(i64, String)>> = Default::default();
    for (claim, sid, old, quote) in rows {
        let chunks = by_source.entry(sid).or_insert_with(|| {
            let mut st = c
                .prepare("SELECT id, text FROM chunks WHERE source_id=?1 ORDER BY seq")
                .unwrap();
            st.query_map(params![sid], |r| {
                Ok((r.get::<_, i64>(0)?, norm(&r.get::<_, String>(1)?)))
            })
            .unwrap()
            .flatten()
            .collect()
        });
        let q = norm(&quote);
        if let Some((cid, _)) = chunks.iter().find(|(_, t)| t.contains(&q)) {
            c.execute(
                "UPDATE OR IGNORE claim_evidence SET chunk_id=?3 WHERE claim_id=?1 AND chunk_id=?2 AND quote=?4",
                params![claim, old, cid, quote],
            )
            .unwrap();
        }
    }
    // A stale row is dropped when the same quote already has a row on a chunk that exists.
    c.execute(
        "DELETE FROM claim_evidence WHERE chunk_id NOT IN (SELECT id FROM chunks) AND EXISTS (
           SELECT 1 FROM claim_evidence o WHERE o.claim_id=claim_evidence.claim_id AND o.quote=claim_evidence.quote
             AND o.chunk_id IN (SELECT id FROM chunks))",
        [],
    )
    .unwrap();
}

/// Thread for a session: the title decides first; the project folder is the fallback.
fn session_thread(
    tl: &[(String, String, String, String, String)],
    rules: &[Regex],
    proj: &str,
    title: &str,
    nchunks: i64,
) -> String {
    let untitled = title.is_empty() || title.starts_with("Untitled session");
    if untitled && nchunks <= 3 {
        return CATCHALL.to_string();
    }
    let generic = cfg().generic_projects.iter().any(|g| g == proj);
    if !untitled {
        for (i, rg) in rules.iter().enumerate() {
            if rg.is_match(&title.to_lowercase()) {
                return tl[i].1.clone();
            }
        }
    }
    if !generic {
        for (i, rg) in rules.iter().enumerate() {
            if rg.is_match(&proj.to_lowercase()) {
                return tl[i].1.clone();
            }
        }
        // No thread rules configured: one thread per project folder.
        if tl.is_empty() && !proj.is_empty() {
            return proj.to_string();
        }
    }
    CATCHALL.to_string()
}

pub fn rethread(c: &Connection) {
    let tl = load_threads();
    let rules: Vec<Regex> = tl
        .iter()
        .map(|t| Regex::new(&format!("(?i){}", if t.4.is_empty() { "$^" } else { &t.4 })).unwrap())
        .collect();
    let rows: Vec<(i64, String, String, i64)> = {
        let mut st = c
            .prepare(
                "SELECT id, project, ifnull(title,''), nchunks FROM sources WHERE kind='session'",
            )
            .unwrap();
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .flatten()
            .collect()
    };
    let pinned: std::collections::HashMap<i64, String> = {
        let mut st = c
            .prepare("SELECT s.id, o.thread FROM overrides o JOIN sources s ON s.ref=o.ref")
            .unwrap();
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .flatten()
            .collect()
    };
    c.execute_batch("BEGIN").unwrap();
    let now = chrono_now();
    for (id, proj, title, n) in rows {
        let t = pinned
            .get(&id)
            .cloned()
            .unwrap_or_else(|| session_thread(&tl, &rules, &proj, &title, n));
        if tl.is_empty() && t == proj {
            c.execute(
                include_str!("sql/ingest_seed_threads.sql"),
                params![
                    format!("t-{:x}", fxhash(&t)),
                    t,
                    "Sessions from this project folder.",
                    "open",
                    "Other",
                    now
                ],
            )
            .unwrap();
        }
        c.execute("UPDATE sources SET thread=?2 WHERE id=?1", params![id, t])
            .unwrap();
    }
    c.execute_batch("COMMIT").unwrap();
}

fn git(dir: &Path, args: &[&str]) -> String {
    let o = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output();
    o.map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

/// Git reader: one source per repo, one passage per commit (last 400), plus a line for uncommitted and unpushed work.
pub fn ingest_git(c: &Connection) -> (usize, usize) {
    let coding = cfg().workspace.clone();
    if coding.as_os_str().is_empty() {
        return (0, 0);
    }
    let (mut n, mut skipped) = (0, 0);
    for (rel, th) in &cfg().repos {
        let dir = coding.join(rel);
        if !dir.join(".git").exists() {
            continue;
        }
        let log = git(&dir, &["log", "-n", "400", "--format=%h|%aI|%s"]);
        if log.is_empty() {
            continue;
        }
        let st = git(&dir, &["status", "--porcelain=v1", "-b"]);
        let sig = fxhash(&format!("{}{}", log, st)) as i64;
        let Some(sid) = upsert_source(
            c,
            &format!("git:{}", rel),
            "commit",
            &format!("{} commits", rel),
            rel,
            Some(th.as_str()),
            &dir.to_string_lossy(),
            sig,
        ) else {
            skipped += 1;
            continue;
        };
        let mut lines = st.lines();
        let head = lines.next().unwrap_or("").to_string();
        let dirty = lines.count();
        add_chunk(
            c,
            sid,
            0,
            "working tree",
            "git",
            "",
            &format!(
                "{} · {} uncommitted file(s)",
                head.trim_start_matches("## "),
                dirty
            ),
        );
        let (mut first, mut last) = (String::new(), String::new());
        for (i, l) in log.lines().enumerate() {
            let mut p = l.splitn(3, '|');
            let (h, d, subj) = (
                p.next().unwrap_or(""),
                p.next().unwrap_or(""),
                p.next().unwrap_or(""),
            );
            if i == 0 {
                last = d.to_string();
            }
            first = d.to_string();
            add_chunk(
                c,
                sid,
                1 + i as i64,
                &format!("commit {}", h),
                "git",
                d,
                &format!("{} {} {}", h, &d[..10.min(d.len())], subj),
            );
        }
        finish(c, sid, &first, &last);
        n += 1;
    }
    (n, skipped)
}

/// Design mock-ups (`docs/screens/*.dc.html`) as text, so the Screens artifact's real content is searchable.
fn ingest_html_screens(c: &Connection) -> (usize, usize) {
    let coding = cfg().workspace.clone();
    if coding.as_os_str().is_empty() || cfg().screens_dir.is_empty() {
        return (0, 0);
    }
    let mut files = vec![];
    walk(&coding.join(&cfg().screens_dir), &mut files, "html");
    let (mut n, mut skipped) = (0, 0);
    c.execute_batch("BEGIN").unwrap();
    for p in files {
        let Ok(raw) = fs::read_to_string(&p) else {
            continue;
        };
        let rel = p.strip_prefix(&coding).unwrap().display().to_string();
        let mt = mtime(&p);
        let title = Regex::new(r"(?is)<title>(.*?)</title>")
            .unwrap()
            .captures(&raw)
            .map(|c| c[1].trim().to_string())
            .unwrap_or(rel.clone());
        let Some(sid) = upsert_source(
            c,
            &format!("note:{}", rel),
            "note",
            &title,
            "",
            Some("Backtrail"),
            &p.to_string_lossy(),
            mt,
        ) else {
            skipped += 1;
            continue;
        };
        let text: String = strip_html(&raw)
            .lines()
            .map(|l| l.trim())
            .filter(|l| l.len() > 2)
            .collect::<Vec<_>>()
            .join("\n");
        for (i, part) in text.chars().collect::<Vec<_>>().chunks(1800).enumerate() {
            add_chunk(
                c,
                sid,
                i as i64,
                "§screen",
                "note",
                "",
                &part.iter().collect::<String>(),
            );
        }
        let ts = mt_to_date(mt);
        finish(c, sid, &ts, &ts);
        n += 1;
    }
    c.execute_batch("COMMIT").unwrap();
    (n, skipped)
}
