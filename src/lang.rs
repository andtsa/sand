//! rust types for the language

pub struct Program(Vec<Function>);

#[derive(Debug, Clone, Copy)]
pub enum Ty {
    Int,
    Bool,
    Unit,
}

pub struct Parameter {
    pub name: String,
    pub ty: Ty,
}

pub struct Function {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub ret_type: Ty,
    pub body: Vec<Statement>,
}

pub enum Statement {
    Declaration {
        name: String,
        ty: Ty,
        val: Expression,
    },

    Assignment {
        name: String,
        val: Expression,
    },

    Expr(Expression),
}

pub enum Expression {
    If {
        cond: Box<Expression>,
        t: Box<Expression>,
        f: Option<Box<Expression>>,
    },
    BinOp {
        left: Box<Expression>,
        op: Bop,
        right: Box<Expression>,
    },
    UnOp {
        op: Uop,
        right: Box<Expression>,
    },
    Call {
        fn_name: String,
        args: Vec<Expression>,
    },
    Var(String),
    Int(i32),
    Bool(bool),
    Unit,
}

#[derive(Debug, Clone, Copy)]
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

#[derive(Debug, Clone, Copy)]
pub enum CompOp {
    Ge,
    Le,
    Eq,
    Gt,
    Lt,
}

#[derive(Debug, Clone, Copy)]
pub enum Uop {
    Neg,
    Not,
}
