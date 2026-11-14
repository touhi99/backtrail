//! The store: one SQLite file in the data directory. Opening it creates missing tables, runs pending migrations and
//! defines the read views (`sql/views.sql`). Small query helpers used by every module live here too.
use rusqlite::{Connection, ToSql, params};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The data directory: `BACKTRAIL_HOME`, or the platform's application data folder.
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

/// True for the store written by `bt demo`; it is never rescanned or backed up.
pub fn is_demo() -> bool {
    home().join(".demo").exists()
}

pub fn open() -> Connection {
    let dir = home();
    std::fs::create_dir_all(&dir).ok();
    let path = dir.join("backtrail.db");
    let c = Connection::open(&path).expect("open db");
    // The store holds session text. Keep it readable by its owner only; a folder named by BACKTRAIL_HOME may be
    // shared with other files, so only the default folder is closed.
    private(&path, 0o600);
    if std::env::var("BACKTRAIL_HOME").is_err() {
        private(&dir, 0o700);
    }
    if let Err(e) = crate::migrate::run(&c, &dir) {
        eprintln!("migration failed, nothing changed: {e}");
        std::process::exit(1);
    }
    c.execute_batch(include_str!("sql/views.sql"))
        .expect("views");
    c
}

/// Set Unix permission bits on a path; does nothing on other systems.
pub fn private(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).ok();
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
}

/// Every row of a query as a JSON object keyed by column name. Blobs become null.
pub fn rows(c: &Connection, sql: &str, args: &[&dyn ToSql]) -> Vec<Value> {
    use rusqlite::types::Value as Sql;
    let mut st = c.prepare(sql).unwrap_or_else(|e| panic!("{e}: {sql}"));
    let names: Vec<String> = st.column_names().map(|n| n.to_string()).collect();
    st.query_map(args, |r| {
        let mut o = serde_json::Map::new();
        for (i, name) in names.iter().enumerate() {
            let v = match r.get::<_, Sql>(i)? {
                Sql::Integer(n) => Value::from(n),
                Sql::Real(f) => Value::from(f),
                Sql::Text(t) => Value::from(t),
                Sql::Null | Sql::Blob(_) => Value::Null,
            };
            o.insert(name.clone(), v);
        }
        Ok(Value::Object(o))
    })
    .unwrap()
    .flatten()
    .collect()
}

/// The first column of the first row as a number; 0 when there is no row.
pub fn count(c: &Connection, sql: &str, args: &[&dyn ToSql]) -> i64 {
    c.query_row(sql, args, |r| r.get(0)).unwrap_or(0)
}

/// The first column of every row as text; nulls are skipped.
pub fn strings(c: &Connection, sql: &str, args: &[&dyn ToSql]) -> Vec<String> {
    let mut st = c.prepare(sql).unwrap_or_else(|e| panic!("{e}: {sql}"));
    st.query_map(args, |r| r.get::<_, String>(0))
        .unwrap()
        .flatten()
        .collect()
}

/// A text field of a row from `rows`; empty when missing or null.
pub fn s<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key].as_str().unwrap_or("")
}

pub fn thread_exists(c: &Connection, name: &str) -> bool {
    count(c, "SELECT COUNT(*) FROM threads WHERE name=?1", &[&name]) > 0
}

/// Lower case with runs of whitespace collapsed: the form in which quotes, names and aliases are compared.
pub fn norm(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// 32-bit FNV-1a. Entity ids and fact ids from before store version 1 use it.
pub fn fxhash(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

/// 64-bit FNV-1a as 16 hex digits: new fact ids and passage hashes.
pub fn id64(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// Append to the change log: who (owner, claude, ingest, ...) did what to which row.
pub fn logw(c: &Connection, actor: &str, action: &str, target: &str, detail: &str) {
    c.execute(
        "INSERT INTO log(ts,actor,action,target,detail) VALUES(?1,?2,?3,?4,?5)",
        params![crate::clock::now(), actor, action, target, detail],
    )
    .ok();
}

/// A source ref as stored. A session may be named by a unique prefix of its ref (`session:1a2b3c4d` for
/// `session:1a2b3c4d-5e6f-...`), so short refs written in notes and older exports still resolve.
pub fn resolve_ref(c: &Connection, r: &str) -> String {
    let exact = count(c, "SELECT COUNT(*) FROM sources WHERE ref=?1", &[&r]) > 0;
    if exact || !r.starts_with("session:") || r.len() < "session:".len() + 8 {
        return r.to_string();
    }
    let hits = strings(
        c,
        "SELECT ref FROM sources WHERE kind='session' AND substr(ref,1,length(?1))=?1 LIMIT 2",
        &[&r],
    );
    if hits.len() == 1 {
        hits[0].clone()
    } else {
        r.to_string()
    }
}

pub fn source_id(c: &Connection, source_ref: &str) -> Option<i64> {
    c.query_row(
        "SELECT id FROM sources WHERE ref=?1",
        [resolve_ref(c, source_ref)],
        |r| r.get(0),
    )
    .ok()
}

/// The passage of a source that holds a quote.
pub struct Passage {
    pub chunk_id: i64,
    /// `owner`, `assistant` or `unknown`, from the role of the turn.
    pub speaker: &'static str,
    pub ts: Option<String>,
}

/// The first passage of a source that contains `quote` word for word (case and spacing aside). Every fact is filed
/// through this check, and `relink` uses it to follow a quote when a source is read again.
pub fn find_quote(c: &Connection, source_id: i64, quote: &str) -> Option<Passage> {
    let want = norm(quote);
    let mut st = c
        .prepare("SELECT id, ifnull(role,''), ts, text FROM chunks WHERE source_id=?1 ORDER BY seq")
        .unwrap();
    st.query_map([source_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?,
        ))
    })
    .unwrap()
    .flatten()
    .find(|(_, _, _, text)| norm(text).contains(&want))
    .map(|(chunk_id, role, ts, _)| Passage {
        chunk_id,
        speaker: match role.as_str() {
            "user" => "owner",
            "assistant" => "assistant",
            _ => "unknown",
        },
        ts,
    })
}
