//! Flume, the Rill formatter.
//!
//! There is exactly one way to format a Rill file, so there are no options:
//!
//! - lines are at most 80 columns wherever the code can be broken,
//! - indentation is 4 spaces, lines end in CRLF and the file ends with one,
//! - `{` stays on the line that opens it,
//! - a `}` that ends a line is followed by a blank line, unless the next line
//!   closes something too,
//! - long lines break inside `()`, `[]`, `{}` and `<>`, and before `|>`.
//!
//! ```
//! let out = flume::format("rill main()->Sample{return 0}").unwrap();
//! assert_eq!(out, "rill main() -> Sample {\r\n    return 0\r\n}\r\n");
//! ```

mod cst;
mod doc;
mod fmt;
mod lexer;
mod parser;

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
