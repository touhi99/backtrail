use rusqlite::{Connection, params};
use std::path::PathBuf;

pub const TYPES: &[&str] = &["thread", "decision", "step", "question", "turn"];
pub const MAX_TITLE: usize = 110;
pub const MAX_BODY: usize = 320;

pub fn home() -> PathBuf {
    if let Ok(p) = std::env::var("BACKTRAIL_HOME") {
        return PathBuf::from(p);
    }
    let h = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    if cfg!(target_os = "macos") {
        return h.join("Library/Application Support/Backtrail");
    }
    match std::env::var("XDG_DATA_HOME") {
        Ok(x) if !x.is_empty() => PathBuf::from(x).join("backtrail"),
        _ => h.join(".local/share/backtrail"),
    }
}

pub fn open() -> Connection {
    let dir = home();
    std::fs::create_dir_all(&dir).ok();
    let c = Connection::open(dir.join("backtrail.db")).expect("open db");
    c.execute_batch(include_str!("sql/schema.sql"))
        .expect("schema");
    let _ = c.execute("ALTER TABLE nodes ADD COLUMN kind TEXT", []);
    let _ = c.execute("ALTER TABLE nodes ADD COLUMN tag TEXT", []);
    let _ = c.execute("ALTER TABLE nodes ADD COLUMN stale TEXT", []);
    for col in [
        "source_id INTEGER",
        "chunk_id INTEGER",
        "quote TEXT",
        "ts TEXT",
        "state TEXT",
    ] {
        let _ = c.execute(&format!("ALTER TABLE links ADD COLUMN {col}"), []);
    }
    crate::embed::ensure(&c);
    crate::kg::ensure(&c);
    c
}

pub fn norm(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub struct Item {
    pub typ: String,
    pub thread: String,
    pub title: String,
    pub body: String,
    pub source: String,
    pub quote: String,
    pub state: Option<String>,
    pub origin: String,
    pub date: Option<String>,
}

/// The write rule: every item cites a source that exists, quotes text found in it, and respects the length limits.
pub fn add_item(c: &Connection, it: &Item) -> Result<String, String> {
    if !TYPES.contains(&it.typ.as_str()) || it.typ == "thread" {
        return Err(format!(
            "type must be one of decision, step, question, turn (got {})",
            it.typ
        ));
    }
    if it.title.trim().is_empty() || it.title.chars().count() > MAX_TITLE {
        return Err(format!("title must be 1..{} chars", MAX_TITLE));
    }
    if it.body.chars().count() > MAX_BODY {
        return Err(format!("body over {} chars", MAX_BODY));
    }
    let known: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE type='thread' AND thread=?1",
            params![it.thread],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if known == 0 {
        return Err(format!("unknown thread '{}'", it.thread));
    }
    let sid: i64 = c
        .query_row(
            "SELECT id FROM sources WHERE ref=?1",
            params![it.source],
            |r| r.get(0),
        )
        .map_err(|_| {
            format!(
                "source '{}' not found: every item must cite an indexed source",
                it.source
            )
        })?;
    if it.quote.trim().len() < 8 {
        return Err("quote of at least 8 chars from the source is required".into());
    }
    let q = norm(&it.quote);
    let mut stmt = c
        .prepare("SELECT id, text FROM chunks WHERE source_id=?1 ORDER BY seq")
        .unwrap();
    let rows = stmt
        .query_map(params![sid], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })
        .unwrap();
    let mut chunk = None;
    for r in rows.flatten() {
        if norm(&r.1).contains(&q) {
            chunk = Some(r.0);
            break;
        }
    }
    let chunk =
        chunk.ok_or_else(|| format!("quote not found in {}: \"{}\"", it.source, it.quote))?;
    let now = it.date.clone().unwrap_or_else(|| chrono_now());
    let id = format!(
        "{}-{:x}",
        &it.typ[..1],
        fxhash(&format!("{}{}{}", it.thread, it.typ, it.title))
    );
    let state = it.state.clone().unwrap_or_else(|| match it.typ.as_str() {
        "step" | "question" => "open".into(),
        _ => "settled".into(),
    });
    c.execute(
        include_str!("sql/db_add_item.sql"),
        params![
            id, it.typ, it.thread, it.title, it.body, state, it.origin, sid, chunk, it.quote, now
        ],
    )
    .map_err(|e| e.to_string())?;
    logw(
        c,
        if it.origin == "suggested" {
            "claude-suggest"
        } else {
            "claude"
        },
        "add",
        &id,
        &it.title,
    );
    Ok(id)
}

/// The write rule for a relation between two threads: both exist, the source exists, and the quote is found in it.
/// Returns (source id, chunk id, chunk timestamp).
pub fn relation_evidence(
    c: &Connection,
    a: &str,
    b: &str,
    source: &str,
    quote: &str,
) -> Result<(i64, i64, String), String> {
    for t in [a, b] {
        let known: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE type='thread' AND thread=?1",
                params![t],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if known == 0 {
            return Err(format!("unknown thread '{t}'"));
        }
    }
    if a == b {
        return Err("a relation needs two different threads".into());
    }
    let sid: i64 = c
        .query_row(
            "SELECT id FROM sources WHERE ref=?1",
            params![source],
            |r| r.get(0),
        )
        .map_err(|_| format!("source '{source}' not found"))?;
    if quote.trim().len() < 8 {
        return Err("quote of at least 8 chars from the source is required".into());
    }
    let q = norm(quote);
    let mut stmt = c
        .prepare("SELECT id, ifnull(ts,''), text FROM chunks WHERE source_id=?1 ORDER BY seq")
        .unwrap();
    let rows = stmt
        .query_map(params![sid], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap();
    for r in rows.flatten() {
        if norm(&r.2).contains(&q) {
            return Ok((sid, r.0, r.1));
        }
    }
    Err(format!("quote not found in {source}: \"{quote}\""))
}

pub fn chrono_now() -> String {
    let out = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

pub fn fxhash(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

/// Append-only change log: who changed what and when (owner, claude, check, ingest).
pub fn logw(c: &Connection, actor: &str, action: &str, target: &str, detail: &str) {
    c.execute(
        "INSERT INTO log(ts,actor,action,target,detail) VALUES(?1,?2,?3,?4,?5)",
        params![chrono_now(), actor, action, target, detail],
    )
    .ok();
}
