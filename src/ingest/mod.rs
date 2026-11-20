pub mod git;
pub mod notes;
pub mod sessions;
pub mod threads;

use crate::db::find_quote;
use regex::Regex;
use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub struct Report {
    pub sessions: (usize, usize),
    pub notes: (usize, usize),
    pub artifacts: (usize, usize),
    pub repos: (usize, usize),
    pub quotes: (usize, usize),
}

pub fn scan(c: &Connection) -> Report {
    threads::seed(c);
    let report = Report {
        sessions: sessions::read_all(c),
        notes: notes::read_notes(c),
        artifacts: notes::read_artifacts(c),
        repos: git::read_repos(c),
        quotes: (0, 0),
    };
    threads::assign_sessions(c);
    crate::items::rehome(c);
    Report {
        quotes: relink(c),
        ..report
    }
}

pub fn rescan(c: &Connection) {
    scan(c);
    if !crate::db::is_demo() {
        backup(c);
    }
}

pub fn backup(c: &Connection) -> Option<String> {
    let dir = crate::db::home().join("backups");
    if std::env::var("BACKTRAIL_HOME").is_err()
        && !dir.exists()
        && let Some(old) = crate::db::home()
            .parent()
            .map(|p| p.join("BacktrailBackups"))
    {
        std::fs::rename(old, &dir).ok();
    }
    std::fs::create_dir_all(&dir).ok()?;
    let name = format!("backtrail-{}.db", &crate::clock::now()[..10]);
    let path = dir.join(&name);
    if path.exists() {
        return None;
    }
    c.execute("VACUUM INTO ?1", params![path.to_string_lossy()])
        .ok()?;
    crate::db::private(&path, 0o600);
    let mut all: Vec<PathBuf> = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "db"))
        .collect();
    all.sort();
    while all.len() > 3 {
        std::fs::remove_file(all.remove(0)).ok();
    }
    crate::db::logw(c, "backup", "backup", &name, "");
    Some(path.display().to_string())
}

fn home_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

fn slug(p: &Path) -> String {
    p.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn mtime(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, ext: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || ["node_modules", "target", "venv"].contains(&name.as_str()) {
            continue;
        }
        if p.is_dir() {
            walk(&p, out, ext);
        } else if p.extension().and_then(|x| x.to_str()) == Some(ext) {
            out.push(p);
        }
    }
}

#[expect(
    clippy::struct_field_names,
    reason = "`ref` is a keyword; `source_ref` matches the name used for source refs across the crate"
)]
struct Source<'a> {
    source_ref: &'a str,
    kind: &'a str,
    title: &'a str,
    project: &'a str,
    thread: Option<&'a str>,
    path: &'a str,
    version: i64,
}

fn begin(c: &Connection, s: &Source<'_>) -> Option<i64> {
    let stored: Option<(i64, i64)> = c
        .query_row(
            "SELECT id, mtime FROM sources WHERE ref=?1",
            [s.source_ref],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    let Some((id, version)) = stored else {
        c.execute(
            "INSERT INTO sources(ref,kind,title,project,thread,path,mtime) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![s.source_ref, s.kind, s.title, s.project, s.thread, s.path, s.version],
        )
        .expect("write to database");
        return Some(c.last_insert_rowid());
    };
    if version == s.version {
        return None;
    }
    c.execute(
        "DELETE FROM fts WHERE rowid IN (SELECT id FROM chunks WHERE source_id=?1)",
        [id],
    )
    .expect("write to database");
    c.execute("DELETE FROM chunks WHERE source_id=?1", [id])
        .expect("write to database");
    c.execute(
        "UPDATE sources SET title=?2, project=?3, thread=?4, mtime=?5, path=?6 WHERE id=?1",
        params![id, s.title, s.project, s.thread, s.version, s.path],
    )
    .expect("write to database");
    Some(id)
}

fn finish(c: &Connection, source_id: i64, first_ts: &str, last_ts: &str) {
    c.execute(
        "UPDATE sources SET nchunks=(SELECT COUNT(*) FROM chunks WHERE source_id=?1), first_ts=?2, last_ts=?3 WHERE id=?1",
        params![source_id, first_ts, last_ts],
    )
    .expect("write to database");
}

fn redact(text: &str) -> std::borrow::Cow<'_, str> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(concat!(
            r"\bsk-[A-Za-z0-9_-]{20,}",
            r"|\bgh[pousr]_[A-Za-z0-9]{36,}",
            r"|\bgithub_pat_[A-Za-z0-9_]{22,}",
            r"|\bAKIA[0-9A-Z]{16}\b",
            r"|\bxox[abprs]-[A-Za-z0-9-]{10,}",
            r"|\bAIza[0-9A-Za-z_-]{35}",
            r"|\beyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}",
            r"|-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
        ))
        .expect("regex literal is valid")
    })
    .replace_all(text, "[redacted secret]")
}

pub fn add_chunk(
    c: &Connection,
    source_id: i64,
    seq: i64,
    locator: &str,
    role: &str,
    ts: &str,
    text: &str,
) -> i64 {
    let text = &*redact(text);
    c.execute(
        "INSERT INTO chunks(source_id,seq,locator,role,ts,text) VALUES(?1,?2,?3,?4,?5,?6)",
        params![source_id, seq, locator, role, ts, text],
    )
    .expect("write to database");
    let id = c.last_insert_rowid();
    c.execute(
        "INSERT INTO fts(rowid,text) VALUES(?1,?2)",
        params![id, text],
    )
    .expect("write to database");
    id
}

fn add_blocks(
    c: &Connection,
    source_id: i64,
    seq: &mut i64,
    locator: &str,
    role: &str,
    text: &str,
    max: usize,
) {
    let chars: Vec<char> = text.chars().collect();
    for part in chars.chunks(max) {
        add_chunk(
            c,
            source_id,
            *seq,
            locator,
            role,
            "",
            &part.iter().collect::<String>(),
        );
        *seq += 1;
    }
}

pub fn relink(c: &Connection) -> (usize, usize) {
    let facts = crate::db::rows(
        c,
        "SELECT cl.id, cl.source_id, cl.quote, cl.chunk_id, k.text FROM claims cl
         LEFT JOIN chunks k ON k.id=cl.chunk_id AND k.source_id=cl.source_id
         WHERE cl.source_id IS NOT NULL AND cl.quote IS NOT NULL",
        &[],
    );
    let (mut traced, mut lost) = (0, 0);
    for f in facts {
        let quote = f["quote"].as_str().unwrap_or("");
        let holds = f["text"]
            .as_str()
            .is_some_and(|t| crate::db::norm(t).contains(&crate::db::norm(quote)));
        if holds {
            traced += 1;
            continue;
        }
        let found = find_quote(c, f["source_id"].as_i64().unwrap_or(0), quote).map(|p| p.chunk_id);
        if found.is_some() {
            traced += 1;
        } else {
            lost += 1;
        }
        if found != f["chunk_id"].as_i64() {
            c.execute(
                "UPDATE claims SET chunk_id=?2 WHERE id=?1",
                params![f["id"].as_str(), found],
            )
            .expect("write to database");
        }
    }
    (traced, lost)
}
