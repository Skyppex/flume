//! Source text to tokens, keeping every comment.
//!
//! Token kinds and line-break tracking mirror the Rill compiler's lexer, so
//! the parser here sees statements end in exactly the same places. Comments
//! are attached to tokens: a comment that starts a line or sits between two
//! tokens on one line leads the next token, and a comment after a token at the
//! end of its line trails that token.

use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Ident,
    Number,
    // Keywords.
    Fn,
    Rill,
    State,
    Let,
    Const,
    For,
    In,
    Return,
    If,
    Else,
    As,
    True,
    False,
    // Punctuation.
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semi,
    Dot,
    DotDot,
    DotDotEq,
    At,
    Arrow,
    Pipe,
    Plus,
    PlusAssign,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    Assign,
    EqEq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    Eof,
}

impl Kind {
    /// How the token is written, for "expected X" messages.
    pub fn describe(self) -> &'static str {
        use Kind::*;
        match self {
            Ident => "a name",
            Number => "a number",
            Fn => "`fn`",
            Rill => "`rill`",
            State => "`state`",
            Let => "`let`",
            Const => "`const`",
            For => "`for`",
            In => "`in`",
            Return => "`return`",
            If => "`if`",
            Else => "`else`",
            As => "`as`",
            True => "`true`",
            False => "`false`",
            LParen => "`(`",
            RParen => "`)`",
            LBrace => "`{`",
            RBrace => "`}`",
            LBracket => "`[`",
            RBracket => "`]`",
            Comma => "`,`",
            Colon => "`:`",
            Semi => "`;`",
            Dot => "`.`",
            DotDot => "`..`",
            DotDotEq => "`..=`",
            At => "`@`",
            Arrow => "`->`",
            Pipe => "`|>`",
            Plus => "`+`",
            PlusAssign => "`+=`",
            Minus => "`-`",
            Star => "`*`",
            Slash => "`/`",
            Percent => "`%`",
            Bang => "`!`",
            Assign => "`=`",
            EqEq => "`==`",
            Ne => "`!=`",
            Lt => "`<`",
            Le => "`<=`",
            Gt => "`>`",
            Ge => "`>=`",
            AndAnd => "`&&`",
            OrOr => "`||`",
            Eof => "end of file",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    /// One entry per source line, trailing whitespace removed and tabs
    /// expanded. Only block comments have more than one.
    pub lines: Vec<String>,
    /// `/* ... */` rather than `// ...`.
    pub block: bool,
    /// A line break separates it from what came before.
    pub newline_before: bool,
    /// A blank line separates it from what came before.
    pub blank_before: bool,
    /// A line break follows it before the next comment or token.
    pub newline_after: bool,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
    /// A line break separates this token from the previous one. Same meaning
    /// as in the compiler: line breaks inside block comments count.
    pub newline_before: bool,
    /// A blank line separates this token from the comment or token before it.
    pub blank_before: bool,
    pub leading: Vec<Comment>,
    pub trailing: Vec<Comment>,
}

impl Token {
    /// A blank line comes before this token's first leading comment, or
    /// before the token itself when it has none.
    pub fn lead_blank(&self) -> bool {
        self.leading
            .first()
            .map_or(self.blank_before, |c| c.blank_before)
    }
}

pub fn lex(src: &str) -> Result<Vec<Token>, Error> {
    let mut lexer = Lexer {
        src,
        bytes: src.as_bytes(),
        pos: 0,
        tokens: Vec::new(),
    };
    lexer.run()?;
    Ok(lexer.tokens)
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    tokens: Vec<Token>,
}

/// Whitespace and comments between two tokens.
struct Trivia {
    /// Each comment with the number of line breaks before it.
    comments: Vec<(Comment, usize)>,
    /// Line breaks between the last comment (or the previous token) and the
    /// next token.
    breaks_after: usize,
    /// Any line break at all, including inside block comments.
    any_newline: bool,
}

impl Lexer<'_> {
    fn peek(&self, ahead: usize) -> u8 {
        self.bytes.get(self.pos + ahead).copied().unwrap_or(0)
    }

    fn run(&mut self) -> Result<(), Error> {
        loop {
            let trivia = self.trivia()?;
            let start = self.pos;
            let kind = self.token()?;
            self.attach(trivia, kind, start);
            if kind == Kind::Eof {
                return Ok(());
            }
        }
    }

    /// Split the comments in `trivia` between the previous token and the one
    /// starting at `start`, then push the new token.
    fn attach(&mut self, trivia: Trivia, kind: Kind, start: usize) {
        let mut comments = trivia.comments.into_iter().peekable();
        if let Some(prev) = self.tokens.last_mut() {
            // Comments on the previous token's line trail it, as long as the
            // line ends before the next token.
            let mut same_line = Vec::new();
            while let Some((_, 0)) = comments.peek() {
                same_line.push(comments.next().unwrap().0);
            }
            let line_ends = match comments.peek() {
                Some(_) => true,
                None => trivia.breaks_after > 0 || kind == Kind::Eof,
            };
            if line_ends || same_line.last().is_some_and(|c| !c.block) {
                prev.trailing = same_line;
                if let Some(last) = prev.trailing.last_mut() {
                    last.newline_after = true;
                }
            } else {
                let mut leading: Vec<Comment> = same_line;
                leading.extend(comments.map(|(c, _)| c));
                return self.push(
                    kind,
                    start,
                    trivia.any_newline,
                    trivia.breaks_after,
                    leading,
                );
            }
        }
        let leading = comments.map(|(c, _)| c).collect();
        self.push(
            kind,
            start,
            trivia.any_newline,
            trivia.breaks_after,
            leading,
        );
    }

    fn push(
        &mut self,
        kind: Kind,
        start: usize,
        newline_before: bool,
        breaks_before: usize,
        leading: Vec<Comment>,
    ) {
        self.tokens.push(Token {
            kind,
            start,
            end: self.pos,
            newline_before,
            blank_before: breaks_before >= 2,
            leading,
            trailing: Vec::new(),
        });
    }

    fn trivia(&mut self) -> Result<Trivia, Error> {
        let mut comments: Vec<(Comment, usize)> = Vec::new();
        let mut breaks = 0;
        let mut any_newline = false;
        loop {
            match (self.peek(0), self.peek(1)) {
                (b'\n', _) => {
                    breaks += 1;
                    any_newline = true;
                    self.pos += 1;
                }
                (b' ' | b'\t' | b'\r', _) => self.pos += 1,
                (b'/', b'/') => {
                    let start = self.pos;
                    while self.pos < self.bytes.len() && self.peek(0) != b'\n' {
                        self.pos += 1;
                    }
                    let text = &self.src[start..self.pos];
                    comments.push((comment(text, false, breaks), breaks));
                    breaks = 0;
                }
                (b'/', b'*') => {
                    let start = self.pos;
                    self.pos += 2;
                    let mut depth = 1;
                    while depth > 0 {
                        match (self.peek(0), self.peek(1)) {
                            (0, _) if self.pos >= self.bytes.len() => {
                                return Err(Error::at(
                                    self.src,
                                    start,
                                    "unterminated block comment",
                                ));
                            }
                            (b'/', b'*') => {
                                depth += 1;
                                self.pos += 2;
                            }
                            (b'*', b'/') => {
                                depth -= 1;
                                self.pos += 2;
                            }
                            (b'\n', _) => {
                                any_newline = true;
                                self.pos += 1;
                            }
                            _ => self.pos += 1,
                        }
                    }
                    let text = &self.src[start..self.pos];
                    comments.push((comment(text, true, breaks), breaks));
                    breaks = 0;
                }
                _ => break,
            }
        }
        // Now that the breaks after each comment are known, record them.
        for i in 0..comments.len() {
            let after = match comments.get(i + 1) {
                Some(&(_, b)) => b,
                None => breaks,
            };
            comments[i].0.newline_after = after > 0;
        }
        Ok(Trivia {
            comments,
            breaks_after: breaks,
            any_newline,
        })
    }

    fn token(&mut self) -> Result<Kind, Error> {
        let start = self.pos;
        let c = self.peek(0);
        if self.pos >= self.bytes.len() {
            return Ok(Kind::Eof);
        }
        if c.is_ascii_digit() {
            self.number();
            return Ok(Kind::Number);
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            // `#` is a sharp, so it is only part of a name right after a note
            // letter, as in `F#4`.
            while self.peek(0).is_ascii_alphanumeric()
                || self.peek(0) == b'_'
                || (self.peek(0) == b'#' && self.pos == start + 1 && matches!(c, b'A'..=b'G'))
            {
                self.pos += 1;
            }
            return Ok(match &self.src[start..self.pos] {
                "fn" => Kind::Fn,
                "rill" => Kind::Rill,
                "state" => Kind::State,
                "let" => Kind::Let,
                "const" => Kind::Const,
                "for" => Kind::For,
                "in" => Kind::In,
                "return" => Kind::Return,
                "if" => Kind::If,
                "else" => Kind::Else,
                "as" => Kind::As,
                "true" => Kind::True,
                "false" => Kind::False,
                _ => Kind::Ident,
            });
        }

        use Kind::*;
        let (kind, len) = match &[c, self.peek(1)] {
            b"->" => (Arrow, 2),
            b"|>" => (Pipe, 2),
            b"+=" => (PlusAssign, 2),
            b".." if self.peek(2) == b'=' => (DotDotEq, 3),
            b".." => (DotDot, 2),
            b"==" => (EqEq, 2),
            b"!=" => (Ne, 2),
            b"<=" => (Le, 2),
            b">=" => (Ge, 2),
            b"&&" => (AndAnd, 2),
            b"||" => (OrOr, 2),
            _ => match c {
                b'(' => (LParen, 1),
                b')' => (RParen, 1),
                b'{' => (LBrace, 1),
                b'}' => (RBrace, 1),
                b'[' => (LBracket, 1),
                b']' => (RBracket, 1),
                b',' => (Comma, 1),
                b':' => (Colon, 1),
                b';' => (Semi, 1),
                b'.' => (Dot, 1),
                b'@' => (At, 1),
                b'+' => (Plus, 1),
                b'-' => (Minus, 1),
                b'*' => (Star, 1),
                b'/' => (Slash, 1),
                b'%' => (Percent, 1),
                b'!' => (Bang, 1),
                b'=' => (Assign, 1),
                b'<' => (Lt, 1),
                b'>' => (Gt, 1),
                _ => {
                    let ch = self.src[start..].chars().next().unwrap();
                    return Err(Error::at(
                        self.src,
                        start,
                        format!("unexpected character `{ch}`"),
                    ));
                }
            },
        };
        self.pos += len;
        Ok(kind)
    }

    /// `48_000`, `0.5`, `1e-3`, each optionally followed by a unit. The text
    /// is kept as written, so the unit is not checked here.
    fn number(&mut self) {
        let digits = |l: &mut Self| {
            while l.peek(0).is_ascii_digit() || l.peek(0) == b'_' {
                l.pos += 1;
            }
        };
        digits(self);
        if self.peek(0) == b'.' && self.peek(1).is_ascii_digit() {
            self.pos += 1;
            digits(self);
        }
        if matches!(self.peek(0), b'e' | b'E') {
            let sign = usize::from(matches!(self.peek(1), b'+' | b'-'));
            if self.peek(1 + sign).is_ascii_digit() {
                self.pos += 1 + sign;
                digits(self);
            }
        }
        while self.peek(0).is_ascii_alphanumeric() || self.peek(0) == b'_' {
            self.pos += 1;
        }
    }
}

fn comment(text: &str, block: bool, breaks_before: usize) -> Comment {
    Comment {
        lines: text
            .split('\n')
            .map(|line| line.replace('\t', "    ").trim_end().to_owned())
            .collect(),
        block,
        newline_before: breaks_before >= 1,
        blank_before: breaks_before >= 2,
        newline_after: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_attach_to_neighbours() {
        let toks = lex("a // t\n// l\n\nb /* i */ c /* e */\nd").unwrap();
        let a = &toks[0];
        assert_eq!(a.trailing[0].lines, ["// t"]);
        let b = &toks[1];
        assert_eq!(b.leading[0].lines, ["// l"]);
        assert!(!b.leading[0].blank_before);
        assert!(b.blank_before);
        assert!(!b.lead_blank());
        let c = &toks[2];
        assert_eq!(c.leading[0].lines, ["/* i */"]);
        assert!(!c.leading[0].newline_after);
        assert_eq!(c.trailing[0].lines, ["/* e */"]);
        assert!(toks[3].leading.is_empty());
    }

    #[test]
    fn newlines_inside_block_comments_count() {
        let toks = lex("a /*\n*/ b").unwrap();
        assert!(toks[1].newline_before);
    }
}
