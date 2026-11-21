mod answers;
mod graph;
mod page;
mod state;

pub use answers::{ask, find, why};
pub use graph::{neighbors, path};
pub use page::{page, trace};
pub use state::{state, stats};

use super::{clip, day};
use rusqlite::Connection;
use std::cell::RefCell;

thread_local! {
    static AS_OF: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn set_as_of(d: Option<&str>) -> bool {
    let Some(d) = d else { return true };
    let ok = d.len() == 10
        && d.char_indices().all(|(i, ch)| {
            if i == 4 || i == 7 {
                ch == '-'
            } else {
                ch.is_ascii_digit()
            }
        });
    if ok {
        AS_OF.with(|a| *a.borrow_mut() = Some(d.to_string()));
    }
    ok
}

fn as_of() -> Option<String> {
    AS_OF.with(|a| a.borrow().clone())
}

pub(super) fn in_force() -> String {
    match as_of() {
        None => "c.status!='retracted'".to_string(),
        Some(d) => {
            let end = "MIN(COALESCE(NULLIF(substr(c.valid_to,1,10),''),'9999'), COALESCE((SELECT substr(s.asserted_at,1,10) FROM claims s WHERE s.id=c.superseded_by),'9999'), COALESCE((SELECT MIN(substr(l.ts,1,10)) FROM log l WHERE l.action='retire' AND l.target=c.id),'9999'), COALESCE(CASE WHEN c.state='superseded' THEN substr(c.updated,1,10) WHEN c.stale LIKE 'superseded%' THEN (SELECT MIN(substr(l.ts,1,10)) FROM log l WHERE l.action='flag' AND l.target=c.id) END,'9999'))";
            format!(
                "(substr(ifnull(c.asserted_at,c.created),1,10)<='{d}' AND {end}>'{d}' AND (c.state!='rejected' OR {end}<'9999'))"
            )
        }
    }
}

struct Row {
    text: String,
    speaker: String,
    ts: Option<String>,
    status: String,
    quote: String,
    stated: String,
    ents: String,
}

fn excl() -> String {
    match std::env::var("BACKTRAIL_EXCLUDE_SESSION") {
        Ok(r)
            if r.chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == ':' || ch == '-')
                && !r.is_empty() =>
        {
            format!(
                " AND NOT EXISTS (SELECT 1 FROM sources xs WHERE xs.id=c.source_id AND substr(xs.ref,1,{})='{r}')",
                r.len()
            )
        }
        _ => String::new(),
    }
}

fn claim_rows(
    c: &Connection,
    where_: &str,
    args: &[&dyn rusqlite::ToSql],
    limit: usize,
) -> Vec<Row> {
    let where_ = &format!(
        "{}{}",
        where_.replace("c.status!='retracted'", &in_force()),
        excl()
    );
    let sql = "SELECT c.text, c.type, ifnull(c.speaker,''), ifnull(c.asserted_at,c.created), CASE WHEN c.stale LIKE 'done|%' THEN 'done' ELSE c.status END, ifnull(c.quote,''), c.stated, ifnull((SELECT group_concat(o.name, ', ') FROM claim_about x JOIN entities o ON o.id=x.entity_id WHERE x.claim_id=c.id),'') FROM claims c WHERE (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%') AND {where} ORDER BY c.asserted_at DESC LIMIT {limit}"
        .replace("{where}", where_)
        .replace("{limit}", &limit.to_string());
    let mut st = c.prepare(&sql).expect("prepare query");
    st.query_map(args, |r| {
        Ok(Row {
            text: r.get(0)?,
            speaker: r.get(2)?,
            ts: r.get(3)?,
            status: r.get(4)?,
            quote: r.get(5)?,
            stated: r.get(6)?,
            ents: r.get(7)?,
        })
    })
    .expect("run query")
    .map(|row| row.expect("decode query row"))
    .map(|mut r: Row| {
        if as_of().is_some() && matches!(r.status.as_str(), "superseded" | "retracted") {
            r.status = "in force then".into();
        }
        r
    })
    .collect()
}

fn who(s: &str) -> &str {
    match s {
        "owner" => "owner",
        "assistant" => "assistant",
        _ => "?",
    }
}

fn line(r: &Row, qlen: usize) -> String {
    let head = format!(
        "{} {} [{}{}] {}",
        day(r.ts.as_deref()),
        who(&r.speaker),
        r.status,
        if r.stated == "inferred" {
            ", inferred"
        } else {
            ""
        },
        r.text
    );
    if r.quote.is_empty() {
        head
    } else {
        format!("{head}\n    \"{}\"", clip(&r.quote, qlen))
    }
}

fn tokens(s: &str) -> usize {
    s.chars().count() / 4
}
