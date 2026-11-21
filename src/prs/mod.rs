pub mod audit;

use crate::clock::now;
use crate::db::{rows, s, strings};
use rusqlite::{Connection, params};
use serde_json::Value;
use std::process::Command;

const FIELDS: &str = "number,title,state,createdAt,mergedAt,closedAt,headRefName,baseRefName";

#[derive(Debug, Default)]
pub struct Refresh {
    pub read: Vec<(String, usize)>,
    pub failed: Vec<(String, String)>,
}

fn due(c: &Connection) -> Vec<String> {
    strings(
        c,
        "SELECT DISTINCT p.repo FROM source_prs p LEFT JOIN pr_repos r ON r.repo=p.repo
          WHERE r.repo IS NULL
             OR (julianday('now') - julianday(r.checked_at) > 1.0/24
                 AND (r.error IS NOT NULL
                      OR EXISTS (SELECT 1 FROM source_prs q LEFT JOIN pr_state g ON g.repo=q.repo AND g.number=q.number
                                  WHERE q.repo=p.repo AND (g.state IS NULL OR g.state='OPEN'))))
          ORDER BY p.repo",
        &[],
    )
}

fn list(repo: &str) -> Result<Vec<Value>, String> {
    let out = Command::new("gh")
        .args([
            "pr", "list", "-R", repo, "--state", "all", "--limit", "2000", "--json", FIELDS,
        ])
        .output()
        .map_err(|e| format!("gh: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(err.lines().next().unwrap_or("gh failed").trim().to_string());
    }
    serde_json::from_slice::<Vec<Value>>(&out.stdout).map_err(|e| format!("gh output: {e}"))
}

fn store(c: &Connection, repo: &str, prs: &[Value]) {
    let at = now();
    let text = |v: &Value, k: &str| v[k].as_str().filter(|x| !x.is_empty()).map(str::to_string);
    for p in prs {
        c.execute(
            "INSERT INTO pr_state(repo, number, title, state, created_at, merged_at, closed_at, head, base, checked_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
             ON CONFLICT(repo, number) DO UPDATE SET title=?3, state=?4, created_at=?5, merged_at=?6, closed_at=?7,
               head=?8, base=?9, checked_at=?10",
            params![
                repo,
                p["number"].as_i64().unwrap_or(0),
                text(p, "title"),
                text(p, "state").unwrap_or_default(),
                text(p, "createdAt"),
                text(p, "mergedAt"),
                text(p, "closedAt"),
                text(p, "headRefName"),
                text(p, "baseRefName"),
                at
            ],
        )
        .expect("write to database");
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repos {
    All,
    Due,
}

pub fn refresh(c: &Connection, which: Repos) -> Refresh {
    let repos = if which == Repos::All {
        strings(c, "SELECT DISTINCT repo FROM source_prs ORDER BY repo", &[])
    } else {
        due(c)
    };
    let mut report = Refresh::default();
    for repo in repos {
        let result = list(&repo);
        let error = result.as_ref().err().cloned();
        if let Ok(prs) = &result {
            store(c, &repo, prs);
            report.read.push((repo.clone(), prs.len()));
        }
        c.execute(
            "INSERT INTO pr_repos(repo, checked_at, error) VALUES(?1,?2,?3)
             ON CONFLICT(repo) DO UPDATE SET checked_at=?2, error=?3",
            params![repo, now(), error],
        )
        .expect("write to database");
        if let Some(e) = error {
            report.failed.push((repo, e));
        }
    }
    report
}

pub fn describe(p: &Value) -> String {
    let name = format!(
        "{}#{}",
        s(p, "repo").rsplit('/').next().unwrap_or(""),
        p["number"].as_i64().unwrap_or(0)
    );
    match s(p, "state") {
        "MERGED" => format!("{name} merged {}", s(p, "merged_on")),
        "CLOSED" => format!("{name} closed without merging"),
        "OPEN" => format!("{name} still open"),
        _ => format!("{name} merge state unchecked"),
    }
}

pub const PROOF_MARK: &str = "; PR opened in that session";

#[derive(Debug)]
pub struct Home {
    pub repo: String,
    pub folder: String,
    pub thread: String,
}

fn github_repo(url: &str) -> Option<String> {
    let rest = url.trim().split("github.com").nth(1)?;
    let path = rest.trim_start_matches([':', '/']).trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    (path.split('/').count() == 2).then(|| path.to_string())
}

fn configured() -> &'static [Home] {
    static HOMES: std::sync::OnceLock<Vec<Home>> = std::sync::OnceLock::new();
    HOMES.get_or_init(|| {
        let conf = crate::config::cfg();
        conf.repos
            .iter()
            .filter_map(|(rel, thread)| {
                let out = Command::new("git")
                    .arg("-C")
                    .arg(conf.workspace.join(rel))
                    .args(["remote", "get-url", "origin"])
                    .output()
                    .ok()?;
                Some(Home {
                    repo: github_repo(&String::from_utf8_lossy(&out.stdout))?,
                    folder: rel.rsplit('/').next().unwrap_or(rel).to_string(),
                    thread: thread.clone(),
                })
            })
            .collect()
    })
}

fn squash(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

fn guess(short: &str) -> Option<String> {
    let rules = &crate::config::cfg().threads;
    if let Some(t) = rules.iter().find(|t| squash(&t.name) == squash(short)) {
        return Some(t.name.clone());
    }
    let hits: Vec<&String> = rules
        .iter()
        .filter(|t| t.kind == "build" && !t.regex.is_empty())
        .filter(|t| {
            regex::Regex::new(&format!("(?i){}", t.regex)).is_ok_and(|re| re.is_match(short))
        })
        .map(|t| &t.name)
        .collect();
    match hits.as_slice() {
        [one] => Some((*one).clone()),
        _ => None,
    }
}

pub fn homes(c: &Connection) -> Vec<Home> {
    let mut all: Vec<Home> = configured()
        .iter()
        .map(|h| Home {
            repo: h.repo.clone(),
            folder: h.folder.clone(),
            thread: h.thread.clone(),
        })
        .collect();
    for repo in strings(c, "SELECT DISTINCT repo FROM source_prs ORDER BY repo", &[]) {
        if all.iter().any(|h| h.repo == repo) {
            continue;
        }
        let short = repo.rsplit('/').next().unwrap_or(&repo).to_string();
        if let Some(thread) = guess(&short) {
            all.push(Home {
                repo,
                folder: short,
                thread,
            });
        }
    }
    all
}

fn family(c: &Connection, thread: &str) -> Vec<String> {
    strings(
        c,
        "SELECT name FROM threads WHERE name=?1 OR parent=?1
            OR name=(SELECT parent FROM threads WHERE name=?1)
            OR parent=(SELECT parent FROM threads WHERE name=?1)",
        &[&thread],
    )
}

pub fn opened_in(c: &Connection, homes: &[Home], source_ref: &str, thread: &str) -> Vec<Value> {
    let family = family(c, thread);
    rows(
        c,
        "SELECT repo, number, state, merged_on FROM session_pr
          WHERE source=?1 AND opened_here=1 GROUP BY repo, number ORDER BY ts",
        &[&source_ref],
    )
    .into_iter()
    .filter(|p| {
        homes
            .iter()
            .any(|h| h.repo == s(p, "repo") && family.contains(&h.thread))
    })
    .collect()
}

pub fn proof_note(c: &Connection, source_ref: &str, thread: &str) -> String {
    let prs = opened_in(c, &homes(c), source_ref, thread);
    if prs.is_empty() {
        return String::new();
    }
    let named: Vec<String> = prs.iter().take(3).map(describe).collect();
    let more = prs.len().saturating_sub(3);
    format!(
        "{PROOF_MARK} for this project, state when the step closed: {}{}",
        named.join(", "),
        if more > 0 {
            format!(" (+{more} more)")
        } else {
            String::new()
        }
    )
}
