use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

pub const DEFAULT_EMBED_MODEL: &str = "embeddinggemma-2:270m";
const DEFAULT_EMBED_URL: &str = "http://127.0.0.1:11434";

pub struct ThreadRule {
    pub name: String,
    pub area: String,
    pub state: String,
    pub summary: String,
    pub kind: String,
    pub regex: String,
    pub parent: String,
}

#[derive(Default)]
pub struct Embeddings {
    pub url: String,
    pub model: String,
    pub query_prefix: String,
    pub document_prefix: String,
    pub batch_cap: usize,
}

pub struct EntitySeed {
    pub name: String,
    pub kind: String,
    pub aliases: Vec<String>,
}

pub struct Config {
    pub workspace: PathBuf,
    pub note_dirs: Vec<String>,
    pub screens_dir: String,
    pub threads: Vec<ThreadRule>,
    pub note_rules: Vec<(String, String)>,
    pub artifact_rules: Vec<(String, String)>,
    pub repos: Vec<(String, String)>,
    pub aliases: HashMap<String, Vec<String>>,
    pub generic_projects: Vec<String>,
    pub entities: Vec<EntitySeed>,
    pub port: u16,
    pub embeddings: Embeddings,
    pub daily_cap: Option<i64>,
    pub agent: String,
}

fn prefixes(model: &str) -> (&'static str, &'static str) {
    if model.contains("embeddinggemma") {
        ("task: search result | query: ", "title: none | text: ")
    } else if model.contains("nomic") {
        ("search_query: ", "search_document: ")
    } else {
        ("", "")
    }
}

fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or("").to_string()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn pairs(v: &Value) -> Vec<(String, String)> {
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|p| {
                    (
                        p[0].as_str().unwrap_or("").to_string(),
                        p[1].as_str().unwrap_or("").to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn checked_regex(owner: &str, pattern: String) -> String {
    match regex::Regex::new(&format!("(?i){pattern}")) {
        Ok(_) => pattern,
        Err(e) => {
            let e = e.to_string();
            let reason = e
                .lines()
                .last()
                .unwrap_or_default()
                .trim_start_matches("error: ");
            eprintln!("config.json: ignoring the regex of {owner} ({pattern:?}): {reason}");
            String::new()
        }
    }
}

fn regex_pairs(v: &Value, key: &str) -> Vec<(String, String)> {
    pairs(&v[key])
        .into_iter()
        .map(|(thread, re)| {
            let re = checked_regex(&format!("{key} entry for '{thread}'"), re);
            (thread, re)
        })
        .collect()
}

fn thread_rule(v: &Value) -> ThreadRule {
    let name = text(v, "name");
    let state = text(v, "state");
    ThreadRule {
        regex: checked_regex(&format!("thread '{name}'"), text(v, "regex")),
        name,
        area: text(v, "area"),
        state: if state.is_empty() {
            "open".into()
        } else {
            state
        },
        summary: text(v, "summary"),
        kind: text(v, "kind"),
        parent: text(v, "parent"),
    }
}

fn checked_parents(mut rules: Vec<ThreadRule>) -> Vec<ThreadRule> {
    let declared: HashMap<String, String> = rules
        .iter()
        .map(|t| (t.name.clone(), t.parent.clone()))
        .collect();
    for t in &mut rules {
        if t.parent.is_empty() {
            continue;
        }
        let reason = match declared.get(&t.parent) {
            None => Some("no thread has that name"),
            Some(_) if t.parent == t.name => Some("a thread cannot be its own parent"),
            Some(grand) if !grand.is_empty() => Some("the parent has a parent itself"),
            Some(_) => None,
        };
        if let Some(reason) = reason {
            eprintln!(
                "config.json: ignoring the parent of thread '{}' ({:?}): {reason}",
                t.name, t.parent
            );
            t.parent.clear();
        }
    }
    rules
}

fn embeddings(v: &Value) -> Embeddings {
    if v == &Value::Bool(false) {
        return Embeddings::default();
    }
    let get = |key: &str| v[key].as_str().map(String::from);
    let model = get("model").unwrap_or_else(|| DEFAULT_EMBED_MODEL.into());
    let (query, document) = prefixes(&model);
    Embeddings {
        url: get("url")
            .unwrap_or_else(|| DEFAULT_EMBED_URL.into())
            .trim_end_matches('/')
            .to_string(),
        query_prefix: get("query_prefix").unwrap_or_else(|| query.into()),
        document_prefix: get("document_prefix").unwrap_or_else(|| document.into()),
        batch_cap: usize::try_from(v["batch_cap"].as_u64().unwrap_or(2000)).unwrap_or(usize::MAX),
        model,
    }
}

fn load() -> Config {
    let home = crate::db::home();
    let read = |name: &str| -> Value {
        std::fs::read_to_string(home.join(name))
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or(Value::Null)
    };
    let v = read("config.json");
    let threads = if v.is_null() {
        read("threads.json")
    } else {
        v["threads"].clone()
    };
    Config {
        workspace: PathBuf::from(text(&v, "workspace")),
        note_dirs: strings(&v["note_dirs"]),
        screens_dir: text(&v, "screens_dir"),
        threads: checked_parents(
            threads
                .as_array()
                .map(|a| a.iter().map(thread_rule).collect())
                .unwrap_or_default(),
        ),
        note_rules: regex_pairs(&v, "note_rules"),
        artifact_rules: regex_pairs(&v, "artifact_rules"),
        repos: pairs(&v["repos"]),
        aliases: v["aliases"]
            .as_object()
            .map(|o| o.iter().map(|(k, x)| (k.clone(), strings(x))).collect())
            .unwrap_or_default(),
        generic_projects: if v["generic_projects"].is_array() {
            strings(&v["generic_projects"])
        } else {
            vec!["home".into()]
        },
        entities: v["entities"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|e| EntitySeed {
                        name: text(e, "name"),
                        kind: text(e, "kind"),
                        aliases: strings(&e["aliases"])
                            .iter()
                            .map(|s| crate::db::norm(s))
                            .collect(),
                    })
                    .filter(|e| !e.name.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        port: v["port"]
            .as_u64()
            .and_then(|p| u16::try_from(p).ok())
            .unwrap_or(8799),
        embeddings: embeddings(&v["embeddings"]),
        daily_cap: v["compile"]["daily_cap"].as_i64(),
        agent: v["agent"]["cli"].as_str().unwrap_or("claude").to_string(),
    }
}

pub fn cfg() -> &'static Config {
    static CONFIG: std::sync::OnceLock<Config> = std::sync::OnceLock::new();
    CONFIG.get_or_init(load)
}
