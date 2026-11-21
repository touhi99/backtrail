use super::ensure;
use crate::clock::now;
use crate::db::{fxhash, norm};
use rusqlite::{Connection, params};

const TOPIC_MIN_SESSIONS: i64 = 3;

const TOPICS: &[(&str, &str, &[&str])] = &[
    ("Rust", "tool", &[]),
    ("Python", "tool", &[]),
    ("TypeScript", "tool", &[]),
    ("JavaScript", "tool", &["js"]),
    ("Bash", "tool", &[]),
    ("HTML", "tool", &[]),
    ("CSS", "tool", &[]),
    ("SQL", "tool", &[]),
    ("React", "tool", &[]),
    ("Next.js", "tool", &["nextjs"]),
    ("Vite", "tool", &[]),
    ("Tailwind", "tool", &["tailwindcss"]),
    ("Tauri", "tool", &[]),
    ("Electron", "tool", &[]),
    ("SwiftUI", "tool", &[]),
    ("FastAPI", "tool", &[]),
    ("Flask", "tool", &[]),
    ("Django", "tool", &[]),
    ("Node.js", "tool", &["nodejs"]),
    ("Svelte", "tool", &[]),
    ("Vue", "tool", &[]),
    ("SQLite", "tool", &[]),
    ("Postgres", "tool", &["postgresql"]),
    ("Redis", "tool", &[]),
    ("DuckDB", "tool", &[]),
    ("Supabase", "service", &[]),
    ("Firebase", "service", &[]),
    ("Embeddings", "topic", &["embedding"]),
    ("RAG", "topic", &[]),
    ("LLM", "topic", &["llms"]),
    ("MCP", "topic", &[]),
    ("Knowledge graph", "topic", &["knowledge graphs"]),
    ("Docker", "tool", &[]),
    ("Git", "tool", &[]),
    ("GitHub", "service", &[]),
    ("Cloudflare", "service", &[]),
    ("AWS", "service", &[]),
    ("Homebrew", "tool", &[]),
    ("Playwright", "tool", &[]),
    ("Puppeteer", "tool", &[]),
    ("npm", "tool", &[]),
    ("Cargo", "tool", &[]),
    ("launchd", "tool", &[]),
    ("Xcode", "tool", &[]),
    ("Figma", "tool", &[]),
    ("Stripe", "service", &[]),
    ("Telegram", "service", &[]),
    ("OpenAI", "service", &[]),
    ("Anthropic", "service", &[]),
    ("Gemini", "model", &[]),
    ("Claude Code", "tool", &[]),
    ("Whisper", "model", &[]),
    ("ElevenLabs", "service", &[]),
    ("iOS", "topic", &[]),
    ("Android", "topic", &[]),
    ("macOS", "topic", &[]),
    ("Chrome extension", "tool", &["chrome extensions"]),
    ("Haiku", "model", &["claude haiku", "claude-haiku-5-5"]),
    ("Sonnet", "model", &["claude sonnet", "claude-sonnet-5-5"]),
    ("Opus", "model", &["claude opus", "claude-opus-5-5"]),
];

pub(super) fn seed_topics(c: &Connection) {
    let now = now();
    for (name, kind, aliases) in TOPICS {
        let id = format!("e-{:x}", fxhash(&norm(name)));
        if c.query_row(
            "SELECT 1 FROM entities WHERE id=?1",
            params![id],
            |_| Ok(()),
        )
        .is_ok()
        {
            continue;
        }
        let terms: Vec<String> = std::iter::once(norm(name))
            .chain(aliases.iter().map(|a| norm(a)))
            .collect();
        let phrase = |t: &str| {
            format!(
                "\"{}\"",
                t.chars()
                    .map(|ch| if ch.is_alphanumeric() { ch } else { ' ' })
                    .collect::<String>()
                    .trim()
            )
        };
        let m = terms
            .iter()
            .map(|t| phrase(t))
            .collect::<Vec<_>>()
            .join(" OR ");
        let n: i64 = c
            .query_row("SELECT COUNT(DISTINCT k.source_id) FROM fts JOIN chunks k ON k.id=fts.rowid WHERE fts MATCH ?1 AND k.role='user'", params![m], |r| {
                r.get(0)
            })
            .unwrap_or(0);
        if n < TOPIC_MIN_SESSIONS {
            continue;
        }
        let id = ensure(c, name, kind, &now);
        for a in &terms {
            c.execute(
                "INSERT OR IGNORE INTO entity_aliases(alias,entity_id) VALUES(?1,?2)",
                params![a, id],
            )
            .expect("write to database");
        }
    }
}
