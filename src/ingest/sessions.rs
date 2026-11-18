//! Claude Code sessions: one `.jsonl` file per session under `~/.claude/projects/<project folder>/`. Only the text of
//! user and assistant turns is stored, in passages of at most `TURN_CHUNK` characters.
use super::threads::SessionRules;
use super::{Source, add_chunk, begin, finish, home_dir, mtime, redact, slug};
use crate::db::id64;
use rusqlite::{Connection, params};
use serde_json::Value;
use std::fmt::Write;
use std::path::Path;

const TURN_CHUNK: usize = 2400;

/// Split one turn into chunks of at most `n` characters. The first chunk is exactly the first `n` characters, so
/// quotes filed when turns were cut at `n` still match it; later cuts prefer a line break or a space.
fn split_turn(s: &str, n: usize) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n {
        return vec![s.to_string()];
    }
    let mut out = vec![chars[..n].iter().collect::<String>()];
    let mut i = n;
    while i < chars.len() {
        let end = (i + n).min(chars.len());
        let mut cut = end;
        if end < chars.len() {
            let lo = end.saturating_sub(300).max(i + 1);
            cut = (lo..end)
                .rev()
                .find(|&k| chars[k] == '\n')
                .or_else(|| (lo..end).rev().find(|&k| chars[k] == ' '))
                .map_or(end, |k| k + 1);
        }
        out.push(chars[i..cut].iter().collect());
        i = cut;
    }
    out
}

/// The text of one session file from byte `from` to the end of its last complete line, and the byte offset after it.
/// A last line without a newline counts only when it is valid JSON, so a line still being written is read next time.
fn read_lines(p: &Path, from: u64) -> Option<(String, u64)> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(p).ok()?;
    f.seek(SeekFrom::Start(from)).ok()?;
    let mut buf = vec![];
    f.read_to_end(&mut buf).ok()?;
    let mut cut = buf.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    if cut < buf.len() && serde_json::from_slice::<Value>(&buf[cut..]).is_ok() {
        cut = buf.len();
    }
    buf.truncate(cut);
    Some((
        String::from_utf8_lossy(&buf).into_owned(),
        from + cut as u64,
    ))
}

/// A check of the 256 bytes before `at`: if they differ next time, the file was rewritten, not appended to.
fn tail_of(p: &Path, at: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    if at == 0 {
        return None;
    }
    let start = at.saturating_sub(256);
    let mut f = std::fs::File::open(p).ok()?;
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = vec![0; usize::try_from(at - start).expect("a tail window fits in memory")];
    f.read_exact(&mut buf).ok()?;
    Some(id64(&String::from_utf8_lossy(&buf)))
}

/// The AI title (empty when none) and the kept turns (role, timestamp, text) of session lines.
fn parse_session(raw: &str) -> (String, Vec<(String, String, String)>) {
    let mut title = String::new();
    let mut turns = vec![];
    for l in raw.lines() {
        let Ok(v) = serde_json::from_str::<Value>(l) else {
            continue;
        };
        let t = v["type"].as_str().unwrap_or("");
        let ts = v["timestamp"].as_str().unwrap_or("").to_string();
        if t == "ai-title" {
            title = v["aiTitle"].as_str().unwrap_or("").to_string();
        } else if t == "user" || t == "assistant" {
            if v["isMeta"].as_bool() == Some(true) {
                continue;
            }
            let mut text = String::new();
            match &v["message"]["content"] {
                Value::String(s) => text.clone_from(s),
                Value::Array(a) => {
                    for b in a {
                        if b["type"] == "text" {
                            text.push_str(b["text"].as_str().unwrap_or(""));
                            text.push('\n');
                        }
                    }
                }
                _ => {}
            }
            let tt = text.trim();
            if tt.len() < 15
                || tt.starts_with("<local-command")
                || tt.starts_with("<command-")
                || tt.starts_with("<system-reminder")
                || tt.starts_with("[SYSTEM")
                || tt.starts_with("Caveat:")
            {
                continue;
            }
            // redacted here as well as in add_chunk, so a token cannot survive by straddling a chunk cut
            turns.push((t.to_string(), ts, redact(tt).into_owned()));
        }
    }
    (title, turns)
}

/// Store turns as passages, numbering passages from `seq0` and turns from `turn0`.
fn add_turns(c: &Connection, sid: i64, seq0: i64, turn0: i64, turns: &[(String, String, String)]) {
    let mut seq = seq0;
    for (i, (role, ts, text)) in turns.iter().enumerate() {
        for (k, part) in split_turn(text, TURN_CHUNK).iter().enumerate() {
            let mut loc = format!(
                "turn {} · {}",
                turn0 + crate::db::sql_int(i) + 1,
                ts.get(..16).unwrap_or("")
            );
            if k > 0 {
                let _ = write!(loc, " · part {}", k + 1);
            }
            add_chunk(c, sid, seq, &loc, role, ts, part);
            seq += 1;
        }
    }
}

/// Keep the turn count in an untitled session's display title current after new turns were added.
fn untitled_count(c: &Connection, sid: i64, proj: &str) {
    c.execute(
        "UPDATE sources SET title='Untitled session ('||?2||' · '||(SELECT COUNT(*) FROM chunks
           WHERE source_id=?1 AND role='user' AND locator NOT LIKE '% · part %')||' turns)'
         WHERE id=?1 AND title LIKE 'Untitled session (%'",
        params![sid, proj],
    )
    .ok();
}

/// Read every session file that is new or changed. Returns (read, unchanged).
pub fn read_all(c: &Connection) -> (usize, usize) {
    let rules = SessionRules::load();
    let root = home_dir().join(".claude/projects");
    let (mut n, mut skipped) = (0, 0);
    let Ok(dirs) = std::fs::read_dir(&root) else {
        return (0, 0);
    };
    // The same session id can exist in two project folders (a folder was renamed or copied). Read only the larger
    // copy; otherwise each run swaps the stored text between the two and claim evidence loses its passage.
    let mut largest: std::collections::HashMap<String, (u64, std::path::PathBuf)> =
        std::collections::HashMap::default();
    for d in std::fs::read_dir(&root).into_iter().flatten().flatten() {
        for f in std::fs::read_dir(d.path()).into_iter().flatten().flatten() {
            let p = f.path();
            if p.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let stem = p
                .file_stem()
                .expect("listed file has a name")
                .to_string_lossy()
                .to_string();
            let size = f.metadata().map_or(0, |m| m.len());
            let e = largest.entry(stem).or_insert((size, p.clone()));
            if size > e.0 {
                *e = (size, p.clone());
            }
        }
    }
    c.execute_batch("BEGIN").expect("write to database");
    for d in dirs.flatten() {
        let dn = d.file_name().to_string_lossy().to_string();
        if dn.contains("private-tmp") || dn.contains("private-var") {
            continue;
        }
        let hs = slug(&home_dir());
        let rest = dn.strip_prefix(&format!("{hs}-")).unwrap_or(&dn);
        let proj = if dn == hs {
            "home".to_string()
        } else {
            ["Desktop-", "Documents-"]
                .iter()
                .find_map(|p| rest.strip_prefix(p))
                .unwrap_or(rest)
                .to_string()
        };
        let Ok(files) = std::fs::read_dir(d.path()) else {
            continue;
        };
        for f in files.flatten() {
            let p = f.path();
            if p.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let stem = p
                .file_stem()
                .expect("listed file has a name")
                .to_string_lossy()
                .to_string();
            if largest.get(&stem).is_some_and(|(_, best)| *best != p) {
                continue;
            }
            let r = format!("session:{stem}");
            let path_s = p.to_string_lossy().to_string();
            let mt = mtime(&p);
            // (id, mtime, bytes read, check of the bytes before that point, path, turns stored)
            let prev: Option<(i64, i64, i64, Option<String>, String, i64)> = c
                .query_row(
                    "SELECT id, mtime, bytes, tail, ifnull(path,''), nturns FROM sources WHERE ref=?1",
                    params![r],
                    |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?, x.get(3)?, x.get(4)?, x.get(5)?)),
                )
                .ok();
            let known: Option<i64> = prev.as_ref().map(|x| x.1).or_else(|| {
                c.query_row(
                    "SELECT mtime FROM scanned WHERE path=?1",
                    params![path_s],
                    |x| x.get(0),
                )
                .ok()
            });
            // mtime has whole-second resolution, so a line written in the same second as the last scan leaves it
            // unchanged; a file longer than the bytes read so far is read again from there.
            let size = f
                .metadata()
                .map_or(0, |m| i64::try_from(m.len()).unwrap_or(i64::MAX));
            let grown = prev.as_ref().is_some_and(|x| x.2 > 0 && size > x.2);
            if known == Some(mt) && !grown {
                skipped += 1;
                continue;
            }
            // Claude Code appends to a session file. When the bytes read last time are unchanged, only the new lines
            // are read and their passages added, so stored passages keep their ids. A file that shrank, moved or
            // changed before that point is read again in full.
            let append = prev.as_ref().filter(|x| {
                x.2 > 0 && x.4 == path_s && x.3.is_some() && tail_of(&p, x.2.unsigned_abs()) == x.3
            });
            let from = append.map_or(0, |x| x.2.unsigned_abs());
            let Some((raw, upto)) = read_lines(&p, from) else {
                continue;
            };
            let (title, turns) = parse_session(&raw);
            let tail = tail_of(&p, upto);
            if let Some(&(sid, _, _, _, _, nturns)) = append {
                if !title.is_empty() {
                    c.execute(
                        "UPDATE sources SET title=?2 WHERE id=?1",
                        params![sid, title],
                    )
                    .expect("write to database");
                }
                if turns.is_empty() {
                    skipped += 1;
                } else {
                    let seq0: i64 = c
                        .query_row(
                            "SELECT ifnull(max(seq)+1,0) FROM chunks WHERE source_id=?1",
                            params![sid],
                            |x| x.get(0),
                        )
                        .unwrap_or(0);
                    add_turns(c, sid, seq0, nturns, &turns);
                    let first: String = c
                        .query_row(
                            "SELECT ifnull(first_ts,'') FROM sources WHERE id=?1",
                            params![sid],
                            |x| x.get(0),
                        )
                        .unwrap_or_default();
                    finish(
                        c,
                        sid,
                        &first,
                        &turns.last().expect("turns checked non-empty above").1,
                    );
                    if title.is_empty() {
                        untitled_count(c, sid, &proj);
                    }
                    n += 1;
                }
                c.execute(
                    "UPDATE sources SET mtime=?2, bytes=?3, tail=?4, nturns=nturns+?5 WHERE id=?1",
                    params![
                        sid,
                        mt,
                        i64::try_from(upto).unwrap_or(i64::MAX),
                        tail,
                        crate::db::sql_int(turns.len())
                    ],
                )
                .expect("write to database");
                continue;
            }
            if turns.is_empty() {
                c.execute(
                    "INSERT OR REPLACE INTO scanned(path,mtime) VALUES(?1,?2)",
                    params![path_s, mt],
                )
                .expect("write to database");
                continue;
            }
            let users = turns.iter().filter(|t| t.0 == "user").count();
            let thread = rules.thread_for(&proj, &title, crate::db::sql_int(turns.len()));
            let disp = if title.is_empty() {
                format!("Untitled session ({proj} · {users} turns)")
            } else {
                title.clone()
            };
            let source = Source {
                source_ref: &r,
                kind: "session",
                title: &disp,
                project: &proj,
                thread: Some(&thread),
                path: &path_s,
                version: mt,
            };
            let Some(sid) = begin(c, &source) else {
                skipped += 1;
                continue;
            };
            add_turns(c, sid, 0, 0, &turns);
            finish(
                c,
                sid,
                &turns[0].1,
                &turns.last().expect("turns checked non-empty above").1,
            );
            c.execute(
                "UPDATE sources SET bytes=?2, tail=?3, nturns=?4 WHERE id=?1",
                params![
                    sid,
                    i64::try_from(upto).unwrap_or(i64::MAX),
                    tail,
                    crate::db::sql_int(turns.len())
                ],
            )
            .expect("write to database");
            n += 1;
        }
    }
    c.execute_batch("COMMIT").expect("write to database");
    (n, skipped)
}

/// Session files on disk that the store has never indexed, with the age of each file in days. Claude Code removes old
/// transcripts (30 days by default), so a session that stays unindexed that long is lost.
pub fn unindexed(c: &Connection) -> Vec<(String, u64)> {
    let root = home_dir().join(".claude/projects");
    let mut out = vec![];
    for d in std::fs::read_dir(&root).into_iter().flatten().flatten() {
        let dn = d.file_name().to_string_lossy().to_string();
        if dn.contains("private-tmp") || dn.contains("private-var") {
            continue;
        }
        for f in std::fs::read_dir(d.path()).into_iter().flatten().flatten() {
            let p = f.path();
            if p.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let id: String = p
                .file_stem()
                .expect("listed file has a name")
                .to_string_lossy()
                .to_string();
            let known: i64 = c
                .query_row(
                    "SELECT (SELECT COUNT(*) FROM sources WHERE ref=?1) + (SELECT COUNT(*) FROM scanned WHERE path=?2)",
                    params![format!("session:{id}"), p.to_string_lossy()],
                    |r| r.get(0),
                )
                .unwrap_or(1);
            if known == 0 {
                let age = f
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .map_or(0, |e| e.as_secs() / 86400);
                out.push((format!("session:{id}"), age));
            }
        }
    }
    out.sort_by_key(|e| std::cmp::Reverse(e.1));
    out
}
