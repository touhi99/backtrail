use crate::{clock, db, ingest, items, kg};
use anyhow::Context;
use rusqlite::Connection;
use serde_json::Value;

use super::fail;

pub(super) fn ingest_command(c: &Connection) {
    if db::is_demo() {
        eprintln!("demo store: ingest is disabled");
        std::process::exit(1);
    }
    let r = ingest::scan(c);
    println!("git repos {} ({} unchanged)", r.repos.0, r.repos.1);
    println!(
        "items relinked {}, quote no longer found {}",
        r.quotes.0, r.quotes.1
    );
    let read = r.sessions.0 + r.other_sessions.0 + r.notes.0 + r.artifacts.0 + r.repos.0;
    if read > 0 {
        let detail = format!(
            "sessions {}, notes {}, artifacts {}, git repos {}",
            r.sessions.0 + r.other_sessions.0,
            r.notes.0,
            r.artifacts.0,
            r.repos.0
        );
        db::logw(c, "ingest", "ingest", "sources", &detail);
    }
    println!(
        "sessions {} new/changed ({} unchanged), other tools {} ({}), notes {} ({}), artifacts {} ({})",
        r.sessions.0,
        r.sessions.1,
        r.other_sessions.0,
        r.other_sessions.1,
        r.notes.0,
        r.notes.1,
        r.artifacts.0,
        r.artifacts.1
    );
}

pub(super) fn agent(set: Option<&str>) {
    let path = db::home().join("config.json");
    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    let mut v: Value = if raw.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(&raw).unwrap_or_else(|e| {
            fail(format!(
                "{} is not valid JSON ({e}); nothing changed",
                path.display()
            ))
        })
    };
    let Some(cli) = set else {
        println!("{}", v["agent"]["cli"].as_str().unwrap_or("claude"));
        return;
    };
    if !["claude", "gemini", "codex", "antigravity", "command"].contains(&cli) {
        fail("agent CLI must be claude, gemini, codex, antigravity or command");
    }
    if !v["agent"].is_object() {
        v["agent"] = serde_json::json!({});
    }
    v["agent"]["cli"] = Value::String(cli.to_string());
    std::fs::create_dir_all(db::home()).ok();
    if !raw.is_empty() {
        std::fs::write(path.with_file_name("config.json.bak-backtrail"), &raw).ok();
    }
    let text = serde_json::to_string_pretty(&v).expect("a JSON value serializes");
    std::fs::write(&path, format!("{text}\n")).unwrap_or_else(|e| fail(e));
    println!("sessions are read through {cli} ({})", path.display());
}

pub(super) fn import(c: &Connection, file: &str) {
    let raw = if file == "-" {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut s).unwrap_or_else(|e| fail(e));
        s
    } else {
        std::fs::read_to_string(file).unwrap_or_else(|e| fail(format!("{file}: {e}")))
    };
    let facts: Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| fail(format!("not JSON: {e}")));
    let (mut ok, mut bad) = (0, 0);
    let mut edges = vec![];
    for x in facts
        .as_array()
        .unwrap_or_else(|| fail("expected a JSON array"))
    {
        let get = |k: &str| x[k].as_str().unwrap_or("").to_string();
        let kind = get("type");
        if kind == "alternative" || kind == "depends" {
            edges.push(x);
            continue;
        }
        let title = if get("title").is_empty() {
            get("text")
        } else {
            get("title")
        };
        let result = if items::TYPES.contains(&kind.as_str()) {
            let thread = if get("thread").is_empty() {
                let source = db::resolve_ref(c, &get("source"));
                c.query_row("SELECT thread FROM sources WHERE ref=?1", [source], |r| {
                    r.get::<_, Option<String>>(0)
                })
                .ok()
                .flatten()
                .unwrap_or_default()
            } else {
                get("thread")
            };
            items::add(
                c,
                &items::NewItem {
                    kind,
                    thread,
                    title: title.clone(),
                    body: get("body"),
                    source: get("source"),
                    quote: get("quote"),
                    state: x["state"].as_str().map(String::from),
                    origin: x["origin"].as_str().unwrap_or("compiled").into(),
                    date: x["date"].as_str().map(String::from),
                    entities: x["entities"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|e| e.as_str().map(String::from))
                        .collect(),
                    valid_to: x["valid_to"]
                        .as_str()
                        .filter(|v| !v.is_empty())
                        .map(String::from),
                    check_how: x["check"]
                        .as_str()
                        .filter(|v| !v.is_empty())
                        .map(String::from),
                },
            )
        } else {
            let entities: Vec<String> = x["entities"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| e.as_str().map(String::from))
                .collect();
            let kind = if kind == "relation" {
                format!("relation:{}", x["kind"].as_str().unwrap_or(""))
            } else {
                kind
            };
            kg::claims::add(
                c,
                &kg::claims::NewClaim {
                    kind: &kind,
                    text: &title,
                    body: &get("body"),
                    source: &get("source"),
                    quote: &get("quote"),
                    entities: &entities,
                    stated: if get("stated") == "inferred" {
                        "inferred"
                    } else {
                        "stated"
                    },
                    origin: "extracted",
                },
            )
        };
        match result {
            Ok(_) => ok += 1,
            Err(e) => {
                bad += 1;
                eprintln!("rejected [{title}]: {e}");
            }
        }
    }
    for x in edges {
        let get = |k: &str| x[k].as_str().unwrap_or("").to_string();
        match import_edge(c, x, &get) {
            Ok(()) => ok += 1,
            Err(e) => {
                bad += 1;
                eprintln!("rejected edge [{}]: {e}", get("type"));
            }
        }
    }
    println!("imported {ok}, rejected {bad}");
}

fn find_claim(c: &Connection, kind: &str, title: &str) -> Option<String> {
    c.query_row(
        "SELECT id FROM claims WHERE type=?1 AND title=?2 AND state!='rejected' ORDER BY created DESC LIMIT 1",
        rusqlite::params![kind, title],
        |r| r.get(0),
    )
    .ok()
}

fn import_edge(c: &Connection, x: &Value, get: &impl Fn(&str) -> String) -> anyhow::Result<()> {
    let source_ref = db::resolve_ref(c, &get("source"));
    let sid = db::source_id(c, &source_ref).context("source not found")?;
    let quote = get("quote");
    let passage = db::find_quote(c, sid, &quote).context("quote not found in source")?;
    let now = clock::now();
    match get("type").as_str() {
        "alternative" => {
            let of = get("of");
            let chosen = find_claim(c, "decision", &of)
                .with_context(|| format!("chosen decision not found: {of}"))?;
            let thread: String = c.query_row(
                "SELECT ifnull(thread,'') FROM claims WHERE id=?1",
                [&chosen],
                |r| r.get(0),
            )?;
            let mut entities: Vec<String> = x["entities"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| e.as_str().map(String::from))
                .collect();
            if entities.is_empty() {
                entities = c
                    .prepare(
                        "SELECT e.name FROM claim_about a JOIN entities e ON e.id=a.entity_id
                         WHERE a.claim_id=?1 ORDER BY a.role='object', e.id",
                    )?
                    .query_map([&chosen], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?;
            }
            let title = if get("title").is_empty() {
                get("text")
            } else {
                get("title")
            };
            let alt = items::add(
                c,
                &items::NewItem {
                    kind: "decision".into(),
                    thread,
                    title,
                    body: get("body"),
                    source: source_ref,
                    quote,
                    state: Some("rejected".into()),
                    origin: "compiled".into(),
                    date: None,
                    entities,
                    valid_to: None,
                    check_how: None,
                },
            )?;
            let (asid, achunk, aquote): (Option<i64>, Option<i64>, Option<String>) = c.query_row(
                "SELECT source_id, chunk_id, quote FROM claims WHERE id=?1",
                [&alt],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            c.execute(
                "INSERT OR IGNORE INTO claim_edges(a,b,kind,reason,source_id,chunk_id,quote,actor,created)
                 VALUES(?1,?2,'considered',?3,?4,?5,?6,'extracted',?7)",
                rusqlite::params![chosen, alt, get("body"), asid, achunk, aquote, now],
            )?;
        }
        "depends" => {
            let step = find_claim(c, "step", &get("step"))
                .with_context(|| format!("step not found: {}", get("step")))?;
            let on = find_claim(c, "step", &get("on"))
                .with_context(|| format!("step not found: {}", get("on")))?;
            c.execute(
                "INSERT OR IGNORE INTO claim_edges(a,b,kind,reason,source_id,chunk_id,quote,actor,created)
                 VALUES(?1,?2,'depends_on',?3,?4,?5,?6,'extracted',?7)",
                rusqlite::params![step, on, get("body"), sid, passage.chunk_id, quote, now],
            )?;
        }
        other => anyhow::bail!("unsupported edge type {other}"),
    }
    Ok(())
}
