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
