use super::{Session, Turn, env_dir, home_dir, jsonl, text_parts};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    env_dir("GEMINI_CLI_HOME")
        .unwrap_or_else(home_dir)
        .join(".gemini")
}

pub fn files() -> Vec<PathBuf> {
    let mut out = vec![];
    for project in std::fs::read_dir(root().join("tmp"))
        .into_iter()
        .flatten()
        .flatten()
    {
        let chats = project.path().join("chats");
        for f in std::fs::read_dir(&chats).into_iter().flatten().flatten() {
            let p = f.path();
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            let name = f.file_name().to_string_lossy().to_string();
            let migrated = ext == "json" && p.with_extension("jsonl").exists();
            if name.starts_with("session-") && (ext == "jsonl" || ext == "json") && !migrated {
                out.push(p);
            }
        }
    }
    out
}

struct Message {
    id: String,
    role: &'static str,
    ts: String,
    text: String,
}

fn message(v: &Value) -> Option<Message> {
    let role = match v["type"].as_str()? {
        "user" => "user",
        "gemini" => "assistant",
        _ => return None,
    };
    let shown = &v["displayContent"];
    let content = if role == "user" && !shown.is_null() {
        shown
    } else {
        &v["content"]
    };
    Some(Message {
        id: v["id"].as_str().unwrap_or("").to_string(),
        role,
        ts: v["timestamp"].as_str().unwrap_or("").to_string(),
        text: text_parts(content),
    })
}

fn upsert(all: &mut Vec<Message>, m: Message) {
    match all.iter_mut().find(|x| !m.id.is_empty() && x.id == m.id) {
        Some(x) => *x = m,
        None => all.push(m),
    }
}

fn project_root(chat: &Path) -> String {
    chat.parent()
        .and_then(Path::parent)
        .and_then(|d| std::fs::read_to_string(d.join(".project_root")).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub fn parse(p: &Path) -> Option<Session> {
    let raw = std::fs::read_to_string(p).ok()?;
    let mut meta = Value::Null;
    let mut all: Vec<Message> = vec![];
    let records: Vec<Value> = if p.extension().is_some_and(|e| e == "json") {
        let doc: Value = serde_json::from_str(&raw).ok()?;
        let msgs = doc["messages"].as_array().cloned().unwrap_or_default();
        meta = doc;
        msgs
    } else {
        jsonl(&raw).collect()
    };
    for v in records {
        if v["sessionId"].is_string() && meta.is_null() {
            meta = v;
        } else if let Some(set) = v.get("$set") {
            if let (Some(m), Some(s)) = (meta.as_object_mut(), set.as_object()) {
                m.extend(s.clone());
            }
        } else if let Some(to) = v["$rewindTo"].as_str() {
            if let Some(i) = all.iter().position(|m| m.id == to) {
                all.truncate(i);
            }
        } else if let Some(patch) = v.get("$patch") {
            let gone: Vec<&str> = patch["removeIds"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            all.retain(|m| !gone.contains(&m.id.as_str()));
        } else if let Some(m) = message(&v) {
            upsert(&mut all, m);
        }
    }
    if meta["kind"] == "subagent" {
        return None;
    }
    let id = meta["sessionId"].as_str()?.to_string();
    let cwd = match project_root(p) {
        s if !s.is_empty() => s,
        _ => meta["directories"][0].as_str().unwrap_or("").to_string(),
    };
    Some(Session {
        harness: "gemini".into(),
        id,
        cwd,
        title: meta["summary"].as_str().unwrap_or("").to_string(),
        turns: all
            .iter()
            .filter_map(|m| Turn::new(m.role, &m.ts, &m.text))
            .collect(),
    })
}
