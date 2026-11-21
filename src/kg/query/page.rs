use super::{claim_rows, line, tokens};
use crate::kg::entities;
use rusqlite::{Connection, params};

pub fn page(c: &Connection, name: &str, budget: usize) {
    let Some((id, ename)) = entities::find(c, name) else {
        println!("no record: no entity named '{name}'");
        return;
    };
    let aliases: Vec<String> = c
        .prepare("SELECT alias FROM entity_aliases WHERE entity_id=?1")
        .expect("prepare query")
        .query_map(params![id], |r| r.get::<_, String>(0))
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .collect();
    let all = claim_rows(
        c,
        "c.id IN (SELECT claim_id FROM claim_about WHERE entity_id=?1) AND c.status!='retracted'",
        &[&id],
        2000,
    );
    let mut out = format!(
        "{ename} ({} claims; also: {})\n",
        all.len(),
        aliases.join(", ")
    );
    let reserve = 220;
    let mut shown = 0;
    for r in all.iter().take(25) {
        let l = format!("{}\n", line(r, 110));
        if tokens(&out) + tokens(&l) + reserve > budget {
            break;
        }
        out.push_str(&l);
        shown += 1;
    }
    let rest = &all[shown..];
    if !rest.is_empty() {
        out.push_str("older, rolled up by month:\n");
        let mut by: Vec<(String, usize)> = vec![];
        for r in rest {
            let m =
                r.ts.as_deref()
                    .and_then(|t| t.get(..7))
                    .unwrap_or("?")
                    .to_string();
            match by.last_mut() {
                Some(l) if l.0 == m => l.1 += 1,
                _ => by.push((m, 1)),
            }
        }
        for (m, n) in by {
            let l = format!("  {m}: {n} claims\n");
            if tokens(&out) + tokens(&l) > budget {
                out.push_str("  …\n");
                break;
            }
            out.push_str(&l);
        }
    }
    print!("{out}");
    println!("[{} tokens, budget {budget}]", tokens(&out));
}

pub fn trace(c: &Connection, name: &str) {
    let Some((id, ename)) = entities::find(c, name) else {
        println!("no record: no entity named '{name}'");
        return;
    };
    let mut rows = claim_rows(
        c,
        "c.id IN (SELECT claim_id FROM claim_about WHERE entity_id=?1) AND c.status!='retracted' AND (c.type LIKE 'relation:%' OR c.type='decision')",
        &[&id],
        60,
    );
    rows.reverse();
    if rows.is_empty() {
        println!("no record: no dated claims about {ename}");
        return;
    }
    println!("{ename}, oldest first:");
    for r in rows.iter().take(30) {
        println!("{}", line(r, 100));
    }
}
