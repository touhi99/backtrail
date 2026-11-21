use crate::{db, kg};
use rusqlite::Connection;

use super::{Args, fail};

fn kg_query_log(c: &Connection, a: &Args, detail: &str) {
    let kind = a.0.get(1).map_or("", String::as_str);
    let detail = match a.opt("--as-of") {
        Some(d) => format!("{detail} as-of {d}").trim().to_string(),
        None => detail.to_string(),
    };
    db::logw(c, "claude", "kg-query", kind, &detail);
}

pub(super) fn kg_command(c: &Connection, a: &Args) {
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
            let (n, e) = kg::export::graph(c, &file).unwrap_or_else(|e| fail(e));
            println!("wrote {n} entities and {e} links to {file}");
            kg_query_log(c, a, "graph");
        }
        Some("export") => {
            kg::export::export(c, &a.at(2, "file")).unwrap_or_else(|e| fail(e));
            println!("ok");
        }
        Some("restore") => {
            let (n, orphans) = kg::export::restore(c, &a.at(2, "file"));
            println!(
                "restored {n} rows, {orphans} claims kept without a quote (source or quote not found)"
            );
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
            println!("ok");
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
