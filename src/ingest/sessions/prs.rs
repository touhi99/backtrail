use super::parse::{PrLink, Push, parse_prs, parse_pushes};
use rusqlite::{Connection, params};

pub(super) fn insert_prs(c: &Connection, sid: i64, prs: &[PrLink]) {
    for p in prs {
        c.execute(
            "INSERT OR IGNORE INTO source_prs(source_id, repo, number, url, ts) VALUES(?1,?2,?3,?4,?5)",
            params![sid, p.repo, p.number, p.url, p.ts],
        )
        .expect("write to database");
    }
}

pub(super) fn backfill_prs(c: &Connection) {
    for row in crate::db::rows(
        c,
        "SELECT id, path FROM sources WHERE kind='session' AND prs_read=0 AND ifnull(path,'')!=''",
        &[],
    ) {
        let Some(sid) = row["id"].as_i64() else {
            continue;
        };
        let raw = std::fs::read_to_string(crate::db::s(&row, "path")).unwrap_or_default();
        insert_prs(c, sid, &parse_prs(&raw));
        c.execute("UPDATE sources SET prs_read=1 WHERE id=?1", params![sid])
            .expect("write to database");
    }
}

pub(super) fn insert_pushes(c: &Connection, sid: i64, pushes: &[Push]) {
    for p in pushes {
        c.execute(
            "INSERT OR IGNORE INTO source_pushes(source_id, repo, branch, sha, ts) VALUES(?1,?2,?3,?4,?5)",
            params![sid, p.repo, p.branch, p.sha, p.ts],
        )
        .expect("write to database");
    }
}

pub(super) fn backfill_pushes(c: &Connection) {
    for row in crate::db::rows(
        c,
        "SELECT id, path FROM sources WHERE kind='session' AND pushes_read=0 AND ifnull(path,'')!=''",
        &[],
    ) {
        let Some(sid) = row["id"].as_i64() else {
            continue;
        };
        let raw = std::fs::read_to_string(crate::db::s(&row, "path")).unwrap_or_default();
        insert_pushes(c, sid, &parse_pushes(&raw));
        c.execute("UPDATE sources SET pushes_read=1 WHERE id=?1", params![sid])
            .expect("write to database");
    }
}
