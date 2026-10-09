//! Golden tests over `tests/cases`, plus properties every formatted file must
//! have, checked against the compiler's own parser.
//!
//! Each `NAME.rill` in `tests/cases` is formatted and compared with
//! `NAME.out`. The `.out` files use LF so they read well in a diff; the
//! formatter's CRLF is turned into LF before comparing. Set `BLESS=1` to
//! write the current output to the `.out` files.

mod common;

use common::{check_properties, lf};
use std::path::{Path, PathBuf};

fn cases_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases")
}

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples")
}

fn rill_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "rill"))
        .collect();
    files.sort();
    files
}

#[test]
fn golden_cases() {
    let bless = std::env::var_os("BLESS").is_some();
    let mut failures = Vec::new();
    for path in rill_files(&cases_dir()) {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let src = std::fs::read_to_string(&path).unwrap();
        let out = lf(&check_properties(&name, &src));
        let expected_path = path.with_extension("out");
        if bless {
            std::fs::write(&expected_path, &out).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&expected_path)
            .unwrap_or_else(|_| panic!("{name}: no .out file; run with BLESS=1"));
        if out != expected {
            failures.push(format!(
                "--- {name}: expected\n{expected}\n--- {name}: got\n{out}"
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn examples_keep_their_meaning() {
    let mut paths = rill_files(&examples_dir());
    paths.extend(rill_files(&examples_dir().join("modules")));
    for path in paths {
        let src = std::fs::read_to_string(&path).unwrap();
        check_properties(&path.display().to_string(), &src);
    }
}

/// Every golden case squeezed into one line per statement and padded with
/// odd spacing still formats to the same thing.
#[test]
fn layout_of_the_input_does_not_matter() {
    for path in rill_files(&cases_dir()) {
        let src = std::fs::read_to_string(&path).unwrap();
        let out = flume::format(&src).unwrap();
        let crlf = flume::format(&src.replace('\n', "\r\n")).unwrap();
        assert_eq!(out, crlf, "{}: CRLF input", path.display());
        let tabs = flume::format(&src.replace("    ", "\t")).unwrap();
        assert_eq!(out, tabs, "{}: tab indentation", path.display());
    }
}
