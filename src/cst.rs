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
}

/// A bracketed, comma-separated list. Each item keeps the comma after it.
pub struct List<X> {
    pub open: T,
    pub items: Vec<(X, Option<T>)>,
    pub close: T,
}

/// `fn name<N>(params) -> Type @ rate / 2 { ... }`, or the same with `rill`.
pub struct Def {
    pub keyword: T,
    pub name: T,
    pub generics: Option<List<T>>,
    pub params: List<Param>,
    pub arrow: T,
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

/// `event note_on(note)`
pub struct EventDecl {
    pub keyword: T,
    pub name: T,
    pub params: Option<List<T>>,
    pub semi: Option<T>,
}

pub enum Type {
    Named(T),
    /// `[elem; size]`
    Frame {
        open: T,
        elem: Box<Type>,
        semi: T,
        size: T,
        close: T,
    },
    /// `fn(A, B) -> R`
    Fn {
        keyword: T,
        params: List<Type>,
        arrow: T,
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
    /// `let` or `state`.
    Binding {
        keyword: T,
        name: T,
        ty: Option<(T, Type)>,
        assign: T,
        value: Expr,
    },
    Assign {
        target: T,
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
    Call {
        callee: T,
        args: List<Arg>,
    },
    /// `input |> callee(args)`
    Pipe {
        input: Box<Expr>,
        pipe: T,
        callee: T,
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
    /// `fn(p: Pitch) -> Freq { ... }`
    Lambda {
        keyword: T,
        params: List<LambdaParam>,
        ret: Option<(T, Type)>,
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
            | Stmt::Handler { keyword, .. } => *keyword,
            Stmt::Assign { target, .. } => *target,
            Stmt::Expr(e) => e.first(),
        }
    }
}

impl Expr {
    pub fn first(&self) -> T {
        match self {
            Expr::Atom(t) => *t,
            Expr::Unary { op, .. } => *op,
            Expr::Binary { lhs, .. } => lhs.first(),
            Expr::Call { callee, .. } => *callee,
            Expr::Pipe { input, .. } => input.first(),
            Expr::Paren { open, .. } => *open,
            Expr::Frame(list) => list.open,
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
        }
    }
}
