use crate::db::{rows, s};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};

const DAY: &str = "substr(COALESCE(NULLIF(c.asserted_at,''), (SELECT first_ts FROM sources WHERE id=c.source_id), c.created),1,10)";

fn decision_rows(c: &Connection, filter: &str, args: &[&dyn rusqlite::ToSql]) -> Vec<Value> {
    decision_rows_in(c, "('settled','superseded')", filter, args)
}

fn decision_rows_in(
    c: &Connection,
    states: &str,
    filter: &str,
    args: &[&dyn rusqlite::ToSql],
) -> Vec<Value> {
    rows(
        c,
        &format!(
            "SELECT c.id, c.title, c.thread, c.state, ifnull(c.quote,'') AS quote, c.source_id, c.chunk_id,
                    ifnull(c.superseded_by,'') AS superseded_by, {DAY} AS day,
                    (SELECT group_concat(e.name, char(31)) FROM claim_about a JOIN entities e ON e.id=a.entity_id
                      WHERE a.claim_id=c.id) AS subjects
             FROM claims c
             WHERE c.type='decision' AND c.state IN {states} AND {filter}
             ORDER BY day, c.id"
        ),
        args,
    )
    .into_iter()
    .map(|mut r| {
        let subjects: Vec<String> = s(&r, "subjects")
            .split('\u{1f}')
            .filter(|x| !x.is_empty())
            .map(String::from)
            .collect();
        r["subjects"] = json!(subjects);
        r
    })
    .collect()
}

fn links_among(c: &Connection, ids: &[String]) -> Vec<Value> {
    let list = json!(ids).to_string();
    rows(
        c,
        "SELECT k.a, k.b, k.kind, ifnull(k.reason,'') AS reason FROM claim_links k
         WHERE k.kind IN ('changed','restates')
           AND (k.a IN (SELECT value FROM json_each(?1)) OR k.b IN (SELECT value FROM json_each(?1)))",
        &[&list],
    )
}

fn reasons_for(c: &Connection, ids: &[String]) -> HashMap<String, Vec<Value>> {
    let list = json!(ids).to_string();
    let mut out: HashMap<String, Vec<Value>> = HashMap::new();
    for r in rows(
        c,
        "SELECT k.a AS decision, r.id, r.title, ifnull(r.quote,'') AS quote, r.source_id, r.chunk_id
         FROM claim_links k JOIN claims r ON r.id=k.b
         WHERE k.kind='because' AND k.a IN (SELECT value FROM json_each(?1))",
        &[&list],
    ) {
        out.entry(s(&r, "decision").to_string()).or_default().push(r);
    }
    out
}

pub fn overview(c: &Connection) -> Value {
    let projects = rows(
        c,
        "SELECT t.name, t.state, t.area, (SELECT MAX(last_ts) FROM sources WHERE thread=t.name) AS last
         FROM threads t JOIN entities e ON e.name=t.name AND e.kind='project'
         WHERE t.state!='dropped'",
        &[],
    );
    let mut out = vec![];
    for mut p in projects {
        let name = s(&p, "name").to_string();
        let mut head = decision_rows(c, "c.state='settled' AND c.thread=?1", &[&name]);
        if head.is_empty() {
            head = decision_rows(
                c,
                "c.state='settled' AND c.id IN (SELECT a.claim_id FROM claim_about a JOIN entities e ON e.id=a.entity_id WHERE e.name=?1)",
                &[&name],
            );
        }
        let Some(latest) = head.last() else { continue };
        let changed = rows(
            c,
            "SELECT COUNT(*) AS n FROM claim_links k JOIN claims d ON d.id=k.a
             WHERE k.kind='changed' AND d.thread=?1",
            &[&name],
        );
        p["headline"] = json!({"id": latest["id"], "title": latest["title"], "day": latest["day"]});
        p["turns"] = changed.first().map_or(json!(0), |r| r["n"].clone());
        p["n"] = json!(head.len());
        out.push(p);
    }
    let names: BTreeSet<String> = out.iter().map(|p| s(p, "name").to_string()).collect();
    let mut edges = vec![];
    for r in rows(
        c,
        "SELECT ea.name AS a, eb.name AS b, COUNT(*) AS w,
                (SELECT c2.title FROM claims c2
                   JOIN claim_about x ON x.claim_id=c2.id JOIN entities ex ON ex.id=x.entity_id AND ex.name=ea.name
                   JOIN claim_about y ON y.claim_id=c2.id JOIN entities ey ON ey.id=y.entity_id AND ey.name=eb.name
                  WHERE c2.type='decision' AND c2.state='settled'
                  ORDER BY COALESCE(NULLIF(c2.asserted_at,''), c2.created) DESC LIMIT 1) AS label
         FROM claims c
         JOIN claim_about a ON a.claim_id=c.id JOIN entities ea ON ea.id=a.entity_id AND ea.kind='project'
         JOIN claim_about b ON b.claim_id=c.id JOIN entities eb ON eb.id=b.entity_id AND eb.kind='project' AND eb.name>ea.name
         WHERE c.type='decision' AND c.state='settled'
         GROUP BY ea.name, eb.name",
        &[],
    ) {
        if names.contains(s(&r, "a")) && names.contains(s(&r, "b")) && !s(&r, "label").is_empty() {
            edges.push(json!({"a": r["a"], "b": r["b"], "kind": "shared", "label": r["label"], "w": r["w"]}));
        }
    }
    for r in rows(
        c,
        "SELECT f.type, f.entities FROM fact f WHERE f.type LIKE 'relation:%' AND f.status!='retracted'",
        &[],
    ) {
        let ends: Vec<&str> = s(&r, "entities").split('\u{1f}').collect();
        if ends.len() == 2 && names.contains(ends[0]) && names.contains(ends[1]) {
            edges.push(json!({"a": ends[0], "b": ends[1], "kind": "relation",
                              "label": s(&r, "type").trim_start_matches("relation:"), "w": 1}));
        }
    }
    for r in rows(
        c,
        "SELECT na.thread AS a, nb.thread AS b, na.title AS now, nb.title AS was FROM claim_links k
         JOIN claims na ON na.id=k.a JOIN claims nb ON nb.id=k.b
         WHERE k.kind='changed' AND na.thread!=nb.thread",
        &[],
    ) {
        if names.contains(s(&r, "a")) && names.contains(s(&r, "b")) {
            edges.push(json!({"a": r["b"], "b": r["a"], "kind": "changed", "label": r["now"], "was": r["was"], "w": 1}));
        }
    }
    json!({"projects": out, "edges": edges})
}

pub fn project(c: &Connection, name: &str) -> Value {
    let mine = decision_rows(
        c,
        "(c.thread=?1 OR c.id IN (SELECT a.claim_id FROM claim_about a JOIN entities e ON e.id=a.entity_id WHERE e.name=?1))",
        &[&name],
    );
    let ids: Vec<String> = mine.iter().map(|d| s(d, "id").to_string()).collect();
    let links = links_among(c, &ids);
    let outside: Vec<String> = links
        .iter()
        .flat_map(|l| [s(l, "a").to_string(), s(l, "b").to_string()])
        .filter(|x| !ids.contains(x))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let list = json!(outside).to_string();
    let others = decision_rows(c, "c.id IN (SELECT value FROM json_each(?1))", &[&list]);
    let mut all = mine;
    for mut o in others {
        o["outside"] = json!(true);
        all.push(o);
    }
    let all_ids: Vec<String> = all.iter().map(|d| s(d, "id").to_string()).collect();
    let reasons = reasons_for(c, &all_ids);
    for d in &mut all {
        d["because"] = json!(reasons.get(s(d, "id")).cloned().unwrap_or_default());
    }
    json!({"name": name, "decisions": all, "links": links})
}

pub fn chain(c: &Connection, id: &str) -> Value {
    let mut seen: BTreeSet<String> = BTreeSet::from([id.to_string()]);
    let mut frontier = vec![id.to_string()];
    let mut links: Vec<Value> = vec![];
    while let Some(cur) = frontier.pop() {
        for l in rows(
            c,
            "SELECT a, b, kind, ifnull(reason,'') AS reason FROM claim_links
             WHERE kind IN ('changed','restates') AND (a=?1 OR b=?1)",
            &[&cur],
        ) {
            for end in [s(&l, "a"), s(&l, "b")] {
                if seen.insert(end.to_string()) {
                    frontier.push(end.to_string());
                }
            }
            if !links
                .iter()
                .any(|x| x["a"] == l["a"] && x["b"] == l["b"] && x["kind"] == l["kind"])
            {
                links.push(l);
            }
        }
    }
    let ids: Vec<String> = seen.into_iter().collect();
    let list = json!(ids).to_string();
    let mut decisions = decision_rows_in(
        c,
        "('settled','superseded','proposed','rejected')",
        "c.id IN (SELECT value FROM json_each(?1))",
        &[&list],
    );
    let reasons = reasons_for(c, &ids);
    for d in &mut decisions {
        d["because"] = json!(reasons.get(s(d, "id")).cloned().unwrap_or_default());
    }
    json!({"focus": id, "decisions": decisions, "links": links})
}
