//! `flume < in.rill > out.rill`

use std::io::{Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    if std::env::args_os().len() > 1 {
        eprintln!("flume: takes no arguments; usage: flume < in.rill > out.rill");
        return ExitCode::from(2);
    }
    let mut src = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut src) {
        eprintln!("flume: cannot read stdin: {e}");
        return ExitCode::FAILURE;
    }
    match flume::format(&src) {
        Ok(out) => {
            let mut stdout = std::io::stdout().lock();
            if let Err(e) = stdout.write_all(out.as_bytes()).and_then(|()| stdout.flush()) {
                eprintln!("flume: cannot write stdout: {e}");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("flume: <stdin>:{e}");
            ExitCode::FAILURE
        }
    }
}
