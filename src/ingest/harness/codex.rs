use super::{Session, Turn, env_dir, home_dir, jsonl, text_parts};
use serde_json::Value;
use std::path::{Path, PathBuf};

const REQUEST_MARK: &str = "## My request for Codex:";
const INJECTED: [&str; 7] = [
    "<user_instructions>",
    "<environment_context>",
    "<environments_instructions>",
    "<skills_instructions>",
    "<plugins_instructions>",
    "<collaboration_mode>",
    "# AGENTS.md instructions",
];

fn root() -> PathBuf {
    env_dir("CODEX_HOME").unwrap_or_else(|| home_dir().join(".codex"))
}

pub fn files() -> Vec<PathBuf> {
    let mut out = vec![];
    for dir in ["sessions", "archived_sessions"] {
        super::super::walk(&root().join(dir), &mut out, "jsonl");
    }
    out.retain(|p| {
        p.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("rollout-"))
    });
    out
}

fn file_id(p: &Path) -> Option<String> {
    let stem = p.file_stem()?.to_string_lossy();
    let rest = stem.strip_prefix("rollout-")?;
    rest.get(20..)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

fn title(id: &str) -> String {
    let raw = std::fs::read_to_string(root().join("session_index.jsonl")).unwrap_or_default();
    jsonl(&raw)
        .filter(|e| e["id"] == id)
        .last()
        .and_then(|e| e["thread_name"].as_str().map(str::to_string))
        .unwrap_or_default()
}

fn typed(message: &str) -> &str {
    message
        .split_once(REQUEST_MARK)
        .map_or(message, |(_, after)| after)
}

fn subagent(meta: &Value) -> bool {
    meta["parent_thread_id"].is_string() || meta["source"].get("subagent").is_some()
}

pub fn parse(p: &Path) -> Option<Session> {
    let raw = std::fs::read_to_string(p).ok()?;
    let (mut id, mut cwd) = (String::new(), String::new());
    let (mut events, mut items) = (vec![], vec![]);
    for v in jsonl(&raw) {
        let ts = v["timestamp"].as_str().unwrap_or("");
        let pl = &v["payload"];
        match (v["type"].as_str(), pl["type"].as_str()) {
            (Some("session_meta"), _) => {
                if subagent(pl) {
                    return None;
                }
                id = pl["id"].as_str().unwrap_or("").to_string();
                cwd = pl["cwd"].as_str().unwrap_or("").to_string();
            }
            (Some("event_msg"), Some("user_message")) => {
                let m = pl["message"].as_str().unwrap_or("");
                events.extend(Turn::new("user", ts, typed(m)));
            }
            (Some("event_msg"), Some("agent_message")) => {
                events.extend(Turn::new(
                    "assistant",
                    ts,
                    pl["message"].as_str().unwrap_or(""),
                ));
            }
            (Some("response_item"), Some("message")) => {
                let role = match pl["role"].as_str() {
                    Some("user") => "user",
                    Some("assistant") => "assistant",
                    _ => continue,
                };
                let text = text_parts(&pl["content"]);
                if !INJECTED.iter().any(|t| text.trim_start().starts_with(t)) {
                    items.extend(Turn::new(role, ts, typed(&text)));
                }
            }
            _ => {}
        }
    }
    if id.is_empty() {
        return None;
    }
    let id = file_id(p).unwrap_or(id);
    Some(Session {
        harness: "codex".into(),
        title: title(&id),
        id,
        cwd,
        turns: if events.is_empty() { items } else { events },
    })
}
