//! Proof that formatting changed only layout.
//!
//! The formatter may move line breaks and spaces, and it may add or drop
//! the few tokens that carry no meaning: trailing commas, `;`, and
//! parentheses around a whole operator chain. Everything else must survive:
//! the syntax tree, read back without those tokens, must be identical, and
//! every comment must still be there word for word (long comments are
//! re-wrapped, and comments too long for their line move above it, so only
//! the words are compared, not where the line breaks fall).

use crate::cst::*;
use crate::lexer::Token;

/// Check that `after` is `before` with only its layout changed.
pub fn same_program(before: &str, after: &str) -> Result<(), String> {
    let (tree_before, comments_before) = summary(before, "the input")?;
    let (tree_after, comments_after) = summary(after, "the formatted output")?;
    if tree_before != tree_after {
        let at = tree_before
            .iter()
            .zip(&tree_after)
            .position(|(a, b)| a != b)
            .unwrap_or(tree_before.len().min(tree_after.len()));
        let show = |tree: &[String]| tree[at.saturating_sub(3)..(at + 4).min(tree.len())].join(" ");
        return Err(format!(
            "the syntax tree changed\n  before: … {} …\n  after:  … {} …",
            show(&tree_before),
            show(&tree_after)
        ));
    }
    let missing = difference(&comments_before, &comments_after);
    let added = difference(&comments_after, &comments_before);
    if !missing.is_empty() || !added.is_empty() {
        let mut message = String::from("the comments changed");
        if !missing.is_empty() {
            message += &format!("\n  missing: {}", missing.join(" "));
        }
        if !added.is_empty() {
            message += &format!("\n  added:   {}", added.join(" "));
        }
        return Err(message);
    }
    Ok(())
}

/// The tree as a flat list of node names and token texts, and every comment
/// word.
fn summary(src: &str, what: &str) -> Result<(Vec<String>, Vec<String>), String> {
    let tokens = crate::lexer::lex(src).map_err(|e| format!("cannot read {what}: {e}"))?;
    let file =
        crate::parser::parse(src, &tokens).map_err(|e| format!("cannot read {what}: {e}"))?;
    let mut w = Walk {
        src,
        tokens: &tokens,
        out: Vec::new(),
    };
    w.file(&file);
    let mut words: Vec<String> = tokens
        .iter()
        .flat_map(|t| t.leading.iter().chain(&t.trailing))
        .flat_map(|c| c.lines.iter())
        .flat_map(|line| line.split_whitespace())
        // Wrapping repeats the `//` marker, so markers are not words.
        .map(|w| w.trim_start_matches(['/', '!']).trim_end_matches("*/"))
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect();
    words.sort();
    Ok((w.out, words))
}

/// Words in `a` that `b` lacks, counting repeats. Both are sorted.
fn difference(a: &[String], b: &[String]) -> Vec<String> {
    let mut missing = Vec::new();
    let mut j = 0;
    for word in a {
        while j < b.len() && b[j] < *word {
            j += 1;
        }
        if j < b.len() && b[j] == *word {
            j += 1;
        } else {
            missing.push(word.clone());
        }
    }
    missing
}

struct Walk<'a> {
    src: &'a str,
    tokens: &'a [Token],
    out: Vec<String>,
}

impl Walk<'_> {
    fn tok(&mut self, t: T) {
        let token = &self.tokens[t];
        self.out.push(self.src[token.start..token.end].to_owned());
    }

    fn open(&mut self, node: &str) {
        self.out.push(format!("({node}"));
    }

    fn close(&mut self) {
        self.out.push(")".to_owned());
    }

    /// The items of a list, without its brackets or commas.
    fn list<X>(&mut self, node: &str, list: &List<X>, mut item: impl FnMut(&mut Self, &X)) {
        self.open(node);
        for (x, _comma) in &list.items {
            item(self, x);
        }
        self.close();
    }

    fn file(&mut self, file: &File) {
        for item in &file.items {
            match item {
                Item::Def(d) => self.def(d),
                Item::Event(e) => {
                    self.open("event");
                    self.tok(e.name);
                    if let Some(params) = &e.params {
                        self.list("params", params, |w, &t| w.tok(t));
                    }
                    self.close();
                }
            }
        }
    }

    fn def(&mut self, d: &Def) {
        self.open("def");
        self.tok(d.keyword);
        self.tok(d.name);
        if let Some(generics) = &d.generics {
            self.list("generics", generics, |w, &t| w.tok(t));
        }
        self.list("params", &d.params, |w, p| {
            w.open("param");
            w.tok(p.name);
            w.ty(&p.ty);
            if let Some((_, value)) = &p.default {
                w.expr(value);
            }
            w.close();
        });
        self.ty(&d.ret);
        if let Some(rate) = &d.rate {
            self.open("rate");
            if let Some((op, n)) = rate.factor {
                self.tok(op);
                self.tok(n);
            }
            self.close();
        }
        self.block(&d.body);
        self.close();
    }

    fn ty(&mut self, ty: &Type) {
        match ty {
            Type::Named(t) => self.tok(*t),
            Type::Frame { elem, size, .. } => {
                self.open("frame-type");
                self.ty(elem);
                self.tok(*size);
                self.close();
            }
            Type::Fn { params, ret, .. } => {
                self.open("fn-type");
                self.list("params", params, |w, t| w.ty(t));
                self.ty(ret);
                self.close();
            }
        }
    }

    fn block(&mut self, b: &Block) {
        self.open("block");
        for (stmt, _semi) in &b.stmts {
            self.stmt(stmt);
        }
        self.close();
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Binding {
                keyword,
                name,
                ty,
                value,
                ..
            } => {
                self.open("binding");
                self.tok(*keyword);
                self.tok(*name);
                if let Some((_, ty)) = ty {
                    self.ty(ty);
                }
                self.expr(value);
                self.close();
            }
            Stmt::Assign { target, value, .. } => {
                self.open("assign");
                self.tok(*target);
                self.expr(value);
                self.close();
            }
            Stmt::Return { value, .. } => {
                self.open("return");
                self.expr(value);
                self.close();
            }
            Stmt::Handler {
                name, params, body, ..
            } => {
                self.open("on");
                self.tok(*name);
                if let Some(params) = params {
                    self.list("params", params, |w, &t| w.tok(t));
                }
                self.block(body);
                self.close();
            }
            Stmt::Expr(e) => self.expr(e),
        }
    }

    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Atom(t) => self.tok(*t),
            // The tree already says how things group.
            Expr::Paren { inner, .. } => self.expr(inner),
            Expr::Unary { op, operand } => {
                self.open("unary");
                self.tok(*op);
                self.expr(operand);
                self.close();
            }
            Expr::Binary { lhs, op, rhs } => {
                self.open("binary");
                self.expr(lhs);
                self.tok(*op);
                self.expr(rhs);
                self.close();
            }
            Expr::Call { callee, args } => {
                self.open("call");
                self.tok(*callee);
                self.list("args", args, |w, a| w.arg(a));
                self.close();
            }
            Expr::Pipe {
                input,
                callee,
                args,
                ..
            } => {
                self.open("pipe");
                self.expr(input);
                self.tok(*callee);
                if let Some(args) = args {
                    self.list("args", args, |w, a| w.arg(a));
                }
                self.close();
            }
            Expr::Frame(list) => self.list("frame", list, |w, e| w.expr(e)),
            Expr::Index { value, index, .. } => {
                self.open("index");
                self.expr(value);
                self.expr(index);
                self.close();
            }
            Expr::Field { value, name, .. } => {
                self.open("field");
                self.expr(value);
                self.tok(*name);
                self.close();
            }
            Expr::Cast { value, ty, .. } => {
                self.open("cast");
                self.expr(value);
                self.ty(ty);
                self.close();
            }
            Expr::If(i) => self.if_expr(i),
            Expr::Block(b) => self.block(b),
            Expr::Lambda {
                params, ret, body, ..
            } => {
                self.open("lambda");
                self.list("params", params, |w, p| {
                    w.tok(p.name);
                    if let Some((_, ty)) = &p.ty {
                        w.ty(ty);
                    }
                });
                if let Some(ty) = ret {
                    self.ty(ty);
                }
                self.block(body);
                self.close();
            }
        }
    }

    fn if_expr(&mut self, i: &If) {
        self.open("if");
        self.expr(&i.cond);
        self.block(&i.then);
        match &i.els {
            Some((_, Else::If(i))) => self.if_expr(i),
            Some((_, Else::Block(b))) => self.block(b),
            None => {}
        }
        self.close();
    }

    fn arg(&mut self, a: &Arg) {
        if let Some((name, _)) = a.name {
            self.tok(name);
            self.out.push(":".to_owned());
        }
        self.expr(&a.value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_and_optional_tokens_are_allowed() {
        let before = "rill m() S { let a = f(1, 2,); return a + b; } // note";
        let after =
            "rill m() S {\n    let a = f(1, 2)\n    return (\n        a + b\n    )\n} // note\n";
        assert_eq!(same_program(before, after), Ok(()));
    }

    #[test]
    fn rewrapped_comments_are_the_same_comments() {
        let before = "// one two three four\nrill m() S {}";
        let after = "// one two\n// three four\nrill m() S {}";
        assert_eq!(same_program(before, after), Ok(()));
    }

    #[test]
    fn changed_code_is_caught() {
        let err = same_program(
            "rill m() S { return a - b }",
            "rill m() S { return a + b }",
        )
        .unwrap_err();
        assert!(err.starts_with("the syntax tree changed"), "{err}");
        // Moving parentheses changes how things group.
        assert!(
            same_program(
                "rill m() S { return (a + b) * c }",
                "rill m() S { return a + b * c }"
            )
            .is_err()
        );
    }

    #[test]
    fn dropped_comments_are_caught() {
        let err = same_program(
            "rill m() S { return 0 } // keep me",
            "rill m() S { return 0 }",
        )
        .unwrap_err();
        assert_eq!(err, "the comments changed\n  missing: keep me");
    }
}
