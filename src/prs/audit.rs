use super::{Home, PROOF_MARK, describe, homes, opened_in};
use crate::db::{rows, s};
use regex::Regex;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::LazyLock;

static FULL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"github\.com/([\w.-]+/[\w.-]+)/pull/(\d+)").expect("regex"));
static NAMED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b([a-z][\w.-]*)\s?#(\d+)\b").expect("regex"));
static BARE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bPR\s+#?(\d+)\b").expect("regex"));
static MERGED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\bmerged\b").expect("regex"));
static NEGATED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bnot (yet )?merged\b|\bunmerged\b|without merging|(before|until|once|when|after) (it is |it's |[^.;]{0,40}is )?merged")
        .expect("regex")
});
static DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(20\d\d-\d\d-\d\d)\b").expect("regex"));

fn counts(c: &Connection) {
    let r = rows(
        c,
        "SELECT COUNT(*) AS n,
                SUM(state='MERGED') AS merged, SUM(state='CLOSED') AS closed, SUM(state='OPEN') AS open,
                SUM(state IS NULL) AS unchecked
         FROM (SELECT DISTINCT repo, number, state FROM session_pr)",
        &[],
    );
    let Some(r) = r.first() else { return };
    let n = |k: &str| r[k].as_i64().unwrap_or(0);
    println!(
        "pull requests linked from sessions: {} ({} merged, {} closed without merging, {} open, {} unchecked)",
        n("n"),
        n("merged"),
        n("closed"),
        n("open"),
        n("unchecked")
    );
    for f in rows(
        c,
        "SELECT repo, error FROM pr_repos WHERE error IS NOT NULL ORDER BY repo",
        &[],
    ) {
        println!("  unchecked repo {}: {}", s(&f, "repo"), s(&f, "error"));
    }
}

fn closing_session(c: &Connection, id: &str) -> Option<String> {
    let re = Regex::new(r"session:[0-9a-f-]{36}").expect("regex");
    rows(
        c,
        "SELECT detail FROM log WHERE target=?1 AND action IN ('finish','built','done') ORDER BY id DESC",
        &[&id],
    )
    .iter()
    .find_map(|r| re.find(s(r, "detail")).map(|m| m.as_str().to_string()))
}

fn landed_later(c: &Connection, p: &Value) -> bool {
    crate::db::count(
        c,
        "SELECT COUNT(*) FROM pr_state g JOIN pr_state x ON x.repo=g.repo AND x.head=g.head
          WHERE g.repo=?1 AND g.number=?2 AND x.state='MERGED'",
        &[&s(p, "repo"), &p["number"].as_i64().unwrap_or(0)],
    ) > 0
}

fn doubtful_steps(c: &Connection, homes: &[Home]) {
    let mut doubtful = vec![];
    let mut backed = 0;
    for st in rows(
        c,
        "SELECT id, ifnull(thread,'') AS thread, title FROM claims
          WHERE type='step' AND state IN ('done','looks_done') ORDER BY updated",
        &[],
    ) {
        let Some(source) = closing_session(c, s(&st, "id")) else {
            continue;
        };
        let prs = opened_in(c, homes, &source, s(&st, "thread"));
        if prs.is_empty() {
            continue;
        }
        if prs
            .iter()
            .any(|p| matches!(s(p, "state"), "MERGED" | "OPEN" | "") || landed_later(c, p))
        {
            backed += usize::from(prs.iter().any(|p| s(p, "state") == "MERGED"));
            continue;
        }
        doubtful.push((st, prs));
    }
    println!(
        "done steps whose closing session opened a PR: {} merged, {} with every PR closed without merging",
        backed,
        doubtful.len()
    );
    for (st, prs) in doubtful {
        let named: Vec<String> = prs.iter().map(describe).collect();
        println!(
            "  doubtful: [{}] {} ({}): {}",
            s(&st, "thread"),
            s(&st, "title"),
            s(&st, "id"),
            named.join(", ")
        );
    }
}

fn candidates(
    c: &Connection,
    homes: &[Home],
    claim: &Value,
    hint: Option<&str>,
    number: i64,
) -> BTreeSet<String> {
    let found: BTreeSet<String> = rows(
        c,
        "SELECT DISTINCT p.repo FROM source_prs p JOIN sources so ON so.id=p.source_id
          WHERE p.number=?1 AND (so.thread=?2 OR so.id=?3)",
        &[
            &number,
            &s(claim, "thread"),
            &claim["source_id"].as_i64().unwrap_or(0),
        ],
    )
    .iter()
    .map(|r| s(r, "repo").to_string())
    .collect();
    match hint {
        Some(h)
            if homes.iter().any(|x| {
                x.folder.eq_ignore_ascii_case(h)
                    || x.repo
                        .rsplit('/')
                        .next()
                        .is_some_and(|n| n.eq_ignore_ascii_case(h))
            }) =>
        {
            homes
                .iter()
                .filter(|x| {
                    x.folder.eq_ignore_ascii_case(h)
                        || x.repo
                            .rsplit('/')
                            .next()
                            .is_some_and(|n| n.eq_ignore_ascii_case(h))
                })
                .map(|x| x.repo.clone())
                .collect()
        }
        Some(h) => {
            let named: BTreeSet<String> = crate::db::strings(
                c,
                "SELECT DISTINCT repo FROM pr_state WHERE lower(repo) LIKE '%/' || lower(?1) AND number=?2",
                &[&h, &number],
            )
            .into_iter()
            .collect();
            if named.is_empty() { found } else { named }
        }
        None => found,
    }
}

fn mentions(text: &str) -> Vec<(Option<String>, Option<String>, i64)> {
    let mut out = vec![];
    for m in FULL.captures_iter(text) {
        out.push((Some(m[1].to_string()), None, m[2].parse().unwrap_or(0)));
    }
    for m in NAMED.captures_iter(text) {
        let hint = m[1].to_string();
        if hint.eq_ignore_ascii_case("pr") || hint.eq_ignore_ascii_case("pull") {
            out.push((None, None, m[2].parse().unwrap_or(0)));
        } else {
            out.push((None, Some(hint), m[2].parse().unwrap_or(0)));
        }
    }
    for m in BARE.captures_iter(text) {
        out.push((None, None, m[1].parse().unwrap_or(0)));
    }
    out
}

fn pr(c: &Connection, repo: &str, number: i64) -> Option<Value> {
    rows(
        c,
        "SELECT repo, number, state, substr(merged_at,1,10) AS merged_on FROM pr_state WHERE repo=?1 AND number=?2",
        &[&repo, &number],
    )
    .into_iter()
    .next()
}

fn near(a: &str, b: &str) -> bool {
    let day = |x: &str| crate::clock::days_ago(&format!("{x}T12:00:00Z"));
    (day(a) - day(b)).abs() <= 1
}

fn merge_claims(c: &Connection, homes: &[Home]) {
    let (mut said, mut agree, mut unknown) = (0, 0, 0);
    let mut wrong = vec![];
    for cl in rows(
        c,
        "SELECT id, ifnull(thread,'') AS thread, title, source_id,
                title || ' ' || ifnull(body,'') || ' ' || ifnull(answer,'') AS text
         FROM claims WHERE state NOT IN ('rejected','superseded') AND type NOT LIKE 'relation:%'",
        &[],
    ) {
        let text = s(&cl, "text").split(PROOF_MARK).next().unwrap_or("");
        if !MERGED.is_match(text) || NEGATED.is_match(text) {
            continue;
        }
        let refs = mentions(text);
        if refs.is_empty() {
            continue;
        }
        said += 1;
        let mut found = vec![];
        for (full, hint, number) in refs {
            let repos = match full {
                Some(r) => BTreeSet::from([r]),
                None => candidates(c, homes, &cl, hint.as_deref(), number),
            };
            if repos.len() == 1
                && let Some(p) = repos.first().and_then(|r| pr(c, r, number))
                && !found.contains(&p)
            {
                found.push(p);
            }
        }
        if found.is_empty() {
            unknown += 1;
            continue;
        }
        let merged: Vec<&Value> = found.iter().filter(|p| s(p, "state") == "MERGED").collect();
        if merged.is_empty() {
            wrong.push((cl.clone(), found.iter().map(describe).collect::<Vec<_>>()));
            continue;
        }
        let dates: Vec<&str> = DATE.find_iter(text).map(|m| m.as_str()).collect();
        if !dates.is_empty() && merged.len() == 1 {
            let on = s(merged[0], "merged_on");
            if !dates.iter().any(|d| near(d, on)) {
                wrong.push((cl.clone(), vec![describe(merged[0])]));
                continue;
            }
        }
        agree += 1;
    }
    println!(
        "claims that say a PR merged: {said}; GitHub agrees: {agree}; disagrees: {}; PR not identified: {unknown}",
        wrong.len()
    );
    for (cl, states) in wrong {
        println!(
            "  disagrees: [{}] {} ({}): GitHub says {}",
            s(&cl, "thread"),
            s(&cl, "title"),
            s(&cl, "id"),
            states.join(", ")
        );
    }
}

pub fn report(c: &Connection) {
    counts(c);
    let homes = homes(c);
    doubtful_steps(c, &homes);
    merge_claims(c, &homes);
}
