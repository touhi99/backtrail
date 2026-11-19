use super::threads::thread_for_document;
use super::{Source, add_blocks, begin, finish, home_dir, mtime, slug, walk};
use crate::clock::iso;
use crate::config::cfg;
use regex::Regex;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

const BLOCK: usize = 1800;

pub fn read_notes(c: &Connection) -> (usize, usize) {
    let (a, b) = read_markdown(c);
    let (x, y) = read_screens(c);
    (a + x, b + y)
}

fn sections(text: &str, heading_of: impl Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    let mut out = vec![("top".to_string(), String::new())];
    for line in text.lines() {
        if let Some(heading) = heading_of(line) {
            out.push((heading, String::new()));
        } else {
            let body = &mut out.last_mut().expect("sections start with a top entry").1;
            body.push_str(line);
            body.push('\n');
        }
    }
    out
}

fn read_markdown(c: &Connection) -> (usize, usize) {
    let workspace = cfg().workspace.clone();
    let mut files = vec![];
    let mut memory_dirs: Vec<(PathBuf, String)> = vec![];
    let projects = home_dir().join(".claude/projects");
    if workspace.as_os_str().is_empty() {
        for d in std::fs::read_dir(&projects).into_iter().flatten().flatten() {
            memory_dirs.push((
                d.path().join("memory"),
                format!("memory/{}", d.file_name().to_string_lossy()),
            ));
        }
    } else {
        for sub in &cfg().note_dirs {
            walk(&workspace.join(sub), &mut files, "md");
        }
        memory_dirs.push((
            projects.join(slug(&workspace)).join("memory"),
            "memory".to_string(),
        ));
    }
    for (dir, _) in &memory_dirs {
        walk(dir, &mut files, "md");
    }
    let (mut read, mut unchanged) = (0, 0);
    c.execute_batch("BEGIN").expect("write to database");
    for p in files {
        let rel = match memory_dirs.iter().find(|(dir, _)| p.starts_with(dir)) {
            Some((dir, label)) => format!(
                "{}/{}",
                label,
                p.strip_prefix(dir)
                    .expect("file was listed under this memory dir")
                    .display()
            ),
            None => p
                .strip_prefix(&workspace)
                .expect("file was listed under the workspace")
                .display()
                .to_string(),
        };
        let Ok(raw) = std::fs::read_to_string(&p) else {
            continue;
        };
        let rel_path = Path::new(&rel);
        let mut title = rel_path
            .file_stem()
            .expect("listed file has a name")
            .to_string_lossy()
            .to_string();
        if title == "README" {
            let folder = rel_path
                .parent()
                .and_then(|x| x.file_name())
                .map(|x| x.to_string_lossy().to_string());
            title = format!("{} README", folder.unwrap_or_default());
        }
        let version = mtime(&p);
        let source = Source {
            source_ref: &format!("note:{rel}"),
            kind: "note",
            title: &title,
            project: "",
            thread: Some(&thread_for_document("note", &rel)),
            path: &p.to_string_lossy(),
            version,
        };
        let Some(id) = begin(c, &source) else {
            unchanged += 1;
            continue;
        };
        let heading = |l: &str| {
            l.starts_with('#')
                .then(|| l.trim_start_matches('#').trim().to_string())
        };
        let mut seq = 0;
        for (head, body) in sections(&raw, heading) {
            let body = body.trim();
            if body.len() >= 12 {
                add_blocks(c, id, &mut seq, &format!("#{head}"), "note", body, BLOCK);
            }
        }
        let ts = iso(version);
        finish(c, id, &ts, &ts);
        read += 1;
    }
    c.execute_batch("COMMIT").expect("write to database");
    (read, unchanged)
}

pub fn read_artifacts(c: &Connection) -> (usize, usize) {
    let Ok(entries) = std::fs::read_dir(crate::db::home().join("artifacts")) else {
        return (0, 0);
    };
    let (mut read, mut unchanged) = (0, 0);
    c.execute_batch("BEGIN").expect("write to database");
    for e in entries.flatten() {
        let artifact_id = e.file_name().to_string_lossy().to_string();
        let p = e.path().join("index.html");
        let Ok(raw) = std::fs::read_to_string(&p) else {
            continue;
        };
        let title = html_title(&raw).unwrap_or_else(|| artifact_id.clone());
        let version = mtime(&p);
        let source = Source {
            source_ref: &format!("artifact:{artifact_id}"),
            kind: "artifact",
            title: &title,
            project: "",
            thread: Some(&thread_for_document("artifact", &title)),
            path: &p.to_string_lossy(),
            version,
        };
        let Some(id) = begin(c, &source) else {
            unchanged += 1;
            continue;
        };
        let heading = |l: &str| l.trim().strip_prefix("## ").map(|h| h.trim().to_string());
        let mut seq = 0;
        for (head, body) in sections(&strip_html(&raw), heading) {
            let body = non_empty_lines(&body, 1);
            if body.len() >= 12 {
                add_blocks(
                    c,
                    id,
                    &mut seq,
                    &format!("§{head}"),
                    "artifact",
                    &body,
                    BLOCK,
                );
            }
        }
        let published: Option<String> = c
            .query_row(
                "SELECT updated FROM artifact_meta WHERE id=?1",
                [&artifact_id],
                |r| r.get(0),
            )
            .ok();
        let ts = published.map_or_else(|| iso(version), |day| format!("{day}T12:00:00Z"));
        finish(c, id, &ts, &ts);
        read += 1;
    }
    c.execute_batch("COMMIT").expect("write to database");
    (read, unchanged)
}

fn read_screens(c: &Connection) -> (usize, usize) {
    let workspace = cfg().workspace.clone();
    if workspace.as_os_str().is_empty() || cfg().screens_dir.is_empty() {
        return (0, 0);
    }
    let mut files = vec![];
    walk(&workspace.join(&cfg().screens_dir), &mut files, "html");
    let (mut read, mut unchanged) = (0, 0);
    c.execute_batch("BEGIN").expect("write to database");
    for p in files {
        let Ok(raw) = std::fs::read_to_string(&p) else {
            continue;
        };
        let rel = p
            .strip_prefix(&workspace)
            .expect("file was listed under the workspace")
            .display()
            .to_string();
        let title = html_title(&raw).unwrap_or_else(|| rel.clone());
        let version = mtime(&p);
        let source = Source {
            source_ref: &format!("note:{rel}"),
            kind: "note",
            title: &title,
            project: "",
            thread: Some("Backtrail"),
            path: &p.to_string_lossy(),
            version,
        };
        let Some(id) = begin(c, &source) else {
            unchanged += 1;
            continue;
        };
        let mut seq = 0;
        add_blocks(
            c,
            id,
            &mut seq,
            "§screen",
            "note",
            &non_empty_lines(&strip_html(&raw), 3),
            BLOCK,
        );
        let ts = iso(version);
        finish(c, id, &ts, &ts);
        read += 1;
    }
    c.execute_batch("COMMIT").expect("write to database");
    (read, unchanged)
}

fn html_title(html: &str) -> Option<String> {
    Regex::new(r"(?is)<title>(.*?)</title>")
        .expect("regex literal is valid")
        .captures(html)
        .map(|m| m[1].trim().to_string())
}

fn non_empty_lines(text: &str, min: usize) -> String {
    text.lines()
        .map(str::trim)
        .filter(|l| l.len() >= min)
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_html(html: &str) -> String {
    let replace = |text: String, pattern: &str, with: &str| {
        Regex::new(pattern)
            .expect("regex literal is valid")
            .replace_all(&text, with)
            .to_string()
    };
    let mut s = html.to_string();
    for block in [
        r"(?is)<style.*?</style>",
        r"(?is)<script.*?</script>",
        r"(?is)<svg.*?</svg>",
        r"(?s)<!--.*?-->",
    ] {
        s = replace(s, block, " ");
    }
    s = replace(
        s,
        r"(?i)</(h[1-6]|p|li|tr|div|section|figcaption)>|<br\s*/?>",
        "\n",
    );
    s = replace(s, r"(?i)<h[1-6][^>]*>", "\n## ");
    s = replace(s, r"<[^>]+>", " ");
    for (entity, ch) in [
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&nbsp;", " "),
        ("&middot;", "·"),
    ] {
        s = s.replace(entity, ch);
    }
    replace(s, r"[ \t]+", " ")
}
