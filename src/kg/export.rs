use crate::db::{find_quote, source_id};
use rusqlite::{Connection, params};
use std::fmt::Write;

pub fn export(c: &Connection, path: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    let q = |sql: &str, cols: &[&str]| -> Vec<serde_json::Value> {
        let mut st = c.prepare(sql).expect("prepare query");
        st.query_map([], |r| {
            let mut o = serde_json::Map::new();
            for (i, k) in cols.iter().enumerate() {
                let v = match r
                    .get::<_, rusqlite::types::Value>(i)
                    .expect("column index within the SELECT")
                {
                    rusqlite::types::Value::Text(t) => serde_json::Value::from(t),
                    rusqlite::types::Value::Integer(n) => serde_json::Value::from(n.to_string()),
                    _ => serde_json::Value::Null,
                };
                o.insert((*k).into(), v);
            }
            Ok(serde_json::Value::Object(o))
        })
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .collect()
    };
    let sets: [(&str, &str, &[&str]); 6] = [
        (
            "entity",
            "SELECT id,name,kind,created FROM entities",
            &["id", "name", "kind", "created"],
        ),
        (
            "alias",
            "SELECT alias,entity_id FROM entity_aliases",
            &["alias", "entity_id"],
        ),
        (
            "claim",
            "SELECT c.id, c.type, c.thread, c.title, c.body, c.state, c.origin, c.speaker, c.asserted_at, c.valid_to, c.superseded_by, c.done_by, c.answer, c.tag, c.stale, s.ref, c.quote, c.stated, c.created, c.updated FROM claims c LEFT JOIN sources s ON s.id=c.source_id",
            &[
                "id",
                "type",
                "thread",
                "title",
                "body",
                "state",
                "origin",
                "speaker",
                "asserted_at",
                "valid_to",
                "superseded_by",
                "done_by",
                "answer",
                "tag",
                "stale",
                "source",
                "quote",
                "stated",
                "created",
                "updated",
            ],
        ),
        (
            "about",
            "SELECT claim_id,entity_id,role FROM claim_about",
            &["claim_id", "entity_id", "role"],
        ),
        (
            "link",
            "SELECT a,b,kind,reason,actor,created FROM claim_links",
            &["a", "b", "kind", "reason", "actor", "created"],
        ),
        (
            "override",
            "SELECT target,field,value,by,ts FROM kg_overrides",
            &["target", "field", "value", "by", "ts"],
        ),
    ];
    for (t, sql, cols) in sets {
        for mut v in q(sql, cols) {
            v["t"] = t.into();
            writeln!(f, "{v}")?;
        }
    }
    Ok(())
}

pub fn restore(c: &Connection, path: &str) -> (usize, usize) {
    let raw = std::fs::read_to_string(path).unwrap_or_default();
    let (mut n, mut orphan) = (0, 0);
    for l in raw.lines() {
        let v: serde_json::Value = match serde_json::from_str(l) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let g = |k: &str| v[k].as_str().map(String::from);
        match v["t"].as_str().unwrap_or("") {
            "entity" => {
                c.execute(
                    "INSERT OR REPLACE INTO entities(id,name,kind,created) VALUES(?1,?2,?3,?4)",
                    params![g("id"), g("name"), g("kind"), g("created")],
                )
                .expect("write to database");
                n += 1;
            }
            "alias" => {
                c.execute(
                    "INSERT OR REPLACE INTO entity_aliases(alias,entity_id) VALUES(?1,?2)",
                    params![g("alias"), g("entity_id")],
                )
                .expect("write to database");
                n += 1;
            }
            "claim" => {
                let hit = locate(c, g("source"), &g("quote").unwrap_or_default());
                let (Some(title), state) = (
                    g("title").or_else(|| g("text")),
                    g("state").or_else(|| {
                        g("status").map(|s| match s.as_str() {
                            "active" => "settled".to_string(),
                            "retracted" => "rejected".to_string(),
                            _ => s,
                        })
                    }),
                ) else {
                    continue;
                };
                if hit.is_none() && g("quote").is_some() {
                    orphan += 1;
                }
                c.execute(
                    "INSERT OR REPLACE INTO claims(id,type,thread,title,body,state,origin,speaker,asserted_at,valid_to,superseded_by,done_by,answer,tag,stale,source_id,chunk_id,quote,stated,created,updated) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
                    params![
                        g("id"),
                        g("type"),
                        g("thread"),
                        title,
                        g("body"),
                        state.unwrap_or("proposed".into()),
                        g("origin"),
                        g("speaker"),
                        g("asserted_at"),
                        g("valid_to"),
                        g("superseded_by"),
                        g("done_by"),
                        g("answer"),
                        g("tag"),
                        g("stale"),
                        hit.map(|h| h.0),
                        hit.map(|h| h.1),
                        g("quote"),
                        g("stated").unwrap_or("stated".into()),
                        g("created"),
                        g("updated")
                    ],
                )
                .expect("write to database");
                n += 1;
            }
            "about" => {
                c.execute(
                    "INSERT OR IGNORE INTO claim_about(claim_id,entity_id,role) VALUES(?1,?2,?3)",
                    params![g("claim_id"), g("entity_id"), g("role")],
                )
                .expect("write to database");
                n += 1;
            }
            "link" => {
                c.execute(
                    "INSERT OR IGNORE INTO claim_links(a,b,kind,reason,actor,created) VALUES(?1,?2,?3,?4,?5,?6)",
                    params![g("a"), g("b"), g("kind"), g("reason"), g("actor"), g("created")],
                )
                .expect("write to database");
                n += 1;
            }
            "override" => {
                c.execute("INSERT OR REPLACE INTO kg_overrides(target,field,value,by,ts) VALUES(?1,?2,?3,?4,?5)", params![g("target"), g("field"), g("value"), g("by"), g("ts")]).expect("write to database");
                n += 1;
            }
            "evidence" => match locate(c, g("source"), &g("quote").unwrap_or_default()) {
                Some((sid, cid)) => {
                    c.execute(
                            "UPDATE claims SET source_id=?2, chunk_id=?3, quote=?4, stated=?5 WHERE id=?1 AND quote IS NULL",
                            params![g("claim_id"), sid, cid, g("quote"), g("stated").unwrap_or("stated".into())],
                        )
                        .expect("write to database");
                    n += 1;
                }
                None => orphan += 1,
            },
            _ => {}
        }
    }
    (n, orphan)
}

fn locate(c: &Connection, source: Option<String>, quote: &str) -> Option<(i64, i64)> {
    let source = source_id(c, &source?)?;
    find_quote(c, source, quote).map(|p| (source, p.chunk_id))
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn graph(c: &Connection, path: &str) -> std::io::Result<(usize, usize)> {
    let live = super::query::in_force();
    let nodes: Vec<(String, String, i64)> = c
        .prepare(&format!(
            "SELECT e.name, ifnull(e.kind,''), (SELECT COUNT(DISTINCT a.claim_id) FROM claim_about a JOIN claims c ON c.id=a.claim_id AND {live} AND (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%') WHERE a.entity_id=e.id) FROM entities e ORDER BY e.name"
        ))
        .expect("prepare query")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .collect();
    let edges: Vec<(String, String, i64, String, String, String)> = c
        .prepare(&format!(
            "SELECT x.name, y.name, COUNT(DISTINCT a.claim_id), MIN(substr(ifnull(c.asserted_at,c.created),1,10)), MAX(substr(ifnull(c.asserted_at,c.created),1,10)),
               (SELECT t.title FROM claims t WHERE t.id=(SELECT c2.id FROM claims c2 JOIN claim_about p ON p.claim_id=c2.id AND p.entity_id=a.entity_id JOIN claim_about q ON q.claim_id=c2.id AND q.entity_id=b.entity_id WHERE {} ORDER BY c2.asserted_at DESC LIMIT 1))
             FROM claim_about a JOIN claims c ON c.id=a.claim_id AND {live} AND (c.type IN ('decision','state','rationale') OR c.type LIKE 'relation:%')
             JOIN claim_about b ON b.claim_id=a.claim_id AND b.entity_id>a.entity_id
             JOIN entities x ON x.id=a.entity_id JOIN entities y ON y.id=b.entity_id
             GROUP BY a.entity_id, b.entity_id ORDER BY 3 DESC",
            live.replace("c.", "c2.")
        ))
        .expect("prepare query")
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                r.get::<_, Option<String>>(5)?.unwrap_or_default(),
            ))
        })
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .collect();
    let out = if path.ends_with(".graphml") {
        let mut o = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n<key id=\"kind\" for=\"node\" attr.name=\"kind\" attr.type=\"string\"/>\n<key id=\"claims\" for=\"node\" attr.name=\"claims\" attr.type=\"int\"/>\n<key id=\"weight\" for=\"edge\" attr.name=\"weight\" attr.type=\"int\"/>\n<key id=\"first\" for=\"edge\" attr.name=\"first\" attr.type=\"string\"/>\n<key id=\"last\" for=\"edge\" attr.name=\"last\" attr.type=\"string\"/>\n<key id=\"latest_claim\" for=\"edge\" attr.name=\"latest_claim\" attr.type=\"string\"/>\n<graph id=\"backtrail\" edgedefault=\"undirected\">\n",
        );
        for (n, k, cl) in &nodes {
            let _ = writeln!(
                o,
                "<node id=\"{}\"><data key=\"kind\">{}</data><data key=\"claims\">{cl}</data></node>",
                xml(n),
                xml(k)
            );
        }
        for (a, b, w, f, l, t) in &edges {
            let _ = writeln!(
                o,
                "<edge source=\"{}\" target=\"{}\"><data key=\"weight\">{w}</data><data key=\"first\">{f}</data><data key=\"last\">{l}</data><data key=\"latest_claim\">{}</data></edge>",
                xml(a),
                xml(b),
                xml(t)
            );
        }
        o.push_str("</graph>\n</graphml>\n");
        o
    } else {
        serde_json::json!({
            "directed": false, "multigraph": false, "graph": {},
            "nodes": nodes.iter().map(|(n, k, cl)| serde_json::json!({"id": n, "kind": k, "claims": cl})).collect::<Vec<_>>(),
            "links": edges.iter().map(|(a, b, w, f, l, t)| serde_json::json!({"source": a, "target": b, "weight": w, "first": f, "last": l, "latest_claim": t})).collect::<Vec<_>>(),
        })
        .to_string()
    };
    std::fs::write(path, out)?;
    Ok((nodes.len(), edges.len()))
}
