use crate::config::cfg;
use crate::db::{id64, logw};
use anyhow::Context;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const BATCH: usize = 32;
const MAX_CHARS: usize = 3000;

pub fn enabled() -> bool {
    let e = &cfg().embeddings;
    !e.url.is_empty() && !e.model.is_empty()
}

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

fn blob(f: &[f32]) -> Vec<u8> {
    f.iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn unblob(b: &[u8]) -> Vec<f32> {
    b.as_chunks::<4>()
        .0
        .iter()
        .map(|x| f32::from_le_bytes([x[0], x[1], x[2], x[3]]))
        .collect()
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
    if !enabled() {
        anyhow::bail!("embeddings are turned off in config.json (\"embeddings\": false)");
    }
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

type Cache = Option<(String, Vec<(i64, Vec<f32>)>)>;
static CACHE: Mutex<Cache> = Mutex::new(None);

pub fn nearest(c: &Connection, q: &str, k: usize) -> anyhow::Result<Vec<(i64, f32)>> {
    if !enabled() {
        anyhow::bail!("off");
    }
    let qv = call(&[q.to_string()], &cfg().embeddings.query_prefix, 4)?
        .pop()
        .context("empty reply")?;
    let sig: String = c.query_row(
        "SELECT (SELECT COUNT(*) FROM vec)||'-'||(SELECT COUNT(*) FROM chunks)",
        [],
        |r| r.get(0),
    )?;
    let mut g = CACHE.lock().map_err(|_| anyhow::anyhow!("cache lock"))?;
    if g.as_ref().is_none_or(|x| x.0 != sig) {
        let all: Vec<(i64, Vec<f32>)> = c
            .prepare("SELECT ch.id, v.v FROM chunks ch JOIN vec v ON v.hash=ch.hash")?
            .query_map([], |r| Ok((r.get(0)?, unblob(&r.get::<_, Vec<u8>>(1)?))))?
            .collect::<rusqlite::Result<_>>()?;
        *g = Some((sig, all));
    }
    let mut scored: Vec<(i64, f32)> = g
        .as_ref()
        .expect("cache filled above")
        .1
        .iter()
        .filter(|(_, v)| v.len() == qv.len())
        .map(|(id, v)| (*id, v.iter().zip(&qv).map(|(a, b)| a * b).sum()))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(k);
    Ok(scored)
}
