//! The `flume` binary: `fmt` and `check` on stdin, a file or a directory.

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

const UNFORMATTED: &str = "rill main()Sample{\nreturn 0}\n";
const FORMATTED: &str = "rill main() Sample {\r\n    return 0\r\n}\r\n";

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
         - rill main()Sample{\n  \
         - return 0}\n  \
         + rill main() Sample {\n  \
         +     return 0\n  \
         + }\n\
         flume: <stdin>: 2 place(s) not formatted\n"
    );
}

#[test]
fn syntax_errors_fail_both_commands_and_write_nothing() {
    for args in [&["fmt"][..], &["fmt", "--check"], &["check"]] {
        let out = run(args, "rill main() Sample {\n    return 0 0\n}\n");
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
fn a_file_path_is_formatted_in_place() {
    let dir = scratch("file_path");
    let file = dir.join("main.rill");
    std::fs::write(&file, UNFORMATTED).unwrap();
    let file = file.to_str().unwrap();

    let out = run(&["check", file], "");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(stderr(&out).starts_with(&format!("{file}:1: lines must end in CRLF\n")));

    for args in [&["fmt", file][..], &["fmt", "--check", file]] {
        std::fs::write(file, UNFORMATTED).unwrap();
        let out = run(args, "ignored");
        assert!(out.status.success(), "{args:?}: {}", stderr(&out));
        assert!(out.stdout.is_empty() && out.stderr.is_empty(), "{args:?}");
        assert_eq!(
            std::fs::read_to_string(file).unwrap(),
            FORMATTED,
            "{args:?}"
        );
    }

    let out = run(&["check", file], "");
    assert!(out.status.success(), "{}", stderr(&out));
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

    // Every file is formatted in place; the broken one is reported and left
    // alone, and the rest are still done.
    let out = run(&["fmt", d], "");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert_eq!(
        stderr(&out),
        format!(
            "flume: {}:1:6: expected a name after the keyword, found `(`\n",
            path("nested/broken.rill")
        )
    );
    let read = |p: &str| std::fs::read_to_string(dir.join(p)).unwrap();
    assert_eq!(read("b.rill"), FORMATTED);
    assert_eq!(read("nested/deeper/a.rill"), FORMATTED);
    assert_eq!(read("nested/broken.rill"), "rill (");
    assert_eq!(read(".hidden/skipped.rill"), "rill (");
    assert_eq!(read("notes.txt"), "rill (");
}

#[test]
fn missing_paths_fail() {
    let out = run(&["check", "/no/such/path.rill"], "");
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).starts_with("flume: cannot read /no/such/path.rill:"));
}

#[test]
fn include_and_exclude_pick_the_files() {
    let dir = scratch("globs");
    for p in [
        "main.rill",
        "src/dsp/osc.rill",
        "src/dsp/filter.rill",
        "src/ui.rill",
        "test/test_osc.rill",
        "test/notes.txt",
    ] {
        let p = dir.join(p);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, UNFORMATTED).unwrap();
    }
    let d = dir.to_str().unwrap();
    // `check` names every file it takes that is not formatted.
    let taken = |args: &[&str]| {
        let out = run(&[&["check", d], args].concat(), "");
        let report = stderr(&out);
        let mut files: Vec<String> = report
            .lines()
            .filter_map(|l| l.strip_prefix("flume: "))
            .filter_map(|l| l.strip_suffix(": 2 place(s) not formatted"))
            .map(|l| {
                l.strip_prefix(d)
                    .unwrap()
                    .trim_start_matches('/')
                    .to_owned()
            })
            .collect();
        files.sort();
        files
    };

    assert_eq!(
        taken(&[]),
        [
            "main.rill",
            "src/dsp/filter.rill",
            "src/dsp/osc.rill",
            "src/ui.rill",
            "test/test_osc.rill"
        ]
    );
    // A name matches at any depth, files or the directories they are in.
    assert_eq!(
        taken(&["--include", "dsp"]),
        ["src/dsp/filter.rill", "src/dsp/osc.rill"]
    );
    assert_eq!(
        taken(&["--include", "*osc*"]),
        ["src/dsp/osc.rill", "test/test_osc.rill"]
    );
    // With a `/`, the whole path below the directory.
    assert_eq!(taken(&["--include", "src/*.rill"]), ["src/ui.rill"]);
    assert_eq!(
        taken(&["--include", "src/**/*.rill", "--include", "main.rill"]),
        [
            "main.rill",
            "src/dsp/filter.rill",
            "src/dsp/osc.rill",
            "src/ui.rill"
        ]
    );
    // Only `.rill` files, whatever the globs say.
    assert_eq!(taken(&["--include", "test"]), ["test/test_osc.rill"]);
    // Exclude wins over include.
    assert_eq!(
        taken(&["--include", "src", "--exclude", "dsp"]),
        ["src/ui.rill"]
    );
    assert_eq!(
        taken(&["--exclude", "test", "--exclude", "src/dsp/f*"]),
        ["main.rill", "src/dsp/osc.rill", "src/ui.rill"]
    );

    // `fmt` writes only what it takes.
    let out = run(&["fmt", d, "--exclude", "src"], "");
    assert!(out.status.success(), "{}", stderr(&out));
    let read = |p: &str| std::fs::read_to_string(dir.join(p)).unwrap();
    assert_eq!(read("main.rill"), FORMATTED);
    assert_eq!(read("test/test_osc.rill"), FORMATTED);
    assert_eq!(read("src/ui.rill"), UNFORMATTED);

    // Nothing left to take is fine, but said.
    let out = run(&["check", d, "--include", "nothing"], "");
    assert!(out.status.success());
    assert!(stderr(&out).starts_with("flume: no .rill files to take in "));

    // A file named outright is matched by its path too.
    let ui = dir.join("src/ui.rill");
    let ui = ui.to_str().unwrap();
    let out = run(&["fmt", ui, "--exclude", "src"], "");
    assert!(out.status.success());
    assert_eq!(stderr(&out), format!("flume: {ui} is not included\n"));
    assert_eq!(read("src/ui.rill"), UNFORMATTED);
    let out = run(&["fmt", ui, "--include", "ui.rill"], "");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(read("src/ui.rill"), FORMATTED);
}

#[test]
fn globs_need_a_path_and_must_be_valid() {
    let out = run(&["fmt", "--include", "*.rill"], "");
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("<PATH>"), "{}", stderr(&out));

    let out = run(&["check", ".", "--exclude", "a[b"], "");
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).starts_with("flume: bad glob `a[b`:"),
        "{}",
        stderr(&out)
    );
}
