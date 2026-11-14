//! Commands that record extraction runs and that edit the store without raw SQL. The binary never calls a model:
//! `skill/extract.sh` pipes `extract-input` to a headless model and the answer back through `import -`, so the
//! quote and length rule still gates every fact.
use crate::db::*;
use rusqlite::{Connection, params};
use serde_json::Value;

pub fn cfgv() -> Value {
    std::fs::read_to_string(home().join("config.json"))
        .ok()
        .and_then(|r| serde_json::from_str::<Value>(&r).ok())
        .map(|v| v["compile"].clone())
        .unwrap_or(Value::Null)
}
pub fn num(v: &Value, k: &str, d: i64) -> i64 {
    v[k].as_i64().unwrap_or(d)
}

/// Record one extraction run (cost is read from the model's JSON output by the script).
pub fn record(c: &Connection, r: &str, ok: bool, detail: &str) {
    let r = &resolve_ref(c, r);
    logw(
        c,
        "extract",
        if ok { "extract" } else { "extract-failed" },
        r,
        detail,
    );
    println!("ok");
}

/// Retire an item without raw SQL. The old item stays, marked rejected with the reason; `--by` links it to its replacement.
pub fn retire(c: &Connection, id: &str, by: Option<String>, reason: &str) {
    let now = chrono_now();
    let n = c
        .execute(
            include_str!("sql/compile_retire.sql"),
            params![id, reason, now],
        )
        .unwrap_or(0);
    if n == 1 {
        if let Some(b) = &by {
            c.execute(
                "UPDATE claims SET superseded_by=?2 WHERE id=?1",
                params![id, b],
            )
            .unwrap();
        }
        logw(
            c,
            "owner",
            "retire",
            id,
            &format!(
                "{reason}{}",
                by.map(|b| format!(" (replaced by {b})"))
                    .unwrap_or_default()
            ),
        );
    }
    println!("{}", if n == 1 { "ok" } else { "no such live item" });
}

/// Pin a session to a thread. `rethread` honours these, so a rescan does not undo them.
pub fn assign(c: &Connection, r: &str, thread: &str) {
    let r = &resolve_ref(c, r);
    let known: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM threads WHERE name=?1",
            params![thread],
            |x| x.get(0),
        )
        .unwrap_or(0);
    if known == 0 {
        eprintln!("rejected: no such thread");
        std::process::exit(1);
    }
    let n = c
        .execute(
            "UPDATE sources SET thread=?2 WHERE ref=?1 AND kind='session'",
            params![r, thread],
        )
        .unwrap_or(0);
    if n == 1 {
        c.execute(
            "INSERT OR REPLACE INTO overrides(ref,thread) VALUES(?1,?2)",
            params![r, thread],
        )
        .unwrap();
        logw(c, "claude", "assign", r, thread);
    }
    println!("{}", if n == 1 { "ok" } else { "no such session" });
}

/// File one relation between two projects. Kinds read "A <kind> B": spun out of, forked from, built on,
/// replaced by. The relation is a claim about the two project entities, with the quote as its evidence.
pub fn relate(
    c: &Connection,
    a: &str,
    b: &str,
    kind: &str,
    source: &str,
    quote: &str,
    state: &str,
) -> Result<(), String> {
    if !["spun out of", "forked from", "built on", "replaced by"].contains(&kind) {
        return Err(format!(
            "kind must be spun out of, forked from, built on or replaced by (got {kind})"
        ));
    }
    relation_evidence(c, a, b, source, quote)?;
    let now = chrono_now();
    crate::kg::entity_for(c, a, "project", &now);
    crate::kg::entity_for(c, b, "project", &now);
    let id = crate::kg::add_claim(
        c,
        &format!("relation:{kind}"),
        &format!("{a} {kind} {b}"),
        "",
        source,
        quote,
        &[a.to_string(), b.to_string()],
        "stated",
        "extracted",
    )?;
    if state != "proposed" {
        c.execute(
            "UPDATE claims SET state=?2, updated=?3 WHERE id=?1",
            params![id, if state == "rejected" { "rejected" } else { "settled" }, chrono_now()],
        )
        .map_err(|e| e.to_string())?;
    }
    logw(
        c,
        if state == "proposed" {
            "claude-suggest"
        } else {
            "claude"
        },
        "relate",
        &format!("{a} {kind} {b}"),
        quote,
    );
    Ok(())
}
