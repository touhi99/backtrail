//! Extraction: which sessions are worth reading with a model, and the text the model is given. The binary never calls
//! a model: `skill/extract.sh` pipes `input` to one and sends its answer back through `bt import`.
use super::{clip, entities, has_word};
use crate::clock::now;
use crate::db::norm;
use rusqlite::{Connection, params};

/// Sessions to extract claims from: at least one owner turn, not automated probes, not yet extracted, newest first,
/// within the daily cap. `entity` restricts to sessions whose owner turns mention that entity (tier 2, lazy).
pub fn queue(
    c: &Connection,
    limit: usize,
    entity: Option<&str>,
    session_id: Option<&str>,
    ignore_cap: bool,
) -> Vec<String> {
    let today = now()[..10].to_string();
    let used: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM log WHERE action IN ('extract','extract-failed') AND ts LIKE ?1",
            params![format!("{today}%")],
            |x| x.get(0),
        )
        .unwrap_or(0);
    let cap = crate::config::cfg().daily_cap;
    let room = if ignore_cap { usize::MAX } else { (cap - used).max(0) as usize };
    // a full session id or any unique prefix of it
    let only = session_id.map(|s| {
        crate::db::resolve_ref(c, &format!("session:{}", s.trim_start_matches("session:")))
    });
    let mut st = c.prepare("SELECT s.ref, s.id FROM sources s WHERE s.kind='session' AND (SELECT COUNT(*) FROM chunks WHERE source_id=s.id AND role='user')>=1 AND ifnull(s.thread,'Unsorted')!='Unsorted' AND NOT (s.ref NOT LIKE 'session:%-%' AND EXISTS (SELECT 1 FROM sources t WHERE t.kind='session' AND t.path=s.path AND t.id!=s.id)) AND NOT EXISTS (SELECT 1 FROM log l WHERE l.target=s.ref AND l.action IN ('extract','kg-extract')) AND ( SELECT COUNT(*) FROM log l WHERE l.target=s.ref AND l.action IN ('extract-failed','kg-extract-failed','compile-failed') )<2 AND NOT EXISTS ( SELECT 1 FROM chunks k WHERE k.source_id=s.id AND k.seq=0 AND (k.text LIKE 'BACKTRAIL-%' OR k.text LIKE 'Reply with%' OR k.text LIKE 'Read the file%' OR k.text LIKE 'Write a %') ) ORDER BY s.last_ts DESC").unwrap();
    let want: Option<Vec<String>> = entity.and_then(|e| entities::find(c, e)).map(|(id, _)| {
        c.prepare("SELECT alias FROM entity_aliases WHERE entity_id=?1")
            .unwrap()
            .query_map(params![id], |r| r.get::<_, String>(0))
            .unwrap()
            .flatten()
            .collect()
    });
    let mut out = vec![];
    for (r, sid) in st
        .query_map([], |x| Ok((x.get::<_, String>(0)?, x.get::<_, i64>(1)?)))
        .unwrap()
        .flatten()
    {
        if out.len() >= limit.min(room) {
            break;
        }
        if only.as_ref().map_or(false, |o| *o != r) {
            continue;
        }
        if let Some(al) = &want {
            let txt: String = c
                .prepare(
                    "SELECT group_concat(text, ' ') FROM chunks WHERE source_id=?1 AND role='user'",
                )
                .unwrap()
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
    // like extract_queue but without the daily cap, to report the size of the backlog
    let Some((id, _)) = entities::find(c, entity) else {
        return vec![];
    };
    let al: Vec<String> = c
        .prepare("SELECT alias FROM entity_aliases WHERE entity_id=?1")
        .unwrap()
        .query_map(params![id], |r| r.get::<_, String>(0))
        .unwrap()
        .flatten()
        .collect();
    let mut st = c
        .prepare("SELECT s.ref, ( SELECT group_concat(text,' ') FROM chunks WHERE source_id=s.id AND role='user' ) FROM sources s WHERE s.kind='session' AND NOT EXISTS (SELECT 1 FROM log l WHERE l.target=s.ref AND l.action IN ('extract','kg-extract'))")
        .unwrap();
    st.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?.unwrap_or_default(),
        ))
    })
    .unwrap()
    .flatten()
    .filter(|(_, t)| {
        let t = norm(t);
        al.iter().any(|a| has_word(&t, a))
    })
    .map(|x| x.0)
    .collect()
}

/// Average cost in USD of the logged extraction runs, 0.01 when none has been logged (16 logged runs averaged 0.009).
pub fn usd_each(c: &Connection) -> f64 {
    let v: Vec<f64> = c
        .prepare("SELECT detail FROM log WHERE action='extract' AND detail LIKE 'usd=%'")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .flatten()
        .filter_map(|d| d.strip_prefix("usd=")?.split_whitespace().next()?.parse().ok())
        .collect();
    if v.is_empty() { 0.01 } else { v.iter().sum::<f64>() / v.len() as f64 }
}

/// Sessions that mention the entity and would be extracted next: the same rules as `queue`, no daily cap.
pub fn unread(c: &Connection, entity: &str) -> usize {
    queue(c, usize::MAX, Some(entity), None, true).len()
}

/// Start `skill/extract.sh --entity E --limit N --no-cap` detached and return at once. The binary still calls no
/// model itself; the script does. Output goes to the same log file as the SessionEnd run.
pub fn start_in_background(
    c: &Connection,
    entity: &str,
    limit: usize,
) -> Result<serde_json::Value, String> {
    let (_, name) = entities::find(c, entity).ok_or("no such entity")?;
    let exe = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .map_err(|e| e.to_string())?;
    let script = exe
        .ancestors()
        .map(|a| a.join("skill/extract.sh"))
        .find(|p| p.exists())
        .ok_or("skill/extract.sh not found next to the build")?;
    let log = std::env::temp_dir().join("backtrail-extract.log");
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .map_err(|e| e.to_string())?;
    let n = unread(c, &name).min(limit);
    std::process::Command::new(script)
        .args(["--entity", &name, "--limit", &limit.to_string(), "--no-cap"])
        .stdin(std::process::Stdio::null())
        .stdout(out.try_clone().map_err(|e| e.to_string())?)
        .stderr(out)
        .spawn()
        .map_err(|e| e.to_string())?;
    crate::db::logw(c, "owner", "kg-extract-start", &name, &format!("{n} sessions"));
    Ok(serde_json::json!({"ok": true, "started": n}))
}

/// How many sessions that name the entity in an owner turn have no claims extracted yet (tier 2 backlog).
pub fn backlog(c: &Connection, entity: &str) -> usize {
    unextracted_mentioning(c, entity).len()
}

/// The text a small model reads for one session: the thread's existing items (to avoid repeats), entity names, owner
/// turns in full, assistant turns clipped.
pub fn input(c: &Connection, r: &str) {
    let r = &crate::db::resolve_ref(c, r);
    let Ok((sid, thread, ts)) = c.query_row("SELECT id, ifnull(thread,''), ifnull(last_ts,'') FROM sources WHERE ref=?1 AND kind='session'", params![r], |x| Ok((x.get::<_, i64>(0)?, x.get::<_, String>(1)?, x.get::<_, String>(2)?))) else { eprintln!("no such session"); std::process::exit(1) };
    println!(
        "BACKTRAIL-EXTRACT\nSESSION {r}\nTHREAD {thread}\nDATE {}\n",
        ts.get(..10).unwrap_or("")
    );
    println!("EXISTING ITEMS ON THIS THREAD (do not repeat these):");
    for (t, title) in c
        .prepare("SELECT type, title FROM claims WHERE thread=?1 AND type IN ('decision','step','question','turn') AND state!='rejected' ORDER BY created DESC LIMIT 40")
        .unwrap()
        .query_map(params![thread], |x| {
            Ok((x.get::<_, String>(0)?, x.get::<_, String>(1)?))
        })
        .unwrap()
        .flatten()
    {
        println!("- {t}: {}", clip(&title, 110));
    }
    crate::finish::print_open_steps(c, &ts);
    println!("\nENTITIES (use these names exactly):");
    for (n, k) in c
        .prepare("SELECT name, ifnull(kind,'') FROM entities ORDER BY name")
        .unwrap()
        .query_map([], |x| Ok((x.get::<_, String>(0)?, x.get::<_, String>(1)?)))
        .unwrap()
        .flatten()
    {
        println!("- {n} ({k})");
    }
    println!("\nTURNS:");
    let mut used = 0usize;
    let mut q = c
        .prepare("SELECT locator, role, text FROM chunks WHERE source_id=?1 ORDER BY seq")
        .unwrap();
    for (l, role, t) in q
        .query_map(params![sid], |x| {
            Ok((
                x.get::<_, String>(0)?,
                x.get::<_, String>(1)?,
                x.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .flatten()
    {
        let user = role == "user";
        let s = clip(&t, if user { 900 } else { 220 });
        if used + s.len() > 22000 && !user {
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
