//! Flume, the Rill formatter.
//!
//! There is exactly one way to format a Rill file, so nothing about the
//! layout can be configured:
//!
//! - lines are at most 80 columns wherever the code can be broken,
//! - indentation is 4 spaces, lines end in CRLF and the file ends with one,
//! - `{` stays on the line that opens it,
//! - a `}` that ends a line is followed by a blank line, unless the next line
//!   closes something too,
//! - a statement whose code spans several lines has a blank line before and
//!   after it, shared with its neighbours and left out at the start and end
//!   of a block,
//! - long lines break inside `()`, `[]`, `{}` and `<>`, and before `|>`.
//!
//! ```
//! let out = flume::format("rill main()Sample{return 0}").unwrap();
//! assert_eq!(out, "rill main() Sample {\r\n    return 0\r\n}\r\n");
//! ```

mod check;
mod cst;
mod doc;
mod fmt;
mod lexer;
mod parser;
mod verify;

pub use check::Issue;

use std::fmt as stdfmt;

/// A file the formatter cannot read, with the position of the problem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// 1-based.
    pub line: usize,
    /// 1-based, in characters.
    pub column: usize,
    pub message: String,
}

impl Error {
    pub(crate) fn at(src: &str, offset: usize, message: impl Into<String>) -> Error {
        let before = &src[..offset];
        let line_start = before.rfind('\n').map_or(0, |i| i + 1);
        Error {
            line: before.matches('\n').count() + 1,
            column: before[line_start..].chars().count() + 1,
            message: message.into(),
        }
    }
}

impl stdfmt::Display for Error {
    fn fmt(&self, f: &mut stdfmt::Formatter<'_>) -> stdfmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for Error {}

/// Format a whole Rill file.
pub fn format(src: &str) -> Result<String, Error> {
    let tokens = lexer::lex(src)?;
    let file = parser::parse(src, &tokens)?;
    Ok(doc::print(&fmt::file(src, &tokens, &file)))
}

/// What [`format`] would change in `src`. Empty when it is formatted.
pub fn check(src: &str) -> Result<Vec<Issue>, Error> {
    Ok(check::issues(src, &format(src)?))
}

/// Check that `after` differs from `before` only in layout: the same syntax
/// tree once line breaks, spaces, trailing commas, `;` and parentheses are
/// set aside, and the same comments word for word.
pub fn verify(before: &str, after: &str) -> Result<(), String> {
    verify::same_program(before, after)
}
