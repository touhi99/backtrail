use crate::clock::now;
use crate::db::{home, rows, s};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::io::Read;

const MAX_SHOWN: usize = 2;
const MIN_SHARED: usize = 2;
const MIN_SCORE: f64 = 9.0;
const MIN_SCORE_IN_PROJECT: f64 = 7.0;

fn stems(text: &str) -> HashSet<String> {
    crate::items::content_words(text)
        .into_iter()
        .map(|w| w.chars().take(6).collect())
        .collect()
}

pub struct Hit {
    pub id: String,
    pub title: String,
    pub quote: String,
    pub day: String,
    pub score: f64,
}

const TOP: &str = "COALESCE(NULLIF((SELECT parent FROM threads WHERE name=thread),''), thread)";

fn top(c: &Connection, thread: &str) -> String {
    rows(
        c,
        &format!("SELECT {TOP} AS top FROM (SELECT ?1 AS thread)"),
        &[&thread],
    )
    .first()
    .map_or_else(|| thread.to_string(), |r| s(r, "top").to_string())
}

pub fn scope(c: &Connection, session: &str, cwd: &str) -> Option<String> {
    let usable = |t: &str| {
        !t.is_empty()
            && crate::db::count(
                c,
                "SELECT COUNT(*) FROM threads WHERE name=?1 AND IFNULL(kind,'') NOT IN ('','discussion','idea')",
                &[&t],
            ) > 0
    };
    let own = rows(
        c,
        "SELECT IFNULL(thread,'') AS thread FROM sources WHERE ref=?1",
        &[&format!("session:{session}")],
    );
    if let Some(t) = own.first().map(|r| top(c, s(r, "thread")))
        && usable(&t)
    {
        return Some(t);
    }
    if cwd.is_empty() {
        return None;
    }
    let folder: String = cwd
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect();
    let split = rows(
        c,
        &format!(
            "SELECT {TOP} AS thread, COUNT(*) AS n FROM sources WHERE kind='session' AND path LIKE ?1 AND thread IS NOT NULL
             GROUP BY 1 ORDER BY n DESC"
        ),
        &[&format!("%/{folder}/%")],
    );
    let total: i64 = split.iter().map(|r| r["n"].as_i64().unwrap_or(0)).sum();
    let top = split.first()?;
    let t = s(top, "thread").to_string();
    (top["n"].as_i64().unwrap_or(0) * 5 >= total * 4 && usable(&t)).then_some(t)
}

pub fn matches(c: &Connection, prompt: &str, scope: Option<&str>) -> Vec<Hit> {
    let scope = scope.map(|t| top(c, t));
    let words = stems(prompt);
    if words.len() < 2 {
        return vec![];
    }
    let named: HashSet<String> = crate::kg::entities::mentions(c, prompt)
        .into_iter()
        .map(|(id, _)| id)
        .filter(|id| {
            crate::db::count(
                c,
                "SELECT COUNT(*) FROM entities WHERE id=?1 AND kind='project'",
                &[id],
            ) > 0
        })
        .collect();
    let today = now()[..10].to_string();
    let decisions = rows(
        c,
        &format!("SELECT c.id, {TOP} AS thread, c.title, IFNULL(c.quote,'') AS quote, substr(IFNULL(c.asserted_at, c.created),1,10) AS day,
                (SELECT group_concat(a.entity_id, char(31)) FROM claim_about a WHERE a.claim_id=c.id) AS ents
         FROM claims c
         WHERE c.type='decision' AND c.state='settled' AND IFNULL(c.superseded_by,'')=''
           AND IFNULL(c.stale,'') NOT LIKE 'superseded%' AND (IFNULL(c.valid_to,'')='' OR c.valid_to>=?1)"),
        &[&today],
    );
    let docs: Vec<HashSet<String>> = decisions.iter().map(|d| stems(s(d, "title"))).collect();
    let mut df: HashMap<&str, usize> = HashMap::new();
    for doc in &docs {
        for w in doc {
            *df.entry(w.as_str()).or_default() += 1;
        }
    }
    let mut entity_df: HashMap<&str, usize> = HashMap::new();
    for d in &decisions {
        for e in s(d, "ents").split('\u{1f}').filter(|e| !e.is_empty()) {
            *entity_df.entry(e).or_default() += 1;
        }
    }
    let n = decisions.len().max(1) as f64;
    let idf = |count: usize| (n / count.max(1) as f64).ln();
    let mut hits: Vec<Hit> = decisions
        .iter()
        .zip(&docs)
        .filter_map(|(d, doc)| {
            let shared: Vec<&String> = doc.intersection(&words).collect();
            if shared.len() < MIN_SHARED {
                return None;
            }
            let mut score: f64 = shared
                .iter()
                .map(|w| idf(df.get(w.as_str()).copied().unwrap_or(1)))
                .sum();
            let same_subject = s(d, "ents")
                .split('\u{1f}')
                .filter(|e| named.contains(*e))
                .map(|e| idf(entity_df.get(e).copied().unwrap_or(1)))
                .fold(0.0, f64::max);
            score += same_subject;
            let in_scope = scope.as_deref() == Some(s(d, "thread"));
            if same_subject == 0.0 && !in_scope {
                return None;
            }
            let floor = if same_subject == 0.0 {
                MIN_SCORE_IN_PROJECT
            } else {
                MIN_SCORE
            };
            (score >= floor).then(|| Hit {
                id: s(d, "id").to_string(),
                title: s(d, "title").to_string(),
                quote: s(d, "quote").to_string(),
                day: s(d, "day").to_string(),
                score,
            })
        })
        .collect();
    hits.sort_by(|a, b| b.score.total_cmp(&a.score));
    hits
}

fn shown_path(session: &str) -> Option<std::path::PathBuf> {
    let safe: String = session
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .collect();
    (!safe.is_empty()).then(|| home().join("recall").join(safe))
}

pub fn hook(c: &Connection, harness: Option<crate::hooks::Harness>) {
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw).ok();
    let input: Value = serde_json::from_str(&raw).unwrap_or_default();
    let field = |keys: &[&str]| {
        keys.iter()
            .find_map(|k| input[*k].as_str())
            .or_else(|| {
                input["workspacePaths"][0]
                    .as_str()
                    .filter(|_| keys.contains(&"cwd"))
            })
            .unwrap_or("")
            .to_string()
    };
    let prompt = field(&["prompt", "lastUserInput"]);
    let session = field(&["session_id", "conversationId"]);
    let cwd = field(&["cwd"]);
    let (prompt, session, cwd) = (prompt.as_str(), session.as_str(), cwd.as_str());
    let path = shown_path(session);
    let seen: HashSet<String> = path
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| t.lines().map(String::from).collect())
        .unwrap_or_default();
    let fresh: Vec<Hit> = matches(c, prompt, scope(c, session, cwd).as_deref())
        .into_iter()
        .filter(|h| !seen.contains(&h.id))
        .take(MAX_SHOWN)
        .collect();
    if fresh.is_empty() {
        return;
    }
    let mut text = String::from(
        "Backtrail: settled decisions that may bear on this request (cite or follow them; ask before reversing one):\n",
    );
    for h in &fresh {
        let _ = writeln!(text, "- {} ({}) — said: \"{}\"", h.title, h.day, h.quote);
    }
    crate::hooks::emit(harness, crate::hooks::prompt_event(harness), &text);
    if let Some(p) = path {
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let ids: Vec<&str> = seen
            .iter()
            .map(String::as_str)
            .chain(fresh.iter().map(|h| h.id.as_str()))
            .collect();
        std::fs::write(p, ids.join("\n")).ok();
    }
}
