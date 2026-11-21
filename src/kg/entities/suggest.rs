use crate::clock::now;
use crate::db::logw;
use rusqlite::{Connection, params};

pub fn examples(c: &Connection, term: &str, n: usize) -> Vec<(String, String, i64)> {
    let like = format!("%{}%", term.replace(['%', '_'], ""));
    let mut st = c
        .prepare("SELECT ifnull(c.ts,''), c.text, c.source_id FROM chunks c JOIN sources s ON s.id=c.source_id WHERE s.kind='session' AND c.role='user' AND length(c.text)<3000 AND c.text NOT LIKE '%task-notification%' AND c.text NOT LIKE 'This session is being continued%' AND c.text LIKE ?1 ORDER BY c.ts DESC LIMIT ?2")
        .expect("prepare query");
    let n_rows = n * 4;
    st.query_map(params![like, crate::db::sql_int(n_rows)], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })
    .expect("run query")
    .map(|row| row.expect("decode query row"))
    .map(|(ts, text, sid)| {
        let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let chars: Vec<char> = flat.chars().collect();
        let low: Vec<char> = flat.to_lowercase().chars().collect();
        let t: Vec<char> = term.to_lowercase().chars().collect();
        let at = (0..=low.len().saturating_sub(t.len()))
            .find(|&i| low.get(i..i + t.len()) == Some(&t[..]))
            .unwrap_or(0);
        let from = at.saturating_sub(70);
        let to = (at + t.len() + 70).min(chars.len());
        let mut snip: String = chars[from..to].iter().collect();
        if from > 0 {
            snip = format!("…{snip}");
        }
        if to < chars.len() {
            snip.push('…');
        }
        (ts.get(..10).unwrap_or("").to_string(), snip, sid)
    })
    .fold(Vec::<(String, String, i64)>::new(), |mut v, x| {
        if v.len() < n && !v.iter().any(|y| y.1 == x.1) {
            v.push(x);
        }
        v
    })
}

pub fn ignore_suggestion(c: &Connection, term: &str) {
    c.execute(
        "INSERT OR REPLACE INTO kg_overrides(target,field,value,by,ts) VALUES(?1,'entity_ignore','1','owner',?2)",
        params![term.to_lowercase(), now()],
    )
    .expect("write to database");
}

pub fn unignore_suggestion(c: &Connection, term: &str) -> bool {
    let n = c
        .execute(
            "DELETE FROM kg_overrides WHERE target=?1 AND field='entity_ignore'",
            params![term.to_lowercase()],
        )
        .unwrap_or(0);
    if n > 0 {
        logw(c, "owner", "kg-unignore", term, "");
    }
    n > 0
}

pub fn ignored_terms(c: &Connection) -> Vec<String> {
    c.prepare("SELECT target FROM kg_overrides WHERE field='entity_ignore' ORDER BY ts DESC")
        .expect("prepare query")
        .query_map([], |r| r.get::<_, String>(0))
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .collect()
}

type AliasHit = (String, String, usize);
type NameCandidate = (String, usize, usize);

pub fn suggest_data(c: &Connection) -> (Vec<AliasHit>, Vec<NameCandidate>) {
    let words: std::collections::HashSet<String> = std::fs::read_to_string("/usr/share/dict/words")
        .unwrap_or_default()
        .lines()
        .map(|w| w.trim().to_lowercase())
        .collect();
    let known: std::collections::HashSet<String> = c
        .prepare("SELECT alias FROM entity_aliases")
        .expect("prepare query")
        .query_map([], |r| r.get::<_, String>(0))
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .collect();
    let mut st = c.prepare("SELECT c.text, c.source_id FROM chunks c JOIN sources s ON s.id=c.source_id WHERE s.kind='session' AND c.role='user' AND length(c.text)<3000 AND c.text NOT LIKE '%task-notification%' AND c.text NOT LIKE 'This session is being continued%' AND c.text NOT LIKE '<bash-%'").expect("prepare query");
    let mut df: std::collections::HashMap<String, (usize, std::collections::HashSet<i64>, usize)> =
        std::collections::HashMap::default();
    for (t, sid) in st
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .expect("run query")
        .map(|row| row.expect("decode query row"))
    {
        let mut seen = std::collections::HashSet::new();
        let mut prev_end = true;
        for w in t.split_whitespace() {
            let ends = w.ends_with(['.', '!', '?', ':']);
            let core = w.trim_matches(|ch: char| !(ch.is_alphanumeric()));
            let lead_end = prev_end;
            prev_end = ends;
            if core.chars().count() < 4
                || !core.chars().next().is_some_and(char::is_alphabetic)
                || core.contains("http")
                || w.contains(['\'', '’', '/'])
            {
                continue;
            }
            let l = core.to_lowercase();
            if !seen.insert(l.clone()) {
                continue;
            }
            let e = df.entry(l).or_default();
            e.0 += 1;
            e.1.insert(sid);
            if core
                .chars()
                .next()
                .expect("core has at least 4 chars")
                .is_uppercase()
                && !lead_end
            {
                e.2 += 1;
            }
        }
    }
    let squash = |s: &str| s.replace(['-', '_', ' ', '.'], "");
    let known_sq: std::collections::HashMap<String, String> =
        known.iter().map(|k| (squash(k), k.clone())).collect();
    let ignored: std::collections::HashSet<String> = c
        .prepare("SELECT target FROM kg_overrides WHERE field='entity_ignore'")
        .expect("prepare query")
        .query_map([], |r| r.get::<_, String>(0))
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .collect();
    let in_dict = |w: &str| -> bool {
        let stems = [
            w.to_string(),
            w.trim_end_matches('s').to_string(),
            w.trim_end_matches("es").to_string(),
            w.trim_end_matches("ed").to_string(),
            w.trim_end_matches('d').to_string(),
            w.trim_end_matches("ing").to_string(),
            format!("{}e", w.trim_end_matches("ing")),
            w.trim_end_matches("ly").to_string(),
            w.trim_end_matches("er").to_string(),
        ];
        stems.iter().any(|x| words.contains(x))
    };
    let mut alias_hits = vec![];
    let mut cands = vec![];
    for (w, (n, ss, caps)) in &df {
        if *n < 5 || ss.len() < 3 || known.contains(w) || ignored.contains(w) {
            continue;
        }
        let base = [
            ".md", ".json", ".toml", ".yaml", ".yml", ".lock", ".rs", ".ts", ".tsx", ".js", ".py",
            ".sh", ".html", ".css", ".svg", ".txt", ".db",
        ]
        .iter()
        .find_map(|e| w.strip_suffix(e))
        .unwrap_or(w);
        let sq = squash(base);
        let trimmed = sq.trim_end_matches('s').to_string();
        if let Some(k) = known_sq.get(&sq).or_else(|| known_sq.get(&trimmed)) {
            alias_hits.push((w.clone(), k.clone(), *n));
            continue;
        }
        if !in_dict(w) || *caps * 2 >= *n {
            cands.push((w.clone(), *n, ss.len(), *caps));
        }
    }
    cands.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)).then(a.0.cmp(&b.0)));
    alias_hits.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    cands.truncate(150);
    let want: std::collections::HashSet<&str> = cands.iter().map(|x| x.0.as_str()).collect();
    let mut seen: std::collections::HashMap<String, (usize, usize)> =
        std::collections::HashMap::default();
    let mut st = c
        .prepare("SELECT c.text FROM chunks c JOIN sources s ON s.id=c.source_id WHERE s.kind='session' AND c.role!='user'")
        .expect("prepare query");
    for t in st
        .query_map([], |r| r.get::<_, String>(0))
        .expect("run query")
        .map(|row| row.expect("decode query row"))
    {
        let mut prev_end = true;
        for w in t.split_whitespace() {
            let ends = w.ends_with(['.', '!', '?', ':']);
            let lead = prev_end;
            prev_end = ends;
            let core = w.trim_matches(|ch: char| !ch.is_alphanumeric());
            let l = core.to_lowercase();
            if lead || !want.contains(l.as_str()) {
                continue;
            }
            let e = seen.entry(l).or_default();
            e.0 += 1;
            if core.chars().next().is_some_and(char::is_uppercase) {
                e.1 += 1;
            }
        }
    }
    cands.retain(|(w, ..)| {
        seen.get(w)
            .is_some_and(|&(n, caps)| n >= 3 && caps * 2 >= n)
    });
    alias_hits.truncate(15);
    cands.truncate(30);
    (
        alias_hits,
        cands.into_iter().map(|(w, n, s, _)| (w, n, s)).collect(),
    )
}

pub fn suggest(c: &Connection) {
    let (alias_hits, cands) = suggest_data(c);
    println!(
        "likely aliases of known names (same letters, different spacing or plural); add with `kg entity alias`:"
    );
    for (w, k, n) in &alias_hits {
        println!("  {w} -> {k}  ({n} turns)");
    }
    println!(
        "\nterms that may be new entities (not in the dictionary, or mostly capitalised); add with `kg entity add`:"
    );
    for (w, n, s) in &cands {
        println!("  {w}  ({n} turns, {s} sessions)");
    }
    println!(
        "\nnot suggested on purpose: spellings within one edit of a known name collide with real words. Dismiss a term with `kg entity ignore <term>`."
    );
}
