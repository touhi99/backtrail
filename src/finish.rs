use crate::db::{Passage, find_quote, logw, rows, s, source_id};
use crate::items;
use rusqlite::{Connection, params};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evidence {
    Confirmed,
    Unconfirmed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Run {
    Apply,
    Dry,
}

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
        "SELECT id, type, ifnull(thread,'') AS thread, title, created, state FROM claims
          WHERE id=?1 AND type IN ('step','question','decision')",
        &[&id],
    );
    let Some(step) = step.first() else {
        anyhow::bail!("{id}: no such step, question or decision");
    };
    let open_states: &[&str] = match s(step, "type") {
        "question" => &["answered"],
        "decision" => &["proposed"],
        _ => &["open", "doing", "proposed"],
    };
    if !open_states.contains(&s(step, "state")) {
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
        match s(step, "type") {
            "question" => {
                c.execute(
                    "UPDATE claims SET state='done', updated=?2 WHERE id=?1",
                    params![id, crate::clock::now()],
                )?;
                logw(
                    c,
                    "check",
                    "finish",
                    id,
                    &format!("done: {source}: {}", clip(&proof, 240)),
                );
            }
            "decision" => {
                items::flag(c, id, "done", &clip(&proof, 200))?;
                logw(c, "check", "finish", id, &format!("done: {source}"));
            }
            _ => {
                items::done(c, id, "evidence", &clip(&proof, 300))?;
                logw(c, "check", "finish", id, &format!("done: {source}"));
            }
        }
    }
    Ok(format!(
        "{verdict}: [{}] {}  <-  {proof}",
        s(step, "thread"),
        s(step, "title")
    ))
}

pub fn queue(c: &Connection) -> Vec<String> {
    crate::db::strings(
        c,
        "SELECT id FROM claims WHERE julianday('now') - julianday(created) > 1
           AND ((type='step' AND state IN ('open','doing','proposed'))
             OR (type='question' AND state='answered')
             OR (type='decision' AND state='proposed'))
           AND NOT EXISTS (SELECT 1 FROM log l WHERE l.target=claims.id AND l.action='finish-checked'
                           AND julianday('now') - julianday(l.ts) < 7)
         ORDER BY created",
        &[],
    )
}

pub fn input(c: &Connection, id: &str) {
    let step = rows(
        c,
        "SELECT id, type, ifnull(thread,'') AS thread, title, ifnull(body,'') AS body, ifnull(answer,'') AS answer, created
         FROM claims WHERE id=?1 AND type IN ('step','question','decision')",
        &[&id],
    );
    let Some(step) = step.first() else {
        eprintln!("no such step, question or decision");
        std::process::exit(1)
    };
    logw(c, "check", "finish-checked", id, "");
    println!(
        "BACKTRAIL-FINISH\n{} {} [{}] {}\nCREATED {}",
        s(step, "type").to_uppercase(),
        s(step, "id"),
        s(step, "thread"),
        s(step, "title"),
        &s(step, "created")[..10.min(s(step, "created").len())]
    );
    if !s(step, "body").is_empty() {
        println!("DETAIL {}", s(step, "body"));
    }
    if s(step, "type") == "question" && !s(step, "answer").is_empty() {
        println!("ANSWER GIVEN {}", s(step, "answer"));
    }
    println!("\nPASSAGES (written after the item was filed, most similar first):");
    let created = s(step, "created").to_string();
    let query = format!(
        "{} {} {}",
        s(step, "title"),
        s(step, "body"),
        s(step, "answer")
    );
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

pub fn evidence(c: &Connection, id: &str, source: &str, quote: &str) {
    let step = rows(
        c,
        "SELECT type, title, ifnull(body,'') AS body, ifnull(answer,'') AS answer FROM claims
          WHERE id=?1 AND type IN ('step','question','decision')",
        &[&id],
    );
    let Some(step) = step.first() else {
        eprintln!("no such step, question or decision");
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
        "{} {}\nDETAIL {}\nANSWER GIVEN {}\n\nQUOTE {}\n\nPASSAGE\n{}",
        s(step, "type").to_uppercase(),
        s(step, "title"),
        s(step, "body"),
        s(step, "answer"),
        quote,
        clip(&text, 3000)
    );
}
