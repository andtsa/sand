//! rust types for the language

#[derive(Debug, Clone)]
pub struct Program(pub Vec<Function>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ty {
    Int,
    Bool,
    Unit,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub ty: Ty,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub ret_type: Ty,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Statement {
    Declaration { name: String, ty: Ty, val: Expr },

    Assignment { name: String, val: Expr },

    Expr(Expr),
}

/// `Expr` wraps an `Expression` and carries start/end positions (line,col)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Expr {
    pub expr: Expression,
    pub start: (usize, usize),
    pub end: (usize, usize),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Expression {
    If {
        cond: Box<Expr>,
        t: Box<Expr>,
        f: Box<Expr>,
    },
    While {
        cond: Box<Expr>,
        body: Box<Expr>,
    },
    BinOp {
        left: Box<Expr>,
        op: Bop,
        right: Box<Expr>,
    },
    UnOp {
        op: Uop,
        right: Box<Expr>,
    },
    Call {
        fn_name: String,
        args: Vec<Expr>,
    },
    Var(String),
    Int(i64),
    Bool(bool),
    Unit,
    Block {
        statements: Vec<Statement>,
        expr: Option<Box<Expr>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Bop {
    Plus,
    Minus,
    Mult,
    Div,
    Pow,
    And,
    Or,
    Xor,
    Comp(CompOp),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompOp {
    Ge,
    Le,
    Eq,
    Ne,
    Gt,
    Lt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Uop {
    Neg,
    Not,
}
