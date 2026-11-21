use crate::{
    check, clock, config, coverage, db, embed, finish, hooks, howto, ingest, items, kg, recall,
    resolve, search, server, views,
};

use super::{Args, fail, setup};

const USAGE: &str = "backtrail ingest [--force] | build | check [--pairs] | match | resolve <id> --by <id> --reason T | resolve --expired | unresolve <id> | resolve-queue [--limit N] | resolve-packet <id> | resolve-apply <file|-> | howto-queue [--limit N] [--again] | howto-packet <id> | howto-apply | artifacts [--manifest FILE] | \
search <q> [--thread --kind --state --from --to --text] | embed [--limit N] | log [--n N] | link A B kind | \
relate A B kind --source REF --quote Q [--state S] | status [--all] | ended <session id> | home | skip <session ref> | show <ref> | \
add <type> --thread --title --source --quote [--body --check --state --origin --date] | import <facts.json|-> | \
set-thread NAME [--state S] [--summary X] | done <step> [--by B] [--proof P] | built <step> --evidence TEXT [--check TEXT] | \
flag <id> --as live|in_effect|superseded|done --reason R | reply <q> --text TEXT | retire <id> [--by ID] --reason R | \
assign <ref> <thread> | suggest-threads | extract-input <ref> | finish <step> --source REF --quote Q [--verified] [--dry] | finish-input <step> | finish-evidence <step> --source REF --quote Q | finish-queue | extract-queue [--limit N] [--session ID] [--entity NAME] | \
extract-log <ref> [--failed 1] [--detail D] | kg [state|export|restore|lint|suggest|merge|entity|page|trace|why|neighbors|path|passage|find] | \
brief [--thread T | --all] [--hook H] | recall [--text PROMPT [--thread T]] [--hook H] | backup | serve [--port N] | hooks [status|install|remove] [--harness H] | agent [claude|gemini|codex|antigravity|command]";

const HELP: &str =
    "Backtrail: decisions, steps and questions from coding-agent sessions, each with its source.

  bt build                    index sessions and notes; offers to read recent sessions
  bt brief [--thread T|--all] what was decided and what is next
  bt search <words>           search with cited sources
  bt serve [--port N]         web app on port 8799
  bt hooks [status|install|remove] [--harness claude|gemini|codex|antigravity]
  bt status [--all]           which sessions are read into the trail, and why not
  bt check                    lost quotes, orphan threads, stale work, claims GitHub contradicts
  bt prs [--all]              read the GitHub state of PRs that sessions opened

Every command: bt help all. Reference: docs/reference.md";

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "private helper; every call passes another function's result"
)]
fn report(changed: bool, missing: &str) {
    println!("{}", if changed { "ok" } else { missing });
}

fn report_or_fail(result: anyhow::Result<bool>, missing: &str) {
    match result {
        Ok(changed) => report(changed, missing),
        Err(e) => fail(&e),
    }
}

fn hook_harness(a: &Args) -> Option<hooks::Harness> {
    a.opt("--hook")
        .map(|n| hooks::Harness::parse(&n).unwrap_or_else(|| fail(format!("unknown --hook '{n}'"))))
}

fn exit_quietly_on_closed_stdout() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info.payload();
        let msg = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("");
        if msg.starts_with("failed printing to stdout") && msg.contains("Broken pipe") {
            std::process::exit(0);
        }
        default(info);
    }));
}

pub(super) fn run() {
    exit_quietly_on_closed_stdout();
    let a = Args(std::env::args().skip(1).collect());
    let cmd = a.0.first().map_or("help", String::as_str);
    if cmd == "hooks" {
        return hooks::run(&a.0);
    }
    if cmd == "agent" {
        return setup::agent(a.0.get(1).map(String::as_str));
    }
    let quiet = cmd == "brief" || cmd == "recall" && !a.has("--text");
    if quiet && std::env::var("BACKTRAIL_COMPILE").is_ok() {
        return;
    }
    let c = db::open();
    match cmd {
        "ingest" => {
            if a.has("--force") {
                c.execute(
                    "UPDATE sources SET mtime=0, bytes=0 WHERE kind='session'",
                    [],
                )
                .expect("write to database");
                c.execute("DELETE FROM scanned WHERE path LIKE '%.jsonl'", [])
                    .expect("write to database");
            }
            setup::ingest_command(&c);
        }
        "build" => {
            setup::ingest_command(&c);
            println!("entities: {}", kg::entities::sync_configured(&c));
            match embed::run(&c, usize::MAX) {
                Ok((n, left)) => println!("embedded {n} passages, {left} pending"),
                Err(e) => eprintln!(
                    "embed failed: {e}\nOllama is required. Start it, run `ollama pull {}` if the model is missing, \
                     then `backtrail embed`.",
                    config::DEFAULT_EMBED_MODEL
                ),
            }
            let unread =
                kg::extract::queue(&c, usize::MAX, None, None, kg::extract::Cap::Ignore).len();
            if unread > 0 {
                println!(
                    "{unread} sessions queued; the server reads them in the background, about {:.0} model calls each.",
                    kg::extract::calls_each(&c)
                );
            }
        }
        "add" => {
            let it = items::NewItem {
                kind: a.0.get(1).cloned().unwrap_or_default(),
                thread: a.text("--thread"),
                title: a.text("--title"),
                body: a.text("--body"),
                source: a.text("--source"),
                quote: a.text("--quote"),
                state: a.opt("--state"),
                origin: a.opt("--origin").unwrap_or("compiled".into()),
                date: a.opt("--date"),
                entities: a
                    .text("--entities")
                    .split(',')
                    .map(|e| e.trim().to_string())
                    .filter(|e| !e.is_empty())
                    .collect(),
                valid_to: a.opt("--valid-to"),
                check_how: a.opt("--check"),
            };
            match items::add(&c, &it) {
                Ok(id) => {
                    println!("ok {id}");
                    resolve::print_candidates(&c, &id);
                }
                Err(e) => fail(&e),
            }
        }
        "import" => setup::import(&c, &a.at(1, "file")),
        "set-thread" => {
            let name = a.at(1, "thread name");
            if let Some(state) = a.opt("--state") {
                c.execute("UPDATE threads SET state=?2 WHERE name=?1", [&name, &state])
                    .expect("write to database");
            }
            if let Some(summary) = a.opt("--summary") {
                c.execute(
                    "UPDATE threads SET body=?2 WHERE name=?1",
                    [&name, &summary],
                )
                .expect("write to database");
            }
            println!("ok");
        }
        "done" => report_or_fail(
            items::done(
                &c,
                &a.at(1, "id"),
                &a.opt("--by").unwrap_or("claude".into()),
                &a.text("--proof"),
            ),
            "no such step",
        ),
        "built" => report_or_fail(
            items::built(&c, &a.at(1, "id"), &a.text("--evidence"), a.opt("--check")),
            "no such open step",
        ),
        "flag" => report_or_fail(
            items::flag(&c, &a.at(1, "id"), &a.text("--as"), &a.text("--reason")),
            "no such proposed item",
        ),
        "answer" => report_or_fail(
            items::answer(&c, &a.at(1, "id"), a.text("--text").trim()),
            "no such open question",
        ),
        "reply" => report_or_fail(
            items::reply(&c, &a.at(1, "id"), &a.text("--text")),
            "no such open question from the owner",
        ),
        "retire" => report(
            items::retire(
                &c,
                &a.at(1, "id"),
                a.opt("--by").as_deref(),
                &a.text("--reason"),
            ),
            "no such live item",
        ),
        "suggest-threads" => ingest::threads::suggest(&c),
        "assign" => match ingest::threads::pin(&c, &a.at(1, "session ref"), &a.at(2, "thread")) {
            Ok(found) => report(found, "no such session"),
            Err(e) => fail(&e),
        },
        "relate" => {
            let state = a.opt("--state").unwrap_or("proposed".into());
            let kind = a.at(3, "kind");
            match kg::claims::relate(
                &c,
                &a.at(1, "from thread"),
                &a.at(2, "to thread"),
                &kind,
                &a.text("--source"),
                &a.text("--quote"),
                &state,
            ) {
                Ok(()) => println!("ok"),
                Err(e) => fail(&e),
            }
        }
        "link" => {
            let (from, to) = (a.at(1, "from thread"), a.at(2, "to thread"));
            let kind = a.0.get(3).cloned().unwrap_or("built on".into());
            kg::claims::owner_relation(&c, &from, &to, &kind, &clock::now());
            db::logw(&c, "claude", "link", &format!("{from} -> {to}"), &kind);
            println!("ok");
        }
        "extract-input" => kg::extract::input(
            &c,
            &a.at(1, "session ref"),
            if a.has("--full") {
                kg::extract::Turns::All
            } else {
                kg::extract::Turns::New
            },
        ),
        "finish" => match finish::finish(
            &c,
            &a.at(1, "step id"),
            &a.text("--source"),
            &a.text("--quote"),
            if a.has("--verified") {
                finish::Evidence::Confirmed
            } else {
                finish::Evidence::Unconfirmed
            },
            if a.has("--dry") {
                finish::Run::Dry
            } else {
                finish::Run::Apply
            },
        ) {
            Ok(line) => println!("{line}"),
            Err(e) => fail(&e),
        },
        "finish-input" => finish::input(&c, &a.at(1, "step id")),
        "finish-evidence" => finish::evidence(
            &c,
            &a.at(1, "step id"),
            &a.text("--source"),
            &a.text("--quote"),
        ),
        "finish-queue" => finish::queue(&c).iter().for_each(|id| println!("{id}")),
        "extract-queue" => {
            let queue = kg::extract::queue(
                &c,
                a.num("--limit", 3),
                a.opt("--entity").as_deref(),
                a.opt("--session").as_deref(),
                if a.has("--no-cap") {
                    kg::extract::Cap::Ignore
                } else {
                    kg::extract::Cap::Daily
                },
            );
            for r in queue {
                println!("{r}");
            }
        }
        "extract-log" => {
            let ok = a.opt("--failed").is_none();
            let source = db::resolve_ref(&c, &a.at(1, "session ref"));
            db::logw(
                &c,
                "extract",
                if ok { "extract" } else { "extract-failed" },
                &source,
                &a.text("--detail"),
            );
            println!("ok");
        }
        "recall" if a.has("--text") => {
            for h in recall::matches(&c, &a.text("--text"), a.opt("--thread").as_deref()) {
                println!("{:5.1}  {}  {}  ({})", h.score, h.day, h.title, h.id);
            }
        }
        "recall" => recall::hook(&c, hook_harness(&a)),
        "brief" => {
            if !db::is_demo() {
                ingest::rescan(&c);
            }
            let text = views::brief(
                &c,
                a.opt("--thread").as_deref(),
                if a.has("--all") {
                    views::Detail::Full
                } else {
                    views::Detail::Short
                },
            );
            let harness = hook_harness(&a);
            hooks::emit(harness, "SessionStart", &text);
        }
        "backup" => println!(
            "{}",
            ingest::backup(&c).unwrap_or("already backed up today".into())
        ),
        "keep" => println!("kept {}", items::keep_all(&c)),
        "check" if a.has("--pairs") => check::pairs(&c),
        "check" => check::check(&c),
        "match" => check::match_commits(&c),
        "prs" => {
            let r = crate::prs::refresh(
                &c,
                if a.has("--all") {
                    crate::prs::Repos::All
                } else {
                    crate::prs::Repos::Due
                },
            );
            for (repo, n) in &r.read {
                println!("{repo}: {n} pull requests");
            }
            for (repo, e) in &r.failed {
                eprintln!("{repo}: unchecked ({e})");
            }
            let (pushes, unread) = crate::prs::refresh_pushes(&c);
            println!("pushes checked: {pushes}; unchecked: {unread}");
            if r.read.is_empty() && r.failed.is_empty() {
                println!("no repo due; --all reads every repo");
            }
        }
        "resolve" => {
            if a.has("--expired") {
                println!("expired {}", resolve::expire(&c));
            } else {
                report_or_fail(
                    resolve::resolve(
                        &c,
                        &a.at(1, "item id"),
                        &a.text("--by"),
                        &a.text("--reason"),
                    ),
                    "no such open item",
                );
            }
        }
        "unresolve" => report(
            resolve::unresolve(&c, &a.at(1, "item id")),
            "not a resolved item",
        ),
        "resolve-packet" if a.has("--reverses") => resolve::reverse_packet(&c, &a.at(1, "item id")),
        "resolve-packet" => resolve::packet(&c, &a.at(1, "item id")),
        "resolve-apply" => resolve::apply_cli(&c, &a.at(1, "file")),
        "howto-queue" => howto::queue(
            &c,
            a.num("--limit", 30),
            if a.has("--again") {
                howto::Asked::Include
            } else {
                howto::Asked::Skip
            },
        ),
        "howto-packet" => howto::packet(&c, &a.at(1, "step id")),
        "howto-apply" => howto::apply_cli(&c),
        "resolve-queue" if a.has("--reverses") => resolve::reverse_queue(&c, a.num("--limit", 20)),
        "resolve-queue" => resolve::queue(&c, a.num("--limit", 20)),
        "artifacts" => check::artifacts(&c, a.opt("--manifest").as_deref()),
        "pending" | "status" => {
            if !coverage::status(
                &c,
                if a.has("--all") {
                    coverage::Show::All
                } else {
                    coverage::Show::Open
                },
            ) {
                std::process::exit(1);
            }
        }
        "ended" => {
            let r = db::resolve_ref(
                &c,
                &format!(
                    "session:{}",
                    a.at(1, "session id").trim_start_matches("session:")
                ),
            );
            db::logw(&c, "hook", "ended", &r, "");
        }
        "home" => println!("{}", db::home().display()),
        "skip" => {
            let r = db::resolve_ref(&c, &a.at(1, "session ref"));
            let id = db::source_id(&c, &r).unwrap_or(0);
            report(items::skip_session(&c, id, "claude"), "no such session");
        }
        "show" => check::show(&c, &a.at(1, "source ref")),
        "log" => check::log(&c, a.num("--n", 25)),
        "embed" => embed::run_cli(&c, a.num("--limit", usize::MAX)),
        "search" => {
            let mut words = vec![];
            let mut i = 1;
            while i < a.0.len() {
                match a.0[i].as_str() {
                    "--thread" | "--kind" | "--state" | "--from" | "--to" => i += 2,
                    "--text" => i += 1,
                    word => {
                        words.push(word);
                        i += 1;
                    }
                }
            }
            let filters = search::Filters {
                thread: a.text("--thread"),
                kind: a.text("--kind"),
                state: a.text("--state"),
                from: a.text("--from"),
                to: a.text("--to"),
            };
            search::print(
                &c,
                &words.join(" "),
                &filters,
                if a.has("--text") {
                    search::Mode::Text
                } else {
                    search::Mode::Semantic
                },
            );
        }
        "kg" => super::kg::kg_command(&c, &a),
        "serve" => {
            let port = a.num("--port", config::cfg().port);
            server::serve(&c, port);
        }
        "help" if a.0.get(1).is_some_and(|x| x == "all") => println!("{USAGE}"),
        _ => println!("{HELP}"),
    }
}
