//! `flume fmt [PATH]` and `flume check [PATH]`. Without a path they read
//! stdin.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// The Rill formatter. Reads a file, a directory of them, or stdin.
///
/// There is one way to format Rill, so nothing about the layout can be
/// configured: 80 columns, 4 spaces, CRLF.
#[derive(Parser)]
#[command(version, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Format and write the result to stdout. Files are never changed.
    Fmt {
        /// A file, or a directory to search for `.rill` files. Each file
        /// found is written under a `==> path <==` line. Reads stdin when
        /// left out.
        path: Option<PathBuf>,
        /// Before writing, make sure only the layout changed: the syntax tree
        /// must be the same and no comment may be lost. Writes nothing for a
        /// file where that fails.
        #[arg(long)]
        check: bool,
    },
    /// Show what `fmt` would change, without writing anything to stdout.
    ///
    /// Exits with 1 if anything would change.
    Check {
        /// A file, or a directory to search for `.rill` files. Reads stdin
        /// when left out.
        path: Option<PathBuf>,
    },
}

/// One file to work on.
struct Input {
    /// What it is called in messages.
    name: String,
    /// `None` for stdin.
    path: Option<PathBuf>,
}

impl Input {
    fn read(&self) -> Result<String, String> {
        let mut src = String::new();
        match &self.path {
            None => std::io::stdin()
                .read_to_string(&mut src)
                .map(|_| src)
                .map_err(|e| format!("cannot read stdin: {e}")),
            Some(path) => {
                std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", self.name))
            }
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let path = match &cli.command {
        Command::Fmt { path, .. } | Command::Check { path } => path,
    };

    let (inputs, many) = match path {
        None => (
            vec![Input {
                name: "<stdin>".to_owned(),
                path: None,
            }],
            false,
        ),
        Some(dir) if dir.is_dir() => {
            let mut files = Vec::new();
            if let Err(e) = find_rill_files(dir, &mut files) {
                eprintln!("flume: {e}");
                return ExitCode::FAILURE;
            }
            if files.is_empty() {
                eprintln!("flume: no .rill files in {}", dir.display());
            }
            let inputs = files
                .into_iter()
                .map(|path| Input {
                    name: path.display().to_string(),
                    path: Some(path),
                })
                .collect();
            (inputs, true)
        }
        Some(file) => (
            vec![Input {
                name: file.display().to_string(),
                path: Some(file.clone()),
            }],
            false,
        ),
    };

    let mut ok = true;
    for (i, input) in inputs.iter().enumerate() {
        ok &= match input.read() {
            Ok(src) => match &cli.command {
                Command::Fmt { check, .. } => fmt(input, &src, many.then_some(i), *check),
                Command::Check { .. } => check(input, &src),
            },
            Err(e) => {
                eprintln!("flume: {e}");
                false
            }
        };
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Every `.rill` file under `dir`, sorted. Hidden directories, like `.git`,
/// and links to directories are skipped.
fn find_rill_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        paths.push((entry.path(), entry.file_type().ok()));
    }
    paths.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, kind) in paths {
        let hidden = path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'));
        if kind.is_some_and(|k| k.is_dir()) {
            if !hidden {
                find_rill_files(&path, files)?;
            }
        } else if path.extension().is_some_and(|e| e == "rill") && path.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

/// Format one file to stdout. When it is one of several, at `place`, a
/// `==> name <==` line comes first, after a blank line unless it is the
/// first. Returns whether it worked.
fn fmt(input: &Input, src: &str, place: Option<usize>, check: bool) -> bool {
    let name = &input.name;
    let out = match flume::format(src) {
        Ok(out) => out,
        Err(e) => {
            eprintln!("flume: {name}:{e}");
            return false;
        }
    };
    if check && let Err(e) = flume::verify(src, &out) {
        eprintln!("flume: {name}: formatting would change more than the layout: {e}");
        return false;
    }
    let mut stdout = std::io::stdout().lock();
    let header = match place {
        None => String::new(),
        Some(0) => format!("==> {name} <==\r\n"),
        Some(_) => format!("\r\n==> {name} <==\r\n"),
    };
    if let Err(e) = stdout
        .write_all(header.as_bytes())
        .and_then(|()| stdout.write_all(out.as_bytes()))
        .and_then(|()| stdout.flush())
    {
        eprintln!("flume: cannot write stdout: {e}");
        return false;
    }
    true
}

/// Report what formatting would change in one file. Returns whether nothing
/// would.
fn check(input: &Input, src: &str) -> bool {
    let name = &input.name;
    match flume::check(src) {
        Ok(issues) if issues.is_empty() => true,
        Ok(issues) => {
            for issue in &issues {
                eprintln!("{name}:{issue}");
            }
            eprintln!("flume: {name}: {} place(s) not formatted", issues.len());
            false
        }
        Err(e) => {
            eprintln!("flume: {name}:{e}");
            false
        }
    }
}
