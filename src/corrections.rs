use crate::db::{logw, rows, s, sql_int};
use anyhow::Context;
use rusqlite::{Connection, params};
use serde_json::Value;

const TYPED: &str = "FROM chunks u JOIN sources src ON src.id=u.source_id
     WHERE src.kind='session' AND u.role='user' AND substr(u.text,1,1) NOT IN ('<','[')
       AND u.text NOT LIKE 'This session is being continued%'
       AND substr(u.text,1,40) NOT IN (SELECT substr(x.text,1,40) FROM chunks x WHERE x.role='user'
                                       GROUP BY 1 HAVING COUNT(DISTINCT x.source_id) > 3)
       AND EXISTS (SELECT 1 FROM chunks a WHERE a.source_id=u.source_id AND a.role='assistant' AND a.seq<u.seq)";

fn prune(c: &Connection) {
    c.execute(
        &format!("DELETE FROM turn_tags WHERE chunk_id NOT IN (SELECT u.id {TYPED})"),
        [],
    )
    .ok();
}

fn tail(text: &str, max: usize) -> String {
    let n = text.chars().count();
    text.chars().skip(n.saturating_sub(max)).collect()
}

fn mask_keys(text: &str) -> String {
    text.split(' ')
        .map(|w| match w.find("sk-") {
            Some(i) => format!("{}sk-…", &w[..i]),
            None => w.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn head(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

pub const KINDS: [&str; 6] = [
    "misread",
    "unchecked",
    "off_target",
    "overreach",
    "too_much",
    "unresearched",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pass {
    Tags,
    Kinds,
}

pub fn packet(c: &Connection, limit: usize, model: &str, pass: Pass) {
    prune(c);
    let kinds = pass == Pass::Kinds;
    let pick = if kinds {
        "AND EXISTS (SELECT 1 FROM turn_tags t WHERE t.chunk_id=u.id AND t.correction=1 AND t.kind IS NULL)"
    } else {
        "AND NOT EXISTS (SELECT 1 FROM turn_tags t WHERE t.chunk_id=u.id AND t.model=?2)"
    };
    let limit_n = sql_int(limit);
    let turns = rows(
        c,
        &format!(
            "SELECT u.id, u.text AS owner,
                    (SELECT a.text FROM chunks a WHERE a.source_id=u.source_id AND a.role='assistant' AND a.seq<u.seq
                     ORDER BY a.seq DESC LIMIT 1) AS agent
             {TYPED} {pick}
             ORDER BY u.id DESC LIMIT ?1"
        ),
        &if kinds {
            vec![&limit_n as &dyn rusqlite::ToSql]
        } else {
            vec![&limit_n as &dyn rusqlite::ToSql, &model]
        },
    );
    if turns.is_empty() {
        return;
    }
    println!("BACKTRAIL-CORRECTIONS");
    for t in &turns {
        println!(
            "\nTURN {}\nAGENT: {}\nOWNER: {}",
            t["id"],
            tail(s(t, "agent"), 400).replace('\n', " "),
            head(s(t, "owner"), 800).replace('\n', " ")
        );
    }
}

fn apply_kinds(c: &Connection, kinds: &serde_json::Map<String, Value>) -> anyhow::Result<String> {
    let mut n = 0;
    for (id, kind) in kinds {
        let id: i64 = id
            .trim_start_matches(|ch: char| !ch.is_ascii_digit())
            .parse()?;
        let kind = kind.as_str().unwrap_or("");
        if KINDS.contains(&kind) || kind == "unsorted" {
            n += c.execute(
                "UPDATE turn_tags SET kind=?2 WHERE chunk_id=?1 AND correction=1",
                params![id, kind],
            )?;
        }
    }
    logw(c, "corrections", "kinds", "", &format!("{n} kinds"));
    Ok(format!("kinds for {n} corrections"))
}

pub fn apply(c: &Connection, v: &Value) -> anyhow::Result<String> {
    if let Some(kinds) = v["kinds"].as_object() {
        return apply_kinds(c, kinds);
    }
    let ids = |key: &str| -> Vec<i64> {
        v[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| x.as_i64().or_else(|| x.as_str()?.parse().ok()))
            .collect()
    };
    let turns = ids("turns");
    if turns.is_empty() {
        anyhow::bail!("no turns");
    }
    let yes = ids("corrections");
    let model = v["model"].as_str().context("no model")?;
    let now = crate::clock::now();
    c.execute_batch("BEGIN")?;
    for id in &turns {
        c.execute(
            "INSERT OR REPLACE INTO turn_tags(chunk_id, correction, model, ts) VALUES(?1,?2,?3,?4)",
            params![id, yes.contains(id), model, now],
        )?;
    }
    c.execute_batch("COMMIT")?;
    let hit = turns.iter().filter(|t| yes.contains(t)).count();
    logw(
        c,
        "corrections",
        "tagged",
        model,
        &format!("{} turns, {hit} corrections", turns.len()),
    );
    Ok(format!("tagged {} turns, {hit} corrections", turns.len()))
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

pub fn report(c: &Connection, weeks: usize) {
    prune(c);
    let left = crate::db::count(
        c,
        &format!("SELECT COUNT(*) {TYPED} AND u.id NOT IN (SELECT chunk_id FROM turn_tags)"),
        &[],
    );
    let by_week = rows(
        c,
        "SELECT date(u.ts, 'weekday 0', '-6 days') AS week, COUNT(*) AS turns, SUM(t.correction) AS corrections
         FROM turn_tags t JOIN chunks u ON u.id=t.chunk_id
         GROUP BY week ORDER BY week DESC LIMIT ?1",
        &[&sql_int(weeks)],
    );
    println!("week        turns  corrections  rate");
    for w in by_week.iter().rev() {
        let turns = w["turns"].as_i64().unwrap_or(0);
        let hit = w["corrections"].as_i64().unwrap_or(0);
        println!(
            "{}  {turns:>5}  {hit:>11}  {:>3}%",
            s(w, "week"),
            (hit * 100).checked_div(turns).unwrap_or(0)
        );
    }
    if left > 0 {
        println!("{left} turns not tagged yet; skill/corrections.sh tags them");
    }
}

pub fn view(c: &Connection) -> Value {
    prune(c);
    let weeks: Vec<Value> = rows(
        c,
        "SELECT date(u.ts, 'weekday 0', '-6 days') AS week, COUNT(*) AS turns, SUM(t.correction) AS corrections
         FROM turn_tags t JOIN chunks u ON u.id=t.chunk_id
         GROUP BY week ORDER BY week DESC LIMIT 8",
        &[],
    )
    .into_iter()
    .rev()
    .map(|mut w| {
        let by_kind = rows(
            c,
            "SELECT t.kind, COUNT(*) AS n FROM turn_tags t JOIN chunks u ON u.id=t.chunk_id
             WHERE t.correction=1 AND t.kind IS NOT NULL AND date(u.ts, 'weekday 0', '-6 days')=?1 GROUP BY t.kind",
            &[&s(&w, "week").to_string()],
        );
        w["kinds"] = by_kind
            .iter()
            .map(|k| (s(k, "kind").to_string(), k["n"].clone()))
            .collect::<serde_json::Map<_, _>>()
            .into();
        w
    })
    .collect();
    let since = weeks
        .first()
        .map_or(String::new(), |w| s(w, "week").to_string());
    let kinds: Vec<Value> = KINDS
        .iter()
        .map(|kind| {
            let n = crate::db::count(
                c,
                "SELECT COUNT(*) FROM turn_tags t JOIN chunks u ON u.id=t.chunk_id WHERE t.kind=?1 AND u.ts >= ?2",
                &[kind, &since],
            );
            serde_json::json!({"kind": kind, "n": n})
        })
        .collect();
    let threads = rows(
        c,
        "SELECT ifnull(src.thread,'Unsorted') AS thread, COUNT(*) AS turns, SUM(t.correction) AS corrections
         FROM turn_tags t JOIN chunks u ON u.id=t.chunk_id JOIN sources src ON src.id=u.source_id
         WHERE u.ts >= ?1 AND ifnull(src.thread,'Unsorted') <> 'Unsorted'
         GROUP BY 1 HAVING COUNT(*) >= 15 ORDER BY COUNT(*) DESC LIMIT 12",
        &[&since],
    );
    let untagged = crate::db::count(
        c,
        &format!("SELECT COUNT(*) {TYPED} AND u.id NOT IN (SELECT chunk_id FROM turn_tags)"),
        &[],
    );
    serde_json::json!({"weeks": weeks, "kinds": kinds, "threads": threads, "untagged": untagged})
}

pub fn messages(c: &Connection, kind: &str, thread: &str, week: &str) -> Value {
    let since = rows(
        c,
        "SELECT date(u.ts, 'weekday 0', '-6 days') AS week FROM turn_tags t JOIN chunks u ON u.id=t.chunk_id
         GROUP BY week ORDER BY week DESC LIMIT 1 OFFSET 7",
        &[],
    )
    .first()
    .map_or(String::new(), |w| s(w, "week").to_string());
    let filter =
        "FROM turn_tags t JOIN chunks u ON u.id=t.chunk_id JOIN sources src ON src.id=u.source_id
         WHERE t.correction=1 AND u.ts >= ?1
           AND (?2 = '' OR t.kind = ?2)
           AND (?3 = '' OR ifnull(src.thread,'Unsorted') = ?3)
           AND (?4 = '' OR date(u.ts, 'weekday 0', '-6 days') = ?4)";
    let args: [&dyn rusqlite::ToSql; 4] = [&since, &kind, &thread, &week];
    let n = crate::db::count(c, &format!("SELECT COUNT(*) {filter}"), &args);
    let items: Vec<Value> = rows(
        c,
        &format!(
            "SELECT u.id AS chunk_id, u.source_id, ifnull(src.thread,'Unsorted') AS thread, t.kind, u.ts, u.text
             {filter} ORDER BY u.ts DESC LIMIT 60"
        ),
        &args,
    )
    .into_iter()
    .map(|q| {
        serde_json::json!({"chunk_id": q["chunk_id"], "source_id": q["source_id"], "thread": q["thread"],
                           "kind": q["kind"], "ts": q["ts"], "text": mask_keys(&head(s(&q, "text"), 280))})
    })
    .collect();
    serde_json::json!({"n": n, "items": items})
}
