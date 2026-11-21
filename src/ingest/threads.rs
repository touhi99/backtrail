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
    Regex::new(&format!(
        "(?i){}",
        if pattern.is_empty() { "$^" } else { pattern }
    ))
    .expect("config regexes are checked when the config loads")
}

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
        let kind = if t.kind == "build" || t.kind == "plan" {
            "project"
        } else {
            "topic"
        };
        crate::kg::entities::ensure(c, &t.name, kind, &ts);
    }
    c.execute(
        "UPDATE threads SET parent=NULL WHERE parent IS NOT NULL",
        [],
    )
    .expect("write to database");
    for t in cfg().threads.iter().filter(|t| !t.parent.is_empty()) {
        c.execute(
            "UPDATE threads SET parent=?2 WHERE name=?1",
            params![t.name, t.parent],
        )
        .expect("write to database");
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
        if self.0.is_empty() && !project.is_empty() {
            return project.into();
        }
        UNSORTED.into()
    }
}

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
