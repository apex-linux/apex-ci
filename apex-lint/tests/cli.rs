#![allow(
    clippy::expect_used,
    reason = "test helpers panic on a broken scratch directory"
)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("apex-lint-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create the scratch directory");
        Self(dir)
    }

    fn file(&self, name: &str, lines: &[&str]) -> String {
        let path = self.0.join(name);
        fs::write(&path, lines.join("\n") + "\n").expect("write a scratch file");
        path.to_str().expect("UTF-8 scratch path").to_owned()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn apex_lint(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_apex-lint"))
        .args(args)
        .output()
        .expect("apex-lint runs");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

const THREE_LINE_BLOCK: [&str; 6] = [
    "#!/bin/sh",
    "# SPDX-License-Identifier: GPL-3.0-or-later",
    "# one",
    "# two",
    "# three",
    "echo done",
];

#[test]
fn comments_prints_path_line_rule_message_and_exits_1() {
    let scratch = Scratch::new("text");
    let script = scratch.file("run.sh", &THREE_LINE_BLOCK);
    let (code, stdout, stderr) = apex_lint(&["comments", &script]);
    assert_eq!(code, 1, "{stderr}");
    assert!(
        stdout.starts_with(&format!(
            "{script}:3: block: 3 comment lines with no code between them"
        )),
        "{stdout}"
    );
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    assert!(stderr.contains("docs/CONTRIBUTING.md"), "{stderr}");
}

#[test]
fn comments_json_lists_the_same_violation() {
    let scratch = Scratch::new("json");
    let script = scratch.file("run.sh", &THREE_LINE_BLOCK);
    let (code, stdout, _) = apex_lint(&["comments", "--json", &script]);
    assert_eq!(code, 1);
    assert!(stdout.starts_with("[\n  {\"path\": "), "{stdout}");
    assert!(
        stdout.contains("\"line\": 3, \"rule\": \"block\""),
        "{stdout}"
    );
}

#[test]
fn clean_and_out_of_scope_files_exit_0() {
    let scratch = Scratch::new("clean");
    let script = scratch.file("run.sh", &["#!/bin/sh", "# why", "echo done"]);
    let prose = scratch.file("NOTES.md", &["# one", "# two", "# three"]);
    let lock = scratch.file("Cargo.lock", &["# one", "# two", "# three"]);
    assert_eq!(
        apex_lint(&["comments", &script, &prose, &lock]),
        (0, String::new(), String::new())
    );
}

#[test]
fn strip_prints_the_file_without_its_comment_lines() {
    let scratch = Scratch::new("strip");
    let script = scratch.file("run.sh", &THREE_LINE_BLOCK);
    let (code, stdout, stderr) = apex_lint(&["strip", &script]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        stdout,
        "#!/bin/sh\n# SPDX-License-Identifier: GPL-3.0-or-later\necho done\n"
    );
}

#[test]
fn strip_refuses_a_file_without_a_known_comment_syntax() {
    let scratch = Scratch::new("unknown");
    let license = scratch.file("LICENSE", &["GNU GENERAL PUBLIC LICENSE"]);
    let (code, _, stderr) = apex_lint(&["strip", &license]);
    assert_eq!(code, 2);
    assert!(stderr.contains("no known comment syntax"), "{stderr}");
}

#[test]
fn a_missing_command_or_option_is_a_usage_error() {
    let (code, _, stderr) = apex_lint(&[]);
    assert_eq!(code, 2);
    assert!(stderr.contains("Usage: apex-lint comments"), "{stderr}");
    assert_eq!(apex_lint(&["comments", "--fix"]).0, 2);
    let (code, stdout, _) = apex_lint(&["--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("apex-lint strip FILE..."), "{stdout}");
}
