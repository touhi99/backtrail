use anyhow::Context;
use serde_json::{Value, json};
use std::path::PathBuf;

fn commands() -> anyhow::Result<[(&'static str, String, Value); 3]> {
    let exe = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .map_err(|e| anyhow::anyhow!("cannot find this program: {e}"))?;
    let script = exe
        .ancestors()
        .map(|a| a.join("skill/session-end.sh"))
        .find(|p| p.exists())
        .context(
            "skill/session-end.sh not found next to the build; run hooks from a built checkout",
        )?;
    Ok([
        (
            "SessionStart",
            format!("{} brief", exe.display()),
            json!({"timeout": 10, "statusMessage": "Backtrail brief"}),
        ),
        (
            "UserPromptSubmit",
            format!("{} recall", exe.display()),
            json!({"timeout": 5}),
        ),
        (
            "SessionEnd",
            script.display().to_string(),
            json!({"timeout": 10}),
        ),
    ])
}

fn settings_path() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".claude/settings.json")
}

fn ours(cmd: &str, want: &str) -> bool {
    cmd == want || cmd == "bt brief" && want.ends_with(" brief")
}

fn has(v: &Value, event: &str, want: &str) -> bool {
    v["hooks"][event]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|g| g["hooks"].as_array().into_iter().flatten())
        .any(|h| ours(h["command"].as_str().unwrap_or(""), want))
}

pub fn run(args: &[String]) {
    let path = settings_path();
    let raw = std::fs::read_to_string(&path).ok();
    let mut v: Value = match &raw {
        Some(r) => serde_json::from_str(r).unwrap_or_else(|e| {
            eprintln!(
                "{} is not valid JSON ({e}); nothing changed",
                path.display()
            );
            std::process::exit(1);
        }),
        None => json!({}),
    };
    let cmds = commands().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let action = args.get(1).map_or("status", std::string::String::as_str);
    if action == "status" {
        for (event, cmd, _) in &cmds {
            println!(
                "{event}: {}",
                if has(&v, event, cmd) {
                    "installed"
                } else {
                    "not installed"
                }
            );
        }
        println!(
            "`backtrail hooks install` or `backtrail hooks remove` changes {}",
            path.display()
        );
        return;
    }
    if action != "install" && action != "remove" {
        eprintln!("usage: backtrail hooks [status|install|remove]");
        std::process::exit(1);
    }
    if let Some(r) = &raw {
        std::fs::write(path.with_extension("json.bak-backtrail"), r).ok();
    }
    for (event, cmd, extra) in &cmds {
        if action == "install" {
            if has(&v, event, cmd) {
                println!("{event}: already installed");
                continue;
            }
            let mut hook = json!({"type": "command", "command": cmd});
            for (k, x) in extra.as_object().expect("hook extras are a JSON object") {
                hook[k] = x.clone();
            }
            let arr = v["hooks"][event].take();
            let mut arr = arr.as_array().cloned().unwrap_or_default();
            arr.push(json!({"hooks": [hook]}));
            v["hooks"][event] = Value::Array(arr);
            println!("{event}: added `{cmd}`");
        } else if let Some(groups) = v["hooks"][event].as_array_mut() {
            for g in groups.iter_mut() {
                if let Some(h) = g["hooks"].as_array_mut() {
                    h.retain(|x| !ours(x["command"].as_str().unwrap_or(""), cmd));
                }
            }
            groups.retain(|g| g["hooks"].as_array().is_none_or(|h| !h.is_empty()));
            println!("{event}: removed");
        }
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(&v).expect("a JSON value serializes")
    );
    if let Err(e) = std::fs::write(&path, text) {
        eprintln!("cannot write {}: {e}", path.display());
        std::process::exit(1);
    }
    println!(
        "written {} (copy of the old file: settings.json.bak-backtrail)",
        path.display()
    );
}
