mod compile;
mod db;
mod embed;
mod ingest;
mod server;

use db::*;
use serde_json::Value;

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("help");
    let c = open();
    match cmd {
        "ingest" => {
            if home().join(".demo").exists() { eprintln!("demo store: ingest is disabled"); std::process::exit(1); }
            ingest::seed_threads(&c);
            let (a, sa) = ingest::ingest_sessions(&c);
            let (b, sb) = ingest::ingest_notes(&c);
            let (d, sd) = ingest::ingest_artifacts(&c);
            let (g, sg) = ingest::ingest_git(&c);
            println!("git repos {g} ({sg} unchanged)");
            ingest::rethread(&c);
            let (ok, lost) = ingest::relink(&c);
            println!("items relinked {ok}, quote no longer found {lost}");
            if a + b + d + g > 0 { logw(&c, "ingest", "ingest", "sources", &format!("sessions {a}, notes {b}, artifacts {d}, git repos {g}")); }
            println!("sessions {a} new/changed ({sa} unchanged), notes {b} ({sb}), artifacts {d} ({sd})");
        }
        "add" => {
            // backtrail add <type> --thread T --title X --source REF --quote Q [--body B] [--state S] [--origin O]
            let it = Item {
                typ: args.get(1).cloned().unwrap_or_default(),
                thread: arg(&args, "--thread").unwrap_or_default(),
                title: arg(&args, "--title").unwrap_or_default(),
                body: arg(&args, "--body").unwrap_or_default(),
                source: arg(&args, "--source").unwrap_or_default(),
                quote: arg(&args, "--quote").unwrap_or_default(),
                state: arg(&args, "--state"),
                origin: arg(&args, "--origin").unwrap_or("compiled".into()),
                date: arg(&args, "--date"),
            };
            match add_item(&c, &it) { Ok(id) => println!("ok {id}"), Err(e) => { eprintln!("rejected: {e}"); std::process::exit(1); } }
        }
        "import" => {
            let raw = if args.get(1).map(|s| s.as_str()) == Some("-") { let mut s = String::new(); std::io::Read::read_to_string(&mut std::io::stdin(), &mut s).expect("stdin"); s } else { std::fs::read_to_string(args.get(1).expect("file")).expect("read") };
            let v: Value = serde_json::from_str(&raw).expect("json");
            let (mut ok, mut bad) = (0, 0);
            for x in v.as_array().expect("array") {
                let g = |k: &str| x[k].as_str().unwrap_or("").to_string();
                let it = Item { typ: g("type"), thread: g("thread"), title: g("title"), body: g("body"), source: g("source"), quote: g("quote"),
                    state: x["state"].as_str().map(|s| s.to_string()), origin: x["origin"].as_str().unwrap_or("compiled").into(), date: x["date"].as_str().map(|s| s.to_string()) };
                match add_item(&c, &it) { Ok(_) => ok += 1, Err(e) => { bad += 1; eprintln!("rejected [{}]: {e}", it.title); } }
            }
            println!("imported {ok}, rejected {bad}");
        }
        "set-thread" => {
            // backtrail set-thread NAME --state moving --summary "..."
            let name = args.get(1).expect("name");
            if let Some(s) = arg(&args, "--state") { c.execute("UPDATE nodes SET state=?2 WHERE type='thread' AND thread=?1", rusqlite::params![name, s]).unwrap(); }
            if let Some(s) = arg(&args, "--summary") { c.execute("UPDATE nodes SET body=?2 WHERE type='thread' AND thread=?1", rusqlite::params![name, s]).unwrap(); }
            println!("ok");
        }
        "done" => {
            // backtrail done <id> --by claude|evidence --proof "what shows it"
            let id = args.get(1).expect("id");
            let by = arg(&args, "--by").unwrap_or("claude".into());
            let proof = arg(&args, "--proof").unwrap_or_default();
            if by != "owner" && proof.trim().is_empty() { eprintln!("rejected: a step closed by Claude or evidence needs --proof"); std::process::exit(1); }
            let n = c.execute("UPDATE nodes SET state='done', done_by=?2, answer=?3, updated=?4 WHERE id=?1 AND type='step'", rusqlite::params![id, by, proof, chrono_now()]).unwrap();
            logw(&c, &by, "done", id, &proof);
            println!("{}", if n == 1 { "ok" } else { "no such step" });
        }
        "compile-input" => compile::input(&c, args.get(1).expect("session ref")),
        "compile-queue" => compile::queue(&c, arg(&args, "--session"), arg(&args, "--limit").and_then(|d| d.parse().ok()).unwrap_or(5)),
        "compile-log" => compile::record(&c, args.get(1).expect("session ref"), arg(&args, "--failed").is_none(), &arg(&args, "--detail").unwrap_or_default()),
        "retire" => compile::retire(&c, args.get(1).expect("id"), arg(&args, "--by"), &arg(&args, "--reason").unwrap_or_default()),
        "assign" => compile::assign(&c, args.get(1).expect("session ref"), args.get(2).expect("thread")),
        "flag" => {
            // backtrail flag <id> --as live|in_effect|superseded|done --reason "..."  (proposed items only; the owner still accepts or rejects)
            let id = args.get(1).expect("id");
            let kind = arg(&args, "--as").unwrap_or_default();
            if !["live", "in_effect", "superseded", "done"].contains(&kind.as_str()) { eprintln!("rejected: --as must be live, in_effect, superseded or done"); std::process::exit(1); }
            let reason: String = arg(&args, "--reason").unwrap_or_default().chars().take(200).collect();
            if kind != "live" && reason.trim().is_empty() { eprintln!("rejected: a flag other than live needs --reason"); std::process::exit(1); }
            let n = c.execute("UPDATE nodes SET stale=?2 WHERE id=?1 AND state='proposed'", rusqlite::params![id, format!("{kind}|{reason}")]).unwrap();
            if n == 1 { logw(&c, "claude", "flag", id, &format!("{kind}: {reason}")); }
            println!("{}", if n == 1 { "ok" } else { "no such proposed item" });
        }
        "reply" => {
            // backtrail reply <question id> --text "..."  (Claude answers a question the owner typed in the app)
            let id = args.get(1).expect("id");
            let tx: String = arg(&args, "--text").unwrap_or_default().chars().take(600).collect();
            if tx.trim().is_empty() { eprintln!("rejected: --text is required"); std::process::exit(1); }
            let n = c.execute("UPDATE nodes SET state='answered', answer=?2, done_by='claude', updated=?3 WHERE id=?1 AND type='question' AND origin='owner' AND state='open'", rusqlite::params![id, tx, chrono_now()]).unwrap();
            if n == 1 { logw(&c, "claude", "reply", id, &tx); }
            println!("{}", if n == 1 { "ok" } else { "no such open question from the owner" });
        }
        "built" => {
            // backtrail built <step id> --evidence "..."  (Claude says a step is built; the owner still confirms)
            let id = args.get(1).expect("id");
            let ev: String = arg(&args, "--evidence").unwrap_or_default().chars().take(300).collect();
            if ev.trim().is_empty() { eprintln!("rejected: --evidence is required"); std::process::exit(1); }
            let now = chrono_now();
            let n = c.execute("UPDATE nodes SET state='looks_done', done_by='claude', answer=?2, updated=?3 WHERE id=?1 AND type='step' AND state IN ('open','doing')", rusqlite::params![id, ev, now]).unwrap();
            if n == 1 { logw(&c, "claude", "built", id, &ev); }
            println!("{}", if n == 1 { "ok" } else { "no such open step" });
        }
        "brief" => {
            if std::env::var("BACKTRAIL_COMPILE").is_ok() { return; }
            if !home().join(".demo").exists() { server::rescan(&c); }
            server::brief(&c, arg(&args, "--thread"))
        }
        "backup" => println!("{}", server::backup(&c).unwrap_or("already backed up today".into())),
        "check" => if args.iter().any(|a| a == "--pairs") { server::pairs(&c) } else { server::check(&c) },
        "match" => server::match_evidence(&c),
        "artifacts" => server::artifacts(&c, arg(&args, "--manifest")),
        "embed" => embed::run_cli(&c, arg(&args, "--limit").and_then(|d| d.parse().ok()).unwrap_or(usize::MAX)),
        "search" => {
            // backtrail search <words> [--thread T] [--kind session|note|artifact|commit] [--state S] [--from YYYY-MM-DD] [--to YYYY-MM-DD] [--text]
            let flags = ["--thread", "--kind", "--state", "--from", "--to"];
            let mut words = vec![]; let mut i = 1;
            while i < args.len() { if flags.contains(&args[i].as_str()) { i += 2 } else if args[i] == "--text" { i += 1 } else { words.push(args[i].clone()); i += 1 } }
            let f = server::Filters { thread: arg(&args, "--thread").unwrap_or_default(), kind: arg(&args, "--kind").unwrap_or_default(), state: arg(&args, "--state").unwrap_or_default(), from: arg(&args, "--from").unwrap_or_default(), to: arg(&args, "--to").unwrap_or_default() };
            server::search_cli(&c, &words.join(" "), &f, !args.iter().any(|a| a == "--text"))
        }
        "log" => server::log_cli(&c, arg(&args, "--n").and_then(|d| d.parse().ok()).unwrap_or(25)),
        "relate-input" => compile::relate_input(&c, args.get(1).expect("session ref")),
        "relate" => {
            // backtrail relate "A" "B" "spun out of" --source session:xxxxxxxx --quote "..." [--state proposed]
            let (a, b, k) = (args.get(1).expect("from thread"), args.get(2).expect("to thread"), args.get(3).expect("kind"));
            match compile::relate(&c, a, b, k, &arg(&args, "--source").unwrap_or_default(), &arg(&args, "--quote").unwrap_or_default(), &arg(&args, "--state").unwrap_or("proposed".into())) {
                Ok(()) => println!("ok"),
                Err(e) => { eprintln!("rejected: {e}"); std::process::exit(1) }
            }
        }
        "link" => {
            // backtrail link "A" "B" "built on"
            let (a, b, k) = (args.get(1).expect("from thread"), args.get(2).expect("to thread"), args.get(3).cloned().unwrap_or("built on".into()));
            c.execute("INSERT OR IGNORE INTO links(a,b,kind) VALUES(?1,?2,?3)", rusqlite::params![format!("T:{a}"), format!("T:{b}"), k]).unwrap();
            logw(&c, "claude", "link", &format!("{a} -> {b}"), &k);
            println!("ok");
        }
        "pending" => server::pending(&c, arg(&args, "--days").and_then(|d| d.parse().ok()).unwrap_or(3)),
        "show" => server::show(&c, args.get(1).expect("source ref")),
        "demo" => server::demo(&c),
        "serve" => server::serve(c, arg(&args, "--port").and_then(|p| p.parse().ok()).unwrap_or(ingest::cfg().port)),
        _ => println!("backtrail ingest | check | match | artifacts | search <q> [--thread --kind --state --from --to --text] | embed [--limit N] | log | link A B kind | pending [--days N] | show <ref> | demo (needs BACKTRAIL_HOME) | add <type> --thread --title --source --quote | import <items.json> | set-thread | compile-input <ref> | compile-queue [--session ID] | compile-log <ref> | retire <id> --by <id> --reason | assign <ref> <thread> | reply <q> --text TEXT | built <step> --evidence TEXT | flag <id> --as live|in_effect|superseded|done --reason | brief [--thread T] | serve [--port N]"),
    }
}
