use crate::db::{count, logw, rows, s, strings};
use anyhow::Context;
use rusqlite::{Connection, params};
use serde_json::Value;

const DOC_DAY: &str = "substr(COALESCE(NULLIF(d.last_ts,''), datetime(d.mtime,'unixepoch')),1,10)";

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Again {
    Skip,
    Include,
}

pub fn queue(c: &Connection, limit: usize, again: Again) {
    for r in strings(
        c,
        "SELECT d.ref FROM sources d LEFT JOIN doc_stale k ON k.source_id=d.id
         WHERE d.kind IN ('note','artifact') AND IFNULL(d.thread,'') NOT IN ('','Unsorted')
           AND (?2 OR k.source_id IS NULL OR IFNULL(k.mtime,0)!=IFNULL(d.mtime,0)
                OR EXISTS (SELECT 1 FROM claims x WHERE x.thread=d.thread AND x.updated>k.checked)
                OR EXISTS (SELECT 1 FROM sources n WHERE n.thread=d.thread AND n.kind IN ('note','artifact')
                             AND n.id!=d.id AND datetime(n.mtime,'unixepoch')>k.checked))
         ORDER BY d.mtime DESC LIMIT ?1",
        &[&crate::db::sql_int(limit), &(again == Again::Include)],
    ) {
        println!("{r}");
    }
}

pub fn packet(c: &Connection, doc_ref: &str) {
    let me = rows(
        c,
        &format!(
            "SELECT d.id, d.kind, IFNULL(d.title,'') AS title, d.thread, {DOC_DAY} AS day,
                    IFNULL((SELECT group_concat(text, char(10)) FROM (SELECT text FROM chunks WHERE source_id=d.id ORDER BY seq)),'') AS text
             FROM sources d WHERE d.ref=?1 AND d.kind IN ('note','artifact')"
        ),
        &[&doc_ref],
    );
    let Some(me) = me.first() else {
        eprintln!("no such note or artifact");
        std::process::exit(1)
    };
    let (thread, day) = (s(me, "thread"), s(me, "day"));
    println!(
        "BACKTRAIL-STALE\nDOC {doc_ref}\nKIND {}\nTITLE {}\nTHREAD {thread}\nDATED {day}\n\nDOC TEXT:\n{}\n",
        s(me, "kind"),
        s(me, "title"),
        clip(s(me, "text"), 7000)
    );
    println!("STORE ITEMS ON THE THREAD CHANGED ON OR AFTER {day}:");
    for r in rows(
        c,
        "SELECT id, type, state, substr(updated,1,10) AS day, title, IFNULL(superseded_by,'') AS by,
                IFNULL(stale,'') AS stale
         FROM claims WHERE thread=?1 AND type IN ('decision','step')
           AND state IN ('settled','done','superseded','rejected','parked','answered')
           AND NOT (state='superseded' AND IFNULL(answer,'') LIKE 'parked:%')
           AND substr(updated,1,10)>=?2
         ORDER BY updated DESC LIMIT 50",
        &[&thread, &day],
    ) {
        let note = match (s(&r, "by"), s(&r, "stale")) {
            ("", x) if x.starts_with("superseded") => " (superseded)".to_string(),
            ("", _) => String::new(),
            (by, _) => format!(" (replaced by {by})"),
        };
        println!(
            "  {} · {} · {} · {} · {}{note}",
            s(&r, "id"),
            s(&r, "day"),
            s(&r, "type"),
            s(&r, "state"),
            s(&r, "title")
        );
    }
    println!("\nNEWER NOTES AND ARTIFACTS ON THE THREAD:");
    for r in rows(
        c,
        &format!(
            "SELECT d.ref, IFNULL(d.title,'') AS title, {DOC_DAY} AS day,
                    IFNULL((SELECT text FROM chunks WHERE source_id=d.id ORDER BY seq LIMIT 1),'') AS head
             FROM sources d WHERE d.thread=?1 AND d.kind IN ('note','artifact') AND d.id!=?2
               AND {DOC_DAY}>?3 ORDER BY day DESC LIMIT 8"
        ),
        &[&thread, &me["id"].as_i64(), &day],
    ) {
        println!(
            "  {} · {} · {}\n    {}",
            s(&r, "ref"),
            s(&r, "day"),
            s(&r, "title"),
            clip(&s(&r, "head").replace('\n', " "), 400)
        );
    }
}

fn known(c: &Connection, id: &str) -> bool {
    count(
        c,
        "SELECT (SELECT COUNT(*) FROM claims WHERE id=?1) + (SELECT COUNT(*) FROM sources WHERE ref=?1 AND kind IN ('note','artifact'))",
        &[&id],
    ) > 0
}

pub fn apply(c: &Connection, v: &Value) -> anyhow::Result<String> {
    let doc_ref = v["id"].as_str().context("no id")?;
    let stale = v["stale"].as_bool().context("no stale verdict")?;
    let reason = clip(v["reason"].as_str().unwrap_or("").trim(), 300);
    let by: Vec<&str> = v["by"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|b| known(c, b))
        .collect();
    if stale && (by.is_empty() || reason.is_empty()) {
        anyhow::bail!("a stale verdict needs a reason and an item or newer doc from the packet");
    }
    let n = c.execute(
        "INSERT INTO doc_stale(source_id, stale, reason, by_refs, mtime, checked)
         SELECT id, ?2, ?3, ?4, mtime, ?5 FROM sources WHERE ref=?1 AND kind IN ('note','artifact')
         ON CONFLICT(source_id) DO UPDATE SET stale=?2, reason=?3, by_refs=?4, mtime=excluded.mtime, checked=?5",
        params![
            doc_ref,
            stale,
            reason,
            by.join(" "),
            crate::clock::now()
        ],
    )?;
    if n == 0 {
        anyhow::bail!("not a note or artifact");
    }
    let verdict = if stale { "stale" } else { "current" };
    logw(c, "stale", verdict, doc_ref, &reason);
    Ok(format!("{verdict}: {reason}"))
}

pub fn apply_cli(c: &Connection) {
    let mut raw = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut raw).ok();
    match serde_json::from_str::<Value>(&raw)
        .map_err(anyhow::Error::from)
        .and_then(|v| apply(c, &v))
    {
        Ok(m) => println!("{m}"),
        Err(e) => {
            eprintln!("rejected: {e}");
            std::process::exit(1)
        }
    }
}

pub fn summary(c: &Connection) {
    let n = |q: &str| count(c, q, &[]);
    println!(
        "notes and artifacts: {} marked stale, {} current, {} not checked (skill/stale.sh; list with `bt stale`)",
        n("SELECT COUNT(*) FROM doc_stale WHERE stale=1"),
        n("SELECT COUNT(*) FROM doc_stale WHERE stale=0"),
        n(
            "SELECT COUNT(*) FROM sources d WHERE d.kind IN ('note','artifact') AND IFNULL(d.thread,'') NOT IN ('','Unsorted')
               AND NOT EXISTS (SELECT 1 FROM doc_stale k WHERE k.source_id=d.id)"
        )
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    Stale,
    All,
}

pub fn report(c: &Connection, show: Show) {
    summary(c);
    for r in rows(
        c,
        "SELECT d.ref, d.thread, IFNULL(d.title,'') AS title, k.stale, k.reason, k.by_refs, substr(k.checked,1,10) AS day
         FROM doc_stale k JOIN sources d ON d.id=k.source_id
         WHERE ?1 OR k.stale=1 ORDER BY k.stale DESC, d.thread, d.ref",
        &[&(show == Show::All)],
    ) {
        let mark = if r["stale"].as_i64() == Some(1) {
            "stale  "
        } else {
            "current"
        };
        println!(
            "  {mark} [{}] {} ({})\n          {} · by {} · checked {}",
            s(&r, "thread"),
            s(&r, "title"),
            s(&r, "ref"),
            s(&r, "reason"),
            s(&r, "by_refs"),
            s(&r, "day")
        );
    }
}
