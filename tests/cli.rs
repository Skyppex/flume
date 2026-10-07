//! The `flume` binary: `fmt` and `check` read stdin; nothing else is read.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_flume"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).unwrap()
}

const UNFORMATTED: &str = "rill main()->Sample{\nreturn 0}\n";
const FORMATTED: &str = "rill main() -> Sample {\r\n    return 0\r\n}\r\n";

#[test]
fn fmt_formats_stdin_to_stdout_with_crlf() {
    for args in [&["fmt"][..], &["fmt", "--check"]] {
        let out = run(args, UNFORMATTED);
        assert!(out.status.success(), "{args:?}: {}", stderr(&out));
        assert_eq!(out.stdout, FORMATTED.as_bytes(), "{args:?}");
        assert!(out.stderr.is_empty(), "{args:?}");
    }
}

#[test]
fn check_reports_without_writing_stdout() {
    let out = run(&["check"], FORMATTED);
    assert!(out.status.success());
    assert!(out.stdout.is_empty() && out.stderr.is_empty());

    let out = run(&["check"], UNFORMATTED);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert_eq!(
        stderr(&out),
        "<stdin>:1: lines must end in CRLF\n\
         <stdin>:1: not formatted\n  \
         - rill main()->Sample{\n  \
         - return 0}\n  \
         + rill main() -> Sample {\n  \
         +     return 0\n  \
         + }\n\
         flume: <stdin>: 2 place(s) not formatted\n"
    );
}

#[test]
fn syntax_errors_fail_both_commands_and_write_nothing() {
    for args in [&["fmt"][..], &["fmt", "--check"], &["check"]] {
        let out = run(args, "rill main() -> Sample {\n    return 0 0\n}\n");
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert!(out.stdout.is_empty());
        assert_eq!(
            stderr(&out),
            "flume: <stdin>:2:14: expected a line break or `;` after the statement, found `0`\n"
        );
    }
}

#[test]
fn help_and_usage() {
    for args in [
        &["-h"][..],
        &["--help"],
        &["fmt", "-h"],
        &["check", "--help"],
    ] {
        let out = run(args, "");
        assert!(out.status.success(), "{args:?}");
        assert!(String::from_utf8_lossy(&out.stdout).contains("Usage: flume"));
    }
    // No command shows the help, as an error.
    let out = run(&[], "");
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("Usage: flume <COMMAND>"));
    // Nothing about the layout can be configured.
    let out = run(&["fmt", "--width", "100"], "");
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
}

/// A fresh directory for one test, under cargo's temporary directory.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_file_path_reads_like_stdin() {
    let dir = scratch("file_path");
    let file = dir.join("main.rill");
    std::fs::write(&file, UNFORMATTED).unwrap();
    let file = file.to_str().unwrap();

    let out = run(&["fmt", file], "ignored");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(out.stdout, FORMATTED.as_bytes());
    // The file itself is left alone.
    assert_eq!(std::fs::read_to_string(file).unwrap(), UNFORMATTED);

    let out = run(&["check", file], "");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(stderr(&out).starts_with(&format!("{file}:1: lines must end in CRLF\n")));
}

#[test]
fn a_directory_covers_every_rill_file_in_it() {
    let dir = scratch("directory");
    std::fs::create_dir_all(dir.join("nested/deeper")).unwrap();
    std::fs::create_dir_all(dir.join(".hidden")).unwrap();
    std::fs::write(dir.join("b.rill"), FORMATTED).unwrap();
    std::fs::write(dir.join("nested/deeper/a.rill"), UNFORMATTED).unwrap();
    std::fs::write(dir.join("nested/broken.rill"), "rill (").unwrap();
    std::fs::write(dir.join(".hidden/skipped.rill"), "rill (").unwrap();
    std::fs::write(dir.join("notes.txt"), "rill (").unwrap();
    let d = dir.to_str().unwrap();
    let path = |p: &str| dir.join(p).display().to_string();

    // Every file is formatted under a header, in path order; the broken one
    // is reported and the rest still come out.
    let out = run(&["fmt", d], "");
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(out.stdout.clone()).unwrap(),
        format!(
            "==> {} <==\r\n{FORMATTED}\r\n==> {} <==\r\n{FORMATTED}",
            path("b.rill"),
            path("nested/deeper/a.rill"),
        )
    );
    assert_eq!(
        stderr(&out),
        format!(
            "flume: {}:1:6: expected a name after the keyword, found `(`\n",
            path("nested/broken.rill")
        )
    );

    let out = run(&["check", d], "");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    let report = stderr(&out);
    assert!(!report.contains("b.rill"), "{report}");
    assert!(report.contains(&format!(
        "flume: {}: 2 place(s)",
        path("nested/deeper/a.rill")
    )));
    assert!(report.contains(&path("nested/broken.rill")));
    assert!(!report.contains("skipped") && !report.contains("notes"));
}

#[test]
fn missing_paths_fail() {
    let out = run(&["check", "/no/such/path.rill"], "");
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).starts_with("flume: cannot read /no/such/path.rill:"));
}
