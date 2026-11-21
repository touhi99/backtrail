use super::super::redact;
use super::worth_keeping;
use serde_json::Value;
use std::collections::HashMap;

pub(super) struct PrLink {
    pub(super) repo: String,
    pub(super) number: i64,
    pub(super) url: String,
    pub(super) ts: String,
}

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn pr_link(v: &Value) -> Option<PrLink> {
    if v["type"] != "pr-link" {
        return None;
    }
    let url = v["prUrl"].as_str().unwrap_or("");
    if url.is_empty() {
        return None;
    }
    Some(PrLink {
        repo: v["prRepository"].as_str().unwrap_or("").to_string(),
        number: v["prNumber"].as_i64().unwrap_or(0),
        url: url.to_string(),
        ts: v["timestamp"].as_str().unwrap_or("").to_string(),
    })
}

pub(super) struct Push {
    pub(super) repo: String,
    pub(super) branch: String,
    pub(super) sha: String,
    pub(super) ts: String,
}

static TO: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(
        r"^To (?:https://github\.com/|git@github\.com:)([\w.-]+/[\w.-]+?)(?:\.git)?/?$",
    )
    .expect("regex")
});
static UPDATE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(
        r"^\s*\+?\s*[0-9a-f]{7,40}\.\.\.?([0-9a-f]{7,40})\s+(main|master) -> (main|master)\b",
    )
    .expect("regex")
});

fn pushes_in(text: &str, ts: &str, out: &mut Vec<Push>) {
    let mut repo: Option<String> = None;
    for line in text.lines() {
        let line = line.trim_end();
        if let Some(m) = TO.captures(line.trim_start()) {
            repo = Some(m[1].to_string());
        } else if let (Some(r), Some(m)) = (&repo, UPDATE.captures(line)) {
            out.push(Push {
                repo: r.clone(),
                branch: m[3].to_string(),
                sha: m[1].to_string(),
                ts: ts.to_string(),
            });
        }
    }
}

pub(super) fn parse_pushes(raw: &str) -> Vec<Push> {
    let mut out = vec![];
    for v in raw
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
    {
        let Value::Array(blocks) = &v["message"]["content"] else {
            continue;
        };
        let ts = v["timestamp"].as_str().unwrap_or("");
        for b in blocks {
            if b["type"] == "tool_result" && b["is_error"].as_bool() != Some(true) {
                pushes_in(&result_text_full(&b["content"]), ts, &mut out);
            }
        }
    }
    out
}

fn result_text_full(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

pub(super) fn parse_prs(raw: &str) -> Vec<PrLink> {
    raw.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| pr_link(&v))
        .collect()
}

fn tool_summary(name: &str, input: &Value) -> String {
    let key = match name {
        "Bash" => Some("command"),
        "Read" | "Write" | "Edit" => Some("file_path"),
        "Grep" | "Glob" => Some("pattern"),
        _ => None,
    };
    let raw = match key.and_then(|k| input[k].as_str()) {
        Some(field) => field.to_string(),
        None => serde_json::to_string(input).unwrap_or_default(),
    };
    clip(&raw, 200)
}

fn result_text(content: &Value) -> String {
    clip(&result_text_full(content), 700)
}

pub(super) fn parse_session(raw: &str) -> (String, Vec<(String, String, String)>, Vec<PrLink>) {
    let mut title = String::new();
    let mut turns = vec![];
    let mut prs = vec![];
    let mut tools: HashMap<String, (String, String)> = HashMap::new();
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
                        match b["type"].as_str().unwrap_or("") {
                            "text" => {
                                text.push_str(b["text"].as_str().unwrap_or(""));
                                text.push('\n');
                            }
                            "tool_use" => {
                                let name = b["name"].as_str().unwrap_or("").to_string();
                                let summary = tool_summary(&name, &b["input"]);
                                if let Some(id) = b["id"].as_str() {
                                    tools.insert(id.to_string(), (name, summary));
                                }
                            }
                            "tool_result" if b["is_error"].as_bool() == Some(true) => {
                                let bts = b["timestamp"].as_str().unwrap_or(ts.as_str());
                                let (name, summary) = b["tool_use_id"]
                                    .as_str()
                                    .and_then(|id| tools.get(id))
                                    .cloned()
                                    .unwrap_or_else(|| ("a tool".to_string(), String::new()));
                                let head = if summary.is_empty() {
                                    format!("tool {name} failed:")
                                } else {
                                    format!("tool {name} failed: {summary}")
                                };
                                let text = format!("{head}\n{}", result_text(&b["content"]));
                                turns.push((
                                    "tool".to_string(),
                                    bts.to_string(),
                                    redact(&text).into_owned(),
                                ));
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
            let tt = text.trim();
            if !worth_keeping(tt) {
                continue;
            }
            turns.push((t.to_string(), ts, redact(tt).into_owned()));
        } else if let Some(p) = pr_link(&v) {
            prs.push(p);
        }
    }
    (title, turns, prs)
}
