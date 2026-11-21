use crate::clock::now;
use crate::db::{count, logw, rows, s, strings};
use anyhow::Context;
use rusqlite::{Connection, params};
use serde_json::Value;

use super::candidates::candidates;
use super::clip;
use super::lifecycle::{expire, resolve};
use super::reverse::apply_reverse;

pub fn queue(c: &Connection, limit: usize) {
    expire(c);
    for id in strings(
        c,
        "SELECT id FROM claims WHERE state='proposed' AND type IN ('decision','step','question')
           AND NOT EXISTS (SELECT 1 FROM log l WHERE l.target=claims.id AND l.action='resolve-checked')
         ORDER BY created LIMIT ?1",
        &[&crate::db::sql_int(limit)],
    ) {
        println!("{id}");
    }
}

pub fn packet(c: &Connection, id: &str) {
    let me = rows(
        c,
        "SELECT type, ifnull(thread,'') AS thread, title, ifnull(body,'') AS body, ifnull(quote,'') AS quote,
                substr(ifnull(asserted_at,created),1,10) AS day
         FROM claims WHERE id=?1",
        &[&id],
    );
    let Some(me) = me.first() else {
        eprintln!("no such item");
        std::process::exit(1)
    };
    println!("BACKTRAIL-RESOLVE\nITEM {id}\nTODAY {}", &now()[..10]);
    println!(
        "TYPE {}\nTHREAD {}\nDATE {}\nTITLE {}\nBODY {}\nQUOTE {}",
        s(me, "type"),
        s(me, "thread"),
        s(me, "day"),
        s(me, "title"),
        s(me, "body"),
        s(me, "quote")
    );
    let about = strings(
        c,
        "SELECT e.name FROM claim_about a JOIN entities e ON e.id=a.entity_id WHERE a.claim_id=?1",
        &[&id],
    );
    println!("ENTITIES NOW {}", about.join(", "));
    println!("\nENTITIES (use these names exactly):");
    for r in rows(
        c,
        "SELECT name, ifnull(kind,'') AS kind FROM entities ORDER BY name",
        &[],
    ) {
        println!("- {} ({})", s(&r, "name"), s(&r, "kind"));
    }
    println!("\nTHREADS:");
    for t in strings(c, "SELECT name FROM threads ORDER BY name", &[]) {
        println!("- {t}");
    }
    println!("\nCANDIDATES:");
    for k in candidates(c, id, 8) {
        println!(
            "- {} [{}] {} {}: {}",
            k.id,
            k.thread,
            k.kind,
            k.state,
            clip(&k.title, 160)
        );
    }
}

pub fn apply(c: &Connection, v: &Value) -> anyhow::Result<String> {
    if v.get("replaces").is_some() {
        return apply_reverse(c, v);
    }
    let id = v["id"].as_str().context("no id")?;
    let kind: String = c
        .query_row("SELECT type FROM claims WHERE id=?1", [id], |r| r.get(0))
        .context("no such item")?;
    let mut done = vec![];
    for name in v["entities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if let Some((eid, _)) = crate::kg::entities::find(c, name) {
            c.execute(
                "INSERT OR IGNORE INTO claim_about(claim_id,entity_id,role) VALUES(?1,?2,'subject')",
                params![id, eid],
            )
            .ok();
            done.push(format!("about {name}"));
        }
    }
    if let Some(t) = v["thread"].as_str().filter(|t| !t.is_empty()) {
        let now_thread: String = c
            .query_row(
                "SELECT ifnull(thread,'') FROM claims WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .unwrap_or_default();
        let main = v["entities"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(Value::as_str)
            .unwrap_or("");
        let on_project = count(
            c,
            "SELECT COUNT(*) FROM entities WHERE name=?1 AND kind='project'",
            &[&now_thread],
        ) > 0;
        if t != now_thread && (!on_project || t == main) && crate::items::move_to(c, id, t) {
            done.push(format!("thread {t} (was {now_thread})"));
        }
    }
    if let Some(day) = v["valid_to"].as_str().filter(|d| d.len() >= 10) {
        c.execute(
            "UPDATE claims SET valid_to=?2 WHERE id=?1 AND valid_to IS NULL",
            params![id, day],
        )
        .ok();
        done.push(format!("valid_to {day}"));
        expire(c);
    }
    let by = v["resolves_by"].as_str().unwrap_or("");
    if !by.is_empty() && resolve(c, id, by, v["reason"].as_str().unwrap_or(""))? {
        done.push(format!("resolved by {by}"));
    }
    logw(
        c,
        "resolver",
        "resolve-checked",
        id,
        &format!("{kind}: {}", done.join("; ")),
    );
    Ok(done.join("; "))
}

pub fn apply_cli(c: &Connection, file: &str) {
    let mut raw = String::new();
    if file == "-" {
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut raw).ok();
    } else {
        raw = std::fs::read_to_string(file).unwrap_or_default();
    }
    match serde_json::from_str::<Value>(&raw) {
        Ok(v) => match apply(c, &v) {
            Ok(done) => println!(
                "{}",
                if done.is_empty() {
                    "kept as is".into()
                } else {
                    done
                }
            ),
            Err(e) => {
                eprintln!("rejected: {e}");
                std::process::exit(1)
            }
        },
        Err(e) => {
            eprintln!("not JSON: {e}");
            std::process::exit(1)
        }
    }
}
