use super::entities;
use crate::db::rows;
use rusqlite::Connection;
use serde_json::{Value, json};

pub fn entities(c: &Connection) -> Value {
    json!(rows(
        c,
        "SELECT * FROM entity_stat ORDER BY claims DESC, name",
        &[]
    ))
}

pub fn graph(c: &Connection) -> Value {
    json!({
        "nodes": rows(c, "SELECT * FROM entity_stat", &[]),
        "edges": rows(
            c,
            "SELECT x.entity_id AS a, y.entity_id AS b, COUNT(*) AS w
             FROM claim_about x JOIN claim_about y ON x.claim_id=y.claim_id AND x.entity_id<y.entity_id
             GROUP BY 1, 2",
            &[],
        ),
        "none": rows(
            c,
            "SELECT COUNT(*) AS n FROM fact f WHERE NOT EXISTS (SELECT 1 FROM claim_about z WHERE z.claim_id=f.id)",
            &[],
        ),
    })
}

pub fn lineage(c: &Connection) -> Value {
    json!(rows(
        c,
        "SELECT f.id, f.type, f.text, f.speaker, f.asserted_at, f.status, replace(f.entities, char(31), ' → ') AS path,
                f.quote, f.stated, f.source_id, f.source_title
         FROM fact f
         WHERE f.type LIKE 'relation:%' AND f.status!='retracted'
         ORDER BY f.asserted_at, f.id",
        &[],
    ))
}

pub fn suggestions(c: &Connection) -> Value {
    let (aliases, terms) = entities::suggest_data(c);
    json!({
        "hidden": entities::ignored_terms(c),
        "aliases": aliases.iter().map(|(alias, entity, turns)| json!({"alias": alias, "entity": entity, "turns": turns})).collect::<Vec<_>>(),
        "entities": terms.iter().enumerate().map(|(i, (term, turns, sessions))| json!({"term": term, "turns": turns, "sessions": sessions,
            "examples": if i < 40 { entities::examples(c, term, 5).into_iter().map(|(d, s, id)| json!({"date": d, "text": s, "source_id": id})).collect::<Vec<_>>() } else { vec![] }})).collect::<Vec<_>>(),
    })
}

pub fn claims(c: &Connection, entity: &str, text: &str, status: &str, speaker: &str) -> Value {
    const FILTER: &str = "(?1='' OR (?1='_none' AND NOT EXISTS (SELECT 1 FROM claim_about z WHERE z.claim_id=f.id))
              OR EXISTS (SELECT 1 FROM claim_about z WHERE z.claim_id=f.id AND z.entity_id=?1))
         AND (?2='' OR f.text LIKE '%'||?2||'%' OR ifnull(f.body,'') LIKE '%'||?2||'%' OR ifnull(f.quote,'') LIKE '%'||?2||'%')
         AND (?3='' OR f.status=?3) AND (?4='' OR f.speaker=?4)";
    let args: [&dyn rusqlite::ToSql; 4] = [&entity, &text, &status, &speaker];
    let total = crate::db::count(
        c,
        &format!("SELECT COUNT(*) FROM fact f WHERE {FILTER}"),
        &args,
    );
    let claims = rows(
        c,
        &format!(
            "SELECT f.id, f.type, f.text, f.body, f.speaker, f.asserted_at, f.status,
                    replace(f.entities, char(31), ' | ') AS entities
             FROM fact f WHERE {FILTER} ORDER BY f.asserted_at DESC, f.id LIMIT 150"
        ),
        &args,
    );
    let ids = json!(claims.iter().map(|claim| &claim["id"]).collect::<Vec<_>>()).to_string();
    let evidence = rows(
        c,
        "SELECT f.id AS claim_id, f.quote, f.stated, f.asserted_at AS ts, f.source_id, f.source, f.source_title
         FROM fact f WHERE f.quote IS NOT NULL AND f.id IN (SELECT value FROM json_each(?1))",
        &[&ids],
    );
    json!({"total": total, "claims": claims, "evidence": evidence})
}
