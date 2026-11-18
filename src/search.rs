//! Search over passages and items. Passages are ranked by keyword (BM25) and, when the embedding server answers,
//! by vector similarity; the two rankings are merged by reciprocal rank fusion and then adjusted by what the store
//! knows about each passage. Items are searched in their own index over title and body.
use crate::config::cfg;
use crate::db::{rows, s};
use rusqlite::{Connection, ToSql};
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Default)]
pub struct Filters {
    pub thread: String,
    /// Source kind: session, note, artifact or commit.
    pub kind: String,
    /// Keep passages whose source has a filed item in this state.
    pub state: String,
    pub from: String,
    pub to: String,
}

/// Candidates taken from each ranking, and results returned.
const CANDIDATES: usize = 200;
const RESULTS: usize = 40;
/// Reciprocal rank fusion constant.
const RRF_K: f64 = 60.0;

/// Passage columns, and the filter every passage query shares: ?2 thread, ?3 kind, ?4 from, ?5 to, ?6 item state.
const PASSAGE: &str = "ch.id AS chunk_id, s.id AS source_id, s.ref, s.kind, s.title, s.thread, s.last_ts, ch.locator, ch.text AS body";
const PASSAGE_FILTER: &str = "(?2='' OR s.thread=?2) AND (?3='' OR s.kind=?3)
    AND (?4='' OR substr(s.last_ts,1,10)>=?4) AND (?5='' OR substr(s.last_ts,1,10)<=?5)
    AND (?6='' OR EXISTS(SELECT 1 FROM item i WHERE i.source_id=s.id AND i.state=?6))";

/// FTS expression for a query: every word as a prefix. A word that is a thread name or alias in `config.json` also
/// matches the other names of that thread.
fn fts_query(q: &str) -> String {
    let groups: Vec<Vec<String>> = cfg()
        .aliases
        .iter()
        .map(|(name, aliases)| {
            std::iter::once(name)
                .chain(aliases)
                .map(|x| x.to_lowercase())
                .collect()
        })
        .collect();
    q.split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| {
            let lower = word.to_lowercase();
            let mut alternatives = vec![format!("\"{word}\"*")];
            for group in groups.iter().filter(|g| g.contains(&lower)) {
                alternatives.extend(
                    group
                        .iter()
                        .filter(|x| **x != lower)
                        .map(|x| format!("\"{}\"", x.replace('"', ""))),
                );
            }
            if alternatives.len() == 1 {
                alternatives.remove(0)
            } else {
                format!("({})", alternatives.join(" OR "))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Index of items over title and body, in a temp table of this connection. Rebuilt when the items changed.
fn refresh_item_index(c: &Connection) {
    static BUILT_FOR: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
    let version: String = c
        .query_row(
            "SELECT COUNT(*)||'-'||IFNULL(MAX(updated),'')||'-'||IFNULL(MAX(created),'') FROM claims",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default();
    let exists = crate::db::count(
        c,
        "SELECT COUNT(*) FROM sqlite_temp_master WHERE name='item_fts'",
        &[],
    ) > 0;
    let mut built_for = BUILT_FOR.lock().expect("lock poisoned");
    if exists && *built_for == version {
        return;
    }
    c.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS temp.item_fts USING fts5(title, body, tokenize='porter unicode61');
         DELETE FROM temp.item_fts;
         INSERT INTO temp.item_fts(rowid, title, body)
           SELECT rowid, title, IFNULL(body,'') FROM claims WHERE type IN ('decision','step','question','turn');",
    )
    .ok();
    *built_for = version;
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A snippet as plain text, with matches in [brackets].
pub fn plain_snippet(html: &str) -> String {
    html.replace("<mark>", "[")
        .replace("</mark>", "]")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

/// Item states that mean the owner kept the item.
const KEPT: &[&str] = &["settled", "done", "open", "answered", "looks_done", "doing"];

/// An item that cites a passage or its source.
struct Citing<'a> {
    item: &'a Value,
    /// Rejected, or marked replaced.
    dead: bool,
    /// The owner kept it: it is not dead and its state is one of `KEPT`.
    kept: bool,
    /// It cites this passage, not only the same source.
    direct: bool,
}

/// How much the store trusts a passage, as a factor on its rank, with the reasons.
fn weigh(passage: &Value, thread_state: &str, citing: &[Citing<'_>]) -> (f64, Vec<&'static str>) {
    let mut factor = 1.0;
    let mut why = vec![];
    let mut apply = |f: f64, reason: &'static str| {
        factor *= f;
        why.push(reason);
    };
    match thread_state {
        "dropped" => apply(0.6, "thread dropped"),
        "parked" => apply(0.85, "thread parked"),
        _ => {}
    }
    let direct: Vec<&Citing<'_>> = citing.iter().filter(|x| x.direct).collect();
    if direct.iter().any(|x| x.kept) {
        apply(1.3, "cited by a kept item");
    } else if !direct.is_empty() && direct.iter().all(|x| x.dead) {
        apply(0.5, "cited only by rejected or replaced items");
    } else if !direct.is_empty() {
        apply(1.1, "cited by a suggestion");
    } else if citing.iter().any(|x| x.kept) {
        apply(1.1, "same source has a kept item");
    }
    if matches!(passage["kind"].as_str(), Some("note" | "artifact")) {
        factor *= 1.15;
    }
    (factor, why)
}

/// Whether a search may use embeddings or only the keyword index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Semantic,
    Text,
}

pub fn search(c: &Connection, q: &str, f: &Filters, mode: Mode) -> Value {
    fn filters<'a>(text: &'a dyn ToSql, f: &'a Filters) -> [&'a dyn ToSql; 6] {
        [text, &f.thread, &f.kind, &f.from, &f.to, &f.state]
    }
    let fts = fts_query(q);
    let (embedded, passages) = crate::embed::coverage(c);
    let mut res = json!({"results": [], "items": [], "mode": "text", "note": "", "embedded": embedded, "passages": passages});
    if fts.is_empty() {
        return res;
    }

    // 1. Candidates by keyword. Each passage's fused score starts with its keyword rank.
    let keyword_sql = format!(
        "SELECT {PASSAGE}, snippet(fts, 0, char(1), char(2), '…', 28) AS snip, bm25(fts) AS score
         FROM fts JOIN chunks ch ON ch.id=fts.rowid JOIN sources s ON s.id=ch.source_id
         WHERE fts MATCH ?1 AND {PASSAGE_FILTER} ORDER BY score, ch.id LIMIT {CANDIDATES}"
    );
    let mut by_keyword = rows(c, &keyword_sql, &filters(&fts, f));
    if by_keyword.is_empty() && fts.contains(' ') {
        // No passage holds every word (a question typed as a sentence): rank passages that hold some of them.
        let any_word = q
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|word| word.chars().count() >= 3)
            .map(|word| format!("\"{word}\"*"))
            .collect::<Vec<_>>()
            .join(" OR ");
        if !any_word.is_empty() {
            by_keyword = rows(c, &keyword_sql, &filters(&any_word, f));
        }
    }
    let mut candidates: HashMap<i64, Value> = HashMap::new();
    let mut fused: HashMap<i64, f64> = HashMap::new();
    for (rank, row) in by_keyword.into_iter().enumerate() {
        let id = row["chunk_id"]
            .as_i64()
            .expect("keyword rows carry chunk_id");
        *fused.entry(id).or_insert(0.0) += 1.0 / (RRF_K + rank as f64 + 1.0);
        candidates.insert(id, row);
    }

    // 2. Candidates by meaning, when the embedding server answers.
    if mode == Mode::Text {
        res["note"] = json!("keyword search only (requested)");
    } else if !crate::embed::enabled() {
        res["note"] = json!("keyword search only: embeddings are turned off in config.json");
    } else {
        match crate::embed::nearest(c, q, CANDIDATES) {
            Err(e) => {
                res["note"] = json!(format!(
                    "keyword search only: the embedding server did not answer ({e}). Start Ollama and run `ollama pull embeddinggemma-2:270m` once."
                ));
            }
            Ok(nearest) => {
                res["mode"] = json!("hybrid");
                if embedded < passages {
                    res["note"] =
                        json!(format!("{embedded} of {passages} passages embedded so far"));
                }
                let ids = json!(nearest.iter().map(|x| x.0).collect::<Vec<_>>()).to_string();
                let found = rows(
                    c,
                    &format!(
                        "SELECT {PASSAGE}, '' AS snip FROM chunks ch JOIN sources s ON s.id=ch.source_id
                         WHERE ch.id IN (SELECT value FROM json_each(?1)) AND {PASSAGE_FILTER}"
                    ),
                    &filters(&ids, f),
                );
                let mut by_id: HashMap<i64, Value> = found
                    .into_iter()
                    .map(|r| {
                        (
                            r["chunk_id"].as_i64().expect("chunk rows carry chunk_id"),
                            r,
                        )
                    })
                    .collect();
                let mut rank = 0.0;
                for (id, _) in nearest {
                    if let Some(row) = by_id.remove(&id) {
                        rank += 1.0;
                        *fused.entry(id).or_insert(0.0) += 1.0 / (RRF_K + rank);
                        candidates.entry(id).or_insert(row);
                    }
                }
            }
        }
    }

    // 3. Adjust each candidate by its thread's state and by the items that cite it or its source.
    let thread_state: HashMap<String, String> = rows(c, "SELECT name, state FROM threads", &[])
        .iter()
        .map(|r| (s(r, "name").to_string(), s(r, "state").to_string()))
        .collect();
    let mut source_ids: Vec<i64> = candidates
        .values()
        .filter_map(|r| r["source_id"].as_i64())
        .collect();
    source_ids.sort_unstable();
    source_ids.dedup();
    let citing_items = rows(
        c,
        "SELECT id, type, title, state, IFNULL(stale,'') AS stale, source_id, chunk_id FROM item
         WHERE source_id IN (SELECT value FROM json_each(?1))",
        &[&json!(source_ids).to_string()],
    );
    let mut ranked: Vec<(f64, i64, Value)> = vec![];
    for (id, mut row) in candidates {
        let source = row["source_id"].as_i64();
        let mut citing: Vec<Citing<'_>> = citing_items
            .iter()
            .filter(|item| item["source_id"].as_i64() == source)
            .map(|item| {
                let dead =
                    s(item, "state") == "rejected" || s(item, "stale").starts_with("superseded");
                Citing {
                    item,
                    dead,
                    kept: !dead && KEPT.contains(&s(item, "state")),
                    direct: item["chunk_id"].as_i64() == Some(id),
                }
            })
            .collect();
        citing.sort_by_key(|x| !x.direct);
        let state = thread_state
            .get(s(&row, "thread"))
            .map_or("", |x| x.as_str());
        let (factor, why) = weigh(&row, state, &citing);
        let snippet = if s(&row, "snip").is_empty() {
            escape_html(&s(&row, "body").chars().take(240).collect::<String>())
        } else {
            escape_html(s(&row, "snip"))
                .replace('\u{1}', "<mark>")
                .replace('\u{2}', "</mark>")
        };
        row["snip"] = json!(snippet);
        row["thread_state"] = json!(state);
        row["why"] = json!(why);
        row["items"] = citing
            .iter()
            .take(3)
            .map(|x| json!({"id": x.item["id"], "type": x.item["type"], "title": x.item["title"], "state": x.item["state"], "dead": x.dead}))
            .collect();
        row.as_object_mut()
            .expect("result row is a JSON object")
            .remove("body");
        ranked.push((fused[&id] * factor, id, row));
    }
    // Best score first; equal scores in passage order, so the same query always gives the same list.
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    res["results"] = ranked.into_iter().take(RESULTS).map(|x| x.2).collect();

    // 4. Filed items, searched in their own index.
    refresh_item_index(c);
    res["items"] = json!(rows(
        c,
        "SELECT n.id, n.type, n.title, n.thread, n.state, IFNULL(n.stale,'') AS stale, n.review
         FROM temp.item_fts f JOIN claims n ON n.rowid=f.rowid LEFT JOIN sources s ON s.id=n.source_id
         WHERE item_fts MATCH ?1 AND (?2='' OR n.thread=?2) AND (?3='' OR s.kind=?3)
           AND (?4='' OR substr(IFNULL(s.last_ts,n.created),1,10)>=?4)
           AND (?5='' OR substr(IFNULL(s.last_ts,n.created),1,10)<=?5)
           AND (?6='' OR n.state=?6) AND (n.state!='rejected' OR ?6='rejected')
         ORDER BY (CASE WHEN n.state IN ('settled','done','open','answered','looks_done','doing')
                         AND IFNULL(n.stale,'') NOT LIKE 'superseded%' THEN 0 ELSE 1 END),
                  bm25(item_fts, 3.0, 1.0) LIMIT 20",
        &filters(&fts, f),
    ));
    res
}

/// `bt search`: the mode, matching items, and the ten best passages with the reasons for their rank.
pub fn print(c: &Connection, q: &str, f: &Filters, mode: Mode) {
    let r = search(c, q, f, mode);
    let note = Some(s(&r, "note"))
        .filter(|n| !n.is_empty())
        .map(|n| format!(" ({n})"))
        .unwrap_or_default();
    println!("mode: {}{note}", s(&r, "mode"));
    for i in r["items"].as_array().expect("search result has items") {
        println!(
            "ITEM [{}] {} {}: {}",
            s(i, "thread"),
            s(i, "type"),
            s(i, "state"),
            s(i, "title")
        );
    }
    for x in r["results"]
        .as_array()
        .expect("search result has results")
        .iter()
        .take(10)
    {
        let why: Vec<&str> = x["why"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|w| w.as_str())
            .collect();
        let why = if why.is_empty() {
            String::new()
        } else {
            format!(" | {}", why.join("; "))
        };
        println!(
            "{} | {} | {}{why}\n    {}",
            s(x, "ref"),
            s(x, "locator"),
            x["thread"].as_str().unwrap_or("-"),
            plain_snippet(s(x, "snip")).replace('\n', " ")
        );
    }
}
