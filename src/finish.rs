//! Closing open steps from evidence. A model reads a session (at session end) or the passages most like a step (the
//! backlog sweep) and names a step it saw finished, with a quote. `finish` checks the quote against the source and
//! closes it only when a second model confirmed that the passage shows this step finished (`--verified`). Unconfirmed,
//! nothing changes: in a test run the first model named a step for an unrelated line. Commit subjects were tried as
//! corroboration and dropped: on a busy thread some commit always lands within a day, and their embeddings score
//! unrelated commits above the right one.
use crate::db::{Passage, find_quote, logw, rows, s, source_id};
use crate::items;
use rusqlite::{Connection, params};

/// Open and in-progress steps, every thread: a session is often filed under another thread than the step it finishes.
pub fn open_steps(c: &Connection, before: &str) -> Vec<(String, String, String)> {
    rows(
        c,
        "SELECT id, ifnull(thread,'') AS thread, title FROM claims WHERE type='step' AND state IN ('open','doing')
           AND (?1='' OR julianday(created) <= julianday(?1)) ORDER BY created LIMIT 60",
        &[&before],
    )
    .iter()
    .map(|r| (s(r, "id").into(), s(r, "thread").into(), s(r, "title").into()))
    .collect()
}

/// The packet section that lists open steps with their ids (`extract-input`).
pub fn print_open_steps(c: &Connection, before: &str) {
    let steps = open_steps(c, before);
    if steps.is_empty() {
        return;
    }
    println!(
        "\nOPEN STEPS (id [thread] title). File a \"finished\" item only for a step these turns show finished:"
    );
    for (id, thread, title) in steps {
        println!("- {id} [{thread}] {title}");
    }
}

fn clip(t: &str, n: usize) -> String {
    let t = t.trim().replace('\n', " ");
    if t.chars().count() <= n {
        t
    } else {
        format!("{}…", t.chars().take(n - 1).collect::<String>())
    }
}

/// Whether a second model confirmed that the passage shows the step finished (`--verified`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evidence {
    Confirmed,
    Unconfirmed,
}

/// Whether [`finish`] writes the result or only reports it (`--dry`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Run {
    Apply,
    Dry,
}

/// `bt finish <step> --source REF --quote Q [--verified] [--dry]`: what the evidence supports, then (unless dry) the
/// change.
pub fn finish(
    c: &Connection,
    id: &str,
    source: &str,
    quote: &str,
    evidence: Evidence,
    run: Run,
) -> anyhow::Result<String> {
    let step = rows(
        c,
        "SELECT id, ifnull(thread,'') AS thread, title, created, state FROM claims WHERE id=?1 AND type='step'",
        &[&id],
    );
    let Some(step) = step.first() else {
        anyhow::bail!("{id}: no such step");
    };
    if !["open", "doing", "proposed"].contains(&s(step, "state")) {
        anyhow::bail!("{id}: not open (state {})", s(step, "state"));
    }
    if quote.trim().len() < 12 {
        anyhow::bail!("{id}: quote too short to check");
    }
    let Some(sid) = source_id(c, source) else {
        anyhow::bail!("{id}: no such source {source}");
    };
    let Some(Passage { speaker, .. }) = find_quote(c, sid, quote) else {
        anyhow::bail!("{id}: quote not found in {source}");
    };
    let title: String = c
        .query_row(
            "SELECT ifnull(title,'') FROM sources WHERE id=?1",
            params![sid],
            |r| r.get(0),
        )
        .unwrap_or_default();
    if evidence == Evidence::Unconfirmed {
        anyhow::bail!("{id}: not confirmed by a second model; left open");
    }
    let proof = format!(
        "\"{}\" ({}, {}){}",
        clip(quote, 180),
        if speaker == "owner" { "you" } else { "Claude" },
        clip(&title, 60),
        "; confirmed by a second model"
    );
    let verdict = "done";
    if run == Run::Apply {
        items::done(c, id, "evidence", &clip(&proof, 300))?;
        logw(c, "check", "finish", id, &format!("done: {source}"));
    }
    Ok(format!(
        "{verdict}: [{}] {}  <-  {proof}",
        s(step, "thread"),
        s(step, "title")
    ))
}

/// Steps the backlog sweep looks at: open, in progress or suggested, filed more than a day ago.
pub fn queue(c: &Connection) -> Vec<String> {
    crate::db::strings(
        c,
        "SELECT id FROM claims WHERE type='step' AND state IN ('open','doing','proposed') AND julianday('now') - julianday(created) > 1
         ORDER BY created",
        &[],
    )
}

/// `bt finish-input <step>`: the step and the passages written after it that are most like it, for a model to judge.
pub fn input(c: &Connection, id: &str) {
    let step = rows(
        c,
        "SELECT id, ifnull(thread,'') AS thread, title, ifnull(body,'') AS body, created FROM claims WHERE id=?1 AND type='step'",
        &[&id],
    );
    let Some(step) = step.first() else {
        eprintln!("no such step");
        std::process::exit(1)
    };
    println!(
        "BACKTRAIL-FINISH\nSTEP {} [{}] {}\nCREATED {}",
        s(step, "id"),
        s(step, "thread"),
        s(step, "title"),
        &s(step, "created")[..10.min(s(step, "created").len())]
    );
    if !s(step, "body").is_empty() {
        println!("DETAIL {}", s(step, "body"));
    }
    println!("\nPASSAGES (written after the step was filed, most similar first):");
    let created = s(step, "created").to_string();
    let query = format!("{} {}", s(step, "title"), s(step, "body"));
    let ids: Vec<i64> = match crate::embed::nearest(c, &query, 400) {
        Ok(hits) => hits.into_iter().map(|(id, _)| id).collect(),
        Err(_) => crate::db::strings(
            c,
            "SELECT rowid FROM fts WHERE fts MATCH ?1 ORDER BY rank LIMIT 400",
            &[&fts_words(&query)],
        )
        .iter()
        .filter_map(|x| x.parse().ok())
        .collect(),
    };
    let mut shown = 0;
    for cid in ids {
        let r = rows(
            c,
            "SELECT so.ref, so.kind, ch.locator, ifnull(ch.role,'') AS role, ch.text FROM chunks ch JOIN sources so ON so.id=ch.source_id
              WHERE ch.id=?1 AND so.kind IN ('session','note','commit') AND julianday(ch.ts) > julianday(?2)",
            &[&cid, &created],
        );
        let Some(r) = r.first() else { continue };
        let who = match s(r, "role") {
            "user" => "owner",
            "assistant" => "assistant",
            _ => s(r, "kind"),
        };
        println!(
            "\n[{} | {} | {}]\n{}",
            s(r, "ref"),
            s(r, "locator"),
            who,
            clip(s(r, "text"), 700)
        );
        shown += 1;
        if shown == 10 {
            break;
        }
    }
    if shown == 0 {
        println!("(none)");
    }
}

fn fts_words(q: &str) -> String {
    q.split(|ch: char| !ch.is_alphanumeric())
        .filter(|w| w.len() > 3)
        .map(|w| format!("\"{w}\""))
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// `bt finish-evidence <step> --source REF --quote Q`: the step and the whole passage that holds the quote, for the
/// second model that confirms an assistant's report.
pub fn evidence(c: &Connection, id: &str, source: &str, quote: &str) {
    let step = rows(
        c,
        "SELECT title, ifnull(body,'') AS body FROM claims WHERE id=?1 AND type='step'",
        &[&id],
    );
    let Some(step) = step.first() else {
        eprintln!("no such step");
        std::process::exit(1)
    };
    let passage = source_id(c, source)
        .and_then(|sid| find_quote(c, sid, quote))
        .and_then(|p| {
            c.query_row(
                "SELECT text FROM chunks WHERE id=?1",
                params![p.chunk_id],
                |r| r.get::<_, String>(0),
            )
            .ok()
        });
    let Some(text) = passage else {
        eprintln!("quote not found");
        std::process::exit(1)
    };
    println!(
        "STEP {}\nDETAIL {}\n\nQUOTE {}\n\nPASSAGE\n{}",
        s(step, "title"),
        s(step, "body"),
        quote,
        clip(&text, 3000)
    );
}
