//! The `flume` binary: stdin in, stdout out, no options.

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

#[test]
fn formats_stdin_to_stdout_with_crlf() {
    let out = run(&[], "rill main()->Sample{\nreturn 0}\n");
    assert!(out.status.success());
    assert_eq!(
        out.stdout,
        b"rill main() -> Sample {\r\n    return 0\r\n}\r\n"
    );
    assert!(out.stderr.is_empty());
}

#[test]
fn rejects_any_argument() {
    for args in [&["--width", "100"][..], &["--help"], &["file.rill"]] {
        let out = run(args, "");
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(out.stdout.is_empty());
    }
}

#[test]
fn reports_syntax_errors_and_writes_nothing() {
    let out = run(&[], "rill main() -> Sample {\n    return 0 0\n}\n");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert_eq!(
        String::from_utf8(out.stderr).unwrap(),
        "flume: <stdin>:2:14: expected a line break or `;` after the statement, found `0`\n"
    );
}
