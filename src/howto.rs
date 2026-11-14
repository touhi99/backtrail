//! Backfill of `check_how` for steps the owner must confirm by hand. The binary lists the steps and records the
//! text; `skill/howto.sh` pipes each packet to a headless model.
use crate::db::{logw, rows, s, strings};
use rusqlite::{Connection, params};
use serde_json::Value;

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// Steps waiting on the owner (open, in progress or built) with no check text yet and none ruled out.
pub fn queue(c: &Connection, limit: usize, again: bool) {
    for id in strings(
        c,
        "SELECT id FROM claims WHERE type='step' AND state IN ('open','doing','looks_done') AND check_how IS NULL
           AND (?2 OR NOT EXISTS (SELECT 1 FROM log l WHERE l.target=claims.id AND l.action='howto-checked'))
         ORDER BY updated DESC LIMIT ?1",
        &[&(limit as i64), &again],
    ) {
        println!("{id}");
    }
}

/// What the model reads for one step: the step, what was built, the passage it came from and the thread's summary.
pub fn packet(c: &Connection, id: &str) {
    let me = rows(
        c,
        "SELECT c.thread, c.title, ifnull(c.body,'') AS body, ifnull(c.answer,'') AS answer, c.state,
                ifnull(c.quote,'') AS quote, ifnull((SELECT text FROM chunks WHERE id=c.chunk_id),'') AS passage,
                ifnull((SELECT body FROM threads WHERE name=c.thread),'') AS summary
         FROM claims c WHERE c.id=?1 AND c.type='step'",
        &[&id],
    );
    let Some(me) = me.first() else {
        eprintln!("no such step");
        std::process::exit(1)
    };
    println!(
        "BACKTRAIL-HOWTO\nITEM {id}\nTHREAD {}\nTHREAD SUMMARY {}\nSTATE {}\nTITLE {}\nBODY {}\nWHAT WAS BUILT {}\nQUOTE {}\n\nSOURCE PASSAGE:\n{}",
        s(me, "thread"),
        s(me, "summary"),
        s(me, "state"),
        s(me, "title"),
        s(me, "body"),
        s(me, "answer"),
        s(me, "quote"),
        clip(&s(me, "passage"), 2500)
    );
}

/// Apply `{"id", "check"}`. An empty check records that the step needs no hand check, so it is not asked again.
pub fn apply(c: &Connection, v: &Value) -> Result<String, String> {
    let id = v["id"].as_str().ok_or("no id")?;
    let check = clip(v["check"].as_str().unwrap_or("").trim(), 300);
    if check.is_empty() {
        logw(c, "howto", "howto-checked", id, "no hand check");
        return Ok("no hand check needed".into());
    }
    let n = c
        .execute(
            "UPDATE claims SET check_how=?2 WHERE id=?1 AND type='step' AND check_how IS NULL",
            params![id, check],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("not a step without a check".into());
    }
    logw(c, "howto", "howto", id, &check);
    Ok(format!("check: {check}"))
}

pub fn apply_cli(c: &Connection) {
    let mut raw = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut raw).ok();
    match serde_json::from_str::<Value>(&raw).map_err(|e| e.to_string()).and_then(|v| apply(c, &v)) {
        Ok(m) => println!("{m}"),
        Err(e) => {
            eprintln!("rejected: {e}");
            std::process::exit(1)
        }
    }
}
