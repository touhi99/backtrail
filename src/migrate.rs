use crate::clock::now;
use crate::db::{count, logw, private};
use rusqlite::{Connection, params};
use std::path::Path;

fn has_table(c: &Connection, name: &str) -> bool {
    count(
        c,
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        &[&name],
    ) > 0
}

fn sql(c: &Connection, statement: &str) -> anyhow::Result<()> {
    Ok(c.execute_batch(statement)?)
}

fn add_column(c: &Connection, table: &str, column: &str) -> anyhow::Result<()> {
    match c.execute(&format!("ALTER TABLE {table} ADD COLUMN {column}"), []) {
        Err(e) if !e.to_string().contains("duplicate column") => Err(e.into()),
        _ => Ok(()),
    }
}

fn backup(c: &Connection, dir: &Path, name: &str) -> anyhow::Result<()> {
    let file = dir.join(name);
    if !file.exists() {
        c.execute("VACUUM INTO ?1", params![file.to_string_lossy()])
            .map_err(|e| anyhow::anyhow!("backup: {e}"))?;
        private(&file, 0o600);
    }
    Ok(())
}

fn transaction<T>(c: &Connection, body: impl FnOnce() -> anyhow::Result<T>) -> anyhow::Result<T> {
    sql(c, "BEGIN")?;
    match body() {
        Ok(v) => sql(c, "COMMIT").map(|()| v),
        Err(e) => {
            c.execute_batch("ROLLBACK").ok();
            Err(e)
        }
    }
}

pub fn run(c: &Connection, dir: &Path) -> anyhow::Result<()> {
    let old_layout = has_table(c, "nodes");
    if old_layout {
        backup(c, dir, "backtrail.pre-unify.db")?;
        if has_table(c, "claims") && !has_table(c, "claims_old") {
            sql(
                c,
                "ALTER TABLE claims RENAME TO claims_old; DROP INDEX IF EXISTS claims_type;",
            )?;
        }
    }
    sql(c, include_str!("sql/schema.sql"))?;
    add_column(c, "chunks", "hash TEXT")?;
    add_column(c, "claims", "check_how TEXT")?;
    add_column(c, "sources", "harness TEXT")?;
    sql(c, "CREATE INDEX IF NOT EXISTS chunks_hash ON chunks(hash)")?;
    if old_layout {
        unify(c)?;
    }
    if count(c, "PRAGMA user_version", &[]) < 1 {
        version_1(c, dir)?;
    }
    Ok(())
}

fn unify(c: &Connection) -> anyhow::Result<()> {
    let n = |q: &str| count(c, q, &[]);
    let items = n("SELECT COUNT(*) FROM nodes WHERE type!='thread'");
    let threads = n("SELECT COUNT(*) FROM nodes WHERE type='thread'");
    let loose = n(
        "SELECT COUNT(*) FROM claims_old WHERE id NOT IN (SELECT 'c-'||id FROM nodes WHERE type!='thread')",
    );
    let bare = crate::db::rows(
        c,
        "SELECT a, b, kind FROM links WHERE a LIKE 'T:%' AND b LIKE 'T:%' AND (source_id IS NULL OR quote IS NULL)",
        &[],
    );
    transaction(c, || {
        sql(c, include_str!("sql/migrate_unify.sql"))?;
        let ts = now();
        for link in &bare {
            let name = |key: &str| {
                link[key]
                    .as_str()
                    .unwrap_or("")
                    .trim_start_matches("T:")
                    .to_string()
            };
            crate::kg::claims::owner_relation(
                c,
                &name("a"),
                &name("b"),
                link["kind"].as_str().unwrap_or(""),
                &ts,
            );
        }
        let want = items + loose + crate::db::sql_int(bare.len());
        let have = n("SELECT COUNT(*) FROM claims");
        if have != want {
            anyhow::bail!("expected {want} rows in claims, found {have}");
        }
        if n("SELECT COUNT(*) FROM threads") != threads {
            anyhow::bail!("thread count differs");
        }
        sql(
            c,
            "DROP TABLE claim_evidence; DROP TABLE claims_old; DROP TABLE nodes; DROP TABLE links;",
        )
    })?;
    let detail = format!(
        "{items} items, {loose} graph-only claims, {} bare relations, {threads} threads",
        bare.len()
    );
    logw(c, "migrate", "unify", "claims", &detail);
    Ok(())
}

fn version_1(c: &Connection, dir: &Path) -> anyhow::Result<()> {
    if count(c, "SELECT COUNT(*) FROM sources", &[]) > 0 {
        backup(c, dir, "backtrail.pre-v1.db")?;
    }
    add_column(c, "sources", "bytes INTEGER NOT NULL DEFAULT 0")?;
    add_column(c, "sources", "tail TEXT")?;
    add_column(c, "sources", "nturns INTEGER NOT NULL DEFAULT 0")?;
    add_column(c, "claims", "review TEXT")?;
    let sessions = crate::db::rows(
        c,
        "SELECT id, ref, ifnull(path,'') AS path FROM sources WHERE kind='session'",
        &[],
    );
    let kept = transaction(c, || {
        for row in &sessions {
            let (id, old) = (row["id"].as_i64(), row["ref"].as_str().unwrap_or(""));
            let Some(stem) = Path::new(row["path"].as_str().unwrap_or("")).file_stem() else {
                continue;
            };
            let new = format!("session:{}", stem.to_string_lossy());
            if new == old {
                continue;
            }
            c.execute("UPDATE sources SET ref=?2 WHERE id=?1", params![id, new])
                .map_err(|e| anyhow::anyhow!("{old}: {e}"))?;
            c.execute(
                "UPDATE overrides SET ref=?2 WHERE ref=?1",
                params![old, new],
            )?;
            c.execute(
                "UPDATE log SET target=?2 WHERE target=?1",
                params![old, new],
            )?;
        }
        let kept = crate::items::keep_all(c);
        sql(c, "PRAGMA user_version=1")?;
        Ok(kept)
    })?;
    if kept > 0 {
        logw(
            c,
            "migrate",
            "v1",
            "claims",
            &format!("{kept} proposed facts kept by the rule"),
        );
    }
    Ok(())
}
