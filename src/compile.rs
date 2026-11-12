//! Commands that prepare and record cheap-model compile runs, and that edit the store without raw SQL.
//! The binary never calls a model: `skill/compile.sh` pipes `compile-input` to a headless model and the
//! answer back through `import -`, so the quote and length rule still gates every item.
use crate::db::*;
use rusqlite::{Connection, params};
use serde_json::Value;

fn cfgv() -> Value {
    std::fs::read_to_string(home().join("config.json"))
        .ok()
        .and_then(|r| serde_json::from_str::<Value>(&r).ok())
        .map(|v| v["compile"].clone())
        .unwrap_or(Value::Null)
}
fn num(v: &Value, k: &str, d: i64) -> i64 {
    v[k].as_i64().unwrap_or(d)
}

fn clip(s: &str, n: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() <= n {
        s
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

/// The text a cheap model reads for one session: the thread's existing item titles (to avoid repeats), every owner
/// turn, and a short clip of each assistant turn. Capped; the middle assistant turns are dropped first.
pub fn input(c: &Connection, r: &str) {
    let cap = num(&cfgv(), "max_chars", 24000) as usize;
    let Ok((sid, thread, ts)) = c.query_row("SELECT id, ifnull(thread,''), ifnull(last_ts,'') FROM sources WHERE ref=?1 AND kind='session'", params![r], |x| Ok((x.get::<_, i64>(0)?, x.get::<_, String>(1)?, x.get::<_, String>(2)?))) else { eprintln!("no such session"); std::process::exit(1) };
    println!(
        "BACKTRAIL-COMPILE\nSESSION {r}\nTHREAD {thread}\nDATE {}\n",
        ts.get(..10).unwrap_or("")
    );
    println!("EXISTING ITEMS ON THIS THREAD (do not repeat these):");
    let mut st = c.prepare(include_str!("sql/compile_input.sql")).unwrap();
    for t in st
        .query_map(params![thread], |x| {
            Ok((x.get::<_, String>(0)?, x.get::<_, String>(1)?))
        })
        .unwrap()
        .flatten()
    {
        println!("- {}: {}", t.0, clip(&t.1, 110));
    }
    println!("\nTURNS:");
    let mut q = c
        .prepare("SELECT locator, role, text FROM chunks WHERE source_id=?1 ORDER BY seq")
        .unwrap();
    let mut turns: Vec<(String, bool, String)> = q
        .query_map(params![sid], |x| {
            Ok((
                x.get::<_, String>(0)?,
                x.get::<_, String>(1)? == "user",
                x.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .flatten()
        .map(|(l, u, t)| (l, u, clip(&t, if u { 900 } else { 260 })))
        .collect();
    let size = |t: &Vec<(String, bool, String)>| {
        t.iter()
            .map(|x| x.2.chars().count() + x.0.len() + 12)
            .sum::<usize>()
    };
    while size(&turns) > cap {
        let mid = turns.len() / 2;
        match (0..turns.len())
            .map(|i| (i as i64 - mid as i64).unsigned_abs() as usize)
            .enumerate()
            .filter(|(i, _)| !turns[*i].1)
            .min_by_key(|(_, d)| *d)
        {
            Some((i, _)) => {
                turns.remove(i);
            }
            None => {
                let last = turns.len() - 1;
                turns.remove(last / 2);
            }
        }
    }
    for (l, u, t) in turns {
        println!("[{} · {}] {}", l, if u { "owner" } else { "assistant" }, t);
    }
}

/// Sessions worth a cheap compile: not Unsorted, enough owner turns, nothing filed from them yet, not tried before,
/// and under the daily cap. With a session id only that session is considered.
pub fn queue(c: &Connection, session_id: Option<String>, limit: usize) {
    let v = cfgv();
    let (min_turns, cap) = (num(&v, "min_owner_turns", 4), num(&v, "daily_cap", 8));
    let today = chrono_now()[..10].to_string();
    let used: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM log WHERE action IN ('compile','compile-failed') AND ts LIKE ?1",
            params![format!("{today}%")],
            |x| x.get(0),
        )
        .unwrap_or(0);
    let room = (cap - used).max(0) as usize;
    let only = session_id.map(|s| format!("session:{}", s.chars().take(8).collect::<String>()));
    let mut st = c.prepare(include_str!("sql/compile_queue.sql")).unwrap();
    for r in st
        .query_map(params![min_turns], |x| x.get::<_, String>(0))
        .unwrap()
        .flatten()
        .filter(|r| only.as_ref().map_or(true, |o| o == r))
        .take(limit.min(room))
    {
        println!("{r}");
    }
}

/// Record one compile run (cost is read from the model's JSON output by the script).
pub fn record(c: &Connection, r: &str, ok: bool, detail: &str) {
    logw(
        c,
        "compile",
        if ok { "compile" } else { "compile-failed" },
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
                "INSERT OR IGNORE INTO links(a,b,kind) VALUES(?1,?2,'replaced by')",
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
    let known: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE type='thread' AND thread=?1",
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

/// The packet for relation extraction: every thread name, then the same turns `compile-input` prints.
pub fn relate_input(c: &Connection, r: &str) {
    println!("BACKTRAIL-RELATE\nALL THREADS:");
    let mut st = c
        .prepare(include_str!("sql/compile_relate_input.sql"))
        .unwrap();
    for t in st
        .query_map([], |x| Ok((x.get::<_, String>(0)?, x.get::<_, String>(1)?)))
        .unwrap()
        .flatten()
    {
        println!("- {} :: {}", t.0, clip(&t.1, 90));
    }
    println!();
    input(c, r);
}

/// File one relation between two threads. Kinds read "A <kind> B": spun out of, forked from, built on, replaced by.
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
    let (sid, cid, ts) = relation_evidence(c, a, b, source, quote)?;
    c.execute(
        include_str!("sql/compile_relate.sql"),
        params![
            format!("T:{a}"),
            format!("T:{b}"),
            kind,
            sid,
            cid,
            quote,
            ts,
            state
        ],
    )
    .map_err(|e| e.to_string())?;
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
    crate::kg::migrate_links(c);
    Ok(())
}
