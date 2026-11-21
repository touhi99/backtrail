use crate::config::cfg;
use crate::db::{id64, logw};
use anyhow::Context;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use std::time::{Duration, Instant};

const BATCH: usize = 32;
const MAX_CHARS: usize = 3000;

fn fill_hashes(c: &Connection) {
    let todo: Vec<(i64, String)> = c
        .prepare("SELECT id, text FROM chunks WHERE hash IS NULL")
        .expect("prepare query")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .collect();
    if todo.is_empty() {
        return;
    }
    c.execute_batch("BEGIN").ok();
    for (id, t) in todo {
        c.execute(
            "UPDATE chunks SET hash=?2 WHERE id=?1",
            params![id, id64(&t)],
        )
        .ok();
    }
    c.execute_batch("COMMIT").ok();
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "embeddings are stored as f32"
)]
fn call(texts: &[String], prefix: &str, secs: u64) -> anyhow::Result<Vec<Vec<f32>>> {
    let e = &cfg().embeddings;
    let input: Vec<String> = texts
        .iter()
        .map(|t| format!("{prefix}{}", t.chars().take(MAX_CHARS).collect::<String>()))
        .collect();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(secs)))
        .build()
        .into();
    let mut resp = agent
        .post(&format!("{}/api/embed", e.url))
        .send_json(json!({"model": e.model, "input": input, "keep_alive": "30m"}))?;
    let v: Value = resp.body_mut().read_json()?;
    let arr = v["embeddings"]
        .as_array()
        .context("no embeddings in reply")?;
    if arr.len() != texts.len() {
        anyhow::bail!("embedding count mismatch");
    }
    Ok(arr
        .iter()
        .map(|row| {
            let mut f: Vec<f32> = row
                .as_array()
                .map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect())
                .unwrap_or_default();
            let n = f.iter().map(|x| x * x).sum::<f32>().sqrt();
            if n > 0.0 {
                for x in &mut f {
                    *x /= n;
                }
            }
            f
        })
        .collect())
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "each value is scaled into -127..=127 before the cast"
)]
pub fn blob(f: &[f32]) -> Vec<u8> {
    let scale = f.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
    let mut out = scale.to_le_bytes().to_vec();
    out.extend(f.iter().map(|x| {
        let q = if scale > 0.0 {
            (x / scale * 127.0).round()
        } else {
            0.0
        };
        (q as i8).to_le_bytes()[0]
    }));
    out
}

pub fn unblob_f32(b: &[u8]) -> Vec<f32> {
    b.as_chunks::<4>()
        .0
        .iter()
        .map(|x| f32::from_le_bytes([x[0], x[1], x[2], x[3]]))
        .collect()
}

fn score(b: &[u8], q: &[f32]) -> Option<f32> {
    let (head, vals) = b.split_at_checked(4)?;
    if vals.len() != q.len() {
        return None;
    }
    let scale = f32::from_le_bytes(head.try_into().ok()?);
    let dot: f32 = vals
        .iter()
        .zip(q)
        .map(|(v, x)| f32::from(i8::from_le_bytes([*v])) * x)
        .sum();
    Some(dot * scale)
}

struct Hit(f32, String);

impl PartialEq for Hit {
    fn eq(&self, o: &Self) -> bool {
        self.0.total_cmp(&o.0) == Ordering::Equal
    }
}
impl Eq for Hit {}
impl PartialOrd for Hit {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Hit {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0.total_cmp(&o.0)
    }
}

pub fn coverage(c: &Connection) -> (i64, i64) {
    let have = c
        .query_row(
            "SELECT COUNT(*) FROM chunks ch JOIN vec v ON v.hash=ch.hash",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let all = c
        .query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))
        .unwrap_or(0);
    (have, all)
}

pub fn run(c: &Connection, limit: usize) -> anyhow::Result<(usize, i64)> {
    let e = &cfg().embeddings;
    fill_hashes(c);
    c.execute("DELETE FROM vec WHERE model!=?1", params![e.model])
        .ok();
    c.execute(
        "DELETE FROM vec WHERE hash NOT IN (SELECT hash FROM chunks WHERE hash IS NOT NULL)",
        [],
    )
    .ok();
    let todo: Vec<(String, String)> = c
        .prepare("SELECT hash, MIN(text) FROM chunks WHERE hash IS NOT NULL AND hash NOT IN (SELECT hash FROM vec) GROUP BY hash LIMIT ?1")
        .expect("prepare query")
        .query_map(params![crate::db::sql_int(limit)], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("run query")
        .map(|row| row.expect("decode query row"))
        .collect();
    let mut done = 0;
    for group in todo.chunks(BATCH) {
        let texts: Vec<String> = group.iter().map(|g| g.1.clone()).collect();
        let vs = call(&texts, &e.document_prefix, 120)?;
        c.execute_batch("BEGIN").ok();
        for ((h, _), v) in group.iter().zip(vs) {
            c.execute(
                "INSERT OR REPLACE INTO vec(hash,model,v) VALUES(?1,?2,?3)",
                params![h, e.model, blob(&v)],
            )
            .ok();
        }
        c.execute_batch("COMMIT").ok();
        done += group.len();
    }
    let pending: i64 = c
        .query_row(
            "SELECT COUNT(DISTINCT hash) FROM chunks WHERE hash NOT IN (SELECT hash FROM vec)",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    Ok((done, pending))
}

pub fn run_cli(c: &Connection, limit: usize) {
    let t = Instant::now();
    match run(c, limit) {
        Ok((n, left)) => {
            println!(
                "embedded {n} passages in {:.1}s, {left} pending",
                t.elapsed().as_secs_f32()
            );
            if n > 0 {
                logw(
                    c,
                    "embed",
                    "embed",
                    &cfg().embeddings.model,
                    &format!("{n} passages"),
                );
            }
        }
        Err(e) => {
            eprintln!("embed: {e}");
            std::process::exit(1);
        }
    }
}

pub fn nearest(c: &Connection, q: &str, k: usize) -> anyhow::Result<Vec<(i64, f32)>> {
    let qv = call(&[q.to_string()], &cfg().embeddings.query_prefix, 4)?
        .pop()
        .context("empty reply")?;
    let mut best: BinaryHeap<Reverse<Hit>> = BinaryHeap::with_capacity(k + 1);
    let mut st = c.prepare("SELECT hash, v FROM vec")?;
    let mut rows = st.query([])?;
    while let Some(r) = rows.next()? {
        let Some(s) = score(r.get_ref(1)?.as_blob()?, &qv) else {
            continue;
        };
        if best.len() < k || best.peek().is_some_and(|w| s > w.0.0) {
            best.push(Reverse(Hit(s, r.get_ref(0)?.as_str()?.to_string())));
            if best.len() > k {
                best.pop();
            }
        }
    }
    let mut lookup = c.prepare_cached("SELECT id FROM chunks WHERE hash=?1")?;
    let mut scored = vec![];
    for Reverse(Hit(s, hash)) in best {
        for id in lookup.query_map(params![hash], |r| r.get::<_, i64>(0))? {
            scored.push((id?, s));
        }
    }
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.truncate(k);
    Ok(scored)
}
