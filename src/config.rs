//! `<data dir>/config.json`: everything that is specific to one machine. Every key is optional; without the file each
//! Claude Code project folder is a thread and only memory notes are indexed. Keys are described in docs/reference.md.
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

pub const DEFAULT_EMBED_MODEL: &str = "embeddinggemma-2:270m";
const DEFAULT_EMBED_URL: &str = "http://127.0.0.1:11434";

/// A thread defined in the config. `regex` is matched against session titles and project folder names.
pub struct ThreadRule {
    pub name: String,
    pub area: String,
    pub state: String,
    pub summary: String,
    pub kind: String,
    pub regex: String,
}

/// The embedding server. An empty `url` means embeddings are off.
#[derive(Default)]
pub struct Embeddings {
    pub url: String,
    pub model: String,
    pub query_prefix: String,
    pub document_prefix: String,
    /// Passages embedded per background run.
    pub batch_cap: usize,
}

/// An entity created from the config, with the spellings that count as a mention of it.
pub struct EntitySeed {
    pub name: String,
    pub kind: String,
    pub aliases: Vec<String>,
}

pub struct Config {
    /// Root folder that note and repository paths are relative to.
    pub workspace: PathBuf,
    pub note_dirs: Vec<String>,
    pub screens_dir: String,
    pub threads: Vec<ThreadRule>,
    /// `(thread name, regex)` pairs that assign notes and artifacts to threads.
    pub note_rules: Vec<(String, String)>,
    pub artifact_rules: Vec<(String, String)>,
    /// `(repository path, thread name)` pairs.
    pub repos: Vec<(String, String)>,
    /// Other names for a thread, used in search and for mention edges.
    pub aliases: HashMap<String, Vec<String>>,
    /// Project folder names that never decide a thread.
    pub generic_projects: Vec<String>,
    pub entities: Vec<EntitySeed>,
    pub port: u16,
    pub embeddings: Embeddings,
    /// Extraction runs allowed per day.
    pub daily_cap: i64,
}

/// Query and document prefixes each embedding model family expects; empty for unknown models.
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

fn thread_rule(v: &Value) -> ThreadRule {
    let state = text(v, "state");
    ThreadRule {
        name: text(v, "name"),
        area: text(v, "area"),
        state: if state.is_empty() {
            "open".into()
        } else {
            state
        },
        summary: text(v, "summary"),
        kind: text(v, "kind"),
        regex: text(v, "regex"),
    }
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
        batch_cap: v["batch_cap"].as_u64().unwrap_or(2000) as usize,
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
    // An older `threads.json` (an array of threads) is still read when there is no config.json.
    let threads = if v.is_null() {
        read("threads.json")
    } else {
        v["threads"].clone()
    };
    Config {
        workspace: PathBuf::from(text(&v, "workspace")),
        note_dirs: strings(&v["note_dirs"]),
        screens_dir: text(&v, "screens_dir"),
        threads: threads
            .as_array()
            .map(|a| a.iter().map(thread_rule).collect())
            .unwrap_or_default(),
        note_rules: pairs(&v["note_rules"]),
        artifact_rules: pairs(&v["artifact_rules"]),
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
        port: v["port"].as_u64().unwrap_or(8799) as u16,
        embeddings: embeddings(&v["embeddings"]),
        daily_cap: v["compile"]["daily_cap"].as_i64().unwrap_or(8),
    }
}

/// The config, read once per process.
pub fn cfg() -> &'static Config {
    static CONFIG: std::sync::OnceLock<Config> = std::sync::OnceLock::new();
    CONFIG.get_or_init(load)
}
