use super::{Session, Turn, jsonl};
use std::path::{Path, PathBuf};

pub fn files() -> Vec<PathBuf> {
    let mut out = vec![];
    super::super::walk(&crate::db::home().join("inbox"), &mut out, "jsonl");
    out
}

pub fn parse(p: &Path) -> Option<Session> {
    let raw = std::fs::read_to_string(p).ok()?;
    let mut lines = jsonl(&raw);
    let head = lines.next()?;
    if head["backtrail_session"].as_i64() != Some(1) {
        return None;
    }
    let field = |k: &str| head[k].as_str().unwrap_or("").to_string();
    let harness = field("harness");
    let id = field("id");
    if harness.is_empty() || id.is_empty() {
        return None;
    }
    let turns = lines
        .filter_map(|v| {
            let role = match v["role"].as_str()? {
                "user" => "user",
                "assistant" => "assistant",
                _ => return None,
            };
            Turn::new(role, v["ts"].as_str().unwrap_or(""), v["text"].as_str()?)
        })
        .collect();
    Some(Session {
        id: format!("{harness}-{id}"),
        harness,
        cwd: field("cwd"),
        title: field("title"),
        turns,
    })
}
