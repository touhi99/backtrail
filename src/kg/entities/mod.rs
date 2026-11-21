mod link;
mod suggest;
mod topics;

pub use link::{link_claim, relink_all};
pub use suggest::{
    examples, ignore_suggestion, ignored_terms, suggest, suggest_data, unignore_suggestion,
};

use super::has_word;
use crate::clock::now;
use crate::config::cfg;
use crate::db::{fxhash, logw, norm};
use rusqlite::{Connection, params};

pub fn ensure(c: &Connection, name: &str, kind: &str, now: &str) -> String {
    let id = format!("e-{:x}", fxhash(&norm(name)));
    c.execute(
        "INSERT OR IGNORE INTO entities(id,name,kind,created) VALUES(?1,?2,?3,?4)",
        params![id, name, kind, now],
    )
    .expect("write to database");
    c.execute(
        "INSERT OR IGNORE INTO entity_aliases(alias,entity_id) VALUES(?1,?2)",
        params![norm(name), id],
    )
    .expect("write to database");
    id
}

pub fn find(c: &Connection, name: &str) -> Option<(String, String)> {
    let look = |n: &str| {
        c.query_row(
            "SELECT e.id, e.name FROM entity_aliases a JOIN entities e ON e.id=a.entity_id WHERE a.alias=?1",
            params![norm(n)],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()
    };
    look(name).or_else(|| {
        let n = name.trim_end();
        ["project", "topic", "tool", "service", "model", "site"]
            .iter()
            .find_map(|k| n.strip_suffix(&format!(" ({k})")))
            .and_then(look)
    })
}

pub fn mentions(c: &Connection, text: &str) -> Vec<(String, String)> {
    let hay = norm(text);
    let mut st = c.prepare("SELECT e.id, e.name, a.alias FROM entities e JOIN entity_aliases a ON a.entity_id=e.id").expect("prepare query");
    let mut out: Vec<(String, String)> = vec![];
    for (id, name, alias) in st
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .expect("run query")
        .map(|row| row.expect("decode query row"))
    {
        if !alias.is_empty() && has_word(&hay, &alias) && !out.iter().any(|o| o.0 == id) {
            out.push((id, name));
        }
    }
    out
}

pub fn sync_configured(c: &Connection) -> usize {
    let now = now();
    topics::seed_topics(c);
    let list = &cfg().entities;
    for crate::config::EntitySeed {
        name,
        kind,
        aliases,
    } in list
    {
        let id = ensure(c, name, kind, &now);
        c.execute("UPDATE entities SET kind=?2 WHERE id=?1", params![id, kind])
            .expect("write to database");
        for a in aliases {
            c.execute(
                "INSERT OR IGNORE INTO entity_aliases(alias,entity_id) VALUES(?1,?2)",
                params![a, id],
            )
            .expect("write to database");
        }
    }
    list.len()
}

pub fn add(c: &Connection, name: &str, kind: &str, aliases: &[String]) -> String {
    let id = ensure(c, name, kind, &now());
    c.execute("UPDATE entities SET kind=?2 WHERE id=?1", params![id, kind])
        .expect("write to database");
    for a in aliases {
        c.execute(
            "INSERT OR REPLACE INTO entity_aliases(alias,entity_id) VALUES(?1,?2)",
            params![norm(a), id],
        )
        .expect("write to database");
    }
    relink_all(c);
    logw(c, "owner", "kg-entity", name, &aliases.join(", "));
    id
}

pub fn add_alias(c: &Connection, entity: &str, alias: &str) -> bool {
    let Some((id, name)) = find(c, entity) else {
        return false;
    };
    c.execute(
        "INSERT OR REPLACE INTO entity_aliases(alias,entity_id) VALUES(?1,?2)",
        params![norm(alias), id],
    )
    .expect("write to database");
    relink_all(c);
    logw(c, "owner", "kg-alias", &name, alias);
    true
}
