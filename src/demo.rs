use crate::db::home;
use rusqlite::{Connection, params};

pub fn demo(c: &Connection) {
    if std::env::var("BACKTRAIL_HOME").is_err() {
        eprintln!(
            "set BACKTRAIL_HOME to an empty folder first; demo never writes to the default store"
        );
        std::process::exit(1);
    }
    let h = home();
    std::fs::write(h.join(".demo"), "demo store\n").ok();
    std::fs::write(h.join("threads.json"), r#"[
 {"area":"Web","name":"Recipe site","state":"moving","summary":"Static site for sharing family recipes.","regex":"recipe|site"},
 {"area":"Web","name":"Photo archive","state":"parked","summary":"Scan, tag and search old prints.","regex":"photo|scan"},
 {"area":"Tools","name":"Budget tracker","state":"built","summary":"CSV import and monthly summary.","regex":"budget|csv"},
 {"area":"Ideas, no repo","name":"Garden planner","state":"open","summary":"Planting calendar by zone.","regex":"garden"},
 {"area":"Tools","name":"Old backup script","state":"dropped","summary":"Replaced by the system backup.","regex":"backup"}]"#).ok();
    crate::ingest::threads::seed(c);
    let docs: &[(&str, &str, &str, &str)] = &[
        (
            "note:demo/recipe-site plan.md",
            "recipe-site plan",
            "Recipe site",
            "# Plan\nThe site is static: one Markdown file per recipe, built into HTML. Search runs in the browser, so there is no server to host.\n# Open\nWhich host to use is not decided. A custom domain costs about 12 euros a year.",
        ),
        (
            "note:demo/photo archive notes.md",
            "photo archive notes",
            "Photo archive",
            "# Scanning\nA flatbed scan at 600 dpi takes about 40 seconds per print. Tags come from the album name written on the back.\n# Parked\nThe batch of 300 prints is on hold until the new scanner arrives.\n# Export\nTag lists are exported as CSV, reusing the budget importer.",
        ),
        (
            "note:demo/budget csv import.md",
            "budget csv import",
            "Budget tracker",
            "# Import\nThe bank export is a semicolon CSV with German dates. The importer maps columns by header name and rejects rows with an unknown date format.\n# Done\nMonthly summary shows totals per category.",
        ),
        (
            "note:demo/garden ideas.md",
            "garden ideas",
            "Garden planner",
            "# Idea\nA calendar that shows what to sow in which week for a given frost date. Data source could be a public seed table.\n# Reuse\nThe planner can be built on the recipe site's Markdown pipeline.",
        ),
        (
            "note:demo/old backup script.md",
            "old backup script",
            "Old backup script",
            "# Status\nThe script is replaced by the system backup, which also covers the photo folder.",
        ),
    ];
    for (n, (r, t, th, body)) in docs.iter().enumerate() {
        c.execute("INSERT OR IGNORE INTO sources(ref,kind,title,thread,first_ts,last_ts,mtime) VALUES(?1,'note',?2,?3,'2026-11-02T12:00:00Z','2026-11-09T12:00:00Z',1)", params![r, t, th])
            .expect("write to database");
        c.execute(
            "UPDATE sources SET last_ts=?2 WHERE ref=?1",
            params![r, format!("2026-11-{:02}T12:00:00Z", 3 + n * 2)],
        )
        .expect("write to database");
        let sid: i64 = c
            .query_row("SELECT id FROM sources WHERE ref=?1", params![r], |x| {
                x.get(0)
            })
            .expect("run query");
        for (i, part) in body.split("\n# ").enumerate() {
            let text = part.trim_start_matches("# ").to_string();
            c.execute(
                "INSERT INTO chunks(source_id,seq,locator,role,text,ts) VALUES(?1,?2,?3,'note',?4,?5)",
                params![
                    sid,
                    crate::db::sql_int(i),
                    format!("#part {}", i + 1),
                    text,
                    format!("2026-11-{:02}T12:00:00Z", 3 + n * 2)
                ],
            )
            .expect("write to database");
            let cid = c.last_insert_rowid();
            c.execute(
                "INSERT INTO fts(rowid,text) VALUES(?1,?2)",
                params![cid, text],
            )
            .expect("write to database");
        }
        c.execute("UPDATE sources SET nchunks=(SELECT COUNT(*) FROM chunks WHERE source_id=?1) WHERE id=?1", params![sid]).expect("write to database");
    }
    let items = [
        (
            "decision",
            "Recipe site",
            "Static site, no server",
            "One Markdown file per recipe, built into HTML.",
            "note:demo/recipe-site plan.md",
            "The site is static",
            "settled",
            "compiled",
        ),
        (
            "question",
            "Recipe site",
            "Which host to use?",
            "Not decided; a domain is about 12 euros a year.",
            "note:demo/recipe-site plan.md",
            "Which host to use is not decided",
            "open",
            "compiled",
        ),
        (
            "step",
            "Recipe site",
            "Build search in the browser",
            "",
            "note:demo/recipe-site plan.md",
            "Search runs in the browser",
            "open",
            "compiled",
        ),
        (
            "decision",
            "Photo archive",
            "Tags come from the album name on the back",
            "",
            "note:demo/photo archive notes.md",
            "Tags come from the album name",
            "settled",
            "compiled",
        ),
        (
            "question",
            "Photo archive",
            "Scan the 300 prints before the new scanner?",
            "On hold until it arrives.",
            "note:demo/photo archive notes.md",
            "on hold until the new scanner arrives",
            "open",
            "compiled",
        ),
        (
            "decision",
            "Budget tracker",
            "Importer rejects unknown date formats",
            "",
            "note:demo/budget csv import.md",
            "rejects rows with an unknown date format",
            "settled",
            "compiled",
        ),
        (
            "step",
            "Budget tracker",
            "Monthly summary per category",
            "",
            "note:demo/budget csv import.md",
            "Monthly summary shows totals per category",
            "done",
            "compiled",
        ),
        (
            "decision",
            "Recipe site",
            "Search runs in the browser",
            "No server; the Search index ships with the HTML.",
            "note:demo/recipe-site plan.md",
            "Search runs in the browser",
            "settled",
            "compiled",
        ),
        (
            "decision",
            "Recipe site",
            "Keep hosting under 12 euros a year",
            "",
            "note:demo/recipe-site plan.md",
            "A custom domain costs about 12 euros a year",
            "settled",
            "compiled",
        ),
        (
            "decision",
            "Photo archive",
            "Scan at 600 dpi",
            "Scanner setting for every print.",
            "note:demo/photo archive notes.md",
            "A flatbed scan at 600 dpi",
            "settled",
            "compiled",
        ),
        (
            "decision",
            "Photo archive",
            "Wait for the new scanner",
            "The batch of 300 prints stays on hold.",
            "note:demo/photo archive notes.md",
            "on hold until the new scanner arrives",
            "proposed",
            "suggested",
        ),
        (
            "decision",
            "Budget tracker",
            "Map CSV columns by header name",
            "",
            "note:demo/budget csv import.md",
            "maps columns by header name",
            "settled",
            "compiled",
        ),
        (
            "decision",
            "Budget tracker",
            "Bank CSV uses German dates",
            "",
            "note:demo/budget csv import.md",
            "semicolon CSV with German dates",
            "settled",
            "compiled",
        ),
        (
            "decision",
            "Garden planner",
            "Sow calendar keyed to the frost date",
            "",
            "note:demo/garden ideas.md",
            "for a given frost date",
            "proposed",
            "suggested",
        ),
        (
            "decision",
            "Photo archive",
            "Export tag lists as CSV",
            "Reuses the import code from the budget tracker.",
            "note:demo/photo archive notes.md",
            "exported as CSV",
            "settled",
            "compiled",
        ),
        (
            "decision",
            "Garden planner",
            "Build the planner on the Markdown pipeline",
            "",
            "note:demo/garden ideas.md",
            "built on the recipe site",
            "proposed",
            "suggested",
        ),
        (
            "decision",
            "Old backup script",
            "Retire the script",
            "The system backup also covers the photo folder.",
            "note:demo/old backup script.md",
            "replaced by the system backup",
            "settled",
            "compiled",
        ),
        (
            "step",
            "Garden planner",
            "Find a public seed table",
            "Suggested while reading the idea note.",
            "note:demo/garden ideas.md",
            "public seed table",
            "proposed",
            "suggested",
        ),
    ];
    for (ty, th, ti, bo, so, qu, st, or) in items {
        crate::kg::entities::ensure(c, th, "project", &crate::clock::now());
        let it = crate::items::NewItem {
            kind: ty.into(),
            thread: th.into(),
            title: ti.into(),
            body: bo.into(),
            source: so.into(),
            quote: qu.into(),
            state: Some(st.into()),
            origin: or.into(),
            date: Some("2026-11-09T12:00:00Z".into()),
            entities: vec![th.into()],
            valid_to: None,
            check_how: None,
        };
        if let Err(e) = crate::items::add(c, &it) {
            eprintln!("demo item rejected: {e}");
        }
    }
    c.execute("UPDATE claims SET done_by='evidence', answer='summary view exists' WHERE title='Monthly summary per category'", []).ok();
    for (name, kind) in [
        ("Recipe site", "project"),
        ("Photo archive", "project"),
        ("Budget tracker", "project"),
        ("Garden planner", "project"),
        ("Old backup script", "project"),
        ("Markdown", "topic"),
        ("HTML", "topic"),
        ("CSV", "topic"),
        ("Search", "topic"),
        ("Scanner", "tool"),
    ] {
        crate::kg::entities::add(c, name, kind, &[name.to_lowercase()]);
    }
    for (a, b, quote, src) in [
        (
            "Photo archive",
            "Budget tracker",
            "reusing the budget importer",
            "note:demo/photo archive notes.md",
        ),
        (
            "Garden planner",
            "Recipe site",
            "built on the recipe site's Markdown pipeline",
            "note:demo/garden ideas.md",
        ),
    ] {
        if let Err(e) = crate::kg::claims::relate(c, a, b, "built on", src, quote, "proposed") {
            eprintln!("demo relation rejected: {e}");
        }
    }
    println!(
        "demo store written to {}; run BACKTRAIL_HOME={} backtrail serve --port 8800",
        h.display(),
        h.display()
    );
}
