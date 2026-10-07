//! `flume fmt < in.rill > out.rill`, or `flume check < in.rill`.

use std::io::{Read, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// The Rill formatter. Reads a file on stdin.
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
    /// Format stdin and write the result to stdout.
    Fmt {
        /// Before writing, make sure only the layout changed: the syntax tree
        /// must be the same and no comment may be lost. Writes nothing if
        /// that fails.
        #[arg(long)]
        check: bool,
    },
    /// Show what `fmt` would change, without writing the file.
    ///
    /// Exits with 1 if anything would change.
    Check,
}

/// What stdin is called in messages.
const NAME: &str = "<stdin>";

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut src = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut src) {
        eprintln!("flume: cannot read stdin: {e}");
        return ExitCode::FAILURE;
    }
    match cli.command {
        Command::Fmt { check } => fmt(&src, check),
        Command::Check => check(&src),
    }
}

fn fmt(src: &str, check: bool) -> ExitCode {
    let out = match flume::format(src) {
        Ok(out) => out,
        Err(e) => {
            eprintln!("flume: {NAME}:{e}");
            return ExitCode::FAILURE;
        }
    };
    if check && let Err(e) = flume::verify(src, &out) {
        eprintln!("flume: {NAME}: formatting would change more than the layout: {e}");
        return ExitCode::FAILURE;
    }
    let mut stdout = std::io::stdout().lock();
    if let Err(e) = stdout
        .write_all(out.as_bytes())
        .and_then(|()| stdout.flush())
    {
        eprintln!("flume: cannot write stdout: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn check(src: &str) -> ExitCode {
    match flume::check(src) {
        Ok(issues) if issues.is_empty() => ExitCode::SUCCESS,
        Ok(issues) => {
            for issue in &issues {
                eprintln!("{NAME}:{issue}");
            }
            eprintln!("flume: {NAME}: {} place(s) not formatted", issues.len());
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("flume: {NAME}:{e}");
            ExitCode::FAILURE
        }
    }
}
