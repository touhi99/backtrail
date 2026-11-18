//! Git history of the repositories listed in `config.json` (`repos`). A repository is read again when its log or
//! its working-tree status changed.
use super::{Source, add_chunk, begin, finish};
use crate::config::cfg;
use crate::db::fxhash;
use rusqlite::Connection;
use std::path::Path;

fn git(dir: &Path, args: &[&str]) -> String {
    let o = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output();
    o.map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

/// Git reader: one source per repo, one passage per commit (last 400), plus a line for uncommitted and unpushed work.
pub fn read_repos(c: &Connection) -> (usize, usize) {
    let workspace = cfg().workspace.clone();
    if workspace.as_os_str().is_empty() {
        return (0, 0);
    }
    let (mut n, mut skipped) = (0, 0);
    for (rel, th) in &cfg().repos {
        let dir = workspace.join(rel);
        if !dir.join(".git").exists() {
            continue;
        }
        let log = git(&dir, &["log", "-n", "400", "--format=%h|%aI|%s"]);
        if log.is_empty() {
            continue;
        }
        let st = git(&dir, &["status", "--porcelain=v1", "-b"]);
        let source = Source {
            source_ref: &format!("git:{rel}"),
            kind: "commit",
            title: &format!("{rel} commits"),
            project: rel,
            thread: Some(th.as_str()),
            path: &dir.to_string_lossy(),
            version: i64::from(fxhash(&format!("{log}{st}"))),
        };
        let Some(sid) = begin(c, &source) else {
            skipped += 1;
            continue;
        };
        let mut lines = st.lines();
        let head = lines.next().unwrap_or("").to_string();
        let dirty = lines.count();
        add_chunk(
            c,
            sid,
            0,
            "working tree",
            "git",
            "",
            &format!(
                "{} · {} uncommitted file(s)",
                head.trim_start_matches("## "),
                dirty
            ),
        );
        let (mut first, mut last) = (String::new(), String::new());
        for (i, l) in log.lines().enumerate() {
            let mut p = l.splitn(3, '|');
            let (h, d, subj) = (
                p.next().unwrap_or(""),
                p.next().unwrap_or(""),
                p.next().unwrap_or(""),
            );
            if i == 0 {
                last = d.to_string();
            }
            first = d.to_string();
            add_chunk(
                c,
                sid,
                1 + crate::db::sql_int(i),
                &format!("commit {h}"),
                "git",
                d,
                &format!("{} {} {}", h, &d[..10.min(d.len())], subj),
            );
        }
        finish(c, sid, &first, &last);
        n += 1;
    }
    (n, skipped)
}
