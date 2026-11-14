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
    .unwrap()
}

/// Create the configured threads and `Unsorted`. An existing thread keeps its state and summary.
pub fn seed(c: &Connection) {
    let ts = now();
    for t in &cfg().threads {
        c.execute(
            UPSERT_THREAD,
            params![t.name, t.state, t.area, t.summary, ts],
        )
        .unwrap();
        if !t.kind.is_empty() {
            c.execute(
                "UPDATE threads SET kind=?2 WHERE name=?1 AND (kind IS NULL OR kind='')",
                params![t.name, t.kind],
            )
            .ok();
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
    .unwrap();
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
        if !untitled {
            if let Some(name) = self.matching(title) {
                return name;
            }
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
    c.execute_batch("BEGIN").unwrap();
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
            .unwrap();
        }
        c.execute(
            "UPDATE sources SET thread=?2 WHERE id=?1",
            params![s["id"].as_i64(), thread],
        )
        .unwrap();
    }
    c.execute_batch("COMMIT").unwrap();
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
pub fn pin(c: &Connection, session_ref: &str, thread: &str) -> Result<bool, String> {
    if !thread_exists(c, thread) {
        return Err("no such thread".into());
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
        .unwrap();
        logw(c, "claude", "assign", &session_ref, thread);
    }
    Ok(found)
}
