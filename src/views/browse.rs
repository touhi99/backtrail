use crate::db::{rows, s};
use rusqlite::Connection;
use serde_json::{Value, json};

pub fn items(c: &Connection, kind: &str, state: &str, thread: &str, thread_kind: &str) -> Value {
    json!(rows(
        c,
        "SELECT i.* FROM item i JOIN threads t ON t.name=i.thread
         WHERE (?1='' OR i.type=?1) AND (?2='' OR i.state=?2) AND (?3='' OR i.thread=?3) AND (?4='' OR t.kind=?4)
         ORDER BY i.created DESC LIMIT 300",
        &[&kind, &state, &thread, &thread_kind],
    ))
}

pub fn source(c: &Connection, id: i64) -> Value {
    let source = rows(c, "SELECT * FROM source_stat WHERE id=?1", &[&id]);
    let chunks = rows(
        c,
        "SELECT id, locator, role, ts, text FROM chunks WHERE source_id=?1 ORDER BY seq LIMIT 600",
        &[&id],
    );
    let items = rows(c, "SELECT * FROM item WHERE source_id=?1", &[&id]);
    json!({"source": source.first(), "chunks": chunks, "items": items})
}

pub fn source_file(c: &Connection, id: i64) -> Value {
    let found = rows(
        c,
        "SELECT kind, ifnull(path,'') AS path FROM sources WHERE id=?1",
        &[&id],
    );
    let (kind, path) = found
        .first()
        .map_or(("", ""), |r| (s(r, "kind"), s(r, "path")));
    let text = if kind == "note" || kind == "artifact" {
        std::fs::read_to_string(path).unwrap_or_default()
    } else {
        String::new()
    };
    json!({"kind": kind, "text": text})
}
