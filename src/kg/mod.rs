pub mod claims;
pub mod entities;
pub mod export;
pub mod extract;
pub mod lint;
pub mod query;
pub mod screen;

fn clip(text: &str, max: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= max {
        line
    } else {
        format!("{}…", line.chars().take(max).collect::<String>())
    }
}

fn day(ts: Option<&str>) -> String {
    ts.and_then(|t| t.get(..10)).unwrap_or("?").to_string()
}

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
