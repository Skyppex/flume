//! Tokens to concrete syntax tree.
//!
//! This follows the compiler's parser (`rill/src/lang/parser.rs`) rule for
//! rule, including where a line break ends a statement, so a file means the
//! same thing to the formatter as it does to the compiler. Unlike the
//! compiler it stops at the first error.

use crate::Error;
use crate::cst::*;
use crate::lexer::{Kind, Token};

pub fn parse(src: &str, tokens: &[Token]) -> Result<File, Error> {
    let mut p = Parser {
        src,
        tokens,
        pos: 0,
        nest: 0,
    };
    let mut items = Vec::new();
    while !p.at(Kind::Eof) {
        items.push(p.item()?);
    }
    Ok(File { items, eof: p.pos })
}

type PResult<X> = Result<X, Error>;

struct Parser<'a> {
    src: &'a str,
    tokens: &'a [Token],
    pos: usize,
    /// Depth of enclosing `(` / `[`. Line breaks only end statements at 0.
    nest: u32,
}

impl Parser<'_> {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek_kind(&self, ahead: usize) -> Kind {
        self.tokens[(self.pos + ahead).min(self.tokens.len() - 1)].kind
    }

    fn at(&self, kind: Kind) -> bool {
        self.peek().kind == kind
    }

    fn at_ident(&self, name: &str) -> bool {
        let t = self.peek();
        t.kind == Kind::Ident && &self.src[t.start..t.end] == name
    }

    fn bump(&mut self) -> T {
        let t = self.pos;
        if self.peek().kind != Kind::Eof {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, kind: Kind) -> Option<T> {
        self.at(kind).then(|| self.bump())
    }

    fn expect(&mut self, kind: Kind, context: &str) -> PResult<T> {
        match self.eat(kind) {
            Some(t) => Ok(t),
            None => Err(self.unexpected(&format!("{} {context}", kind.describe()))),
        }
    }

    fn unexpected(&self, expected: &str) -> Error {
        let t = self.peek();
        let found = match t.kind {
            Kind::Ident | Kind::Number => format!("`{}`", &self.src[t.start..t.end]),
            k => k.describe().to_owned(),
        };
        Error::at(
            self.src,
            t.start,
            format!("expected {expected}, found {found}"),
        )
    }

    fn ident(&mut self, context: &str) -> PResult<T> {
        self.expect(Kind::Ident, context)
    }

    fn item(&mut self) -> PResult<Item> {
        if self.at(Kind::Fn) || self.at(Kind::Rill) {
            return Ok(Item::Def(self.def()?));
        }
        if self.at_ident("event") {
            return Ok(Item::Event(self.event_decl()?));
        }
        Err(self.unexpected("`fn`, `rill` or `event`"))
    }

    /// `(a, b)`: names only, as in event handlers.
    fn names(&mut self) -> PResult<List<T>> {
        let open = self.expect(Kind::LParen, "")?;
        self.nest += 1;
        let mut items = Vec::new();
        while !self.at(Kind::RParen) {
            let name = self.ident("as an event parameter")?;
            let comma = self.eat(Kind::Comma);
            items.push((name, comma));
            if comma.is_none() {
                break;
            }
        }
        let close = self.expect(Kind::RParen, "to close the event parameter list")?;
        self.nest -= 1;
        Ok(List { open, items, close })
    }

    fn event_decl(&mut self) -> PResult<EventDecl> {
        let keyword = self.bump();
        let name = self.ident("after `event`")?;
        let kind = self.ident("as the event's kind, after its name")?;
        let filters = if self.at(Kind::LParen) {
            Some(self.args()?)
        } else {
            None
        };
        let semi = self.eat(Kind::Semi);
        if semi.is_none() && !self.at(Kind::Eof) && !self.peek().newline_before {
            return Err(self.unexpected("a line break or `;` after the event declaration"));
        }
        Ok(EventDecl {
            keyword,
            name,
            kind,
            filters,
            semi,
        })
    }

    fn def(&mut self) -> PResult<Def> {
        let keyword = self.bump();
        let name = self.ident("after the keyword")?;

        let generics = if let Some(open) = self.eat(Kind::Lt) {
            let mut items = Vec::new();
            loop {
                let name = self.ident("as a size parameter")?;
                let comma = self.eat(Kind::Comma);
                items.push((name, comma));
                if comma.is_none() || self.at(Kind::Gt) {
                    break;
                }
            }
            let close = self.expect(Kind::Gt, "to close the size parameters")?;
            Some(List { open, items, close })
        } else {
            None
        };

        let open = self.expect(Kind::LParen, "after the name")?;
        self.nest += 1;
        let mut items = Vec::new();
        while !self.at(Kind::RParen) {
            let name = self.ident("as a parameter name")?;
            let colon = self.expect(Kind::Colon, "and a type after the parameter name")?;
            let ty = self.ty()?;
            let default = match self.eat(Kind::Assign) {
                Some(assign) => Some((assign, self.expr()?)),
                None => None,
            };
            let comma = self.eat(Kind::Comma);
            items.push((
                Param {
                    name,
                    colon,
                    ty,
                    default,
                },
                comma,
            ));
            if comma.is_none() {
                break;
            }
        }
        let close = self.expect(Kind::RParen, "to close the parameter list")?;
        self.nest -= 1;

        if self.at(Kind::Arrow) {
            return Err(self.unexpected("a return type, without `->`"));
        }
        let ret = self.ty()?;

        let rate = if let Some(at) = self.eat(Kind::At) {
            if !self.at_ident("rate") {
                return Err(self.unexpected("`rate` after `@`"));
            }
            let rate = self.bump();
            let factor = if self.at(Kind::Slash) || self.at(Kind::Star) {
                let op = self.bump();
                Some((op, self.int()?))
            } else {
                None
            };
            Some(Rate { at, rate, factor })
        } else {
            None
        };

        let body = self.block()?;
        Ok(Def {
            keyword,
            name,
            generics,
            params: List { open, items, close },
            ret,
            rate,
            body,
        })
    }

    fn int(&mut self) -> PResult<T> {
        let t = self.peek();
        let text = &self.src[t.start..t.end];
        if t.kind == Kind::Number && text.bytes().all(|b| b.is_ascii_digit() || b == b'_') {
            return Ok(self.bump());
        }
        Err(self.unexpected("a positive whole number"))
    }

    fn ty(&mut self) -> PResult<Type> {
        if let Some(keyword) = self.eat(Kind::Fn) {
            let open = self.expect(Kind::LParen, "after `fn` in a function type")?;
            self.nest += 1;
            let mut items = Vec::new();
            while !self.at(Kind::RParen) {
                let ty = self.ty()?;
                let comma = self.eat(Kind::Comma);
                items.push((ty, comma));
                if comma.is_none() {
                    break;
                }
            }
            let close = self.expect(Kind::RParen, "to close the parameter types")?;
            self.nest -= 1;
            if self.at(Kind::Arrow) {
                return Err(self.unexpected("a return type, without `->`"));
            }
            let ret = self.ty()?;
            return Ok(Type::Fn {
                keyword,
                params: List { open, items, close },
                ret: Box::new(ret),
            });
        }
        if let Some(open) = self.eat(Kind::LBracket) {
            self.nest += 1;
            let elem = self.ty()?;
            let semi = self.expect(Kind::Semi, "and a channel count, as in `[Sample; 2]`")?;
            let size = if self.at(Kind::Ident) {
                self.bump()
            } else {
                self.int()?
            };
            let close = self.expect(Kind::RBracket, "to close the frame type")?;
            self.nest -= 1;
            return Ok(Type::Frame {
                open,
                elem: Box::new(elem),
                semi,
                size,
                close,
            });
        }
        Ok(Type::Named(self.ident("as a type")?))
    }

    fn block(&mut self) -> PResult<Block> {
        let open = self.expect(Kind::LBrace, "to start a block")?;
        let saved = std::mem::replace(&mut self.nest, 0);
        let mut stmts = Vec::new();
        while !self.at(Kind::RBrace) {
            if self.at(Kind::Eof) {
                return Err(Error::at(
                    self.src,
                    self.tokens[open].start,
                    "this `{` is never closed",
                ));
            }
            stmts.push(self.stmt()?);
        }
        let close = self.bump();
        self.nest = saved;
        Ok(Block { open, stmts, close })
    }

    fn stmt(&mut self) -> PResult<(Stmt, Option<T>)> {
        let stmt = if self.at_ident("on") && self.peek_kind(1) == Kind::Ident {
            let keyword = self.bump();
            let name = self.bump();
            let params = if self.at(Kind::LParen) {
                Some(self.names()?)
            } else {
                None
            };
            let body = self.block()?;
            // A handler needs no terminator after its `}`.
            return Ok((
                Stmt::Handler {
                    keyword,
                    name,
                    params,
                    body,
                },
                self.eat(Kind::Semi),
            ));
        } else {
            match self.peek().kind {
                Kind::Let | Kind::State => {
                    let keyword = self.bump();
                    let name = self.ident("as the variable name")?;
                    let ty = match self.eat(Kind::Colon) {
                        Some(colon) => Some((colon, self.ty()?)),
                        None => None,
                    };
                    let assign = self.eat(Kind::Assign);
                    let value = match assign {
                        Some(_) => Some(self.expr()?),
                        None if self.tokens[keyword].kind == Kind::Let && ty.is_some() => None,
                        None => return Err(self.unexpected("and an initial value")),
                    };
                    Stmt::Binding {
                        keyword,
                        name,
                        ty,
                        assign,
                        value,
                    }
                }
                Kind::Return => {
                    let keyword = self.bump();
                    let value = self.expr()?;
                    Stmt::Return { keyword, value }
                }
                Kind::For => {
                    let keyword = self.bump();
                    let name = self.ident("after `for`")?;
                    let in_kw = self.expect(Kind::In, "after the loop variable")?;
                    let saved = std::mem::replace(&mut self.nest, 0);
                    let iter = self.expr()?;
                    self.nest = saved;
                    let body = self.block()?;
                    return Ok((
                        Stmt::For {
                            keyword,
                            name,
                            in_kw,
                            iter,
                            body,
                        },
                        self.eat(Kind::Semi),
                    ));
                }
                Kind::Ident if self.at_assignment() => {
                    let target = self.assign_target()?;
                    let assign = if self.at(Kind::Assign) || self.at(Kind::PlusAssign) {
                        self.bump()
                    } else {
                        return Err(self.unexpected("`=` or `+=` in assignment"));
                    };
                    let value = self.expr()?;
                    Stmt::Assign {
                        target,
                        assign,
                        value,
                    }
                }
                _ => Stmt::Expr(self.expr()?),
            }
        };

        let semi = self.eat(Kind::Semi);
        if semi.is_none()
            && !self.at(Kind::RBrace)
            && !self.at(Kind::Eof)
            && !self.peek().newline_before
        {
            return Err(self.unexpected("a line break or `;` after the statement"));
        }
        Ok((stmt, semi))
    }

    /// True if the next token continues the current expression rather than
    /// starting a new statement on the next line.
    fn continues(&self) -> bool {
        let t = self.peek();
        !t.newline_before || self.nest > 0 || t.kind == Kind::Pipe
    }

    fn expr(&mut self) -> PResult<Expr> {
        let mut lhs = self.range()?;
        while self.continues() && self.at(Kind::Pipe) {
            let pipe = self.bump();
            let callee = self.ident("after `|>`")?;
            let sizes = if self.at_size_call_args() {
                Some(self.size_args()?)
            } else {
                None
            };
            let args = if self.at(Kind::LParen) && !self.peek().newline_before {
                Some(self.args()?)
            } else {
                None
            };
            lhs = Expr::Pipe {
                input: Box::new(lhs),
                pipe,
                callee,
                sizes,
                args,
            };
        }
        Ok(lhs)
    }

    fn range(&mut self) -> PResult<Expr> {
        let lhs = self.binary(0)?;
        if !self.continues() || !(self.at(Kind::DotDot) || self.at(Kind::DotDotEq)) {
            return Ok(lhs);
        }
        let op = self.bump();
        let rhs = self.binary(0)?;
        Ok(Expr::Range {
            start: Box::new(lhs),
            op,
            end: Box::new(rhs),
        })
    }

    /// Precedence climbing over the binary operators below `|>`.
    fn binary(&mut self, min_level: u8) -> PResult<Expr> {
        let mut lhs = self.cast()?;
        loop {
            if !self.continues() {
                break;
            }
            let Some(level) = binop_level(self.peek().kind) else {
                break;
            };
            if level < min_level {
                break;
            }
            let op = self.bump();
            let rhs = self.binary(level + 1)?;
            if level == CMP_LEVEL
                && binop_level(self.peek().kind) == Some(CMP_LEVEL)
                && self.continues()
            {
                return Err(Error::at(
                    self.src,
                    self.peek().start,
                    "comparisons cannot be chained",
                ));
            }
            lhs = Expr::Binary {
                lhs: Box::new(lhs),
                op,
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    /// `x as Float`. Binds tighter than the binary operators and looser
    /// than a leading `-`, so `-x as Int` is `(-x) as Int`.
    fn cast(&mut self) -> PResult<Expr> {
        let mut e = self.unary()?;
        while self.continues() && self.at(Kind::As) {
            let keyword = self.bump();
            let ty = self.ty()?;
            e = Expr::Cast {
                value: Box::new(e),
                keyword,
                ty,
            };
        }
        Ok(e)
    }

    fn unary(&mut self) -> PResult<Expr> {
        if matches!(self.peek().kind, Kind::Minus | Kind::Plus | Kind::Bang) {
            let op = self.bump();
            let operand = self.unary()?;
            return Ok(Expr::Unary {
                op,
                operand: Box::new(operand),
            });
        }
        self.postfix()
    }

    fn postfix(&mut self) -> PResult<Expr> {
        let mut e = self.primary()?;
        while !self.peek().newline_before {
            if self.at(Kind::LBracket) {
                let open = self.bump();
                self.nest += 1;
                let index = self.expr()?;
                let close = self.expect(Kind::RBracket, "to close the index")?;
                self.nest -= 1;
                e = Expr::Index {
                    value: Box::new(e),
                    open,
                    index: Box::new(index),
                    close,
                };
            } else if let Some(dot) = self.eat(Kind::Dot) {
                let name = self.ident("after `.`")?;
                e = Expr::Field {
                    value: Box::new(e),
                    dot,
                    name,
                };
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn args(&mut self) -> PResult<List<Arg>> {
        let open = self.expect(Kind::LParen, "")?;
        self.nest += 1;
        let mut items = Vec::new();
        while !self.at(Kind::RParen) {
            let name = if self.at(Kind::Ident) && self.peek_kind(1) == Kind::Colon {
                let name = self.bump();
                Some((name, self.bump()))
            } else {
                None
            };
            let value = self.expr()?;
            let comma = self.eat(Kind::Comma);
            items.push((Arg { name, value }, comma));
            if comma.is_none() {
                break;
            }
        }
        let close = self.expect(Kind::RParen, "to close the argument list")?;
        self.nest -= 1;
        Ok(List { open, items, close })
    }

    fn size_args(&mut self) -> PResult<List<T>> {
        let open = self.expect(Kind::Lt, "")?;
        let mut items = Vec::new();
        loop {
            let size = if self.at(Kind::Ident) {
                self.bump()
            } else {
                self.int()?
            };
            let comma = self.eat(Kind::Comma);
            items.push((size, comma));
            if comma.is_none() || self.at(Kind::Gt) {
                break;
            }
        }
        let close = self.expect(Kind::Gt, "to close the size arguments")?;
        Ok(List { open, items, close })
    }

    fn at_size_call_args(&self) -> bool {
        if self.peek().newline_before || self.peek().kind != Kind::Lt {
            return false;
        }
        let mut pos = self.pos + 1;
        loop {
            match self.tokens.get(pos).map(|t| t.kind) {
                Some(Kind::Ident | Kind::Number) => pos += 1,
                _ => return false,
            }
            match self.tokens.get(pos).map(|t| t.kind) {
                Some(Kind::Comma) => pos += 1,
                Some(Kind::Gt) => {
                    let Some(next) = self.tokens.get(pos + 1) else {
                        return false;
                    };
                    return !next.newline_before && next.kind == Kind::LParen;
                }
                _ => return false,
            }
        }
    }

    fn at_assignment(&self) -> bool {
        if matches!(self.peek_kind(1), Kind::Assign | Kind::PlusAssign) {
            return true;
        }
        if self.peek_kind(1) != Kind::LBracket {
            return false;
        }
        let mut pos = self.pos + 1;
        let mut depth = 0u32;
        loop {
            match self.tokens.get(pos).map(|t| t.kind) {
                Some(Kind::LBracket | Kind::LParen) => depth += 1,
                Some(Kind::RBracket | Kind::RParen) => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return matches!(
                            self.tokens.get(pos + 1).map(|t| t.kind),
                            Some(Kind::Assign | Kind::PlusAssign)
                        );
                    }
                }
                Some(Kind::Eof | Kind::RBrace) | None => return false,
                _ => {}
            }
            pos += 1;
        }
    }

    fn assign_target(&mut self) -> PResult<Expr> {
        let mut e = Expr::Atom(self.ident("as the assignment target")?);
        while self.at(Kind::LBracket) && !self.peek().newline_before {
            let open = self.bump();
            self.nest += 1;
            let index = self.expr()?;
            let close = self.expect(Kind::RBracket, "to close the index")?;
            self.nest -= 1;
            e = Expr::Index {
                value: Box::new(e),
                open,
                index: Box::new(index),
                close,
            };
        }
        Ok(e)
    }

    fn primary(&mut self) -> PResult<Expr> {
        match self.peek().kind {
            Kind::Number | Kind::True | Kind::False => Ok(Expr::Atom(self.bump())),
            Kind::Ident => {
                let name = self.bump();
                let sizes = if self.at_size_call_args() {
                    Some(self.size_args()?)
                } else {
                    None
                };
                if self.at(Kind::LParen) && !self.peek().newline_before {
                    let args = self.args()?;
                    Ok(Expr::Call {
                        callee: name,
                        sizes,
                        args,
                    })
                } else {
                    if sizes.is_some() {
                        return Err(self.unexpected("`(` after explicit size arguments"));
                    }
                    Ok(Expr::Atom(name))
                }
            }
            Kind::LParen => {
                let open = self.bump();
                self.nest += 1;
                let inner = self.expr()?;
                let close = self.expect(Kind::RParen, "to close the parenthesis")?;
                self.nest -= 1;
                Ok(Expr::Paren {
                    open,
                    inner: Box::new(inner),
                    close,
                })
            }
            Kind::LBracket => {
                let open = self.bump();
                self.nest += 1;
                let mut items = Vec::new();
                while !self.at(Kind::RBracket) {
                    let e = self.expr()?;
                    let comma = self.eat(Kind::Comma);
                    items.push((e, comma));
                    if comma.is_none() {
                        break;
                    }
                }
                let close = self.expect(Kind::RBracket, "or `,` in the frame")?;
                self.nest -= 1;
                if items.is_empty() {
                    return Err(Error::at(
                        self.src,
                        self.tokens[open].start,
                        "a frame needs at least one channel",
                    ));
                }
                Ok(Expr::Frame(List { open, items, close }))
            }
            Kind::If => Ok(Expr::If(self.if_expr()?)),
            Kind::Fn => self.lambda(),
            Kind::LBrace => Ok(Expr::Block(self.block()?)),
            _ => Err(self.unexpected("an expression")),
        }
    }

    /// `fn(p) { ... }`: a fn without a name.
    fn lambda(&mut self) -> PResult<Expr> {
        let keyword = self.bump();
        if self.at(Kind::Ident) {
            return Err(Error::at(
                self.src,
                self.peek().start,
                "a fn with a name can only be defined at the top level",
            ));
        }
        let open = self.expect(Kind::LParen, "after `fn`")?;
        self.nest += 1;
        let mut items = Vec::new();
        while !self.at(Kind::RParen) {
            let name = self.ident("as a parameter name")?;
            let ty = match self.eat(Kind::Colon) {
                Some(colon) => Some((colon, self.ty()?)),
                None => None,
            };
            let comma = self.eat(Kind::Comma);
            items.push((LambdaParam { name, ty }, comma));
            if comma.is_none() {
                break;
            }
        }
        let close = self.expect(Kind::RParen, "to close the parameter list")?;
        self.nest -= 1;
        if self.at(Kind::Arrow) {
            return Err(self.unexpected("a return type, without `->`"));
        }
        let ret = match self.at(Kind::LBrace) {
            true => None,
            false => Some(self.ty()?),
        };
        let body = self.block()?;
        Ok(Expr::Lambda {
            keyword,
            params: List { open, items, close },
            ret,
            body,
        })
    }

    fn if_expr(&mut self) -> PResult<If> {
        let keyword = self.expect(Kind::If, "")?;
        // The condition is not inside brackets even if the `if` is.
        let saved = std::mem::replace(&mut self.nest, 0);
        let cond = self.expr()?;
        self.nest = saved;
        let then = self.block()?;
        let els = match self.eat(Kind::Else) {
            Some(kw) if self.at(Kind::If) => Some((kw, Else::If(Box::new(self.if_expr()?)))),
            Some(kw) => Some((kw, Else::Block(self.block()?))),
            None => None,
        };
        Ok(If {
            keyword,
            cond: Box::new(cond),
            then,
            els,
        })
    }
}

pub const CMP_LEVEL: u8 = 2;

/// Binding strength of a binary operator, weakest first.
pub fn binop_level(kind: Kind) -> Option<u8> {
    Some(match kind {
        Kind::OrOr => 0,
        Kind::AndAnd => 1,
        Kind::Lt | Kind::Le | Kind::Gt | Kind::Ge | Kind::EqEq | Kind::Ne => CMP_LEVEL,
        Kind::Plus | Kind::Minus => 3,
        Kind::Star | Kind::Slash | Kind::Percent => 4,
        _ => return None,
    })
}
