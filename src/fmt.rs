//! Syntax tree to layout.
//!
//! Every token goes through [`Fmt::tok`] so its comments come with it.
//! Line breaks are only placed where the compiler allows them: after an
//! opening bracket, after a comma, before a closing bracket, before `|>`,
//! inside blocks, and before a binary operator when inside `(...)` or `[...]`.
//! A binary chain too long for its line outside brackets is wrapped in
//! parentheses so it can break.

use std::cell::RefCell;
use std::collections::HashSet;

use crate::cst::*;
use crate::doc::{Doc, TextKind, concat, group, if_break, indent, text};
use crate::lexer::{Comment, Token};
use crate::parser::binop_level;

pub fn file(src: &str, tokens: &[Token], file: &File) -> Doc {
    let mut f = Fmt {
        src,
        tokens,
        nest: 0,
        detached: RefCell::new(HashSet::new()),
    };
    let mut parts = Vec::new();
    for (i, item) in file.items.iter().enumerate() {
        if i > 0 {
            parts.push(Doc::HardLine);
            let both_events =
                matches!(item, Item::Event(_)) && matches!(file.items[i - 1], Item::Event(_));
            if !both_events || tokens[item.first()].lead_blank() {
                parts.push(Doc::HardLine);
            }
        }
        parts.push(match item {
            Item::Def(d) => f.def(d),
            Item::Event(e) => f.event_decl(e),
            Item::Seq(s) => f.seq_decl(s),
        });
    }
    // Comments after the last item.
    for (i, c) in tokens[file.eof].leading.iter().enumerate() {
        if !parts.is_empty() {
            parts.push(Doc::HardLine);
            if c.blank_before || i == 0 {
                parts.push(Doc::HardLine);
            }
        }
        parts.push(comment(c));
    }
    if !parts.is_empty() {
        parts.push(Doc::HardLine);
    }
    concat(parts)
}

struct Fmt<'a> {
    src: &'a str,
    tokens: &'a [Token],
    /// Depth of `(` / `[` around the current point, counted the way the
    /// compiler counts it. Line breaks before binary operators are only
    /// allowed when it is above 0.
    nest: u32,
    /// Tokens whose leading comments were already placed, ahead of the
    /// groups their expression opens.
    detached: RefCell<HashSet<T>>,
}

/// A comment's text, with any lines after the first kept exactly as written.
fn comment(c: &Comment) -> Doc {
    if !c.block {
        return Doc::Text(c.lines[0].clone(), TextKind::LineComment);
    }
    let mut parts = vec![text(c.lines[0].clone())];
    for line in &c.lines[1..] {
        parts.push(Doc::LiteralLine);
        parts.push(text(line.clone()));
    }
    concat(parts)
}

impl Fmt<'_> {
    fn text_of(&self, t: T) -> &str {
        &self.src[self.tokens[t].start..self.tokens[t].end]
    }

    /// A token with its comments.
    fn tok(&self, t: T) -> Doc {
        self.tok_as(t, TextKind::Code)
    }

    fn tok_as(&self, t: T, kind: TextKind) -> Doc {
        concat(vec![
            self.leading(t),
            Doc::Text(self.text_of(t).to_owned(), kind),
            self.trailing(t),
        ])
    }

    /// A closing token whose leading comments were already placed with
    /// [`Fmt::dangling`]. Its trailing comments are left to the caller, to
    /// go after the group it closes so they do not break it.
    fn closer(&self, t: T, kind: TextKind) -> Doc {
        Doc::Text(self.text_of(t).to_owned(), kind)
    }

    /// Only the comments of a token that is left out, like `;`.
    fn comments_only(&self, t: T) -> Doc {
        concat(vec![self.leading(t), self.trailing(t)])
    }

    /// The leading comments of `t`, to place before an expression that
    /// starts with it, so that the comments' line breaks stay outside the
    /// groups the expression opens.
    fn detach(&self, t: T) -> Doc {
        let doc = self.leading(t);
        self.detached.borrow_mut().insert(t);
        doc
    }

    fn leading(&self, t: T) -> Doc {
        if self.detached.borrow().contains(&t) {
            return Doc::Nil;
        }
        let token = &self.tokens[t];
        let mut parts = Vec::new();
        for (i, c) in token.leading.iter().enumerate() {
            // A comment that started a line still does.
            if c.newline_before {
                parts.push(Doc::FreshLine);
            }
            if i > 0 && c.blank_before {
                parts.push(Doc::HardLine);
            }
            parts.push(comment(c));
            parts.push(if !c.block || c.newline_after {
                Doc::HardLine
            } else {
                text(" ")
            });
        }
        if !token.leading.is_empty() && token.blank_before {
            parts.push(Doc::HardLine);
        }
        concat(parts)
    }

    /// A `//` comment after a token breaks the groups around it, so it can
    /// stay right after its token.
    fn trailing(&self, t: T) -> Doc {
        let mut parts = Vec::new();
        for c in &self.tokens[t].trailing {
            if c.block {
                parts.push(text(" "));
                parts.push(comment(c));
            } else {
                parts.push(Doc::LineSuffix(format!(" {}", c.lines[0])));
                parts.push(Doc::BreakParent);
            }
        }
        concat(parts)
    }

    /// The leading comments of a closing bracket, each on its own line at
    /// the indentation of the contents. Goes inside the [`indent`].
    fn dangling(&self, close: T) -> Doc {
        let mut parts = Vec::new();
        for c in &self.tokens[close].leading {
            parts.push(Doc::HardLine);
            if c.blank_before {
                parts.push(Doc::HardLine);
            }
            parts.push(comment(c));
        }
        concat(parts)
    }

    fn has_dangling(&self, close: T) -> bool {
        !self.tokens[close].leading.is_empty()
    }

    /// `open items close`, on one line or with one item per line and a
    /// trailing comma.
    fn list<X>(&mut self, list: &List<X>, mut item: impl FnMut(&mut Self, &X) -> Doc) -> Doc {
        self.nest += 1;
        let mut inner = vec![Doc::SoftLine];
        let count = list.items.len();
        for (i, (x, comma)) in list.items.iter().enumerate() {
            inner.push(item(self, x));
            if i + 1 < count {
                inner.push(self.tok(comma.expect("a comma between list items")));
                inner.push(Doc::Line);
            } else {
                inner.push(match comma {
                    Some(c) => if_break(self.tok(*c), self.comments_only(*c)),
                    None => if_break(text(","), Doc::Nil),
                });
            }
        }
        self.nest -= 1;
        if count == 0 {
            inner.clear();
        }
        inner.push(self.dangling(list.close));
        let break_before_close = if count == 0 && !self.has_dangling(list.close) {
            Doc::Nil
        } else {
            Doc::SoftLine
        };
        concat(vec![
            group(concat(vec![
                self.tok(list.open),
                indent(concat(inner)),
                break_before_close,
                self.closer(list.close, TextKind::Closer),
            ])),
            self.trailing(list.close),
        ])
    }

    /// Steps keep the lines they were written on, since a line is often a
    /// bar. Written on one line, they stay on one if it fits, and otherwise
    /// go one per line.
    fn seq_decl(&mut self, s: &SeqDecl) -> Doc {
        let mut parts = vec![self.tok(s.keyword), text(" "), self.tok(s.name)];
        if let Some(settings) = &s.settings {
            parts.push(self.list(settings, |f, a| f.setting(a)));
        }
        parts.push(text(" "));
        let list = &s.steps;
        let count = list.items.len();
        let multiline = list
            .items
            .iter()
            .skip(1)
            .any(|(st, _)| self.tokens[st.notes.first()].newline_before);
        self.nest += 1;
        let mut inner = Vec::new();
        for (i, (step, comma)) in list.items.iter().enumerate() {
            if i == 0 {
                inner.push(if multiline { Doc::HardLine } else { Doc::Line });
                inner.push(self.step(step));
            } else if multiline && self.tokens[step.notes.first()].newline_before {
                inner.push(Doc::HardLine);
                inner.push(self.step(step));
            } else if multiline {
                inner.push(text(" "));
                inner.push(self.step(step));
            } else {
                // Each step goes on the current line if it fits there, so a
                // long list fills lines rather than taking one per step.
                let step = self.step(step);
                inner.push(group(concat(vec![Doc::Line, step])));
            }
            let last = i + 1 == count;
            match (comma, last) {
                (Some(c), false) => inner.push(self.tok(*c)),
                (None, false) => inner.push(text(",")),
                (Some(c), true) => inner.push(if_break(self.tok(*c), self.comments_only(*c))),
                (None, true) => inner.push(if_break(text(","), Doc::Nil)),
            }
        }
        self.nest -= 1;
        inner.push(self.dangling(list.close));
        let close = if count == 0 && !self.has_dangling(list.close) {
            text(" ")
        } else if multiline {
            Doc::HardLine
        } else {
            Doc::Line
        };
        parts.push(group(concat(vec![
            self.tok(list.open),
            indent(concat(inner)),
            close,
            self.closer(list.close, TextKind::Closer),
        ])));
        parts.push(self.trailing(list.close));
        concat(parts)
    }

    /// A sequence setting. `meter: 4/4` and `step: 1/8` are a time
    /// signature and a note value, so they are written without spaces.
    fn setting(&mut self, a: &Arg) -> Doc {
        let name = a
            .name
            .map(|(n, _)| &self.src[self.tokens[n].start..self.tokens[n].end]);
        match (name, &a.value) {
            (Some("meter" | "step"), Expr::Binary { lhs, op, rhs })
                if matches!(**lhs, Expr::Atom(_)) && matches!(**rhs, Expr::Atom(_)) =>
            {
                let (n, colon) = a.name.expect("matched");
                let fraction = concat(vec![self.expr(lhs), self.tok(*op), self.expr(rhs)]);
                concat(vec![self.tok(n), self.tok(colon), text(" "), fraction])
            }
            _ => self.arg(a),
        }
    }

    fn step(&mut self, step: &Step) -> Doc {
        let mut parts = vec![self.expr(&step.notes)];
        if let Some((at, v)) = &step.velocity {
            parts.push(self.tok(*at));
            parts.push(self.expr(v));
        }
        concat(parts)
    }

    fn event_decl(&mut self, e: &EventDecl) -> Doc {
        let mut parts = vec![
            self.tok(e.keyword),
            text(" "),
            self.tok(e.name),
            text(" "),
            self.tok(e.kind),
        ];
        if let Some(filters) = &e.filters {
            parts.push(self.list(filters, |f, a| f.arg(a)));
        }
        if let Some(semi) = e.semi {
            parts.push(self.comments_only(semi));
        }
        concat(parts)
    }

    fn def(&mut self, d: &Def) -> Doc {
        let mut parts = vec![self.tok(d.keyword), text(" "), self.tok(d.name)];
        if let Some(generics) = &d.generics {
            parts.push(self.list(generics, |f, &t| f.tok(t)));
        }
        parts.push(self.list(&d.params, |f, p| f.param(p)));
        parts.push(text(" "));
        parts.push(self.ty(&d.ret));
        if let Some(rate) = &d.rate {
            parts.push(text(" "));
            parts.push(self.tok(rate.at));
            parts.push(text(" "));
            parts.push(self.tok(rate.rate));
            if let Some((op, n)) = rate.factor {
                parts.push(text(" "));
                parts.push(self.tok(op));
                parts.push(text(" "));
                parts.push(self.tok(n));
            }
        }
        parts.push(text(" "));
        parts.push(self.block(&d.body, false));
        concat(parts)
    }

    fn param(&mut self, p: &Param) -> Doc {
        let mut parts = vec![
            self.tok(p.name),
            self.tok(p.colon),
            text(" "),
            self.ty(&p.ty),
        ];
        if let Some((assign, value)) = &p.default {
            parts.push(text(" "));
            parts.push(self.tok(*assign));
            parts.push(text(" "));
            parts.push(self.expr(value));
        }
        concat(parts)
    }

    fn ty(&mut self, ty: &Type) -> Doc {
        match ty {
            Type::Named(t) => self.tok(*t),
            Type::Frame {
                open,
                elem,
                semi,
                size,
                close,
            } => {
                self.nest += 1;
                let elem = self.ty(elem);
                self.nest -= 1;
                concat(vec![
                    self.tok(*open),
                    elem,
                    self.tok(*semi),
                    text(" "),
                    self.expr(size),
                    self.tok(*close),
                ])
            }
            Type::Fn {
                keyword,
                params,
                ret,
            } => concat(vec![
                self.tok(*keyword),
                self.list(params, |f, t| f.ty(t)),
                text(" "),
                self.ty(ret),
            ]),
        }
    }

    /// A block may sit on one line, as in `{ x }`, when it holds a single
    /// expression and `inline` allows it. Empty blocks are `{}`.
    fn block(&mut self, b: &Block, inline: bool) -> Doc {
        let saved = std::mem::replace(&mut self.nest, 0);
        let doc = if b.stmts.is_empty() && !self.has_dangling(b.close) {
            concat(vec![
                self.tok(b.open),
                self.closer(b.close, TextKind::BlockClose),
                self.trailing(b.close),
            ])
        } else {
            let one_expr = b.stmts.len() == 1 && matches!(b.stmts[0].0, Stmt::Expr(_));
            let sep = if inline && one_expr && !self.has_dangling(b.close) {
                Doc::Line
            } else {
                Doc::HardLine
            };
            concat(vec![
                self.tok(b.open),
                indent(concat(vec![
                    sep.clone(),
                    self.stmts(&b.stmts),
                    self.dangling(b.close),
                ])),
                sep,
                self.closer(b.close, TextKind::BlockClose),
                self.trailing(b.close),
            ])
        };
        self.nest = saved;
        doc
    }

    /// The statements of a block, one per line. A statement that takes
    /// more than one line gets a blank line before and after it, except at
    /// the start and end of the block.
    fn stmts(&mut self, stmts: &[(Stmt, Option<T>)]) -> Doc {
        let mut parts = Vec::new();
        for (i, (stmt, semi)) in stmts.iter().enumerate() {
            if i > 0 {
                parts.push(Doc::HardLine);
                if self.tokens[stmt.first()].lead_blank() {
                    parts.push(Doc::HardLine);
                }
            }
            parts.push(Doc::StmtStart { blank: i > 0 });
            // Comments above a statement do not make it span lines.
            parts.push(self.detach(stmt.first()));
            parts.push(Doc::StmtCode);
            parts.push(self.stmt(stmt));
            if let Some(semi) = semi {
                parts.push(self.comments_only(*semi));
            }
            parts.push(Doc::StmtEnd {
                blank: i + 1 < stmts.len(),
            });
        }
        concat(parts)
    }

    fn stmt(&mut self, stmt: &Stmt) -> Doc {
        match stmt {
            Stmt::Binding {
                keyword,
                name,
                ty,
                assign,
                value,
            } => {
                let mut parts = vec![self.tok(*keyword), text(" "), self.tok(*name)];
                if let Some((colon, ty)) = ty {
                    parts.push(self.tok(*colon));
                    parts.push(text(" "));
                    parts.push(self.ty(ty));
                }
                if let Some(assign) = assign {
                    parts.push(text(" "));
                    parts.push(self.tok(*assign));
                    parts.push(text(" "));
                    parts.push(self.expr(value.as_ref().expect("value after `=`")));
                }
                concat(parts)
            }
            Stmt::Assign {
                target,
                assign,
                value,
            } => concat(vec![
                self.expr(target),
                text(" "),
                self.tok(*assign),
                text(" "),
                self.expr(value),
            ]),
            Stmt::Return { keyword, value } => {
                concat(vec![self.tok(*keyword), text(" "), self.expr(value)])
            }
            Stmt::Handler {
                keyword,
                name,
                params,
                mode,
                body,
            } => {
                let mut parts = vec![self.tok(*keyword), text(" "), self.tok(*name)];
                if let Some(params) = params {
                    parts.push(self.list(params, |f, &t| f.tok(t)));
                }
                match mode {
                    Some(Mode::Claim { keyword, args }) => {
                        parts.push(text(" "));
                        parts.push(self.tok(*keyword));
                        if let Some(args) = args {
                            parts.push(self.list(args, |f, a| f.arg(a)));
                        }
                    }
                    Some(Mode::Release(t)) => {
                        parts.push(text(" "));
                        parts.push(self.tok(*t));
                    }
                    None => {}
                }
                parts.push(text(" "));
                parts.push(self.block(body, false));
                concat(parts)
            }
            Stmt::For {
                keyword,
                name,
                in_kw,
                iter,
                body,
            } => concat(vec![
                self.tok(*keyword),
                text(" "),
                self.tok(*name),
                text(" "),
                self.tok(*in_kw),
                text(" "),
                self.expr(iter),
                text(" "),
                self.block(body, false),
            ]),
            Stmt::Expr(e) => self.expr(e),
        }
    }

    fn expr(&mut self, e: &Expr) -> Doc {
        let lead = self.detach(e.first());
        concat(vec![lead, self.expr_inner(e)])
    }

    fn expr_inner(&mut self, e: &Expr) -> Doc {
        match e {
            Expr::Atom(t) => self.tok(*t),
            Expr::Unary { op, operand } => concat(vec![self.tok(*op), self.expr(operand)]),
            Expr::Binary { .. } => self.binary(e),
            Expr::Range { start, op, end } => {
                concat(vec![self.expr(start), self.tok(*op), self.expr(end)])
            }
            Expr::Call {
                callee,
                sizes,
                args,
            } => {
                let mut parts = vec![self.tok(*callee)];
                if let Some(sizes) = sizes {
                    parts.push(self.list(sizes, |f, e| f.expr(e)));
                }
                parts.push(self.list(args, |f, a| f.arg(a)));
                concat(parts)
            }
            Expr::Pipe { .. } => self.pipe(e),
            Expr::Paren { open, inner, close } => {
                self.nest += 1;
                // The parentheses already indent a chain they hold alone.
                let inner = match **inner {
                    Expr::Binary { .. } => self.binary_chain(inner, true, false),
                    _ => self.expr(inner),
                };
                self.nest -= 1;
                concat(vec![
                    group(concat(vec![
                        self.tok(*open),
                        indent(concat(vec![Doc::SoftLine, inner, self.dangling(*close)])),
                        Doc::SoftLine,
                        self.closer(*close, TextKind::Closer),
                    ])),
                    self.trailing(*close),
                ])
            }
            Expr::Frame(list) => self.list(list, |f, e| f.expr(e)),
            Expr::Repeat {
                open,
                value,
                semi,
                count,
                close,
            } => concat(vec![
                self.tok(*open),
                self.expr(value),
                self.tok(*semi),
                text(" "),
                self.expr(count),
                self.tok(*close),
            ]),
            Expr::Invoke {
                keyword,
                step,
                id,
                target,
                args,
            } => {
                let mut parts = vec![self.tok(*keyword)];
                for x in [step, id].into_iter().flatten() {
                    parts.push(text(" "));
                    parts.push(self.expr(x));
                }
                parts.push(text(" "));
                parts.push(self.tok(*target));
                if let Some(args) = args {
                    parts.push(self.list(args, |f, a| f.arg(a)));
                }
                concat(parts)
            }
            Expr::Index {
                value,
                open,
                index,
                close,
            } => {
                let value = self.expr(value);
                self.nest += 1;
                let index = self.expr(index);
                self.nest -= 1;
                concat(vec![
                    value,
                    group(concat(vec![
                        self.tok(*open),
                        indent(concat(vec![Doc::SoftLine, index, self.dangling(*close)])),
                        Doc::SoftLine,
                        self.closer(*close, TextKind::Closer),
                    ])),
                    self.trailing(*close),
                ])
            }
            Expr::Cast { value, keyword, ty } => concat(vec![
                self.expr(value),
                text(" "),
                self.tok(*keyword),
                text(" "),
                self.ty(ty),
            ]),
            Expr::Field { value, dot, name } => {
                concat(vec![self.expr(value), self.tok(*dot), self.tok(*name)])
            }
            Expr::If(i) => {
                let mut parts = Vec::new();
                self.if_chain(i, &mut parts);
                group(concat(parts))
            }
            Expr::Block(b) => group(self.block(b, true)),
            Expr::Lambda {
                keyword,
                params,
                ret,
                body,
            } => {
                let mut parts = vec![
                    self.tok(*keyword),
                    self.list(params, |f, p| {
                        let mut parts = vec![f.tok(p.name)];
                        if let Some((colon, ty)) = &p.ty {
                            parts.push(f.tok(*colon));
                            parts.push(text(" "));
                            parts.push(f.ty(ty));
                        }
                        concat(parts)
                    }),
                ];
                if let Some(ty) = ret {
                    parts.push(text(" "));
                    parts.push(self.ty(ty));
                }
                parts.push(text(" "));
                parts.push(self.block(body, true));
                group(concat(parts))
            }
        }
    }

    /// `if c { a } else if d { b } else { c }`. All the blocks share one
    /// group, so they are either all on one line or all broken.
    fn if_chain(&mut self, i: &If, parts: &mut Vec<Doc>) {
        parts.push(self.tok(i.keyword));
        parts.push(text(" "));
        let saved = std::mem::replace(&mut self.nest, 0);
        parts.push(self.expr(&i.cond));
        self.nest = saved;
        parts.push(text(" "));
        parts.push(self.block(&i.then, true));
        if let Some((kw, els)) = &i.els {
            parts.push(text(" "));
            parts.push(self.tok(*kw));
            parts.push(text(" "));
            match els {
                Else::If(i) => self.if_chain(i, parts),
                Else::Block(b) => parts.push(self.block(b, true)),
            }
        }
    }

    fn arg(&mut self, a: &Arg) -> Doc {
        let mut parts = Vec::new();
        if let Some((name, colon)) = a.name {
            parts.extend([self.tok(name), self.tok(colon), text(" ")]);
        }
        if let Some(each) = a.each {
            parts.extend([self.tok(each), text(" ")]);
        }
        parts.push(self.expr(&a.value));
        concat(parts)
    }

    /// `input |> f |> g(x)`, breaking before every `|>` if it does not fit.
    fn pipe(&mut self, e: &Expr) -> Doc {
        let mut stages = Vec::new();
        let mut input = e;
        while let Expr::Pipe {
            input: inner,
            pipe,
            callee,
            sizes,
            args,
        } = input
        {
            stages.push((*pipe, *callee, sizes, args));
            input = inner;
        }
        let first = self.expr(input);
        let mut rest = Vec::new();
        for (pipe, callee, sizes, args) in stages.into_iter().rev() {
            rest.push(Doc::Line);
            rest.push(self.tok(pipe));
            rest.push(text(" "));
            rest.push(self.tok(callee));
            if let Some(sizes) = sizes {
                rest.push(self.list(sizes, |f, e| f.expr(e)));
            }
            if let Some(args) = args {
                rest.push(self.list(args, |f, a| f.arg(a)));
            }
        }
        group(concat(vec![first, indent(concat(rest))]))
    }

    /// A chain of binary operators of one precedence, as in `a + b - c`.
    fn binary(&mut self, e: &Expr) -> Doc {
        if self.nest > 0 {
            return self.binary_chain(e, true, true);
        }
        // Outside brackets a line break before an operator would end the
        // statement, so try laying the chain out as if it were in
        // parentheses, and add them only if it has to break.
        let before = self.detached.borrow().clone();
        self.nest += 1;
        let chain = self.binary_chain(e, true, false);
        self.nest -= 1;
        if chain.forces_break() {
            // Something inside already breaks the line, so parentheses would
            // not help; keep the operators on their lines. The first attempt
            // is thrown away, so its comments are placed again.
            *self.detached.borrow_mut() = before;
            return self.binary_chain(e, false, true);
        }
        group(concat(vec![
            if_break(text("("), Doc::Nil),
            indent(concat(vec![Doc::SoftLine, chain])),
            Doc::SoftLine,
            if_break(Doc::Text(")".into(), TextKind::Closer), Doc::Nil),
        ]))
    }

    /// With `hang`, the lines after the first are indented one level more.
    fn binary_chain(&mut self, e: &Expr, breakable: bool, hang: bool) -> Doc {
        let Expr::Binary { op, .. } = e else {
            unreachable!()
        };
        let level = binop_level(self.tokens[*op].kind);
        let mut operands = Vec::new();
        let mut first = e;
        while let Expr::Binary { lhs, op, rhs } = first
            && binop_level(self.tokens[*op].kind) == level
        {
            operands.push((*op, &**rhs));
            first = lhs;
        }
        let mut rest = Vec::new();
        let first = self.expr(first);
        for (op, rhs) in operands.into_iter().rev() {
            rest.push(if breakable { Doc::Line } else { text(" ") });
            rest.push(self.tok(op));
            rest.push(text(" "));
            rest.push(self.expr(rhs));
        }
        let rest = concat(rest);
        group(concat(vec![first, if hang { indent(rest) } else { rest }]))
    }
}
