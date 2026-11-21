use anyhow::Context;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Harness {
    Claude,
    Gemini,
    Codex,
    Antigravity,
}

pub const ALL: [Harness; 4] = [
    Harness::Claude,
    Harness::Gemini,
    Harness::Codex,
    Harness::Antigravity,
];

const GROUP: &str = "backtrail";

struct Hook {
    event: &'static str,
    command: String,
    extra: Value,
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var(var)
        .ok()
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

impl Harness {
    pub fn parse(name: &str) -> Option<Self> {
        ALL.into_iter().find(|h| h.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::Codex => "codex",
            Self::Antigravity => "antigravity",
        }
    }

    fn gemini_dir() -> PathBuf {
        env_dir("GEMINI_CLI_HOME")
            .unwrap_or_else(home)
            .join(".gemini")
    }

    fn settings(self) -> PathBuf {
        match self {
            Self::Claude => home().join(".claude/settings.json"),
            Self::Gemini => Self::gemini_dir().join("settings.json"),
            Self::Codex => env_dir("CODEX_HOME")
                .unwrap_or_else(|| home().join(".codex"))
                .join("hooks.json"),
            Self::Antigravity => Self::gemini_dir().join("config/hooks.json"),
        }
    }

    fn skill_link(self) -> PathBuf {
        match self {
            Self::Claude => home().join(".claude/skills/backtrail"),
            Self::Gemini | Self::Codex => home().join(".agents/skills/backtrail"),
            Self::Antigravity => Self::gemini_dir().join("config/skills/backtrail"),
        }
    }

    fn hooks(self, exe: &Path, end: &Path) -> Vec<Hook> {
        let bt = exe.display();
        let hook = |event, command: String, extra| Hook {
            event,
            command,
            extra,
        };
        let tag = self.name();
        match self {
            Self::Claude => vec![
                hook(
                    "SessionStart",
                    format!("{bt} brief"),
                    json!({"timeout": 10, "statusMessage": "Backtrail brief"}),
                ),
                hook(
                    "UserPromptSubmit",
                    format!("{bt} recall"),
                    json!({"timeout": 5}),
                ),
                hook(
                    "SessionEnd",
                    end.display().to_string(),
                    json!({"timeout": 10}),
                ),
            ],
            Self::Gemini => vec![
                hook(
                    "SessionStart",
                    format!("{bt} brief --hook {tag}"),
                    json!({"name": "backtrail-brief", "timeout": 10000}),
                ),
                hook(
                    "BeforeAgent",
                    format!("{bt} recall --hook {tag}"),
                    json!({"name": "backtrail-recall", "timeout": 5000}),
                ),
                hook(
                    "SessionEnd",
                    end.display().to_string(),
                    json!({"name": "backtrail-end", "timeout": 10000}),
                ),
            ],
            Self::Codex => vec![
                hook(
                    "SessionStart",
                    format!("{bt} brief --hook {tag}"),
                    json!({"timeout": 10}),
                ),
                hook(
                    "UserPromptSubmit",
                    format!("{bt} recall --hook {tag}"),
                    json!({"timeout": 5}),
                ),
            ],
            Self::Antigravity => vec![
                hook(
                    "SessionStart",
                    format!("{bt} brief --hook {tag}"),
                    json!({}),
                ),
                hook(
                    "PreInvocation",
                    format!("{bt} recall --hook {tag}"),
                    json!({}),
                ),
            ],
        }
    }
}

pub fn emit(harness: Option<Harness>, event: &str, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let out = match harness {
        None | Some(Harness::Claude) => {
            print!("{text}");
            return;
        }
        Some(Harness::Gemini | Harness::Codex) => {
            json!({"hookSpecificOutput": {"hookEventName": event, "additionalContext": text}})
        }
        Some(Harness::Antigravity) if event == "SessionStart" => {
            json!({"injectSteps": [{"systemMessage": {"systemMessage": text}}]})
        }
        Some(Harness::Antigravity) => json!({"injectSteps": [{"ephemeralMessage": text}]}),
    };
    println!("{out}");
}

pub fn prompt_event(harness: Option<Harness>) -> &'static str {
    match harness {
        Some(Harness::Gemini) => "BeforeAgent",
        Some(Harness::Antigravity) => "PreInvocation",
        _ => "UserPromptSubmit",
    }
}

fn ours(cmd: &str, want: &str) -> bool {
    cmd == want || cmd == "bt brief" && want.ends_with(" brief")
}

fn has(v: &Value, h: Harness, hook: &Hook) -> bool {
    if h == Harness::Antigravity {
        return v[GROUP][hook.event]
            .as_array()
            .into_iter()
            .flatten()
            .any(|x| x["command"] == hook.command.as_str());
    }
    v["hooks"][hook.event]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|g| g["hooks"].as_array().into_iter().flatten())
        .any(|x| ours(x["command"].as_str().unwrap_or(""), &hook.command))
}

fn entry(hook: &Hook) -> Value {
    let mut e = json!({"type": "command", "command": hook.command});
    for (k, x) in hook.extra.as_object().into_iter().flatten() {
        e[k] = x.clone();
    }
    e
}

fn add(v: &mut Value, h: Harness, hook: &Hook) {
    if h == Harness::Antigravity {
        if !v[GROUP].is_object() {
            v[GROUP] = json!({});
        }
        v[GROUP][hook.event] = json!([entry(hook)]);
        return;
    }
    let mut groups = v["hooks"][hook.event]
        .take()
        .as_array()
        .cloned()
        .unwrap_or_default();
    groups.push(json!({"hooks": [entry(hook)]}));
    v["hooks"][hook.event] = Value::Array(groups);
}

fn drop_hook(v: &mut Value, h: Harness, hook: &Hook) -> bool {
    if h == Harness::Antigravity {
        return v.as_object_mut().is_some_and(|o| o.remove(GROUP).is_some());
    }
    let Some(groups) = v
        .get_mut("hooks")
        .and_then(|e| e.get_mut(hook.event))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut dropped = false;
    for g in groups.iter_mut() {
        if let Some(list) = g.get_mut("hooks").and_then(Value::as_array_mut) {
            let before = list.len();
            list.retain(|x| !ours(x["command"].as_str().unwrap_or(""), &hook.command));
            dropped |= list.len() != before;
        }
    }
    groups.retain(|g| g["hooks"].as_array().is_none_or(|l| !l.is_empty()));
    dropped
}

fn skill_state(link: &Path, skill: &Path) -> &'static str {
    match std::fs::read_link(link) {
        Ok(t) if t == skill => "linked",
        Ok(_) => "linked elsewhere",
        Err(_) if link.exists() => "a folder is already there",
        Err(_) => "not linked",
    }
}

fn installed(h: Harness, exe: &Path, end: &Path) -> bool {
    let v: Value = std::fs::read_to_string(h.settings())
        .ok()
        .and_then(|r| serde_json::from_str(&r).ok())
        .unwrap_or_default();
    h.hooks(exe, end).iter().any(|hook| has(&v, h, hook))
}

fn link_skill(h: Harness, skill: &Path, action: &str) {
    let install = action == "install";
    let link = h.skill_link();
    let state = skill_state(&link, skill);
    if !install
        && let Ok((exe, end, _)) = paths()
        && let Some(other) = ALL
            .into_iter()
            .find(|o| *o != h && o.skill_link() == link && installed(*o, &exe, &end))
    {
        println!(
            "  skill: kept {} ({} uses it)",
            link.display(),
            other.name()
        );
        return;
    }
    if install && state == "not linked" {
        if let Some(dir) = link.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        match std::os::unix::fs::symlink(skill, &link) {
            Ok(()) => println!("  skill: linked {}", link.display()),
            Err(e) => println!("  skill: cannot link {}: {e}", link.display()),
        }
    } else if !install && state == "linked" {
        std::fs::remove_file(&link).ok();
        println!("  skill: removed {}", link.display());
    } else {
        println!("  skill: {state} ({})", link.display());
    }
}

fn paths() -> anyhow::Result<(PathBuf, PathBuf, PathBuf)> {
    let exe = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .map_err(|e| anyhow::anyhow!("cannot find this program: {e}"))?;
    let skill = exe
        .ancestors()
        .map(|a| a.join("skill"))
        .find(|p| p.join("session-end.sh").exists())
        .context(
            "skill/session-end.sh not found next to the build; run hooks from a built checkout",
        )?;
    Ok((exe, skill.join("session-end.sh"), skill))
}

fn one(h: Harness, action: &str) -> anyhow::Result<()> {
    let (exe, end, skill) = paths()?;
    let path = h.settings();
    let raw = std::fs::read_to_string(&path).ok();
    let mut v: Value = match &raw {
        Some(r) if !r.trim().is_empty() => serde_json::from_str(r)
            .with_context(|| format!("{} is not valid JSON; nothing changed", path.display()))?,
        _ => json!({}),
    };
    let hooks = h.hooks(&exe, &end);
    println!("{} ({})", h.name(), path.display());
    if action == "status" {
        for hook in &hooks {
            let state = if has(&v, h, hook) {
                "installed"
            } else {
                "not installed"
            };
            println!("  {}: {state}", hook.event);
        }
        println!(
            "  skill: {} ({})",
            skill_state(&h.skill_link(), &skill),
            h.skill_link().display()
        );
        return Ok(());
    }
    let mut changed = false;
    for hook in &hooks {
        if action == "install" {
            if has(&v, h, hook) {
                println!("  {}: already installed", hook.event);
                continue;
            }
            add(&mut v, h, hook);
            changed = true;
            println!("  {}: added `{}`", hook.event, hook.command);
        } else if drop_hook(&mut v, h, hook) {
            changed = true;
            println!("  {}: removed", hook.event);
        } else {
            println!("  {}: not installed", hook.event);
        }
    }
    if !changed {
        link_skill(h, &skill, action);
        return Ok(());
    }
    if let Some(r) = &raw {
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().to_string());
        std::fs::write(path.with_file_name(format!("{name}.bak-backtrail")), r).ok();
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(&v).expect("a JSON value serializes")
    );
    std::fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
    link_skill(h, &skill, action);
    if action == "install" && h == Harness::Codex {
        println!("  Codex runs new hooks after they are trusted: open /hooks in Codex once.");
    }
    Ok(())
}

pub fn run(args: &[String]) {
    let action = args
        .get(1)
        .filter(|a| !a.starts_with("--"))
        .map_or("status", String::as_str);
    if !["status", "install", "remove"].contains(&action) {
        eprintln!(
            "usage: backtrail hooks [status|install|remove] [--harness claude|gemini|codex|antigravity]..."
        );
        std::process::exit(1);
    }
    let mut chosen: Vec<Harness> = vec![];
    for (i, a) in args.iter().enumerate() {
        if a == "--harness" {
            let name = args.get(i + 1).map_or("", String::as_str);
            for n in name.split(',').filter(|n| !n.is_empty()) {
                let Some(h) = Harness::parse(n) else {
                    eprintln!("unknown harness '{n}': claude, gemini, codex or antigravity");
                    std::process::exit(1);
                };
                chosen.push(h);
            }
        }
    }
    if chosen.is_empty() {
        chosen = if action == "status" {
            ALL.to_vec()
        } else {
            vec![Harness::Claude]
        };
    }
    for h in chosen {
        if let Err(e) = one(h, action) {
            eprintln!("{}: {e}", h.name());
            std::process::exit(1);
        }
    }
}
