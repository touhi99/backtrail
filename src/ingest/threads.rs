//! Which thread a source belongs to. Threads come from `config.json`; without thread rules there, each Claude Code
//! project folder is its own thread. Sources no rule matches go to `Unsorted`.
use crate::clock::now;
use crate::config::cfg;
use crate::db::{logw, resolve_ref, thread_exists};
use regex::Regex;
use rusqlite::{Connection, params};

pub const UNSORTED: &str = "Unsorted";

const UPSERT_THREAD: &str = "INSERT INTO threads(name,state,area,body,origin,created,updated)
     VALUES(?1,?2,?3,?4,'seed',?5,?5)
     ON CONFLICT(name) DO UPDATE SET area=?3";

fn case_insensitive(pattern: &str) -> Regex {
    // An empty pattern would match everything; `$^` matches nothing.
    Regex::new(&format!(
        "(?i){}",
        if pattern.is_empty() { "$^" } else { pattern }
    ))
    .expect("config regexes are checked when the config loads")
}

/// Create the configured threads, a project entity for each build or plan thread, and `Unsorted`. An existing
/// thread keeps its state and summary.
pub fn seed(c: &Connection) {
    let ts = now();
    for t in &cfg().threads {
        c.execute(
            UPSERT_THREAD,
            params![t.name, t.state, t.area, t.summary, ts],
        )
        .expect("write to database");
        if !t.kind.is_empty() {
            c.execute(
                "UPDATE threads SET kind=?2 WHERE name=?1 AND (kind IS NULL OR kind='')",
                params![t.name, t.kind],
            )
            .ok();
        }
        // A project thread needs its project entity: the extractor and resolver file an item under the thread of
        // its main entity, so without one the items stay on the session's thread.
        if t.kind == "build" || t.kind == "plan" {
            crate::kg::entities::ensure(c, &t.name, "project", &ts);
        }
    }
    c.execute(
        UPSERT_THREAD,
        params![
            UNSORTED,
            "unsorted",
            "Other",
            "Sources no rule matched.",
            ts
        ],
    )
    .expect("write to database");
}

/// The configured thread rules, compiled once per scan.
pub struct SessionRules(Vec<(&'static str, Regex)>);

impl SessionRules {
    pub fn load() -> Self {
        SessionRules(
            cfg()
                .threads
                .iter()
                .map(|t| (t.name.as_str(), case_insensitive(&t.regex)))
                .collect(),
        )
    }

    fn matching(&self, text: &str) -> Option<String> {
        let text = text.to_lowercase();
        self.0
            .iter()
            .find(|(_, re)| re.is_match(&text))
            .map(|(name, _)| name.to_string())
    }

    /// Thread for a session: its title decides first, then its project folder. A short untitled session stays
    /// unsorted.
    pub fn thread_for(&self, project: &str, title: &str, passages: i64) -> String {
        let untitled = title.is_empty() || title.starts_with("Untitled session");
        if untitled && passages <= 3 {
            return UNSORTED.into();
        }
        if !untitled && let Some(name) = self.matching(title) {
            return name;
        }
        if cfg().generic_projects.iter().any(|g| g == project) {
            return UNSORTED.into();
        }
        if let Some(name) = self.matching(project) {
            return name;
        }
        // No thread rules configured: one thread per project folder.
        if self.0.is_empty() && !project.is_empty() {
            return project.into();
        }
        UNSORTED.into()
    }
}

/// Assign every session to its thread again, so a changed rule or a new title takes effect. A session pinned with
/// `bt assign` keeps its thread.
pub fn assign_sessions(c: &Connection) {
    let rules = SessionRules::load();
    let sessions = crate::db::rows(
        c,
        "SELECT s.id, s.project, ifnull(s.title,'') AS title, s.nchunks, o.thread AS pinned
         FROM sources s LEFT JOIN overrides o ON o.ref=s.ref WHERE s.kind='session'",
        &[],
    );
    let ts = now();
    c.execute_batch("BEGIN").expect("write to database");
    for s in sessions {
        let project = s["project"].as_str().unwrap_or("");
        let thread = match s["pinned"].as_str() {
            Some(pinned) => pinned.to_string(),
            None => rules.thread_for(
                project,
                s["title"].as_str().unwrap_or(""),
                s["nchunks"].as_i64().unwrap_or(0),
            ),
        };
        if cfg().threads.is_empty() && thread == project {
            c.execute(
                UPSERT_THREAD,
                params![
                    thread,
                    "open",
                    "Other",
                    "Sessions from this project folder.",
                    ts
                ],
            )
            .expect("write to database");
        }
        c.execute(
            "UPDATE sources SET thread=?2 WHERE id=?1",
            params![s["id"].as_i64(), thread],
        )
        .expect("write to database");
    }
    c.execute_batch("COMMIT").expect("write to database");
}

/// Thread for a note (by its path) or an artifact (by its title): the note or artifact rules when there are any,
/// otherwise the thread rules.
pub fn thread_for_document(kind: &str, key: &str) -> String {
    let key = key.to_lowercase();
    let config = cfg();
    let rules = if kind == "note" {
        &config.note_rules
    } else {
        &config.artifact_rules
    };
    let hit = if rules.is_empty() {
        config
            .threads
            .iter()
            .find(|t| !t.regex.is_empty() && case_insensitive(&t.regex).is_match(&key))
            .map(|t| t.name.clone())
    } else {
        rules
            .iter()
            .find(|(_, re)| case_insensitive(re).is_match(&key))
            .map(|(name, _)| name.clone())
    };
    hit.unwrap_or_else(|| UNSORTED.into())
}

/// Pin a session to a thread (`bt assign`). `assign_sessions` honours the pin, so a rescan does not undo it.
pub fn pin(c: &Connection, session_ref: &str, thread: &str) -> anyhow::Result<bool> {
    if !thread_exists(c, thread) {
        anyhow::bail!("no such thread");
    }
    let session_ref = resolve_ref(c, session_ref);
    let found = c
        .execute(
            "UPDATE sources SET thread=?2 WHERE ref=?1 AND kind='session'",
            params![session_ref, thread],
        )
        .unwrap_or(0)
        == 1;
    if found {
        c.execute(
            "INSERT OR REPLACE INTO overrides(ref,thread) VALUES(?1,?2)",
            params![session_ref, thread],
        )
        .expect("write to database");
        logw(c, "claude", "assign", &session_ref, thread);
    }
    Ok(found)
}

/// `bt suggest-threads`: for each project folder with sessions in `Unsorted`, the thread to assign them to. A thread
/// is suggested when most sorted sessions of that folder are in it, or when its name is part of the folder name.
/// Nothing is changed; each line is the `bt assign` command that applies it.
pub fn suggest(c: &Connection) {
    let projects = crate::db::rows(
        c,
        "SELECT project, COUNT(*) AS n FROM sources WHERE kind='session' AND thread=?1 AND project!='' GROUP BY project ORDER BY n DESC",
        &[&UNSORTED],
    );
    let threads: Vec<String> =
        crate::db::rows(c, "SELECT name FROM threads WHERE name!=?1", &[&UNSORTED])
            .iter()
            .filter_map(|r| r["name"].as_str().map(String::from))
            .collect();
    let flat = |s: &str| -> String {
        s.to_lowercase()
            .chars()
            .filter(|ch| ch.is_alphanumeric())
            .collect()
    };
    let mut shown = 0;
    for p in projects {
        let project = p["project"].as_str().unwrap_or("");
        let n = p["n"].as_i64().unwrap_or(0);
        let by_vote = crate::db::rows(
            c,
            "SELECT thread, COUNT(*) AS n FROM sources WHERE kind='session' AND project=?1 AND thread!=?2 GROUP BY thread ORDER BY n DESC",
            &[&project, &UNSORTED],
        );
        let total: i64 = by_vote.iter().filter_map(|r| r["n"].as_i64()).sum();
        let vote = by_vote.first().and_then(|r| {
            let (t, k) = (r["thread"].as_str()?, r["n"].as_i64()?);
            (total >= 3 && k * 10 >= total * 6)
                .then(|| format!("{k} of {total} sorted sessions of this folder"))
                .map(|why| (t.to_string(), why))
        });
        let named = || {
            threads
                .iter()
                .find(|t| flat(t).len() >= 4 && flat(project).contains(&flat(t)))
                .map(|t| {
                    (
                        t.clone(),
                        "the folder name contains the thread name".to_string(),
                    )
                })
        };
        match vote.or_else(named) {
            Some((t, why)) => {
                shown += 1;
                println!("{project}: {n} unsorted -> {t} ({why})");
                for r in crate::db::rows(
                    c,
                    "SELECT ref FROM sources WHERE kind='session' AND thread=?1 AND project=?2 ORDER BY id",
                    &[&UNSORTED, &project],
                ) {
                    println!("  bt assign {} \"{t}\"", r["ref"].as_str().unwrap_or(""));
                }
            }
            None => println!("{project}: {n} unsorted, no suggestion"),
        }
    }
    if shown == 0 {
        println!("no suggestions");
    }
}
