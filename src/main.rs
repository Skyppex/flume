//! `flume fmt [PATH]` and `flume check [PATH]`. Without a path they read
//! stdin, and `fmt` writes to stdout; with one, `fmt` rewrites the files.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use globset::{GlobSet, GlobSetBuilder};

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
    /// Format files in place, or stdin to stdout.
    Fmt {
        #[command(flatten)]
        files: Files,
        /// Before writing, make sure only the layout changed: the syntax tree
        /// must be the same and no comment may be lost. Leaves a file alone
        /// where that fails.
        #[arg(long)]
        check: bool,
    },
    /// Show what `fmt` would change, without writing anything to stdout.
    ///
    /// Exits with 1 if anything would change.
    Check {
        #[command(flatten)]
        files: Files,
    },
}

/// Which files to work on.
#[derive(Args)]
struct Files {
    /// A file, or a directory to search for `.rill` files. `fmt` formats them
    /// in place. Reads stdin, and `fmt` writes stdout, when left out.
    path: Option<PathBuf>,
    /// Only take the `.rill` files matching this glob, or inside a directory
    /// matching it. Can be given more than once.
    ///
    /// Globs match the path below the directory searched, with `/` between
    /// parts. A glob without a `/` matches a file or directory name at any
    /// depth: `--include 'test_*'`, `--include src/dsp`.
    #[arg(long, value_name = "GLOB", requires = "path")]
    include: Vec<String>,
    /// Leave out the files matching this glob, and everything inside a
    /// directory matching it, even when included. Can be given more than
    /// once. Matches like `--include`.
    #[arg(long, value_name = "GLOB", requires = "path")]
    exclude: Vec<String>,
}

/// The `--include` and `--exclude` globs.
struct Filter {
    /// `None` takes every `.rill` file.
    include: Option<GlobSet>,
    /// Checked against every directory on the way down too.
    exclude: GlobSet,
}

impl Filter {
    fn new(files: &Files) -> Result<Filter, String> {
        let include = if files.include.is_empty() {
            None
        } else {
            Some(globs(&files.include)?)
        };
        Ok(Filter {
            include,
            exclude: globs(&files.exclude)?,
        })
    }

    /// Whether to look inside the directory at `rel`.
    fn enter(&self, rel: &Path) -> bool {
        !self.exclude.is_match(rel)
    }

    /// Whether to take the file at `rel`.
    fn take(&self, rel: &Path) -> bool {
        let mut within = rel.ancestors().filter(|p| !p.as_os_str().is_empty());
        rel.extension().is_some_and(|e| e == "rill")
            && self
                .include
                .as_ref()
                .is_none_or(|include| within.clone().any(|p| include.is_match(p)))
            && !within.any(|p| self.exclude.is_match(p))
    }
}

fn globs(patterns: &[String]) -> Result<GlobSet, String> {
    let mut set = GlobSetBuilder::new();
    for pattern in patterns {
        let trimmed = pattern.trim_start_matches("./").trim_end_matches('/');
        // Like `.gitignore`: without a `/`, a name at any depth.
        let full = if trimmed.contains('/') {
            trimmed.to_owned()
        } else {
            format!("**/{trimmed}")
        };
        let glob = globset::GlobBuilder::new(&full)
            .literal_separator(true)
            .build()
            .map_err(|e| format!("bad glob `{pattern}`: {e}"))?;
        set.add(glob);
    }
    set.build().map_err(|e| e.to_string())
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
    let files = match &cli.command {
        Command::Fmt { files, .. } | Command::Check { files } => files,
    };
    let filter = match Filter::new(files) {
        Ok(filter) => filter,
        Err(e) => {
            eprintln!("flume: {e}");
            return ExitCode::from(2);
        }
    };

    let inputs = match &files.path {
        None => vec![Input {
            name: "<stdin>".to_owned(),
            path: None,
        }],
        Some(dir) if dir.is_dir() => {
            let mut files = Vec::new();
            if let Err(e) = find_files(dir, Path::new(""), &filter, &mut files) {
                eprintln!("flume: {e}");
                return ExitCode::FAILURE;
            }
            if files.is_empty() {
                eprintln!("flume: no .rill files to take in {}", dir.display());
            }
            files
                .into_iter()
                .map(|path| Input {
                    name: path.display().to_string(),
                    path: Some(path),
                })
                .collect()
        }
        // A file named outright is taken whatever its name, unless globs are
        // given; then it is matched by its path as written.
        Some(file) if !files.include.is_empty() || !files.exclude.is_empty() => {
            let rel = file.strip_prefix(".").unwrap_or(file);
            if !filter.take(rel) {
                eprintln!("flume: {} is not included", file.display());
                return ExitCode::SUCCESS;
            }
            vec![Input {
                name: file.display().to_string(),
                path: Some(file.clone()),
            }]
        }
        Some(file) => vec![Input {
            name: file.display().to_string(),
            path: Some(file.clone()),
        }],
    };

    let mut ok = true;
    for input in &inputs {
        ok &= match input.read() {
            Ok(src) => match &cli.command {
                Command::Fmt { check, .. } => fmt(input, &src, *check),
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

/// Every file under `dir` that `filter` takes, sorted. `rel` is where `dir`
/// is below the directory searched. Hidden directories, like `.git`, and
/// links to directories are skipped.
fn find_files(
    dir: &Path,
    rel: &Path,
    filter: &Filter,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        paths.push((entry.path(), entry.file_type().ok()));
    }
    paths.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, kind) in paths {
        let Some(name) = path.file_name() else {
            continue;
        };
        let rel = rel.join(name);
        if kind.is_some_and(|k| k.is_dir()) {
            if !name.to_string_lossy().starts_with('.') && filter.enter(&rel) {
                find_files(&path, &rel, filter, files)?;
            }
        } else if filter.take(&rel) && path.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

/// Format one file: stdin to stdout, or a file in place. A file that is
/// already formatted is not written. Returns whether it worked.
fn fmt(input: &Input, src: &str, check: bool) -> bool {
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
    let written = match &input.path {
        None => {
            let mut stdout = std::io::stdout().lock();
            stdout
                .write_all(out.as_bytes())
                .and_then(|()| stdout.flush())
        }
        Some(_) if out == src => Ok(()),
        Some(path) => std::fs::write(path, &out),
    };
    if let Err(e) = written {
        let target = if input.path.is_some() { name } else { "stdout" };
        eprintln!("flume: cannot write {target}: {e}");
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
