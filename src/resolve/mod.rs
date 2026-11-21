mod candidates;
mod lifecycle;
mod packet;
mod reverse;

pub use candidates::*;
pub use lifecycle::*;
pub use packet::*;
pub use reverse::*;

use crate::db::{rows, s};
use rusqlite::Connection;

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn when(c: &Connection, id: &str) -> String {
    rows(
        c,
        "SELECT COALESCE(NULLIF(c.asserted_at,''), s.first_ts, c.created, '') AS w
         FROM claims c LEFT JOIN sources s ON s.id=c.source_id WHERE c.id=?1",
        &[&id],
    )
    .first()
    .map(|r| s(r, "w").to_string())
    .unwrap_or_default()
}
