//! The local web server: the app's files (embedded in the binary) and its JSON API. It listens on loopback only.
//! Each route calls one function in `views`, `search`, `items` or `kg`; nothing is decided here.
use crate::items::{self, Verdict};
use crate::search::Filters;
use crate::{db, kg, search, views};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::time::Duration;
use tiny_http::{Header, Method, Request, Response, Server};

type Reply = Response<std::io::Cursor<Vec<u8>>>;

#[derive(rust_embed::RustEmbed)]
#[folder = "app/dist/"]
struct Assets;

/// A file of the web app. Any path that is not a built asset gets `index.html`, so page URLs load the app.
fn asset(path: &str) -> Option<Reply> {
    let name = path.trim_start_matches('/');
    let name = if name.is_empty() { "index.html" } else { name };
    let file = match Assets::get(name) {
        Some(file) => file,
        None if name.starts_with("assets/") => return None,
        None => Assets::get("index.html")?,
    };
    let mime = match name.rsplit('.').next().unwrap_or("") {
        "js" => "text/javascript",
        "css" => "text/css",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "woff2" => "font/woff2",
        "json" => "application/json",
        _ => "text/html; charset=utf-8",
    };
    Some(
        Response::from_data(file.data.to_vec())
            .with_header(Header::from_bytes("Content-Type", mime).unwrap()),
    )
}

fn json_reply(v: Value) -> Reply {
    Response::from_string(v.to_string())
        .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
}

fn error_reply(status: u16, message: &str) -> Reply {
    json_reply(json!({"error": message})).with_status_code(status)
}

/// The reply to a change: `{"ok": true|false}`.
fn done(ok: bool) -> Value {
    json!({"ok": ok})
}

/// The reply to a change that can be refused with a reason.
fn done_or_error(result: Result<Value, String>) -> Value {
    result.unwrap_or_else(|error| json!({"ok": false, "error": error}))
}

fn url_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < bytes.len() {
        let hex = (bytes[i] == b'%').then(|| text.get(i + 1..i + 3)).flatten();
        match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (_, Some(byte)) => {
                out.push(byte);
                i += 2;
            }
            (b'+', _) => out.push(b' '),
            (byte, _) => out.push(byte),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// The query parameters of a URL; a missing one reads as empty.
struct Query(Vec<(String, String)>);

impl Query {
    fn parse(url: &str) -> Self {
        let query = url.split_once('?').map_or("", |x| x.1);
        Query(
            query
                .split('&')
                .filter_map(|kv| kv.split_once('='))
                .map(|(k, v)| (k.to_string(), url_decode(v)))
                .collect(),
        )
    }
    fn get(&self, key: &str) -> String {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }
    fn id(&self, key: &str) -> i64 {
        self.get(key).parse().unwrap_or(0)
    }
}

/// True when a Host or Origin header value names this machine (`127.0.0.1`, `localhost` or `[::1]`, any port).
fn loopback(value: &str) -> bool {
    let rest = value.strip_prefix("http://").unwrap_or(value);
    let host = match rest.strip_prefix('[') {
        Some(v6) => v6.split(']').next(),
        None => rest.split(':').next(),
    };
    matches!(host, Some("127.0.0.1" | "localhost" | "::1"))
}

/// Only this machine's browser may talk to the store. A page on another site reaches a loopback server through DNS
/// rebinding (its own name in Host) or a cross-site POST (its own Origin); both are refused.
fn from_this_machine(rq: &Request) -> bool {
    let header = |name: &'static str| {
        rq.headers()
            .iter()
            .find(|h| h.field.equiv(name))
            .map(|h| h.value.as_str())
    };
    header("Host").is_some_and(loopback) && header("Origin").is_none_or(loopback)
}

fn get(c: &Connection, path: &str, q: &Query) -> Option<Value> {
    Some(match path {
        "/api/today" => views::today(c),
        "/api/thread" => views::thread(c, &q.get("name")),
        "/api/focus" => views::focus(c, &q.get("thread")),
        "/api/items" => views::items(
            c,
            &q.get("type"),
            &q.get("state"),
            &q.get("thread"),
            &q.get("kind"),
        ),
        "/api/source" => views::source(c, q.id("id")),
        "/api/source_raw" => views::source_file(c, q.id("id")),
        "/api/source_id" => json!({"id": db::source_id(c, &q.get("ref")).unwrap_or(0)}),
        "/api/graph" => views::graph(c),
        "/api/trail" => views::trail(c),
        "/api/log" => json!(db::rows(
            c,
            "SELECT ts, actor, action, target, detail FROM log ORDER BY id DESC LIMIT 80",
            &[]
        )),
        "/api/search" => {
            let filters = Filters {
                thread: q.get("thread"),
                kind: q.get("kind"),
                state: q.get("state"),
                from: q.get("from"),
                to: q.get("to"),
            };
            search::search(c, &q.get("q"), &filters, q.get("mode") != "text")
        }
        "/api/kg" => kg::screen::entities(c),
        "/api/kg/graph" => kg::screen::graph(c),
        "/api/kg/growth" => kg::screen::growth(c),
        "/api/kg/lineage" => kg::screen::lineage(c),
        "/api/kg/suggest" => kg::screen::suggestions(c),
        "/api/kg/claims" => kg::screen::claims(
            c,
            &q.get("entity"),
            &q.get("q"),
            &q.get("status"),
            &q.get("speaker"),
        ),
        _ => return None,
    })
}

fn post(c: &Connection, path: &str, body: &Value) -> Option<Value> {
    let text = |key: &str| body[key].as_str().unwrap_or("");
    let flag = |key: &str| body[key].as_bool() == Some(true);
    let id = text("id");
    Some(match path {
        "/api/tick" => done(items::tick(c, id)),
        "/api/doing" => done(items::toggle_doing(c, id)),
        "/api/skip_session" => done(items::skip_session(
            c,
            body["id"].as_i64().unwrap_or(0),
            "owner",
        )),
        "/api/notyet" => done(items::not_yet(c, id)),
        "/api/answer" => done_or_error(items::answer(c, id, text("text").trim()).map(done)),
        "/api/add" => {
            let kind = body["type"].as_str().unwrap_or("step");
            let added = items::capture(
                c,
                text("thread"),
                kind,
                text("tag"),
                text("text").trim(),
                text("body").trim(),
            );
            done_or_error(added.map(|id| json!({"ok": true, "id": id})))
        }
        "/api/idea" => done_or_error(
            items::idea(c, text("text").trim()).map(|thread| json!({"ok": true, "thread": thread})),
        ),
        "/api/review" => {
            let verdict = match (flag("accept"), flag("park")) {
                (true, _) => Verdict::Accept,
                (false, true) => Verdict::Park,
                (false, false) => Verdict::Reject,
            };
            done(items::review(c, id, verdict, text("reason")))
        }
        "/api/relation" => done(items::review_relation(
            c,
            text("a"),
            text("b"),
            text("kind"),
            flag("accept"),
        )),
        "/api/move" => done(items::move_to(c, id, text("thread"))),
        "/api/supersede" => done(items::supersede(c, id, text("reason"))),
        "/api/kg/entity" => {
            let name = text("name").trim();
            let kind = body["kind"].as_str().unwrap_or("project");
            done(match (name.is_empty(), kind, text("alias_of")) {
                (true, _, _) => false,
                (_, "ignore", _) => {
                    kg::entities::ignore_suggestion(c, name);
                    true
                }
                (_, _, "") => {
                    kg::entities::add(c, name, kind, &[name.to_lowercase()]);
                    true
                }
                (_, _, entity) => kg::entities::add_alias(c, entity, name),
            })
        }
        _ => return None,
    })
}

fn handle(c: &Connection, rq: &mut Request) -> Reply {
    if !from_this_machine(rq) {
        return error_reply(403, "forbidden");
    }
    let url = rq.url().to_string();
    let path = url.split('?').next().unwrap_or("/");
    let reply = match rq.method() {
        Method::Get if !path.starts_with("/api/") => {
            return asset(path).unwrap_or_else(|| error_reply(404, "not found"));
        }
        Method::Get => get(c, path, &Query::parse(&url)),
        Method::Post => {
            let mut raw = String::new();
            rq.as_reader().read_to_string(&mut raw).ok();
            post(c, path, &serde_json::from_str(&raw).unwrap_or(Value::Null))
        }
        _ => None,
    };
    reply.map_or_else(|| error_reply(404, "not found"), json_reply)
}

/// Every five minutes: read new sources, write the daily backup and embed new passages.
fn background_scan() {
    let mut warned = false;
    loop {
        let c = db::open();
        crate::ingest::rescan(&c);
        crate::resolve::expire(&c);
        if crate::embed::enabled() {
            match crate::embed::run(&c, crate::config::cfg().embeddings.batch_cap) {
                Err(e) if !warned => {
                    warned = true;
                    eprintln!("embed: {e} (further errors are not printed)");
                }
                Ok(_) => warned = false,
                Err(_) => {}
            }
        }
        std::thread::sleep(Duration::from_secs(300));
    }
}

pub fn serve(c: Connection, port: u16) {
    let server = Server::http(format!("127.0.0.1:{port}")).expect("bind");
    println!("backtrail on http://127.0.0.1:{port}");
    if !db::is_demo() {
        std::thread::spawn(background_scan);
    }
    for mut rq in server.incoming_requests() {
        // A failed request answers 500 and the server keeps running.
        let reply = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle(&c, &mut rq)))
            .unwrap_or_else(|_| error_reply(500, "internal error"));
        rq.respond(reply).ok();
    }
}
