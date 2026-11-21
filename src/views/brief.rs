macro_rules! say {
    ($out:expr, $($arg:tt)*) => {
        let _ = writeln!($out, $($arg)*);
    };
}

use super::{decided, threads, waiting_on_claude, waiting_on_owner};
use crate::clock::now;
use crate::config::cfg;
use crate::db::{rows, s};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write;

fn top_of<'a>(all: &'a [Value], name: &'a str) -> &'a str {
    all.iter()
        .find(|t| s(t, "name") == name)
        .map(|t| s(t, "parent"))
        .filter(|p| !p.is_empty())
        .unwrap_or(name)
}

fn next_step(c: &Connection, thread: &str) -> Option<String> {
    rows(
        c,
        "SELECT title FROM item WHERE type='step' AND state='open' AND thread=?1 ORDER BY created LIMIT 1",
        &[&thread],
    )
    .first()
    .map(|r| s(r, "title").to_string())
}

fn brief_short(c: &Connection) -> String {
    let mut out = String::new();
    say!(out, "Backtrail · {}", &now()[..10]);
    say!(out, "{}", crate::coverage::one_line(c));
    let waiting = waiting_on_owner(c);
    let count = |list: &[Value], name: &str| list.iter().filter(|r| s(r, "thread") == name).count();
    say!(
        out,
        "Projects in motion (detail for one: `bt brief --thread \"<name>\"`, everything: `bt brief --all`):"
    );
    let all = threads(c);
    let moving: HashSet<&str> = all
        .iter()
        .filter(|t| s(t, "state") == "moving")
        .map(|t| top_of(&all, s(t, "name")))
        .collect();
    let line = |name: &str, members: &[&Value]| {
        let total = |key: &str| {
            members
                .iter()
                .map(|t| t[key].as_i64().unwrap_or(0))
                .sum::<i64>()
        };
        let mut line = format!("{name}: {} open, {} done", total("open_s"), total("done_s"));
        let n: usize = members.iter().map(|t| count(&waiting, s(t, "name"))).sum();
        if n > 0 {
            let _ = write!(line, ", {n} waiting on the owner");
        }
        if let Some(step) = next_step(c, name) {
            let _ = write!(line, " · next: {step}");
        }
        line
    };
    let mut order: Vec<&str> = vec![];
    for t in &all {
        let top = top_of(&all, s(t, "name"));
        if moving.contains(top) && !order.contains(&top) {
            order.push(top);
        }
    }
    for top in order {
        let children: Vec<&Value> = all.iter().filter(|t| s(t, "parent") == top).collect();
        let mut members: Vec<&Value> = all.iter().filter(|t| s(t, "name") == top).collect();
        members.extend(children.iter().copied());
        say!(out, "  - {}", line(top, &members));
        for child in children.iter().filter(|t| s(t, "state") == "moving") {
            say!(out, "      - {}", line(s(child, "name"), &[child]));
        }
    }
    let mut elsewhere: BTreeMap<String, usize> = BTreeMap::new();
    for q in waiting
        .iter()
        .filter(|q| !moving.contains(top_of(&all, s(q, "thread"))))
    {
        *elsewhere.entry(s(q, "thread").to_string()).or_default() += 1;
    }
    if !elsewhere.is_empty() {
        let list: Vec<String> = elsewhere.iter().map(|(n, k)| format!("{n} {k}")).collect();
        say!(
            out,
            "Also waiting on the owner, in projects not in motion: {}",
            list.join(", ")
        );
    }
    let asks = waiting_on_claude(c);
    if !asks.is_empty() {
        say!(
            out,
            "\nAsked by the owner, waiting on Claude ({}): answer each with `backtrail reply <id> --text ...`",
            asks.len()
        );
        for q in &asks {
            say!(
                out,
                "  - {} [{}] {} {}",
                s(q, "id"),
                s(q, "thread"),
                s(q, "title"),
                s(q, "body")
            );
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    Short,
    Full,
}

pub fn brief(c: &Connection, only: Option<&str>, detail: Detail) -> String {
    let mut out = String::new();
    if only.is_none() && detail == Detail::Short {
        return brief_short(c);
    }
    let all = threads(c);
    let wanted = |row: &Value, key: &str| {
        only.is_none_or(|thread| s(row, key) == thread || top_of(&all, s(row, key)) == thread)
    };
    say!(out, "Backtrail brief · {}", &now()[..10]);

    let waiting: Vec<Value> = waiting_on_owner(c)
        .into_iter()
        .filter(|q| wanted(q, "thread"))
        .collect();
    say!(
        out,
        "\nClaude asked the owner, waiting on the owner ({}): ask when it fits the work, then record the reply with `backtrail answer <id> --text ...`. Unanswered ones are parked after 14 days.",
        waiting.len()
    );
    for q in &waiting {
        say!(
            out,
            "  - {} [{}] {}",
            s(q, "id"),
            s(q, "thread"),
            s(q, "title")
        );
    }

    let asks: Vec<Value> = waiting_on_claude(c)
        .into_iter()
        .filter(|q| wanted(q, "thread"))
        .collect();
    say!(
        out,
        "\nAsked by the owner, waiting on Claude ({}): answer each with `backtrail reply <id> --text ...`",
        asks.len()
    );
    for q in &asks {
        say!(
            out,
            "  - {} [{}] {} {}",
            s(q, "id"),
            s(q, "thread"),
            s(q, "title"),
            s(q, "body")
        );
    }

    say!(out, "\nThreads in motion:");
    for t in all
        .iter()
        .filter(|t| s(t, "state") == "moving" && wanted(t, "name"))
    {
        let name = s(t, "name");
        let parent = s(t, "parent");
        say!(
            out,
            "  - {name}{}: {} open steps, {} done; last activity {}",
            if parent.is_empty() {
                String::new()
            } else {
                format!(" (in {parent})")
            },
            t["open_s"],
            t["done_s"],
            s(
                t,
                if parent.is_empty() {
                    "family_last"
                } else {
                    "last"
                }
            )
            .get(..10)
            .unwrap_or("?")
        );
        let children: Vec<&str> = all
            .iter()
            .filter(|x| s(x, "parent") == name)
            .map(|x| s(x, "name"))
            .collect();
        if !children.is_empty() {
            say!(out, "      sub-threads: {}", children.join(", "));
        }
        if let Some(step) = next_step(c, name) {
            say!(out, "      next: {step}");
        }
        let answered = rows(
            c,
            "SELECT title, answer, origin FROM item WHERE type='question' AND state='answered' AND thread=?1
             ORDER BY updated DESC LIMIT 2",
            &[&name],
        );
        for q in &answered {
            let who = if s(q, "origin") == "owner" {
                "owner asked, Claude answered"
            } else {
                "owner answered"
            };
            say!(out, "      {who} \"{}\": {}", s(q, "title"), s(q, "answer"));
        }
        let decided = decided(c, name, 5);
        if !decided.is_empty() {
            say!(out, "      decided, latest settled:");
        }
        for d in &decided {
            say!(
                out,
                "        {} · {} ({})",
                s(d, "day"),
                s(d, "title"),
                s(d, "how")
            );
        }
    }

    say!(
        out,
        "\nThe store is the source of truth: backtrail serve (http://127.0.0.1:{}). Sources are read only; notes carry no status.",
        cfg().port
    );
    out
}
