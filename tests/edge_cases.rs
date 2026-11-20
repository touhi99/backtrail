use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static N: AtomicUsize = AtomicUsize::new(0);

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "backtrail-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("home")).expect("create test home");
        Sandbox { root }
    }
    fn home(&self) -> PathBuf {
        self.root.join("home")
    }
    fn settings(&self) -> PathBuf {
        self.home().join(".claude/settings.json")
    }
    fn write_settings(&self, text: &str) {
        fs::create_dir_all(self.home().join(".claude")).expect("create .claude dir");
        fs::write(self.settings(), text).expect("write settings.json");
    }
    fn settings_json(&self) -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(self.settings()).expect("read settings.json"))
            .expect("settings.json is JSON")
    }
    fn run(&self, data_dir: Option<&Path>, args: &[&str]) -> Output {
        let mut c = Command::new(env!("CARGO_BIN_EXE_backtrail"));
        c.args(args)
            .env_clear()
            .env("HOME", self.home())
            .env("PATH", "/usr/bin:/bin");
        if let Some(d) = data_dir {
            c.env("BACKTRAIL_HOME", d);
        }
        c.output().expect("run the backtrail binary")
    }
    fn session(&self, folder: &str, id: &str, title: &str) {
        let dir = self.home().join(".claude/projects").join(folder);
        fs::create_dir_all(&dir).expect("create session dir");
        let mut lines = vec![format!(
            r#"{{"type":"ai-title","aiTitle":{}}}"#,
            serde_json::to_string(title).expect("a string serializes")
        )];
        for i in 0..5 {
            let role = if i % 2 == 0 { "user" } else { "assistant" };
            lines.push(format!(
                r#"{{"type":"{role}","timestamp":"2026-11-13T01:2{i}:00Z","message":{{"content":"turn number {i} of the edge case session {id}"}}}}"#
            ));
        }
        fs::write(dir.join(format!("{id}.jsonl")), lines.join("\n")).expect("write session file");
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn out(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn session_thread(_s: &Sandbox, data: &Path, id8: &str) -> String {
    let c = rusqlite::Connection::open(data.join("backtrail.db")).expect("open test store");
    c.query_row(
        "SELECT thread FROM sources WHERE ref=?1",
        [format!("session:{id8}")],
        |r| r.get::<_, String>(0),
    )
    .unwrap_or_else(|e| format!("<no row: {e}>"))
}

#[test]
fn data_dir_follows_backtrail_home() {
    let s = Sandbox::new();
    let data = s.root.join("custom-data");
    let o = s.run(Some(&data), &["check"]);
    assert!(o.status.success(), "{}", out(&o));
    assert!(data.join("backtrail.db").exists());
    assert!(!s.home().join("Library").exists());
    assert!(!s.home().join(".local").exists());
}

#[test]
fn data_dir_default_lives_under_home() {
    let s = Sandbox::new();
    let o = s.run(None, &["check"]);
    assert!(o.status.success(), "{}", out(&o));
    let expected = if cfg!(target_os = "macos") {
        s.home().join("Library/Application Support/Backtrail")
    } else {
        s.home().join(".local/share/backtrail")
    };
    assert!(expected.join("backtrail.db").exists(), "{expected:?}");
}

#[test]
fn data_dir_is_created_with_missing_parents() {
    let s = Sandbox::new();
    let data = s.root.join("a/b/c/d");
    let o = s.run(Some(&data), &["check"]);
    assert!(o.status.success(), "{}", out(&o));
    assert!(data.join("backtrail.db").exists());
}

#[test]
fn data_dir_with_spaces_and_unicode() {
    let s = Sandbox::new();
    let data = s.root.join("Daten ü ordner");
    let o = s.run(Some(&data), &["check"]);
    assert!(o.status.success(), "{}", out(&o));
    assert!(data.join("backtrail.db").exists());
}

#[test]
fn empty_store_commands_do_not_crash() {
    let s = Sandbox::new();
    let data = s.root.join("data");
    for cmd in [
        vec!["ingest"],
        vec!["check"],
        vec!["brief"],
        vec!["pending"],
        vec!["log"],
    ] {
        let o = s.run(Some(&data), &cmd);
        assert!(o.status.success(), "{cmd:?}: {}", out(&o));
    }
}

#[test]
fn unknown_command_prints_usage() {
    let s = Sandbox::new();
    let o = s.run(Some(&s.root.join("data")), &["no-such-command"]);
    assert!(out(&o).contains("bt help all"), "{}", out(&o));
}

#[test]
fn missing_config_uses_defaults() {
    let s = Sandbox::new();
    let data = s.root.join("data");
    s.session("-p-alpha", "aaaa1111", "Alpha work");
    let o = s.run(Some(&data), &["ingest"]);
    assert!(o.status.success(), "{}", out(&o));
    assert!(out(&o).contains("sessions 1 new"), "{}", out(&o));
}

#[test]
fn invalid_config_json_falls_back_to_defaults() {
    let s = Sandbox::new();
    let data = s.root.join("data");
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join("config.json"), "{ this is not json").unwrap();
    s.session("-p-alpha", "aaaa1111", "Alpha work");
    let o = s.run(Some(&data), &["ingest"]);
    assert!(o.status.success(), "{}", out(&o));
    assert!(out(&o).contains("sessions 1 new"), "{}", out(&o));
}

#[test]
fn empty_config_object_is_accepted() {
    let s = Sandbox::new();
    let data = s.root.join("data");
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join("config.json"), "{}").unwrap();
    let o = s.run(Some(&data), &["ingest"]);
    assert!(o.status.success(), "{}", out(&o));
}

#[test]
fn config_with_wrong_types_does_not_crash() {
    let s = Sandbox::new();
    let data = s.root.join("data");
    fs::create_dir_all(&data).unwrap();
    fs::write(
        data.join("config.json"),
        r#"{"workspace": 5, "note_dirs": "x", "threads": {"a": 1}, "port": "no", "embeddings": 3, "aliases": []}"#,
    )
    .unwrap();
    let o = s.run(Some(&data), &["ingest"]);
    assert!(o.status.success(), "{}", out(&o));
}

#[test]
fn legacy_threads_json_is_used_when_config_is_absent() {
    let s = Sandbox::new();
    let data = s.root.join("data");
    fs::create_dir_all(&data).unwrap();
    fs::write(
        data.join("threads.json"),
        r#"[{"area":"Work","name":"Legacy thread","regex":"legacy"}]"#,
    )
    .unwrap();
    s.session("-p-legacy-app", "bbbb2222", "Legacy app work");
    let o = s.run(Some(&data), &["ingest"]);
    assert!(o.status.success(), "{}", out(&o));
    assert!(session_thread(&s, &data, "bbbb2222").contains("Legacy thread"));
}

#[test]
fn config_takes_priority_over_threads_json() {
    let s = Sandbox::new();
    let data = s.root.join("data");
    fs::create_dir_all(&data).unwrap();
    fs::write(
        data.join("threads.json"),
        r#"[{"area":"Work","name":"From threads file","regex":"zeta"}]"#,
    )
    .unwrap();
    fs::write(
        data.join("config.json"),
        r#"{"threads":[{"area":"Work","name":"From config","regex":"zeta"}]}"#,
    )
    .unwrap();
    s.session("-p-zeta", "cccc3333", "Zeta work");
    s.run(Some(&data), &["ingest"]);
    let shown = session_thread(&s, &data, "cccc3333");
    assert!(shown.contains("From config"), "{shown}");
    assert!(!shown.contains("From threads file"), "{shown}");
}

#[test]
fn each_project_folder_becomes_its_own_thread() {
    let s = Sandbox::new();
    let data = s.root.join("data");
    s.session("-p-alpha", "aaaa1111", "Alpha work");
    s.session("-p-beta", "bbbb2222", "Beta work");
    let o = s.run(Some(&data), &["ingest"]);
    assert!(out(&o).contains("sessions 2 new"), "{}", out(&o));
    let a = session_thread(&s, &data, "aaaa1111");
    let b = session_thread(&s, &data, "bbbb2222");
    assert!(a.contains("p-alpha"), "{a}");
    assert!(b.contains("p-beta"), "{b}");
}

#[test]
fn desktop_prefix_is_stripped_from_the_folder_name() {
    let s = Sandbox::new();
    let data = s.root.join("data");
    let slug: String = s
        .home()
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    s.session(
        &format!("{slug}-Desktop-myproject"),
        "dddd4444",
        "Some work",
    );
    s.run(Some(&data), &["ingest"]);
    let shown = session_thread(&s, &data, "dddd4444");
    assert!(shown.contains("myproject"), "{shown}");
    assert!(!shown.contains("Desktop-myproject"), "{shown}");
}

#[test]
fn sessions_in_the_home_folder_are_not_a_thread() {
    let s = Sandbox::new();
    let data = s.root.join("data");
    let slug: String = s
        .home()
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    s.session(&slug, "eeee5555", "General chat");
    s.run(Some(&data), &["ingest"]);
    let shown = session_thread(&s, &data, "eeee5555");
    assert!(shown == "Unsorted", "{shown}");
}

#[test]
fn tmp_folders_are_skipped() {
    let s = Sandbox::new();
    let data = s.root.join("data");
    s.session("-private-tmp-scratch", "ffff6666", "Scratch work");
    let o = s.run(Some(&data), &["ingest"]);
    assert!(out(&o).contains("sessions 0 new"), "{}", out(&o));
}

#[test]
fn short_untitled_session_goes_to_the_catch_all() {
    let s = Sandbox::new();
    let data = s.root.join("data");
    let dir = s.home().join(".claude/projects/-p-short");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("99990000.jsonl"),
        r#"{"type":"user","timestamp":"2026-11-13T01:20:00Z","message":{"content":"a single question that is long enough"}}"#,
    )
    .unwrap();
    s.run(Some(&data), &["ingest"]);
    let shown = session_thread(&s, &data, "99990000");
    assert!(shown == "Unsorted", "{shown}");
}

#[test]
fn session_file_with_garbage_lines_is_still_indexed() {
    let s = Sandbox::new();
    let data = s.root.join("data");
    s.session("-p-mixed", "12121212", "Mixed file");
    let f = s.home().join(".claude/projects/-p-mixed/12121212.jsonl");
    let mut text = fs::read_to_string(&f).unwrap();
    text.push_str("\nnot json at all\n{\"type\":\n\n");
    fs::write(&f, text).unwrap();
    let o = s.run(Some(&data), &["ingest"]);
    assert!(o.status.success(), "{}", out(&o));
    assert!(out(&o).contains("sessions 1 new"), "{}", out(&o));
}

#[test]
fn duplicate_session_id_in_two_folders_is_indexed_once() {
    let s = Sandbox::new();
    let data = s.root.join("data");
    s.session("-p-old-name", "34343434", "Renamed project");
    s.session("-p-new-name", "34343434", "Renamed project");
    let o = s.run(Some(&data), &["ingest"]);
    assert!(out(&o).contains("sessions 1 new"), "{}", out(&o));
}

#[test]
fn second_ingest_leaves_unchanged_sessions_alone() {
    let s = Sandbox::new();
    let data = s.root.join("data");
    s.session("-p-alpha", "aaaa1111", "Alpha work");
    s.run(Some(&data), &["ingest"]);
    let o = s.run(Some(&data), &["ingest"]);
    assert!(
        out(&o).contains("sessions 0 new/changed (1 unchanged)"),
        "{}",
        out(&o)
    );
}

fn commands_of(v: &serde_json::Value, event: &str) -> Vec<String> {
    v["hooks"][event]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|g| g["hooks"].as_array().into_iter().flatten())
        .filter_map(|h| h["command"].as_str().map(String::from))
        .collect()
}

#[test]
fn hooks_install_creates_settings_when_missing() {
    let s = Sandbox::new();
    let o = s.run(None, &["hooks", "install"]);
    assert!(o.status.success(), "{}", out(&o));
    let v = s.settings_json();
    assert_eq!(commands_of(&v, "SessionStart").len(), 1);
    assert!(commands_of(&v, "SessionStart")[0].ends_with(" brief"));
    assert!(commands_of(&v, "SessionEnd")[0].ends_with("session-end.sh"));
    assert!(
        !s.home()
            .join(".claude/settings.json.bak-backtrail")
            .exists()
    );
}

#[test]
fn hooks_install_twice_adds_nothing_new() {
    let s = Sandbox::new();
    s.run(None, &["hooks", "install"]);
    let first = fs::read_to_string(s.settings()).unwrap();
    let o = s.run(None, &["hooks", "install"]);
    assert!(out(&o).contains("already installed"), "{}", out(&o));
    assert_eq!(first, fs::read_to_string(s.settings()).unwrap());
}

#[test]
fn hooks_install_keeps_other_settings_and_hooks() {
    let s = Sandbox::new();
    s.write_settings(
        r#"{"theme":"dark","hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"echo hi"}]}],"Stop":[{"hooks":[{"type":"command","command":"echo stop"}]}]}}"#,
    );
    s.run(None, &["hooks", "install"]);
    let v = s.settings_json();
    assert_eq!(v["theme"], "dark");
    assert_eq!(commands_of(&v, "Stop"), vec!["echo stop"]);
    let start = commands_of(&v, "SessionStart");
    assert_eq!(start.len(), 2);
    assert_eq!(start[0], "echo hi");
}

#[test]
fn hooks_install_writes_a_backup_of_the_old_file() {
    let s = Sandbox::new();
    let original = r#"{"theme":"dark"}"#;
    s.write_settings(original);
    s.run(None, &["hooks", "install"]);
    let bak = s.home().join(".claude/settings.json.bak-backtrail");
    assert_eq!(fs::read_to_string(bak).unwrap(), original);
}

#[test]
fn hooks_install_refuses_invalid_json_and_changes_nothing() {
    let s = Sandbox::new();
    s.write_settings("{ broken");
    let o = s.run(None, &["hooks", "install"]);
    assert!(!o.status.success());
    assert!(out(&o).contains("not valid JSON"), "{}", out(&o));
    assert_eq!(fs::read_to_string(s.settings()).unwrap(), "{ broken");
    assert!(
        !s.home()
            .join(".claude/settings.json.bak-backtrail")
            .exists()
    );
}

#[test]
fn hooks_recognise_a_hand_written_bt_brief() {
    let s = Sandbox::new();
    s.write_settings(
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"bt brief"}]}]}}"#,
    );
    let o = s.run(None, &["hooks", "install"]);
    assert!(
        out(&o).contains("SessionStart: already installed"),
        "{}",
        out(&o)
    );
    assert_eq!(
        commands_of(&s.settings_json(), "SessionStart"),
        vec!["bt brief"]
    );
}

#[test]
fn hooks_remove_deletes_only_our_hooks() {
    let s = Sandbox::new();
    s.write_settings(
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"echo hi"}]}]}}"#,
    );
    s.run(None, &["hooks", "install"]);
    let o = s.run(None, &["hooks", "remove"]);
    assert!(o.status.success(), "{}", out(&o));
    let v = s.settings_json();
    assert_eq!(commands_of(&v, "SessionStart"), vec!["echo hi"]);
    assert_eq!(
        commands_of(&v, "SessionEnd"),
        [] as [std::string::String; 0]
    );
}

#[test]
fn hooks_remove_drops_groups_left_empty() {
    let s = Sandbox::new();
    s.run(None, &["hooks", "install"]);
    s.run(None, &["hooks", "remove"]);
    let v = s.settings_json();
    assert_eq!(
        v["hooks"]["SessionStart"]
            .as_array()
            .map(std::vec::Vec::len),
        Some(0)
    );
    assert_eq!(
        v["hooks"]["SessionEnd"].as_array().map(std::vec::Vec::len),
        Some(0)
    );
}

#[test]
fn hooks_remove_without_a_settings_file_does_not_crash() {
    let s = Sandbox::new();
    let o = s.run(None, &["hooks", "remove"]);
    assert!(o.status.success(), "{}", out(&o));
}

#[test]
fn hooks_remove_also_removes_a_hand_written_bt_brief() {
    let s = Sandbox::new();
    s.write_settings(
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"bt brief"}]}]}}"#,
    );
    s.run(None, &["hooks", "remove"]);
    assert_eq!(
        commands_of(&s.settings_json(), "SessionStart"),
        [] as [std::string::String; 0]
    );
}

#[test]
fn hooks_status_reports_without_writing() {
    let s = Sandbox::new();
    let o = s.run(None, &["hooks"]);
    assert!(
        out(&o).contains("SessionStart: not installed"),
        "{}",
        out(&o)
    );
    assert!(!s.settings().exists());
    s.run(None, &["hooks", "install"]);
    let o = s.run(None, &["hooks", "status"]);
    assert!(out(&o).contains("SessionEnd: installed"), "{}", out(&o));
}

#[test]
fn hooks_unknown_action_is_rejected_and_changes_nothing() {
    let s = Sandbox::new();
    s.write_settings(r#"{"theme":"dark"}"#);
    let o = s.run(None, &["hooks", "frobnicate"]);
    assert!(!o.status.success());
    assert!(out(&o).contains("usage"), "{}", out(&o));
    assert_eq!(
        fs::read_to_string(s.settings()).unwrap(),
        r#"{"theme":"dark"}"#
    );
}
