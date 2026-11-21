use crate::db::{count, rows, s};
use rusqlite::Connection;
use std::time::{SystemTime, UNIX_EPOCH};

const IDLE_SECS: i64 = 30 * 60;
pub const MAX_ATTEMPTS: i64 = 5;
const READ: &str = "('extract','kg-extract','compile')";
const FAILED: &str = "('extract-failed','extract-failed-env','kg-extract-failed','compile-failed')";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    Failed,
    Unread,
    Behind,
    Running,
    Excluded,
    Complete,
}

impl State {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Failed => "failed",
            Self::Unread => "unread",
            Self::Behind => "behind",
            Self::Running => "running",
            Self::Excluded => "excluded",
            Self::Complete => "complete",
        }
    }

    pub const fn queued(self) -> bool {
        matches!(self, Self::Unread | Self::Behind)
    }
}

pub struct Session {
    pub source: String,
    pub id: i64,
    pub thread: String,
    pub title: String,
    pub last_ts: String,
    pub state: State,
    pub detail: String,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

pub fn sessions(c: &Connection) -> Vec<Session> {
    let sql = format!(
        "SELECT s.id, s.ref, ifnull(s.thread,'Unsorted') AS thread, ifnull(s.title,'') AS title,
                ifnull(s.last_ts,'') AS last_ts, ifnull(s.mtime,0) AS mtime,
                (SELECT COUNT(*) FROM chunks WHERE source_id=s.id AND role='user') AS owner_turns,
                EXISTS (SELECT 1 FROM chunks k WHERE k.source_id=s.id AND k.seq=0 AND (k.text LIKE 'BACKTRAIL-%' OR k.text LIKE 'Reply with%' OR k.text LIKE 'Read the file%' OR k.text LIKE 'Write a %')) AS internal,
                (s.ref NOT LIKE 'session:%-%' AND EXISTS (SELECT 1 FROM sources t WHERE t.kind='session' AND t.path=s.path AND t.id!=s.id)) AS duplicate,
                EXISTS (SELECT 1 FROM source_review r WHERE r.source_id=s.id) AS skipped,
                ifnull((SELECT max(ts) FROM log l WHERE l.target=s.ref AND l.action IN {READ}),'') AS read_at,
                ifnull((SELECT max(ts) FROM log l WHERE l.target=s.ref AND l.action='ended'),'') AS ended_at
         FROM sources s WHERE s.kind='session' ORDER BY s.last_ts DESC"
    );
    let now = now_secs();
    rows(c, &sql, &[])
        .iter()
        .map(|r| {
            let source = s(r, "ref").to_string();
            let read_at = s(r, "read_at").to_string();
            let last_ts = s(r, "last_ts").to_string();
            let flag = |k: &str| r[k].as_i64().unwrap_or(0) != 0;
            let fails = count(
                c,
                &format!("SELECT COUNT(*) FROM log WHERE target=?1 AND action IN {FAILED} AND ts>?2"),
                &[&source, &read_at],
            );
            let new_turns = if read_at.is_empty() {
                0
            } else {
                count(
                    c,
                    "SELECT COUNT(*) FROM chunks WHERE source_id=?1 AND role='user' AND ts>?2",
                    &[&r["id"].as_i64().unwrap_or(0), &read_at],
                )
            };
            let ended = !s(r, "ended_at").is_empty() && s(r, "ended_at") >= last_ts.as_str();
            let idle = now - r["mtime"].as_i64().unwrap_or(0) >= IDLE_SECS;
            let (state, detail) = if r["owner_turns"].as_i64().unwrap_or(0) == 0 {
                (State::Excluded, "no owner turn".to_string())
            } else if flag("internal") {
                (State::Excluded, "Backtrail's own call or a probe prompt".to_string())
            } else if flag("duplicate") {
                (State::Excluded, "duplicate of a full session id".to_string())
            } else if flag("skipped") {
                (State::Excluded, "skipped by hand".to_string())
            } else if !read_at.is_empty() && new_turns == 0 {
                (State::Complete, String::new())
            } else if fails >= MAX_ATTEMPTS {
                let err = rows(
                    c,
                    &format!("SELECT detail FROM log WHERE target=?1 AND action IN {FAILED} ORDER BY ts DESC LIMIT 1"),
                    &[&source],
                );
                (
                    State::Failed,
                    format!("{fails} attempts; last: {}", err.first().map_or("", |e| s(e, "detail"))),
                )
            } else if !ended && !idle {
                (State::Running, String::new())
            } else if read_at.is_empty() {
                (State::Unread, retry(fails))
            } else {
                (State::Behind, format!("{new_turns} new owner turns{}", retry(fails)))
            };
            Session {
                source,
                id: r["id"].as_i64().unwrap_or(0),
                thread: s(r, "thread").to_string(),
                title: s(r, "title").to_string(),
                last_ts,
                state,
                detail,
            }
        })
        .collect()
}

fn retry(fails: i64) -> String {
    if fails == 0 {
        String::new()
    } else {
        format!(" (attempt {} of {MAX_ATTEMPTS})", fails + 1)
    }
}

pub struct Graph {
    pub unlinked: i64,
    pub unresolved: i64,
    pub unembedded: i64,
}

pub fn graph(c: &Connection) -> Graph {
    Graph {
        unlinked: count(
            c,
            "SELECT COUNT(*) FROM claims c WHERE state NOT IN ('rejected','superseded')
               AND NOT EXISTS (SELECT 1 FROM claim_about a WHERE a.claim_id=c.id)",
            &[],
        ),
        unresolved: count(
            c,
            "SELECT COUNT(*) FROM claims WHERE state='proposed' AND type IN ('decision','step','question')
               AND NOT EXISTS (SELECT 1 FROM log l WHERE l.target=claims.id AND l.action='resolve-checked')",
            &[],
        ),
        unembedded: count(
            c,
            "SELECT COUNT(*) FROM chunks WHERE hash IS NOT NULL AND hash NOT IN (SELECT hash FROM vec)",
            &[],
        ),
    }
}

pub fn tally(list: &[Session]) -> Vec<(State, usize)> {
    let mut out: Vec<(State, usize)> = vec![];
    for x in list {
        match out.iter_mut().find(|(st, _)| *st == x.state) {
            Some((_, n)) => *n += 1,
            None => out.push((x.state, 1)),
        }
    }
    out.sort();
    out
}

pub fn one_line(c: &Connection) -> String {
    let list = sessions(c);
    let t = tally(&list);
    let n = |st: State| t.iter().find(|(x, _)| *x == st).map_or(0, |x| x.1);
    let open = n(State::Unread) + n(State::Behind);
    let g = graph(c);
    let mut parts = vec![format!(
        "{} of {} sessions read",
        n(State::Complete),
        list.len() - n(State::Excluded)
    )];
    if n(State::Running) > 0 {
        parts.push(format!("{} running", n(State::Running)));
    }
    if open > 0 {
        parts.push(format!("{open} queued for the sweep"));
    }
    if n(State::Failed) > 0 {
        parts.push(format!("{} FAILED", n(State::Failed)));
    }
    if g.unlinked > 0 {
        parts.push(format!("{} items not in the graph", g.unlinked));
    }
    let ok = n(State::Failed) == 0 && g.unlinked == 0;
    format!(
        "Trail: {}{}",
        parts.join(", "),
        if ok { "" } else { " · see `bt status`" }
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Show {
    Open,
    All,
}

pub fn status(c: &Connection, show: Show) -> bool {
    let list = sessions(c);
    let g = graph(c);
    println!("{}", one_line(c));
    for (st, n) in tally(&list) {
        println!("  {:<9} {n}", st.label());
    }
    println!(
        "  graph: {} items without an entity, {} proposed items not checked, {} passages not embedded",
        g.unlinked, g.unresolved, g.unembedded
    );
    for x in list
        .iter()
        .filter(|x| show == Show::All || !matches!(x.state, State::Complete | State::Excluded))
    {
        println!(
            "{:<8} {} | {} | {} | {}{}",
            x.state.label(),
            x.source,
            x.last_ts.get(..16).unwrap_or(&x.last_ts),
            x.thread,
            x.title,
            if x.detail.is_empty() {
                String::new()
            } else {
                format!(" | {}", x.detail)
            }
        );
    }
    !list.iter().any(|x| x.state == State::Failed) && g.unlinked == 0
}
