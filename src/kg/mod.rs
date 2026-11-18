//! The knowledge graph: entities (projects, tools, topics) and the claims about them. Claims are rows of `claims`,
//! the same table that holds work items; `claim_about` says which entities a claim is about. The `fact` view
//! (sql/views.sql) is how the graph is read.
pub mod claims;
pub mod entities;
pub mod export;
pub mod extract;
pub mod lint;
pub mod query;
pub mod screen;

/// Text on one line, cut to `max` characters with an ellipsis.
fn clip(text: &str, max: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= max {
        line
    } else {
        format!("{}…", line.chars().take(max).collect::<String>())
    }
}

/// The `YYYY-MM-DD` of a timestamp; `?` when there is none.
fn day(ts: Option<&str>) -> String {
    ts.and_then(|t| t.get(..10)).unwrap_or("?").to_string()
}

/// True when `word` appears in `text` with no letter or digit directly before or after it.
fn has_word(text: &str, word: &str) -> bool {
    let mut from = 0;
    while let Some(i) = text[from..].find(word) {
        let (start, end) = (from + i, from + i + word.len());
        let before = text[..start]
            .chars()
            .next_back()
            .is_none_or(|ch| !ch.is_alphanumeric());
        let after = text[end..]
            .chars()
            .next()
            .is_none_or(|ch| !ch.is_alphanumeric());
        if before && after {
            return true;
        }
        from = end;
    }
    false
}
