//! rust types for the language

#[derive(Debug, Clone)]
pub struct Program(pub Vec<Function>);

#[derive(Debug, Clone, Copy)]
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
    pub body: Expression,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub enum Expression {
    If {
        cond: Box<Expression>,
        t: Box<Expression>,
        f: Box<Expression>,
    },
    While {
        cond: Box<Expression>,
        body: Box<Expression>,
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
    Block {
        statements: Vec<Statement>,
        expr: Option<Box<Expression>>,
    },
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
    Ne,
    Gt,
    Lt,
}

#[derive(Debug, Clone, Copy)]
pub enum Uop {
    Neg,
    Not,
}
