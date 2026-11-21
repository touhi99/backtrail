mod brief;
mod browse;
mod focus;
mod graph;
mod thread;
mod today;

pub use brief::{Detail, brief};
pub use browse::{items, source, source_file};
pub use focus::focus;
pub use graph::{graph, trail};
pub use thread::thread;
pub use today::{decided, today};

use crate::clock::days_ago;
use crate::db::{rows, s};
use rusqlite::Connection;
use serde_json::Value;

const OPEN_QUESTIONS: &str = "SELECT * FROM item WHERE type='question' AND state='open'";

fn waiting_on_owner(c: &Connection) -> Vec<Value> {
    rows(
        c,
        &format!("{OPEN_QUESTIONS} AND origin!='owner' ORDER BY created DESC"),
        &[],
    )
}

fn waiting_on_claude(c: &Connection) -> Vec<Value> {
    rows(
        c,
        &format!("{OPEN_QUESTIONS} AND origin='owner' ORDER BY created DESC"),
        &[],
    )
}

fn stale(c: &Connection) -> Vec<Value> {
    let mut out = rows(
        c,
        "SELECT i.id, i.title, i.type, i.thread, i.updated AS since, 'doing' AS why
         FROM item i JOIN thread_stat t ON t.name=i.thread
         WHERE i.type='step' AND i.state='doing'
           AND ifnull(t.session_last,'0') < strftime('%Y-%m-%dT%H:%M:%SZ','now','-7 days')",
        &[],
    );
    let open = rows(
        c,
        "SELECT i.id, i.title, i.type, i.thread, i.created AS since, 'open' AS why
         FROM item i JOIN threads t ON t.name=i.thread
         WHERE i.type IN ('step','question') AND i.state='open' AND t.state!='dropped'
         ORDER BY i.created",
        &[],
    );
    out.extend(
        open.into_iter()
            .filter(|r| days_ago(s(r, "since")) >= 7)
            .take(8),
    );
    out
}

fn threads(c: &Connection) -> Vec<Value> {
    rows(
        c,
        "SELECT *, (SELECT MAX(k.last) FROM thread_stat k WHERE k.name=f.top OR k.parent=f.top) AS family_last
         FROM (SELECT *, COALESCE(NULLIF(parent,''), name) AS top FROM thread_stat) f
         ORDER BY family_last DESC, top, parent IS NOT NULL, last DESC",
        &[],
    )
}

fn gist(text: &str) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    if lines.first().map(|l| l.trim()) == Some("---") {
        let mut end = 0;
        for (i, l) in lines.iter().enumerate().skip(1) {
            if l.trim() == "---" {
                end = i;
                break;
            }
            if let Some(d) = l.trim().strip_prefix("description:") {
                return d.trim().trim_matches('"').chars().take(170).collect();
            }
        }
        lines = lines.split_off((end + 1).min(lines.len()));
    }
    let is_prose = |l: &&str| {
        l.chars().count() >= 30
            && !["#", "|", "---", "<"]
                .iter()
                .any(|mark| l.starts_with(mark))
            && !l.contains('›')
    };
    let prose = lines
        .iter()
        .map(|l| l.trim())
        .filter(is_prose)
        .take(4)
        .collect::<Vec<_>>()
        .join(" ");
    let plain = prose
        .replace("**", "")
        .replace('`', "")
        .replace("[[", "")
        .replace("]]", "");
    plain
        .split_inclusive(". ")
        .next()
        .unwrap_or("")
        .chars()
        .take(170)
        .collect()
}
