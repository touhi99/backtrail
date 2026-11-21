use rusqlite::Connection;

pub fn stats(c: &Connection) {
    let n = |sql: &str| -> i64 { c.query_row(sql, [], |r| r.get(0)).unwrap_or(0) };
    println!(
        "entities {} (aliases {}), claims {} (with a quote {}), overrides {}",
        n("SELECT COUNT(*) FROM entities"),
        n("SELECT COUNT(*) FROM entity_aliases"),
        n("SELECT COUNT(*) FROM claims"),
        n("SELECT COUNT(*) FROM claims WHERE quote IS NOT NULL"),
        n("SELECT COUNT(*) FROM kg_overrides")
    );
    println!(
        "claims without evidence: {}",
        n("SELECT COUNT(*) FROM claims c WHERE c.quote IS NULL AND c.origin!='owner'")
    );
}

pub fn state(c: &Connection) {
    let days = |ts: &str| -> i64 { crate::clock::days_ago(ts) };
    let mut st = c.prepare("SELECT t.name, t.state, ( SELECT MAX(last_ts) FROM sources s WHERE s.thread=t.name AND s.kind='session' ), ( SELECT COUNT(*) FROM sources s WHERE s.thread=t.name AND s.kind='session' AND s.last_ts >= date('now','-3 day') ) FROM threads t WHERE t.name!='Unsorted'").expect("prepare query");
    let mut n = 0;
    for (name, state, last, recent) in st
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })
        .expect("run query")
        .map(|row| row.expect("decode query row"))
    {
        let Some(last) = last else { continue };
        let idle = days(&last);
        if state == "moving" && idle >= 14 {
            println!("{name}: marked moving, no session for {idle} days -> quiet?");
            n += 1;
        } else if matches!(state.as_str(), "built" | "parked" | "dropped") && recent >= 2 {
            println!(
                "{name}: marked {state}, {recent} sessions in the last 3 days -> active again?"
            );
            n += 1;
        } else if state == "open" && idle >= 10 {
            println!("{name}: open idea, idle {idle} days");
            n += 1;
        }
    }
    if n == 0 {
        println!("no state changes suggested");
    }
}
