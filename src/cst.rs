//! Concrete syntax tree. Unlike the compiler's tree it keeps every token,
//! as an index into the token list, so comments and literal text survive.

/// Index of a token.
pub type T = usize;

pub struct File {
    pub items: Vec<Item>,
    pub eof: T,
}

pub enum Item {
    Def(Def),
    Event(EventDecl),
    Seq(SeqDecl),
    /// `const NAME: Type = value`: a [`Stmt::Binding`], and the `;` after it.
    Const(Stmt, Option<T>),
    /// `import "lib/osc"`
    Import {
        keyword: T,
        path: T,
        semi: Option<T>,
    },
    /// `export` before another item.
    Export {
        keyword: T,
        item: Box<Item>,
    },
}

/// A bracketed, comma-separated list. Each item keeps the comma after it.
pub struct List<X> {
    pub open: T,
    pub items: Vec<(X, Option<T>)>,
    pub close: T,
}

/// `fn name<N>(params) Type @ rate / 2 { ... }`, or the same with `rill`.
pub struct Def {
    pub keyword: T,
    pub name: T,
    pub generics: Option<List<T>>,
    pub params: List<Param>,
    pub ret: Type,
    pub rate: Option<Rate>,
    pub body: Block,
}

pub struct Param {
    pub name: T,
    pub colon: T,
    pub ty: Type,
    /// `= value`
    pub default: Option<(T, Expr)>,
}

/// `@ rate`, `@ rate / 2`, `@ rate * 2`
pub struct Rate {
    pub at: T,
    pub rate: T,
    pub factor: Option<(T, T)>,
}

/// `event keys note_on(sender: 5, channel: 1)`
pub struct EventDecl {
    pub keyword: T,
    pub name: T,
    pub kind: T,
    /// `(sender: 5, channel: 1)`; always named.
    pub filters: Option<List<Arg>>,
    pub semi: Option<T>,
}

/// `seq riff(step: 1/8) { C4, _, E4@0.5, [G4, B4] }`
pub struct SeqDecl {
    pub keyword: T,
    pub name: T,
    /// `(step: 1/8)`; always named.
    pub settings: Option<List<Arg>>,
    pub steps: List<Step>,
}

/// One step of a sequence.
pub struct Step {
    /// A pitch or chord, or `_` for a rest (an [`Expr::Atom`]).
    pub notes: Expr,
    /// `@ velocity`
    pub velocity: Option<(T, Expr)>,
}

/// `claim`, `claim(tail: 2s)` or `release` after a handler's parameters.
pub enum Mode {
    Claim { keyword: T, args: Option<List<Arg>> },
    Release(T),
}

pub enum Type {
    Named(T),
    /// `[elem; size]`
    Frame {
        open: T,
        elem: Box<Type>,
        semi: T,
        size: Box<Expr>,
        close: T,
    },
    /// `fn(A, B) R`
    Fn {
        keyword: T,
        params: List<Type>,
        ret: Box<Type>,
    },
}

pub struct Block {
    pub open: T,
    /// Each statement with the `;` after it, if any.
    pub stmts: Vec<(Stmt, Option<T>)>,
    pub close: T,
}

pub enum Stmt {
    /// `let`, `state` or `const`.
    Binding {
        keyword: T,
        name: T,
        ty: Option<(T, Type)>,
        assign: Option<T>,
        value: Option<Expr>,
    },
    Assign {
        target: Expr,
        assign: T,
        value: Expr,
    },
    Return {
        keyword: T,
        value: Expr,
    },
    /// `on note_on(note) { ... }`
    Handler {
        keyword: T,
        name: T,
        params: Option<List<T>>,
        mode: Option<Mode>,
        body: Block,
    },
    /// `for x in xs { ... }`
    For {
        keyword: T,
        name: T,
        in_kw: T,
        iter: Expr,
        body: Block,
    },
    Expr(Expr),
}

pub enum Expr {
    /// A number, `true`, `false` or a name.
    Atom(T),
    Unary {
        op: T,
        operand: Box<Expr>,
    },
    Binary {
        lhs: Box<Expr>,
        op: T,
        rhs: Box<Expr>,
    },
    Range {
        start: Box<Expr>,
        op: T,
        end: Box<Expr>,
    },
    Call {
        callee: T,
        sizes: Option<List<Expr>>,
        args: List<Arg>,
    },
    /// `input |> callee(args)`
    Pipe {
        input: Box<Expr>,
        pipe: T,
        callee: T,
        sizes: Option<List<Expr>>,
        args: Option<List<Arg>>,
    },
    Paren {
        open: T,
        inner: Box<Expr>,
        close: T,
    },
    Frame(List<Expr>),
    Index {
        value: Box<Expr>,
        open: T,
        index: Box<Expr>,
        close: T,
    },
    Field {
        value: Box<Expr>,
        dot: T,
        name: T,
    },
    /// `value as Type`
    Cast {
        value: Box<Expr>,
        keyword: T,
        ty: Type,
    },
    If(If),
    Block(Block),
    /// `[synth(); 8]`
    Repeat {
        open: T,
        value: Box<Expr>,
        semi: T,
        count: Box<Expr>,
        close: T,
    },
    /// `invoke id riff(args)`, `trigger 3 id riff(args)`, `halt id riff`
    Invoke {
        keyword: T,
        step: Option<Box<Expr>>,
        id: Option<Box<Expr>>,
        target: T,
        args: Option<List<Arg>>,
    },
    /// `fn(p: Pitch) Freq { ... }`
    Lambda {
        keyword: T,
        params: List<LambdaParam>,
        ret: Option<Type>,
        body: Block,
    },
}

pub struct If {
    pub keyword: T,
    pub cond: Box<Expr>,
    pub then: Block,
    pub els: Option<(T, Else)>,
}

pub enum Else {
    If(Box<If>),
    Block(Block),
}

pub struct Arg {
    /// `name:`
    pub name: Option<(T, T)>,
    /// `each` before the value.
    pub each: Option<T>,
    pub value: Expr,
}

pub struct LambdaParam {
    pub name: T,
    pub ty: Option<(T, Type)>,
}

impl Stmt {
    pub fn first(&self) -> T {
        match self {
            Stmt::Binding { keyword, .. }
            | Stmt::Return { keyword, .. }
            | Stmt::Handler { keyword, .. }
            | Stmt::For { keyword, .. } => *keyword,
            Stmt::Assign { target, .. } => target.first(),
            Stmt::Expr(e) => e.first(),
        }
    }
}

impl Expr {
    pub fn first(&self) -> T {
        match self {
            Expr::Atom(t) => *t,
            Expr::Unary { op, .. } => *op,
            Expr::Binary { lhs, .. } | Expr::Range { start: lhs, .. } => lhs.first(),
            Expr::Call { callee, .. } => *callee,
            Expr::Pipe { input, .. } => input.first(),
            Expr::Paren { open, .. } => *open,
            Expr::Frame(list) => list.open,
            Expr::Repeat { open, .. } => *open,
            Expr::Invoke { keyword, .. } => *keyword,
            Expr::Index { value, .. } | Expr::Field { value, .. } | Expr::Cast { value, .. } => {
                value.first()
            }
            Expr::If(i) => i.keyword,
            Expr::Block(b) => b.open,
            Expr::Lambda { keyword, .. } => *keyword,
        }
    }
}

impl Item {
    pub fn first(&self) -> T {
        match self {
            Item::Def(d) => d.keyword,
            Item::Event(e) => e.keyword,
            Item::Seq(s) => s.keyword,
            Item::Const(Stmt::Binding { keyword, .. }, _) => *keyword,
            Item::Const(..) => unreachable!("a `const` is a binding"),
            Item::Import { keyword, .. } | Item::Export { keyword, .. } => *keyword,
        }
    }
}
