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
    let path = dir.join("backtrail.db");
    let c = Connection::open(&path).expect("open db");
    // The store holds session text. Keep it readable by its owner only; a folder named by BACKTRAIL_HOME may be
    // shared with other files, so only the default folder is closed.
    private(&path, 0o600);
    if std::env::var("BACKTRAIL_HOME").is_err() {
        private(&dir, 0o700);
    }
    // A store from before items and claims were merged still has `nodes`. Its claims table is renamed so the
    // new schema can be created beside it; `unify` copies everything across.
    let old_layout = has_table(&c, "nodes");
    if old_layout {
        let backup = dir.join("backtrail.pre-unify.db");
        if !backup.exists() {
            c.execute("VACUUM INTO ?1", params![backup.to_string_lossy()])
                .expect("backup before migration");
        }
        if has_table(&c, "claims") && !has_table(&c, "claims_old") {
            c.execute_batch("ALTER TABLE claims RENAME TO claims_old; DROP INDEX IF EXISTS claims_type;")
                .expect("rename old claims");
        }
    }
    c.execute_batch(include_str!("sql/schema.sql"))
        .expect("schema");
    crate::embed::ensure(&c);
    crate::kg::ensure(&c);
    if old_layout {
        if let Err(e) = unify(&c) {
            eprintln!("migration failed, nothing changed: {e}");
            std::process::exit(1);
        }
    }
    let version: i64 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap_or(0);
    if version < 1 {
        if let Err(e) = migrate_v1(&c, &dir) {
            eprintln!("migration to store version 1 failed, nothing changed: {e}");
            std::process::exit(1);
        }
    }
    c
}

/// Store version 1:
/// - columns for append-only session reading (`sources.bytes`, `tail`, `nturns`) and for the keep rule
///   (`claims.review`);
/// - session refs carry the whole session id (`session:<uuid>`) instead of its first 8 characters, in `sources`,
///   `overrides` and the log;
/// - the keep rule runs once over every proposed fact.
///
/// The old file is copied to `backtrail.pre-v1.db` first, and the migration runs in one transaction.
fn migrate_v1(c: &Connection, dir: &std::path::Path) -> Result<(), String> {
    let has_sources = has_table(c, "sources")
        && c.query_row("SELECT COUNT(*) FROM sources", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0)
            > 0;
    if has_sources {
        let backup = dir.join("backtrail.pre-v1.db");
        if !backup.exists() {
            c.execute("VACUUM INTO ?1", params![backup.to_string_lossy()])
                .map_err(|e| format!("backup: {e}"))?;
            private(&backup, 0o600);
        }
    }
    for sql in [
        "ALTER TABLE sources ADD COLUMN bytes INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE sources ADD COLUMN tail TEXT",
        "ALTER TABLE sources ADD COLUMN nturns INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE claims ADD COLUMN review TEXT",
    ] {
        if let Err(e) = c.execute(sql, []) {
            if !e.to_string().contains("duplicate column") {
                return Err(e.to_string());
            }
        }
    }
    let refs: Vec<(i64, String, String)> = c
        .prepare("SELECT id, ref, ifnull(path,'') FROM sources WHERE kind='session'")
        .map_err(|e| e.to_string())?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    c.execute_batch("BEGIN").map_err(|e| e.to_string())?;
    let run = || -> Result<usize, String> {
        for (id, old, path) in &refs {
            let Some(stem) = std::path::Path::new(path).file_stem() else {
                continue;
            };
            let new = format!("session:{}", stem.to_string_lossy());
            if &new == old {
                continue;
            }
            c.execute("UPDATE sources SET ref=?2 WHERE id=?1", params![id, new])
                .map_err(|e| format!("{old}: {e}"))?;
            c.execute("UPDATE overrides SET ref=?2 WHERE ref=?1", params![old, new])
                .map_err(|e| e.to_string())?;
            c.execute("UPDATE log SET target=?2 WHERE target=?1", params![old, new])
                .map_err(|e| e.to_string())?;
        }
        Ok(keep_all(c))
    };
    match run() {
        Ok(kept) => {
            c.execute_batch("PRAGMA user_version=1; COMMIT")
                .map_err(|e| e.to_string())?;
            if kept > 0 {
                logw(c, "migrate", "v1", "claims", &format!("{kept} proposed facts kept by the rule"));
            }
            Ok(())
        }
        Err(e) => {
            c.execute_batch("ROLLBACK").ok();
            Err(e)
        }
    }
}

/// A session ref as stored. Accepts the stored ref or a unique prefix of it (`session:1a2b3c4d` for
/// `session:1a2b3c4d-5e6f-...`), so short refs written in notes and older exports still resolve.
pub fn resolve_ref(c: &Connection, r: &str) -> String {
    let exact: i64 = c
        .query_row("SELECT COUNT(*) FROM sources WHERE ref=?1", params![r], |x| x.get(0))
        .unwrap_or(0);
    if exact > 0 || !r.starts_with("session:") || r.len() < "session:".len() + 8 {
        return r.to_string();
    }
    let hits: Vec<String> = c
        .prepare("SELECT ref FROM sources WHERE kind='session' AND substr(ref,1,length(?1))=?1 LIMIT 2")
        .and_then(|mut s| {
            s.query_map(params![r], |x| x.get(0))
                .map(|m| m.flatten().collect())
        })
        .unwrap_or_default();
    if hits.len() == 1 {
        hits[0].clone()
    } else {
        r.to_string()
    }
}

/// Content words of a title, for the overlap test in the keep rule.
fn words(t: &str) -> std::collections::HashSet<String> {
    t.to_lowercase()
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|w| w.len() >= 5 && !crate::server::STOP.contains(w))
        .map(String::from)
        .collect()
}

/// Kinds of fact the keep rule may keep without review.
const KEEP_TYPES: &[&str] = &["decision", "rationale", "state"];

/// The keep rule. A proposed fact is kept (decisions, rationale and state claims become `settled`, `review='auto'`)
/// when the owner said it in so many words: the quote passed the write rule, comes from a turn the owner typed, and
/// is marked `stated`. It stays proposed, with `review='held|<reason>'`, when an earlier fact of the same type on the
/// same thread shares three or more content words with it, since it may repeat or change that fact. Steps, questions,
/// relations and the assistant's words always wait for the owner. Returns true when the fact was kept.
pub fn keep(c: &Connection, id: &str) -> bool {
    let row = c.query_row(
        "SELECT c.type, ifnull(c.thread,''), c.title, ifnull(c.speaker,''), c.stated, ifnull(c.asserted_at,''),
                ifnull(k.role,''), ifnull(k.text,''), ifnull(c.stale,'')
         FROM claims c LEFT JOIN chunks k ON k.id=c.chunk_id
         WHERE c.id=?1 AND c.state='proposed'",
        params![id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
            ))
        },
    );
    let Ok((typ, thread, title, speaker, stated, at, role, text, stale)) = row else {
        return false;
    };
    let typed = role == "user" && !non_typed(&text);
    if !KEEP_TYPES.contains(&typ.as_str()) || speaker != "owner" || stated != "stated" || !typed {
        return false;
    }
    let mine = words(&title);
    let mut st = c
        .prepare(
            "SELECT id, title FROM claims WHERE type=?1 AND ifnull(thread,'')=?2 AND id!=?3 AND state!='rejected'
               AND ifnull(asserted_at,'')<=?4",
        )
        .unwrap();
    let earlier = st
        .query_map(params![typ, thread, id, at], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .unwrap()
        .flatten()
        .find(|(_, t)| mine.intersection(&words(t)).count() >= 3);
    let now = chrono_now();
    if let Some((other, t)) = earlier {
        let why: String = format!("held|may repeat or change {other}: {t}")
            .chars()
            .take(200)
            .collect();
        c.execute("UPDATE claims SET review=?2 WHERE id=?1", params![id, why])
            .ok();
        return false;
    }
    // A Claude flag that only said "still live" or "already in effect" no longer matters once the fact is kept;
    // a "superseded" flag stays, so the kept fact shows as replaced.
    let stale = if stale.starts_with("superseded") { Some(stale) } else { None };
    let n = c
        .execute(
            "UPDATE claims SET state='settled', review='auto', stale=?2, updated=?3 WHERE id=?1 AND state='proposed'",
            params![id, stale, now],
        )
        .unwrap_or(0);
    if n == 1 {
        logw(c, "auto", "keep", id, &title);
    }
    n == 1
}

/// Turns stored with the user role that the owner did not type: a summary written when a session was continued,
/// command output and notifications that Claude Code inserts as user turns.
pub fn non_typed(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("This session is being continued")
        || t.starts_with('<')
        || t.starts_with("[Request interrupted")
}

/// Run the keep rule over every proposed fact, oldest first, so a fact is only compared with facts said before it.
fn keep_all(c: &Connection) -> usize {
    let ids: Vec<String> = c
        .prepare("SELECT id FROM claims WHERE state='proposed' ORDER BY ifnull(asserted_at,''), id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .flatten()
        .collect();
    ids.iter().filter(|id| keep(c, id)).count()
}

/// Set Unix permission bits on a path; does nothing on other systems.
pub fn private(path: &std::path::Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).ok();
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
}

fn has_table(c: &Connection, name: &str) -> bool {
    c.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        params![name],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
        > 0
}

/// One-time move from the old layout (items in `nodes`, graph in `claims` and `claim_evidence`, relations in
/// `links`) to one `claims` table plus `threads`. Item ids are kept, an item and the claim copied from it become
/// one row, and thread relations without a quote become relation claims by the owner. Runs in one transaction
/// and checks the row count before it commits.
fn unify(c: &Connection) -> Result<(), String> {
    let n = |sql: &str| -> i64 { c.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };
    let items = n("SELECT COUNT(*) FROM nodes WHERE type!='thread'");
    let threads = n("SELECT COUNT(*) FROM nodes WHERE type='thread'");
    let loose = n("SELECT COUNT(*) FROM claims_old WHERE id NOT IN (SELECT 'c-'||id FROM nodes WHERE type!='thread')");
    let bare: Vec<(String, String, String)> = {
        let mut st = c
            .prepare("SELECT a,b,kind FROM links WHERE a LIKE 'T:%' AND b LIKE 'T:%' AND (source_id IS NULL OR quote IS NULL)")
            .map_err(|e| e.to_string())?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| e.to_string())?
            .flatten()
            .collect()
    };
    c.execute_batch("BEGIN").map_err(|e| e.to_string())?;
    let run = || -> Result<(), String> {
        c.execute_batch(include_str!("sql/unify.sql"))
            .map_err(|e| e.to_string())?;
        let now = chrono_now();
        for (a, b, kind) in &bare {
            crate::kg::owner_relation(c, &a[2..], &b[2..], kind, &now);
        }
        let want = items + loose + bare.len() as i64;
        let have = n("SELECT COUNT(*) FROM claims");
        if have != want {
            return Err(format!("expected {want} rows in claims, found {have}"));
        }
        if n("SELECT COUNT(*) FROM threads") != threads {
            return Err("thread count differs".into());
        }
        c.execute_batch(include_str!("sql/unify_2.sql"))
            .map_err(|e| e.to_string())?;
        Ok(())
    };
    match run() {
        Ok(()) => {
            c.execute_batch("COMMIT").map_err(|e| e.to_string())?;
            logw(
                c,
                "migrate",
                "unify",
                "claims",
                &format!("{items} items, {loose} graph-only claims, {} bare relations, {threads} threads", bare.len()),
            );
            Ok(())
        }
        Err(e) => {
            c.execute_batch("ROLLBACK").ok();
            Err(e)
        }
    }
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
            "SELECT COUNT(*) FROM threads WHERE name=?1",
            params![it.thread],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if known == 0 {
        return Err(format!("unknown thread '{}'", it.thread));
    }
    let source = resolve_ref(c, &it.source);
    let sid: i64 = c
        .query_row(
            "SELECT id FROM sources WHERE ref=?1",
            params![source],
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
        .prepare("SELECT id, text, role, ts FROM chunks WHERE source_id=?1 ORDER BY seq")
        .unwrap();
    let rows = stmt
        .query_map(params![sid], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })
        .unwrap();
    let mut chunk = None;
    for r in rows.flatten() {
        if norm(&r.1).contains(&q) {
            chunk = Some((r.0, r.2, r.3));
            break;
        }
    }
    let (chunk, role, ts) =
        chunk.ok_or_else(|| format!("quote not found in {}: \"{}\"", it.source, it.quote))?;
    let speaker = match role.as_deref() {
        Some("user") => "owner",
        Some("assistant") => "assistant",
        _ => "unknown",
    };
    let now = it.date.clone().unwrap_or_else(|| chrono_now());
    // The same thread, type and title is the same item: filing it again updates its text and quote. Otherwise it gets
    // a new 64-bit id; a clash with a different item is refused instead of overwriting it.
    let existing: Option<String> = c
        .query_row(
            "SELECT id FROM claims WHERE thread=?1 AND type=?2 AND title=?3",
            params![it.thread, it.typ, it.title],
            |r| r.get(0),
        )
        .ok();
    let fresh = existing.is_none();
    let id = existing.unwrap_or_else(|| {
        format!(
            "{}-{}",
            &it.typ[..1],
            id64(&format!("{}{}{}", it.thread, it.typ, it.title))
        )
    });
    if fresh && taken(c, &id) {
        return Err(format!("id {id} is already used by another item"));
    }
    let state = it.state.clone().unwrap_or_else(|| match it.typ.as_str() {
        "step" | "question" => "open".into(),
        _ => "settled".into(),
    });
    c.execute(
        include_str!("sql/db_add_item.sql"),
        params![
            id, it.typ, it.thread, it.title, it.body, state, it.origin, sid, chunk, it.quote, now, speaker, ts
        ],
    )
    .map_err(|e| e.to_string())?;
    if it.typ == "decision" {
        crate::kg::link_entities(c, &id, &it.thread, &format!("{} {} {}", it.title, it.body, it.quote));
    }
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
    if fresh && state == "proposed" {
        keep(c, &id);
    }
    Ok(id)
}

fn taken(c: &Connection, id: &str) -> bool {
    c.query_row("SELECT COUNT(*) FROM claims WHERE id=?1", params![id], |r| {
        r.get::<_, i64>(0)
    })
    .unwrap_or(0)
        > 0
}

/// 64-bit FNV-1a as 16 hex digits, for new fact ids.
pub fn id64(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
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
                "SELECT COUNT(*) FROM threads WHERE name=?1",
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
            params![resolve_ref(c, source)],
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
