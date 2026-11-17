//! The `backtrail` command. Each command parses its arguments and calls one function; the work happens in the
//! modules below.
mod check;
mod clock;
mod config;
mod db;
mod demo;
mod embed;
mod hooks;
mod howto;
mod ingest;
mod items;
mod kg;
mod migrate;
mod resolve;
mod search;
mod server;
mod views;

use rusqlite::Connection;
use serde_json::Value;

const USAGE: &str = "backtrail ingest [--force] | build | check [--pairs] | match | resolve <id> --by <id> --reason T | resolve --expired | unresolve <id> | resolve-queue [--limit N] | resolve-packet <id> | resolve-apply <file|-> | howto-queue [--limit N] [--again] | howto-packet <id> | howto-apply | artifacts [--manifest FILE] | \
search <q> [--thread --kind --state --from --to --text] | embed [--limit N] | log [--n N] | link A B kind | \
relate A B kind --source REF --quote Q [--state S] | pending [--days N] | skip <session ref> | show <ref> | demo (needs BACKTRAIL_HOME) | \
add <type> --thread --title --source --quote [--body --check --state --origin --date] | import <facts.json|-> | \
set-thread NAME [--state S] [--summary X] | done <step> [--by B] [--proof P] | built <step> --evidence TEXT [--check TEXT] | \
flag <id> --as live|in_effect|superseded|done --reason R | reply <q> --text TEXT | retire <id> [--by ID] --reason R | \
assign <ref> <thread> | suggest-threads | extract-input <ref> | extract-queue [--limit N] [--session ID] [--entity NAME] | \
extract-log <ref> [--failed 1] [--detail D] | kg [state|export|restore|lint|suggest|merge|entity|page|trace|why|neighbors|path|passage|find] | \
brief [--thread T | --all] | backup | serve [--port N] | hooks";

/// The command-line arguments after the command name.
struct Args(Vec<String>);

impl Args {
    /// The value after `--name`.
    fn opt(&self, name: &str) -> Option<String> {
        self.0
            .iter()
            .position(|a| a == name)
            .and_then(|i| self.0.get(i + 1))
            .cloned()
    }
    fn text(&self, name: &str) -> String {
        self.opt(name).unwrap_or_default()
    }
    fn num<T: std::str::FromStr>(&self, name: &str, default: T) -> T {
        self.opt(name)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }
    fn has(&self, name: &str) -> bool {
        self.0.iter().any(|a| a == name)
    }
    /// The n-th positional argument (0 is the command); exits with a message when it is missing.
    fn at(&self, n: usize, what: &str) -> String {
        self.0
            .get(n)
            .cloned()
            .unwrap_or_else(|| fail(&format!("missing {what}")))
    }
    /// Every argument from position `n` on, joined by spaces.
    fn rest(&self, n: usize) -> String {
        self.0.get(n..).unwrap_or_default().join(" ")
    }
}

fn fail(message: &str) -> ! {
    eprintln!("rejected: {message}");
    std::process::exit(1)
}

/// Print `ok`, or `missing` when the change matched nothing.
fn report(changed: bool, missing: &str) {
    println!("{}", if changed { "ok" } else { missing });
}

/// Print the result of a change that can be refused; a refusal exits with status 1.
fn report_or_fail(result: Result<bool, String>, missing: &str) {
    match result {
        Ok(changed) => report(changed, missing),
        Err(e) => fail(&e),
    }
}

fn ingest(c: &Connection) {
    if db::is_demo() {
        eprintln!("demo store: ingest is disabled");
        std::process::exit(1);
    }
    let r = ingest::scan(c);
    println!("git repos {} ({} unchanged)", r.repos.0, r.repos.1);
    println!(
        "items relinked {}, quote no longer found {}",
        r.quotes.0, r.quotes.1
    );
    let read = r.sessions.0 + r.notes.0 + r.artifacts.0 + r.repos.0;
    if read > 0 {
        let detail = format!(
            "sessions {}, notes {}, artifacts {}, git repos {}",
            r.sessions.0, r.notes.0, r.artifacts.0, r.repos.0
        );
        db::logw(c, "ingest", "ingest", "sources", &detail);
    }
    println!(
        "sessions {} new/changed ({} unchanged), notes {} ({}), artifacts {} ({})",
        r.sessions.0, r.sessions.1, r.notes.0, r.notes.1, r.artifacts.0, r.artifacts.1
    );
}

/// `bt import`: a JSON array of facts. Decisions, steps, questions and turns are filed as items; state, rationale
/// and relation facts as claims about entities.
fn import(c: &Connection, file: &str) {
    let raw = if file == "-" {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)
            .unwrap_or_else(|e| fail(&e.to_string()));
        s
    } else {
        std::fs::read_to_string(file).unwrap_or_else(|e| fail(&format!("{file}: {e}")))
    };
    let facts: Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| fail(&format!("not JSON: {e}")));
    let (mut ok, mut bad) = (0, 0);
    for x in facts
        .as_array()
        .unwrap_or_else(|| fail("expected a JSON array"))
    {
        let get = |k: &str| x[k].as_str().unwrap_or("").to_string();
        let kind = get("type");
        let title = if get("title").is_empty() {
            get("text")
        } else {
            get("title")
        };
        let result = if items::TYPES.contains(&kind.as_str()) {
            // Without a thread, the item goes to the thread of its source.
            let thread = if get("thread").is_empty() {
                let source = db::resolve_ref(c, &get("source"));
                c.query_row("SELECT thread FROM sources WHERE ref=?1", [source], |r| {
                    r.get::<_, Option<String>>(0)
                })
                .ok()
                .flatten()
                .unwrap_or_default()
            } else {
                get("thread")
            };
            items::add(
                c,
                &items::NewItem {
                    kind,
                    thread,
                    title: title.clone(),
                    body: get("body"),
                    source: get("source"),
                    quote: get("quote"),
                    state: x["state"].as_str().map(String::from),
                    origin: x["origin"].as_str().unwrap_or("compiled").into(),
                    date: x["date"].as_str().map(String::from),
                    entities: x["entities"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|e| e.as_str().map(String::from))
                        .collect(),
                    valid_to: x["valid_to"].as_str().filter(|v| !v.is_empty()).map(String::from),
                    check_how: x["check"].as_str().filter(|v| !v.is_empty()).map(String::from),
                },
            )
        } else {
            let entities: Vec<String> = x["entities"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| e.as_str().map(String::from))
                .collect();
            let kind = if kind == "relation" {
                format!("relation:{}", x["kind"].as_str().unwrap_or(""))
            } else {
                kind
            };
            kg::claims::add(
                c,
                &kg::claims::NewClaim {
                    kind: &kind,
                    text: &title,
                    body: &get("body"),
                    source: &get("source"),
                    quote: &get("quote"),
                    entities: &entities,
                    stated: if get("stated") == "inferred" {
                        "inferred"
                    } else {
                        "stated"
                    },
                    origin: "extracted",
                },
            )
        };
        match result {
            Ok(_) => ok += 1,
            Err(e) => {
                bad += 1;
                eprintln!("rejected [{title}]: {e}");
            }
        }
    }
    println!("imported {ok}, rejected {bad}");
}

/// Log one history query (`kg-query`, target = its kind) so use can be counted after a few weeks.
fn kg_query_log(c: &Connection, a: &Args, detail: &str) {
    let kind = a.0.get(1).map(String::as_str).unwrap_or("");
    let detail = match a.opt("--as-of") {
        Some(d) => format!("{detail} as-of {d}").trim().to_string(),
        None => detail.to_string(),
    };
    db::logw(c, "claude", "kg-query", kind, &detail);
}

fn kg_command(c: &Connection, a: &Args) {
    // `--as-of DATE` applies to find, page, trace and neighbors; it is not part of a question's words
    if !kg::query::set_as_of(a.opt("--as-of").as_deref()) {
        fail("--as-of needs a date like 2026-11-05");
    }
    let stripped: Vec<String> = {
        let mut v = vec![];
        let mut it = a.0.iter();
        while let Some(x) = it.next() {
            if x == "--as-of" {
                it.next();
            } else {
                v.push(x.clone());
            }
        }
        v
    };
    let words = Args(stripped).rest(2);
    match a.0.get(1).map(String::as_str) {
        Some("state") => kg::query::state(c),
        Some("export") if a.has("--graph") => {
            let file = a.opt("--graph").unwrap_or_else(|| fail("missing file"));
            let (n, e) = kg::export::graph(c, &file).unwrap_or_else(|e| fail(&e.to_string()));
            println!("wrote {n} entities and {e} links to {file}");
            kg_query_log(c, a, "graph");
        }
        Some("export") => {
            kg::export::export(c, &a.at(2, "file")).unwrap_or_else(|e| fail(&e.to_string()));
            println!("ok")
        }
        Some("restore") => {
            let (n, orphans) = kg::export::restore(c, &a.at(2, "file"));
            println!(
                "restored {n} rows, {orphans} claims kept without a quote (source or quote not found)"
            )
        }
        Some("lint") => kg::lint::lint(c),
        Some("suggest") => kg::entities::suggest(c),
        Some("merge") => {
            match kg::claims::merge(c, &a.at(2, "keep id"), &a.at(3, "duplicate id")) {
                Ok(()) => println!("ok"),
                Err(e) => eprintln!("{e}"),
            }
        }
        Some("entity") if a.0.get(2).map(String::as_str) == Some("ignore") => {
            kg::entities::ignore_suggestion(c, &a.at(3, "term"));
            println!("ok")
        }
        Some("entity") => {
            let aliases: Vec<String> =
                a.0.windows(2)
                    .filter(|w| w[0] == "--alias")
                    .map(|w| w[1].clone())
                    .collect();
            let kind = a.opt("--kind").unwrap_or("project".into());
            println!(
                "ok {}",
                kg::entities::add(c, &a.at(3, "name"), &kind, &aliases)
            );
        }
        Some("page") => {
            kg::query::page(c, &a.at(2, "entity"), a.num("--budget", 2000));
            kg_query_log(c, a, "");
        }
        Some("trace") => {
            kg::query::trace(c, &a.at(2, "entity"));
            kg_query_log(c, a, "");
        }
        Some("why") => {
            kg::query::why(c, &words);
        }
        Some("neighbors") => {
            kg::query::neighbors(c, &a.at(2, "entity"), a.num("--depth", 1));
            kg_query_log(c, a, &format!("depth {}", a.num("--depth", 1usize)));
        }
        Some("path") => {
            kg::query::path(c, &a.at(2, "entity A"), &a.at(3, "entity B"));
            kg_query_log(c, a, "");
        }
        Some("passage") => {
            kg::query::ask(c, &words);
        }
        Some("find") => {
            kg::query::find(c, &words);
            kg_query_log(c, a, "");
        }
        _ => kg::query::stats(c),
    }
}

fn main() {
    let a = Args(std::env::args().skip(1).collect());
    let cmd = a.0.first().map(String::as_str).unwrap_or("help");
    if cmd == "hooks" {
        return hooks::run(&a.0);
    }
    if cmd == "brief" && std::env::var("BACKTRAIL_COMPILE").is_ok() {
        // the headless extraction model starts a session too; it gets no brief
        return;
    }
    let c = db::open();
    match cmd {
        "ingest" => {
            if a.has("--force") {
                // Read every session again, for example after the chunking rule changed. Sources whose file is gone
                // are kept.
                c.execute(
                    "UPDATE sources SET mtime=0, bytes=0 WHERE kind='session'",
                    [],
                )
                .unwrap();
                c.execute("DELETE FROM scanned WHERE path LIKE '%.jsonl'", [])
                    .unwrap();
            }
            ingest(&c)
        }
        "build" => {
            // a fresh machine: index every source, create the configured entities, then embed
            ingest(&c);
            println!("entities: {}", kg::entities::sync_configured(&c));
            match embed::run(&c, usize::MAX) {
                Ok((n, left)) => println!("embedded {n} passages, {left} pending"),
                Err(e) => eprintln!(
                    "embed skipped: {e}\nSearch works by keyword only until the embedding server is reachable. \
                     Install Ollama, run `ollama pull {}`, then `backtrail embed`.",
                    config::DEFAULT_EMBED_MODEL
                ),
            }
            // the hook reads one session per session end, so earlier history stays unread until asked for
            let unread = kg::extract::queue(&c, usize::MAX, None, None, true).len();
            if unread > 0 {
                println!(
                    "{unread} sessions have not been read for decisions yet (about ${:.2} on a small model). \
                     Read them now: skill/extract.sh --no-cap --limit {unread}, then skill/resolve.sh",
                    unread as f64 * kg::extract::usd_each(&c)
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
        "import" => import(&c, &a.at(1, "file")),
        "set-thread" => {
            let name = a.at(1, "thread name");
            if let Some(state) = a.opt("--state") {
                c.execute("UPDATE threads SET state=?2 WHERE name=?1", [&name, &state])
                    .unwrap();
            }
            if let Some(summary) = a.opt("--summary") {
                c.execute(
                    "UPDATE threads SET body=?2 WHERE name=?1",
                    [&name, &summary],
                )
                .unwrap();
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
        "extract-input" => kg::extract::input(&c, &a.at(1, "session ref")),
        "extract-queue" => {
            let queue = kg::extract::queue(
                &c,
                a.num("--limit", 3),
                a.opt("--entity").as_deref(),
                a.opt("--session").as_deref(),
                a.has("--no-cap"),
            );
            for r in queue {
                println!("{r}")
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
        "brief" => {
            if !db::is_demo() {
                ingest::rescan(&c);
            }
            views::brief(&c, a.opt("--thread").as_deref(), a.has("--all"))
        }
        "backup" => println!(
            "{}",
            ingest::backup(&c).unwrap_or("already backed up today".into())
        ),
        "keep" => println!("kept {}", items::keep_all(&c)),
        "check" if a.has("--pairs") => check::pairs(&c),
        "check" => check::check(&c),
        "match" => check::match_commits(&c, true),
        "resolve" => {
            if a.has("--expired") {
                println!("expired {}", resolve::expire(&c));
            } else {
                report_or_fail(
                    resolve::resolve(&c, &a.at(1, "item id"), &a.text("--by"), &a.text("--reason")),
                    "no such open item",
                );
            }
        }
        "unresolve" => report(resolve::unresolve(&c, &a.at(1, "item id")), "not a resolved item"),
        "resolve-packet" if a.has("--reverses") => resolve::reverse_packet(&c, &a.at(1, "item id")),
        "resolve-packet" => resolve::packet(&c, &a.at(1, "item id")),
        "resolve-apply" => resolve::apply_cli(&c, &a.at(1, "file")),
        "howto-queue" => howto::queue(&c, a.num("--limit", 30), a.has("--again")),
        "howto-packet" => howto::packet(&c, &a.at(1, "step id")),
        "howto-apply" => howto::apply_cli(&c),
        "resolve-queue" if a.has("--reverses") => resolve::reverse_queue(&c, a.num("--limit", 20)),
        "resolve-queue" => resolve::queue(&c, a.num("--limit", 20)),
        "artifacts" => check::artifacts(&c, a.opt("--manifest").as_deref()),
        "pending" => check::pending(&c, a.num("--days", 3)),
        "skip" => {
            let r = db::resolve_ref(&c, &a.at(1, "session ref"));
            let id = db::source_id(&c, &r).unwrap_or(0);
            report(items::skip_session(&c, id, "claude"), "no such session")
        }
        "show" => check::show(&c, &a.at(1, "source ref")),
        "log" => check::log(&c, a.num("--n", 25)),
        "embed" => embed::run_cli(&c, a.num("--limit", usize::MAX)),
        "search" => {
            // every argument that is not a flag or a flag's value is a search word
            let mut words = vec![];
            let mut i = 1;
            while i < a.0.len() {
                match a.0[i].as_str() {
                    "--thread" | "--kind" | "--state" | "--from" | "--to" => i += 2,
                    "--text" => i += 1,
                    word => {
                        words.push(word);
                        i += 1
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
            search::print(&c, &words.join(" "), &filters, !a.has("--text"))
        }
        "kg" => kg_command(&c, &a),
        "demo" => demo::demo(&c),
        "serve" => {
            let port = a.num("--port", config::cfg().port);
            server::serve(c, port)
        }
        _ => println!("{USAGE}"),
    }
}
