//! Health reports and housekeeping for the command line: `check`, `check --pairs`, `pending`, `match`, `artifacts`,
//! `show` and `log`.
use crate::clock::{days_ago, now};
use crate::db::{count, logw, rows, s};
use crate::items::{COMMON_WORDS, content_words};
use rusqlite::{Connection, params};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

/// `bt check`: facts that lost their quote, sessions not indexed yet, open work in dropped threads, old questions,
/// threads without items, uncited notes, and the graph checks.
pub fn check(c: &Connection) {
    let (traced, lost) = crate::ingest::relink(c);
    println!("claims traced to their quote: {traced}; quote no longer in source: {lost}");
    let unindexed = crate::ingest::sessions::unindexed(c);
    if let Some((_, oldest)) = unindexed.first() {
        println!(
            "session files not indexed yet: {} (oldest {oldest} days; Claude Code removes transcripts after 30 days by default). Run `bt ingest`.",
            unindexed.len()
        );
    }
    for r in rows(
        c,
        "SELECT id, thread, title FROM item WHERE source_id IS NOT NULL AND chunk_id IS NULL",
        &[],
    ) {
        println!(
            "  lost: [{}] {} ({})",
            s(&r, "thread"),
            s(&r, "title"),
            s(&r, "id")
        );
    }
    let in_dropped = rows(
        c,
        "SELECT i.title, i.thread FROM item i JOIN threads t ON t.name=i.thread
         WHERE i.type IN ('step','question') AND i.state='open' AND t.state='dropped'",
        &[],
    );
    for r in &in_dropped {
        println!(
            "  open item in a dropped thread: [{}] {}",
            s(r, "thread"),
            s(r, "title")
        );
    }
    println!(
        "sources in Unsorted: {}",
        count(
            c,
            "SELECT COUNT(*) FROM sources WHERE thread='Unsorted'",
            &[]
        )
    );
    let old_questions = rows(
        c,
        "SELECT created FROM item WHERE type='question' AND state='open' AND origin!='owner'",
        &[],
    )
    .iter()
    .filter(|r| days_ago(s(r, "created")) >= 14)
    .count();
    println!("questions open for 14+ days: {old_questions}");
    let orphans = rows(
        c,
        "SELECT name FROM threads t WHERE t.state NOT IN ('dropped','unsorted')
           AND NOT EXISTS (SELECT 1 FROM item i WHERE i.thread=t.name)",
        &[],
    );
    println!("threads with no filed items (orphans): {}", orphans.len());
    for t in orphans.iter().take(12) {
        println!("  {}", s(t, "name"));
    }
    println!(
        "notes and artifacts cited by no item: {}",
        count(
            c,
            "SELECT COUNT(*) FROM source_stat WHERE kind IN ('note','artifact') AND cited=0",
            &[]
        )
    );
    let plans = rows(
        c,
        "SELECT thread, COUNT(*) AS n, group_concat(title, ' | ') AS titles FROM sources
         WHERE kind IN ('note','artifact') AND thread IS NOT NULL
           AND (lower(title) LIKE '%plan%' OR lower(title) LIKE '%blueprint%' OR lower(title) LIKE '%roadmap%')
         GROUP BY thread HAVING n > 1",
        &[],
    );
    for r in &plans {
        let titles: String = s(r, "titles").chars().take(150).collect();
        println!(
            "  more than one plan source in [{}] ({}): {titles}",
            s(r, "thread"),
            r["n"]
        );
    }
    println!(
        "steps waiting on the owner as 'looks done?': {}",
        count(
            c,
            "SELECT COUNT(*) FROM claims WHERE state='looks_done'",
            &[]
        )
    );
    crate::kg::lint::lint(c);
}

/// `bt check --pairs`: items of one thread that share two or more content words but differ in state or type.
/// A Claude session judges whether one contradicts or replaces the other.
pub fn pairs(c: &Connection) {
    let items = rows(
        c,
        "SELECT thread, type, title, state FROM item WHERE state NOT IN ('rejected','proposed')",
        &[],
    );
    let words: Vec<HashSet<String>> = items.iter().map(|i| content_words(s(i, "title"))).collect();
    let mut n = 0;
    for (i, a) in items.iter().enumerate() {
        for (j, b) in items.iter().enumerate().skip(i + 1) {
            if a["thread"] != b["thread"] || (a["state"] == b["state"] && a["type"] == b["type"]) {
                continue;
            }
            let mut shared: Vec<&String> = words[i].intersection(&words[j]).collect();
            if shared.len() >= 2 {
                shared.sort();
                n += 1;
                println!(
                    "[{}] {} ({} {})  <>  {} ({} {})  shared: {shared:?}",
                    s(a, "thread"),
                    s(a, "title"),
                    s(a, "type"),
                    s(a, "state"),
                    s(b, "title"),
                    s(b, "type"),
                    s(b, "state")
                );
            }
        }
    }
    println!(
        "{n} candidate pairs. Judge each: does one item contradict or supersede the other? Record the answer as a turn or decision."
    );
}

/// `bt pending`: recent sessions in live threads that no item cites yet.
pub fn pending(c: &Connection, days: i64) {
    let sessions = rows(
        c,
        "SELECT s.ref, s.title, s.thread, s.last_ts, s.nchunks,
                (SELECT substr(replace(text,char(10),' '),1,140) FROM chunks
                 WHERE source_id=s.id AND role='user' ORDER BY seq LIMIT 1) AS first
         FROM source_stat s
         WHERE s.kind='session' AND s.thread!='Unsorted' AND s.nchunks>=6 AND s.cited=0 AND s.reviewed=0
         ORDER BY s.last_ts DESC LIMIT 200",
        &[],
    );
    let recent: Vec<&Value> = sessions
        .iter()
        .filter(|r| days_ago(s(r, "last_ts")) <= days)
        .collect();
    for r in &recent {
        println!(
            "{} | {} | {} | {} passages\n    {}",
            s(r, "ref"),
            s(r, "thread"),
            s(r, "title"),
            r["nchunks"],
            s(r, "first")
        );
    }
    println!(
        "{} uncompiled sessions in the last {days} days. Read one with `backtrail show <ref>`, file items with `backtrail add ... --origin suggested --state proposed`.",
        recent.len()
    );
}

/// `bt show`: the passages of one source.
pub fn show(c: &Connection, source_ref: &str) {
    let source_ref = crate::db::resolve_ref(c, source_ref);
    let passages = rows(
        c,
        "SELECT ch.locator, ch.role, substr(ch.text,1,700) AS text FROM chunks ch JOIN sources s ON s.id=ch.source_id
         WHERE s.ref=?1 ORDER BY ch.seq",
        &[&source_ref],
    );
    for p in &passages {
        println!(
            "[{} · {}] {}\n",
            s(p, "locator"),
            s(p, "role"),
            s(p, "text")
        );
    }
}

/// `bt log`: the newest `n` entries of the change log, oldest first.
pub fn log(c: &Connection, n: i64) {
    for r in rows(
        c,
        "SELECT ts, actor, action, target, detail FROM log ORDER BY id DESC LIMIT ?1",
        &[&n],
    )
    .iter()
    .rev()
    {
        println!(
            "## [{}] {} | {} {} | {}",
            s(r, "ts").get(..16).unwrap_or(""),
            s(r, "actor"),
            s(r, "action"),
            s(r, "target"),
            s(r, "detail")
        );
    }
}

/// Mark open steps that a later commit of the thread's repository seems to finish as "looks done?": an ordered
/// two-word phrase from the step's title must appear in the commit subject. A word-overlap heuristic; the owner
/// confirms each one. Steps the owner already answered "not yet" are skipped. Prints what it did.
pub fn match_commits(c: &Connection) {
    let steps = rows(
        c,
        "SELECT id, title, thread, created FROM item i WHERE type='step' AND state='open'
           AND NOT EXISTS (SELECT 1 FROM log l WHERE l.action='not-yet' AND l.target=i.id)",
        &[],
    );
    let (mut compared, mut marked) = (0, 0);
    for step in &steps {
        let title = s(step, "title").to_lowercase();
        let words: Vec<&str> = title
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();
        let usable = |w: &str| w.len() >= 4 && !COMMON_WORDS.contains(&w);
        let phrases: Vec<String> = words
            .windows(2)
            .filter(|pair| usable(pair[0]) && usable(pair[1]))
            .map(|pair| format!("{} {}", pair[0], pair[1]))
            .collect();
        if phrases.is_empty() {
            continue;
        }
        compared += 1;
        let commits = rows(
            c,
            "SELECT ch.text FROM chunks ch JOIN sources s ON s.id=ch.source_id
             WHERE s.kind='commit' AND s.thread=?1 AND ch.locator LIKE 'commit %' AND substr(ch.ts,1,10)>=substr(?2,1,10)",
            &[&s(step, "thread"), &s(step, "created")],
        );
        let finished_by = commits.iter().map(|cm| s(cm, "text")).find(|text| {
            let text = text.to_lowercase();
            phrases.iter().any(|p| text.contains(p.as_str()))
        });
        if let Some(commit) = finished_by {
            let id = s(step, "id");
            c.execute(
                "UPDATE claims SET state='looks_done', done_by='evidence?', answer=?2, updated=?3 WHERE id=?1",
                params![id, commit, now()],
            )
            .expect("write to database");
            logw(c, "check", "looks-done", id, commit);
            println!(
                "looks done? [{}] {}  <-  {commit}",
                s(step, "thread"),
                s(step, "title")
            );
            marked += 1;
        }
    }
    println!(
        "{compared} open steps compared with commits; {marked} marked 'looks done?' (an ordered two-word phrase from the step title must appear in a later commit; the owner confirms)"
    );
}

/// `bt artifacts`: Claude artifact links found in sessions and notes that have no saved snapshot yet. With a
/// manifest, also records publish dates and reports snapshots older than the published version.
pub fn artifacts(c: &Connection, manifest: Option<&str>) {
    if let Some(file) = manifest {
        read_manifest(c, file);
    }
    let link = regex::Regex::new(r"claude\.ai/artifact/([A-Za-z0-9]{20,24})")
        .expect("regex literal is valid");
    let mut mentions: BTreeMap<String, i64> = BTreeMap::new();
    for text in crate::db::strings(
        c,
        "SELECT text FROM chunks WHERE text LIKE '%claude.ai/artifact/%'",
        &[],
    ) {
        for m in link.captures_iter(&text) {
            *mentions.entry(m[1].to_string()).or_insert(0) += 1;
        }
    }
    let saved: HashSet<String> = crate::db::strings(
        c,
        "SELECT substr(ref, 10) FROM sources WHERE kind='artifact'",
        &[],
    )
    .into_iter()
    .collect();
    let missing: Vec<_> = mentions
        .iter()
        .filter(|(id, _)| !saved.contains(*id))
        .collect();
    println!(
        "{} artifact links mentioned, {} already snapshotted, {} missing:",
        mentions.len(),
        mentions.len() - missing.len(),
        missing.len()
    );
    for (id, n) in missing {
        println!("  https://claude.ai/artifact/{id}  ({n} mentions)");
    }
}

/// Reads a listing `[{id,title,updated}]` (from the Artifact tool's list action), records each publish date, and
/// reports which artifacts have no snapshot or a snapshot older than the published version.
fn read_manifest(c: &Connection, file: &str) {
    let listing: Value = std::fs::read_to_string(file)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or(Value::Null);
    let (mut missing, mut stale, mut fresh) = (vec![], vec![], 0);
    for a in listing.as_array().into_iter().flatten() {
        let (id, title, published) = (s(a, "id"), s(a, "title"), s(a, "updated"));
        if id.is_empty() {
            continue;
        }
        c.execute(
            "INSERT INTO artifact_meta(id,title,updated) VALUES(?1,?2,?3)
             ON CONFLICT(id) DO UPDATE SET title=?2, updated=?3",
            params![id, title, published],
        )
        .expect("write to database");
        let source_ref = format!("artifact:{id}");
        let snapshot = rows(
            c,
            "SELECT date(mtime,'unixepoch') AS day FROM sources WHERE ref=?1",
            &[&source_ref],
        );
        let Some(snapshot) = snapshot.first() else {
            missing.push(format!("{id}  {title}"));
            continue;
        };
        let day = s(snapshot, "day");
        if published > day {
            stale.push(format!(
                "{id}  {title} (published {published}, snapshot {day})"
            ));
        } else {
            fresh += 1;
        }
        let ts = format!("{published}T12:00:00Z");
        c.execute(
            "UPDATE sources SET first_ts=?2, last_ts=?2 WHERE ref=?1",
            params![source_ref, ts],
        )
        .ok();
    }
    println!(
        "manifest: {fresh} up to date, {} stale, {} without a snapshot",
        stale.len(),
        missing.len()
    );
    for m in &stale {
        println!("  stale   {m}");
    }
    for m in &missing {
        println!("  missing {m}");
    }
    println!(
        "Re-read these with the Artifact tool (action read, path index.html, out_dir <data dir>/artifacts/<id>), then run `backtrail ingest`."
    );
}
