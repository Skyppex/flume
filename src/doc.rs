//! A small layout language and its printer, after Wadler's "prettier
//! printer" as used by Prettier.
//!
//! A [`Doc::Group`] prints on one line if it fits in [`WIDTH`] columns, and
//! otherwise turns each of its own [`Doc::Line`]s into a line break. Groups
//! that contain a hard break never fit.

pub const WIDTH: usize = 80;
pub const INDENT: usize = 4;
pub const NEWLINE: &str = "\r\n";

/// What a piece of text is, for the passes that run after layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextKind {
    Code,
    /// A `)`, `]` or `>` that closes a list.
    Closer,
    /// A `}` that closes a block.
    BlockClose,
    /// A whole `// ...` comment.
    LineComment,
}

#[derive(Clone, Debug)]
pub enum Doc {
    Nil,
    Text(String, TextKind),
    /// A space, or a line break when its group breaks.
    Line,
    /// Nothing, or a line break when its group breaks.
    SoftLine,
    /// Always a line break.
    HardLine,
    /// A line break back to column 0, for the inside of block comments.
    LiteralLine,
    /// A line break, unless the line is still empty.
    FreshLine,
    Indent(Box<Doc>),
    Group { doc: Box<Doc>, forced: bool },
    Concat(Vec<Doc>),
    /// The first when the enclosing group breaks, otherwise the second.
    IfBreak(Box<Doc>, Box<Doc>),
    /// A trailing `//` comment: printed at the end of the current line, and
    /// any code after it goes on the next line.
    LineSuffix(String),
    /// Makes every enclosing group break.
    BreakParent,
}

pub fn text(s: impl Into<String>) -> Doc {
    Doc::Text(s.into(), TextKind::Code)
}

pub fn concat(docs: Vec<Doc>) -> Doc {
    Doc::Concat(docs)
}

pub fn indent(doc: Doc) -> Doc {
    Doc::Indent(Box::new(doc))
}

pub fn if_break(broken: Doc, flat: Doc) -> Doc {
    Doc::IfBreak(Box::new(broken), Box::new(flat))
}

pub fn group(doc: Doc) -> Doc {
    let forced = doc.forces_break();
    Doc::Group {
        doc: Box::new(doc),
        forced,
    }
}

impl Doc {
    /// Contains a hard break, so no group around it can be flat. Nested
    /// groups already know this about themselves.
    pub fn forces_break(&self) -> bool {
        match self {
            Doc::HardLine | Doc::LiteralLine | Doc::FreshLine | Doc::BreakParent => true,
            Doc::Group { forced, .. } => *forced,
            Doc::Indent(d) => d.forces_break(),
            Doc::Concat(ds) => ds.iter().any(Doc::forces_break),
            Doc::IfBreak(a, b) => a.forces_break() || b.forces_break(),
            Doc::Nil
            | Doc::Text(..)
            | Doc::Line
            | Doc::SoftLine
            | Doc::LineSuffix(_) => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Flat,
    Break,
}

/// One line of output, with what the passes after layout need to know.
#[derive(Default)]
struct Out {
    text: String,
    /// Trailing `//` comments, not yet placed.
    suffix: Vec<String>,
    /// The first text on the line was a block's closing `}`.
    block_close: bool,
    /// Nothing but closing brackets, `,` and `;` came after it.
    only_closers: bool,
    /// The first text on the line closes a list or block.
    starts_closed: bool,
    /// The line is a single `//` comment.
    comment: bool,
    /// Continues a block comment, so is left as it is.
    verbatim: bool,
    /// Holds no text yet, only indentation.
    empty: bool,
    /// Ends in a `//` comment, so code must start a new line.
    ended: bool,
    /// The next line only starts because this one ended in a comment.
    split_by_comment: bool,
}

impl Out {
    fn new(indent: usize, verbatim: bool) -> Out {
        Out {
            text: " ".repeat(indent),
            verbatim,
            empty: true,
            ..Out::default()
        }
    }
}

fn width(s: &str) -> usize {
    s.chars().count()
}

/// Lay `doc` out and return the finished file text.
pub fn print(doc: &Doc) -> String {
    let mut lines = vec![Out::new(0, false)];
    let mut cmds: Vec<(usize, Mode, &Doc)> = vec![(0, Mode::Break, doc)];
    while let Some((ind, mode, doc)) = cmds.pop() {
        match doc {
            Doc::Nil | Doc::BreakParent => {}
            Doc::Text(s, kind) => {
                // Only separators may follow a trailing comment on its line.
                let separator = s.trim().is_empty() || s == "," || s == ";";
                if lines.last().unwrap().ended && !separator {
                    split_after_comment(&mut lines, ind);
                }
                let line = lines.last_mut().unwrap();
                if line.empty {
                    line.empty = false;
                    line.block_close = *kind == TextKind::BlockClose;
                    line.starts_closed = matches!(kind, TextKind::Closer | TextKind::BlockClose);
                    line.only_closers = true;
                    line.comment = *kind == TextKind::LineComment;
                } else {
                    line.comment = false;
                    if !s.chars().all(|c| ")]},;".contains(c)) {
                        line.only_closers = false;
                    }
                }
                line.text.push_str(s);
            }
            Doc::Line | Doc::SoftLine if mode == Mode::Flat => {
                if matches!(doc, Doc::Line) {
                    let line = lines.last_mut().unwrap();
                    line.text.push(' ');
                    line.only_closers = false;
                }
            }
            Doc::Line | Doc::SoftLine | Doc::HardLine => lines.push(Out::new(ind, false)),
            Doc::LiteralLine => lines.push(Out::new(0, true)),
            Doc::FreshLine => {
                if !lines.last().unwrap().empty {
                    lines.push(Out::new(ind, false));
                }
            }
            Doc::Indent(d) => cmds.push((ind + INDENT, mode, d)),
            Doc::Concat(ds) => cmds.extend(ds.iter().rev().map(|d| (ind, mode, d))),
            Doc::IfBreak(broken, flat) => {
                cmds.push((ind, mode, if mode == Mode::Break { broken } else { flat }))
            }
            Doc::LineSuffix(s) => {
                let line = lines.last_mut().unwrap();
                line.suffix.push(s.clone());
                line.ended = true;
            }
            Doc::Group { doc, forced } => {
                // Measure from where the group will actually start.
                if lines.last().unwrap().ended {
                    split_after_comment(&mut lines, ind);
                }
                let mode = if mode == Mode::Flat && !forced {
                    Mode::Flat
                } else {
                    let used = width(&lines.last().unwrap().text);
                    let room = WIDTH as isize - used as isize;
                    if !forced && fits((ind, Mode::Flat, doc), &cmds, room) {
                        Mode::Flat
                    } else {
                        Mode::Break
                    }
                };
                cmds.push((ind, mode, doc));
            }
        }
    }
    finish(lines)
}

fn split_after_comment(lines: &mut Vec<Out>, ind: usize) {
    lines.last_mut().unwrap().split_by_comment = true;
    lines.push(Out::new(ind, false));
}

/// Whether `next` fits in `room` columns flat, along with whatever follows it
/// up to the next possible line break.
fn fits(next: (usize, Mode, &Doc), rest: &[(usize, Mode, &Doc)], mut room: isize) -> bool {
    let mut rest_idx = rest.len();
    let mut stack = vec![(next.1, next.2)];
    // Spaces only take room once text follows them on the same line; at the
    // end of a line they are trimmed.
    let mut spaces = 0;
    loop {
        if room < 0 {
            return false;
        }
        let (mode, doc) = match stack.pop() {
            Some(cmd) => cmd,
            None => {
                if rest_idx == 0 {
                    return true;
                }
                rest_idx -= 1;
                (rest[rest_idx].1, rest[rest_idx].2)
            }
        };
        match doc {
            Doc::Nil | Doc::BreakParent => {}
            // Whatever follows goes on the next line.
            Doc::LineSuffix(_) => return true,
            Doc::Text(s, _) if s.trim().is_empty() => spaces += width(s) as isize,
            Doc::Text(s, _) => {
                room -= spaces + width(s) as isize;
                spaces = 0;
            }
            Doc::Line | Doc::SoftLine => {
                if mode == Mode::Break {
                    return true;
                }
                if matches!(doc, Doc::Line) {
                    spaces += 1;
                }
            }
            Doc::HardLine | Doc::LiteralLine | Doc::FreshLine => return true,
            Doc::Indent(d) => stack.push((mode, d)),
            Doc::Concat(ds) => stack.extend(ds.iter().rev().map(|d| (mode, d))),
            Doc::IfBreak(broken, flat) => {
                stack.push((mode, if mode == Mode::Break { broken } else { flat }))
            }
            Doc::Group { doc, forced } => {
                stack.push((if *forced { Mode::Break } else { mode }, doc));
            }
        }
    }
}

/// The passes after layout, then joining with CRLF.
fn finish(mut lines: Vec<Out>) -> String {
    // Trailing comments that would run past the edge move above their line.
    // If the comment is why the code continues on the next line, it moves
    // down instead, to stay between the same two pieces of code.
    let mut placed: Vec<Out> = Vec::with_capacity(lines.len());
    let mut below: Vec<String> = Vec::new();
    for i in 0..lines.len() {
        let mut line = std::mem::take(&mut lines[i]);
        let code = line.text.trim_end().to_owned();
        let suffix: String = line.suffix.concat();
        let mut moved = Vec::new();
        if !suffix.is_empty() && width(&code) + width(&suffix) > WIDTH {
            moved = line.suffix.iter().map(|c| c.trim_start().to_owned()).collect();
            line.text = code;
        } else {
            line.text = code + &suffix;
        }
        let indent_of = |l: &Out| l.text.len() - l.text.trim_start().len();
        for c in below.drain(..) {
            for text in wrap_comment(indent_of(&line), &c) {
                placed.push(comment_line(text));
            }
        }
        if line.split_by_comment && i + 1 < lines.len() {
            below = moved;
        } else {
            for c in moved {
                for text in wrap_comment(indent_of(&line), &c) {
                    placed.push(comment_line(text));
                }
            }
        }
        placed.push(line);
    }

    // Long comments on their own line wrap at word boundaries.
    let mut wrapped: Vec<Out> = Vec::with_capacity(placed.len());
    for line in placed {
        if line.comment && !line.verbatim && width(&line.text) > WIDTH {
            let indent = line.text.len() - line.text.trim_start().len();
            for text in wrap_comment(indent, line.text.trim_start()) {
                wrapped.push(comment_line(text));
            }
        } else {
            wrapped.push(line);
        }
    }

    // A line that ends a multi-line block is followed by a blank line,
    // unless what follows closes something too.
    let mut out: Vec<String> = Vec::with_capacity(wrapped.len());
    for i in 0..wrapped.len() {
        let line = &wrapped[i];
        out.push(line.text.clone());
        if line.block_close
            && line.only_closers
            && !line.verbatim
            && let Some(next) = wrapped.get(i + 1)
            && !next.text.trim().is_empty()
            && !next.starts_closed
        {
            out.push(String::new());
        }
    }

    // At most one blank line in a row, none at the start or end.
    let mut result = String::new();
    let mut blank = false;
    let mut any = false;
    for line in out {
        if line.trim().is_empty() {
            blank = any;
            continue;
        }
        if blank {
            result.push_str(NEWLINE);
        }
        blank = false;
        any = true;
        result.push_str(&line);
        result.push_str(NEWLINE);
    }
    result
}

fn comment_line(text: String) -> Out {
    Out {
        text,
        comment: true,
        empty: false,
        ..Out::default()
    }
}

/// Break a `//` comment into lines of at most [`WIDTH`] columns at `indent`.
/// A word longer than a whole line stays whole.
pub fn wrap_comment(indent: usize, comment: &str) -> Vec<String> {
    let prefix_len = comment
        .find(|c: char| c != '/' && c != '!')
        .unwrap_or(comment.len());
    let (prefix, body) = comment.split_at(prefix_len);
    let pad = " ".repeat(indent);
    let words: Vec<&str> = body.split_whitespace().collect();
    if words.is_empty() {
        return vec![format!("{pad}{comment}")];
    }
    let start = format!("{pad}{prefix}");
    let mut lines = Vec::new();
    let mut current = start.clone();
    for word in words {
        if current != start && width(&current) + 1 + width(word) > WIDTH {
            lines.push(std::mem::replace(&mut current, start.clone()));
        }
        current.push(' ');
        current.push_str(word);
    }
    lines.push(current);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(items: &[&str]) -> Doc {
        let mut inner = vec![Doc::SoftLine];
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                inner.push(text(","));
                inner.push(Doc::Line);
            }
            inner.push(text(*item));
        }
        inner.push(if_break(text(","), Doc::Nil));
        group(concat(vec![
            text("f("),
            indent(concat(inner)),
            Doc::SoftLine,
            Doc::Text(")".into(), TextKind::Closer),
        ]))
    }

    #[test]
    fn groups_stay_flat_when_they_fit() {
        assert_eq!(print(&list(&["a", "b"])), "f(a, b)\r\n");
    }

    #[test]
    fn groups_break_when_too_wide() {
        let long = "x".repeat(40);
        assert_eq!(
            print(&list(&[&long, &long])),
            format!("f(\r\n    {long},\r\n    {long},\r\n)\r\n")
        );
    }

    #[test]
    fn hard_lines_force_groups_to_break() {
        let doc = group(concat(vec![text("a"), Doc::Line, text("b"), Doc::HardLine]));
        assert_eq!(print(&doc), "a\r\nb\r\n");
    }

    #[test]
    fn comments_wrap_at_words() {
        let words = "word ".repeat(20);
        let lines = wrap_comment(4, &format!("// {words}"));
        assert!(lines.iter().all(|l| l.len() <= WIDTH), "{lines:?}");
        assert!(lines.iter().all(|l| l.starts_with("    // word")));
        assert_eq!(lines.len(), 2);
    }
}
