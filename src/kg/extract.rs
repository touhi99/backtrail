use super::{clip, entities, has_word};
use crate::clock::now;
use crate::db::norm;
use anyhow::Context;
use rusqlite::{Connection, params};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap {
    Daily,
    Ignore,
}

pub fn queue(
    c: &Connection,
    limit: usize,
    entity: Option<&str>,
    session_id: Option<&str>,
    cap: Cap,
) -> Vec<String> {
    let today = now()[..10].to_string();
    let used: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM log WHERE action IN ('extract','extract-failed') AND ts LIKE ?1",
            params![format!("{today}%")],
            |x| x.get(0),
        )
        .unwrap_or(0);
    let room = match cap {
        Cap::Ignore => usize::MAX,
        Cap::Daily => crate::config::cfg()
            .daily_cap
            .map_or(usize::MAX, |n| usize::try_from(n - used).unwrap_or(0)),
    };
    let only = session_id.map(|s| {
        crate::db::resolve_ref(c, &format!("session:{}", s.trim_start_matches("session:")))
    });
    let want: Option<Vec<String>> = entity.and_then(|e| entities::find(c, e)).map(|(id, _)| {
        c.prepare("SELECT alias FROM entity_aliases WHERE entity_id=?1")
            .expect("prepare query")
            .query_map(params![id], |r| r.get::<_, String>(0))
            .expect("run query")
            .map(|row| row.expect("decode query row"))
            .collect()
    });
    let mut out = vec![];
    for x in crate::coverage::sessions(c) {
        if out.len() >= limit.min(room) {
            break;
        }
        let (r, sid) = (x.source, x.id);
        match &only {
            Some(o) if *o != r => continue,
            Some(_) if !(x.state.queued() || x.state == crate::coverage::State::Running) => {
                continue;
            }
            None if !x.state.queued() => continue,
            _ => {}
        }
        if let Some(al) = &want {
            let txt: String = c
                .prepare(
                    "SELECT group_concat(text, ' ') FROM chunks WHERE source_id=?1 AND role='user'",
                )
                .expect("prepare query")
                .query_row(params![sid], |x| x.get::<_, Option<String>>(0))
                .unwrap_or_default()
                .unwrap_or_default();
            let t = norm(&txt);
            if !al.iter().any(|a| has_word(&t, a)) {
                continue;
            }
        }
        out.push(r);
    }
    out
}

fn unextracted_mentioning(c: &Connection, entity: &str) -> Vec<String> {
    let Some((id, _)) = entities::find(c, entity) else {
        return vec![];
    };
    let al: Vec<String> = c
        .prepare("SELECT alias FROM entity_aliases WHERE entity_id=?1")
        .expect("prepare query")
        .query_map(params![id], |r| r.get::<_, String>(0))
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .collect();
    let mut st = c
        .prepare("SELECT s.ref, ( SELECT group_concat(text,' ') FROM chunks WHERE source_id=s.id AND role='user' ) FROM sources s WHERE s.kind='session' AND NOT EXISTS (SELECT 1 FROM log l WHERE l.target=s.ref AND l.action IN ('extract','kg-extract'))")
        .expect("prepare query");
    st.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?.unwrap_or_default(),
        ))
    })
    .expect("run query")
    .map(|row| row.expect("decode query row"))
    .filter(|(_, t)| {
        let t = norm(t);
        al.iter().any(|a| has_word(&t, a))
    })
    .map(|x| x.0)
    .collect()
}

pub fn calls_each(c: &Connection) -> f64 {
    let v: Vec<f64> = c
        .prepare("SELECT detail FROM log WHERE action='extract' AND detail LIKE '%items=%'")
        .expect("prepare query")
        .query_map([], |r| r.get::<_, String>(0))
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .filter_map(|d| {
            d.split_whitespace()
                .find_map(|w| w.strip_prefix("items="))
                .and_then(|n| n.parse::<f64>().ok())
                .map(|items| 1.0 + items)
        })
        .collect();
    if v.is_empty() {
        5.0
    } else {
        v.iter().sum::<f64>() / v.len() as f64
    }
}

pub fn unread(c: &Connection, entity: &str) -> usize {
    queue(c, usize::MAX, Some(entity), None, Cap::Ignore).len()
}

pub fn start_in_background(
    c: &Connection,
    entity: &str,
    limit: usize,
) -> anyhow::Result<serde_json::Value> {
    let (_, name) = entities::find(c, entity).context("no such entity")?;
    let exe = std::env::current_exe().and_then(|p| p.canonicalize())?;
    let script = exe
        .ancestors()
        .map(|a| a.join("skill/extract.sh"))
        .find(|p| p.exists())
        .context("skill/extract.sh not found next to the build")?;
    let log = std::env::temp_dir().join("backtrail-extract.log");
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)?;
    let n = unread(c, &name).min(limit);
    std::process::Command::new(script)
        .args(["--entity", &name, "--limit", &limit.to_string(), "--no-cap"])
        .stdin(std::process::Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out)
        .spawn()?;
    crate::db::logw(
        c,
        "owner",
        "kg-extract-start",
        &name,
        &format!("{n} sessions"),
    );
    Ok(serde_json::json!({"ok": true, "started": n}))
}

pub fn backlog(c: &Connection, entity: &str) -> usize {
    unextracted_mentioning(c, entity).len()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Turns {
    New,
    All,
}

pub fn input(c: &Connection, r: &str, turns: Turns) {
    let r = &crate::db::resolve_ref(c, r);
    let Ok((sid, thread, ts)) = c.query_row("SELECT id, ifnull(thread,''), ifnull(last_ts,'') FROM sources WHERE ref=?1 AND kind='session'", params![r], |x| Ok((x.get::<_, i64>(0)?, x.get::<_, String>(1)?, x.get::<_, String>(2)?))) else { eprintln!("no such session"); std::process::exit(1) };
    println!(
        "BACKTRAIL-EXTRACT\nSESSION {r}\nTHREAD {thread}\nDATE {}\n",
        ts.get(..10).unwrap_or("")
    );
    println!("EXISTING ITEMS ON THIS THREAD (do not repeat these):");
    for (t, title) in c
        .prepare("SELECT type, title FROM claims WHERE thread=?1 AND type IN ('decision','step','question','turn') AND state!='rejected' ORDER BY created DESC LIMIT 40")
        .expect("prepare query")
        .query_map(params![thread], |x| {
            Ok((x.get::<_, String>(0)?, x.get::<_, String>(1)?))
        })
        .expect("run query")
        .map(|row| row.expect("decode query row"))
    {
        println!("- {t}: {}", clip(&title, 110));
    }
    crate::finish::print_open_steps(c, &ts);
    println!("\nENTITIES (use these names exactly):");
    for (n, k) in c
        .prepare("SELECT name, ifnull(kind,'') FROM entities ORDER BY name")
        .expect("prepare query")
        .query_map([], |x| Ok((x.get::<_, String>(0)?, x.get::<_, String>(1)?)))
        .expect("run query")
        .map(|row| row.expect("decode query row"))
    {
        println!("- {n} ({k})");
    }
    let since: String = if turns == Turns::All {
        String::new()
    } else {
        c
            .query_row(
                "SELECT ifnull(max(ts),'') FROM log WHERE target=?1 AND action IN ('extract','kg-extract','compile')",
                params![r],
                |x| x.get(0),
            )
            .unwrap_or_default()
    };
    if since.is_empty() {
        println!("\nTURNS:");
    } else {
        println!(
            "\nTURNS (new since this session was last read on {since}; earlier turns are already filed):"
        );
    }
    let mut used = 0usize;
    let mut q = c
        .prepare("SELECT locator, role, text FROM chunks WHERE source_id=?1 AND (?2='' OR ifnull(ts,'')>?2) ORDER BY seq")
        .expect("prepare query");
    for (l, role, t) in q
        .query_map(params![sid, since], |x| {
            Ok((
                x.get::<_, String>(0)?,
                x.get::<_, String>(1)?,
                x.get::<_, String>(2)?,
            ))
        })
        .expect("run query")
        .map(|row| row.expect("decode query row"))
    {
        let user = role == "user";
        let s = clip(&t, if user { 4000 } else { 600 });
        if used + s.len() > 150_000 && !user {
            continue;
        }
        used += s.len();
        println!(
            "[{} · {}] {}",
            l,
            if user { "owner" } else { "assistant" },
            s
        );
    }
}
