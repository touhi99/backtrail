mod codex;
mod gemini;
mod inbox;

use super::sessions::{add_turns, project_of, worth_keeping};
use super::threads::SessionRules;
use super::{Source, begin, finish, home_dir, mtime, redact, slug};
use rusqlite::{Connection, params};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct Session {
    pub harness: String,
    pub id: String,
    pub cwd: String,
    pub title: String,
    pub turns: Vec<Turn>,
}

pub struct Turn {
    pub role: &'static str,
    pub ts: String,
    pub text: String,
}

struct Reader {
    files: fn() -> Vec<PathBuf>,
    parse: fn(&Path) -> Option<Session>,
}

const READERS: [Reader; 3] = [
    Reader {
        files: codex::files,
        parse: codex::parse,
    },
    Reader {
        files: gemini::files,
        parse: gemini::parse,
    },
    Reader {
        files: inbox::files,
        parse: inbox::parse,
    },
];

impl Turn {
    fn new(role: &'static str, ts: &str, text: &str) -> Option<Self> {
        let text = text.trim();
        worth_keeping(text).then(|| Self {
            role,
            ts: ts.to_string(),
            text: redact(text).into_owned(),
        })
    }
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var(var)
        .ok()
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn jsonl(raw: &str) -> impl Iterator<Item = Value> + '_ {
    raw.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
}

fn text_parts(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .map(text_parts)
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(o) if o.get("thought").and_then(Value::as_bool) != Some(true) => o
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

fn project(s: &Session) -> Option<String> {
    if s.cwd.is_empty() {
        return Some(s.harness.clone());
    }
    let tmp = std::env::temp_dir();
    let cwd = Path::new(&s.cwd);
    if cwd.starts_with(&tmp)
        || cwd.starts_with(crate::db::home())
        || ["/tmp", "/var/folders", "/private"]
            .iter()
            .any(|t| cwd.starts_with(t))
    {
        return None;
    }
    project_of(&slug(Path::new(&s.cwd)))
}

fn known(c: &Connection, path: &str) -> Option<i64> {
    c.query_row(
        "SELECT mtime FROM sources WHERE path=?1 AND kind='session'
         UNION ALL SELECT mtime FROM scanned WHERE path=?1 LIMIT 1",
        params![path],
        |x| x.get(0),
    )
    .ok()
}

fn store(c: &Connection, rules: &SessionRules, s: &Session, path: &str, mt: i64) -> bool {
    let (Some(proj), Some(first), Some(last)) = (project(s), s.turns.first(), s.turns.last())
    else {
        c.execute(
            "INSERT OR REPLACE INTO scanned(path,mtime) VALUES(?1,?2)",
            params![path, mt],
        )
        .expect("write to database");
        return false;
    };
    let users = s.turns.iter().filter(|t| t.role == "user").count();
    let n = crate::db::sql_int(s.turns.len());
    let thread = rules.thread_for(&proj, &s.title, n);
    let title = if s.title.is_empty() {
        format!("Untitled session ({proj} · {users} turns)")
    } else {
        s.title.clone()
    };
    let r = format!("session:{}", s.id);
    let Some(sid) = begin(
        c,
        &Source {
            source_ref: &r,
            kind: "session",
            title: &title,
            project: &proj,
            thread: Some(&thread),
            path,
            version: mt,
        },
    ) else {
        c.execute("UPDATE sources SET path=?2 WHERE ref=?1", params![r, path])
            .expect("write to database");
        return false;
    };
    let turns: Vec<(String, String, String)> = s
        .turns
        .iter()
        .map(|t| (t.role.to_string(), t.ts.clone(), t.text.clone()))
        .collect();
    add_turns(c, sid, 0, 0, &turns);
    finish(c, sid, &first.ts, &last.ts);
    c.execute(
        "UPDATE sources SET nturns=?2, harness=?3 WHERE id=?1",
        params![sid, n, s.harness],
    )
    .expect("write to database");
    true
}

pub fn read_all(c: &Connection) -> (usize, usize) {
    let rules = SessionRules::load();
    let (mut n, mut skipped) = (0, 0);
    let mut seen = HashSet::new();
    c.execute_batch("BEGIN").expect("write to database");
    for reader in &READERS {
        for p in (reader.files)() {
            let path = p.to_string_lossy().to_string();
            let mt = mtime(&p);
            if known(c, &path) == Some(mt) {
                skipped += 1;
                continue;
            }
            let parsed = (reader.parse)(&p).filter(|s| seen.insert(s.id.clone()));
            let Some(s) = parsed else {
                c.execute(
                    "INSERT OR REPLACE INTO scanned(path,mtime) VALUES(?1,?2)",
                    params![path, mt],
                )
                .expect("write to database");
                skipped += 1;
                continue;
            };
            if store(c, &rules, &s, &path, mt) {
                n += 1;
            } else {
                skipped += 1;
            }
        }
    }
    c.execute_batch("COMMIT").expect("write to database");
    (n, skipped)
}

pub fn unindexed(c: &Connection) -> Vec<(String, u64)> {
    READERS
        .iter()
        .flat_map(|r| (r.files)())
        .filter(|p| known(c, &p.to_string_lossy()).is_none())
        .map(|p| {
            let age = std::fs::metadata(&p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .map_or(0, |e| e.as_secs() / 86400);
            (p.to_string_lossy().to_string(), age)
        })
        .collect()
}
