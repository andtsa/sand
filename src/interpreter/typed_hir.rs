//! a simple interpreter for the typed_hir IR

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use anyhow::anyhow;

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::UniqVar;
use crate::ir_types::typed_hir::*;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::*;

impl TypedProgram {
    /// run the main function of the program and return an expression
    /// that's either Int, Bool, or Unit
    pub fn interpret(&self, ctx: &CompileCtx) -> anyhow::Result<Expression> {
        // find the main function
        let (_, main_fn) = self
            .functions
            .iter()
            .find(|(f, _)| ctx.is_main(**f))
            .ok_or_else(|| anyhow!("no main function found"))?;

        // empty environment
        let env = Env::new();
        // just evaluate the body of the main function
        self.evaluate(&main_fn.body.expr, &env)
    }
}

// A reference-counted, interior-mutable environment handle
pub type EnvRef = Rc<RefCell<Env>>;

#[derive(Debug)]
pub struct Env {
    data: BTreeMap<UniqVar, Expression>,
    /// pointer to the outer environment
    outer: Option<EnvRef>,
}

impl Env {
    fn new() -> EnvRef {
        Rc::new(RefCell::new(Env {
            data: BTreeMap::new(),
            outer: None,
        }))
    }

    fn with_outer(outer: &EnvRef) -> EnvRef {
        Rc::new(RefCell::new(Env {
            data: BTreeMap::new(),
            outer: Some(Rc::clone(outer)),
        }))
    }

    fn assign(&mut self, name: UniqVar, val: Expression) -> anyhow::Result<()> {
        #[allow(clippy::map_entry)]
        if self.data.contains_key(&name) {
            self.data.insert(name, val);
            Ok(())
        } else if let Some(ref outer) = self.outer {
            outer.borrow_mut().assign(name, val)
        } else {
            Err(anyhow::anyhow!("variable not found: {:?}", name))
        }
    }

    fn get(&self, name: &UniqVar) -> Option<Expression> {
        if let Some(v) = self.data.get(name) {
            Some(v.clone())
        } else if let Some(ref outer) = self.outer {
            outer.borrow().get(name)
        } else {
            None
        }
    }

    fn add_variable(&mut self, name: UniqVar, val: Expression) {
        self.data.insert(name, val);
    }
}

impl Expr {
    pub fn evaluate(&self, prog: &TypedProgram, env: &EnvRef) -> anyhow::Result<Expression> {
        prog.evaluate(&self.expr, env)
    }
}

impl TypedProgram {
    /// evaluate the expression and return the resulting expression
    pub fn evaluate(&self, expr: &Expression, env: &EnvRef) -> anyhow::Result<Expression> {
        match expr {
            Expression::If { cond, t, f } => {
                let cond_val = cond.evaluate(self, env)?;
                match cond_val {
                    Expression::Bool(true) => t.evaluate(self, env),
                    Expression::Bool(false) => f.evaluate(self, env),
                    e => Err(anyhow!(
                        "condition {cond:?} must evaluate to a boolean, got {e:?}"
                    )),
                }
            }
            Expression::While { cond, body } => {
                let mut result = Expression::Unit;
                loop {
                    let cond_val = cond.evaluate(self, env)?;
                    match cond_val {
                        Expression::Bool(true) => {
                            result = body.evaluate(self, env)?;
                        }
                        Expression::Bool(false) => break,
                        e => {
                            return Err(anyhow!(
                                "condition {cond:?} must evaluate to a boolean, got {e:?}"
                            ));
                        }
                    }
                }
                Ok(result)
            }
            Expression::BinOp { left, op, right } => {
                let left_val = left.evaluate(self, env)?;
                let right_val = right.evaluate(self, env)?;
                match (left_val, right_val, op) {
                    (Expression::Int(l), Expression::Int(r), Bop::Plus) => {
                        Ok(Expression::Int(l + r))
                    }
                    (Expression::Int(l), Expression::Int(r), Bop::Minus) => {
                        Ok(Expression::Int(l - r))
                    }
                    (Expression::Int(l), Expression::Int(r), Bop::Mult) => {
                        Ok(Expression::Int(l * r))
                    }
                    (Expression::Int(l), Expression::Int(r), Bop::Div) => {
                        Ok(Expression::Int(l / r))
                    }
                    (Expression::Int(l), Expression::Int(r), Bop::Pow) => {
                        Ok(Expression::Int(l.pow(r as u32)))
                    }
                    (Expression::Int(l), Expression::Int(r), Bop::Comp(cop)) => match cop {
                        CompOp::Eq => Ok(Expression::Bool(l == r)),
                        CompOp::Ne => Ok(Expression::Bool(l != r)),
                        CompOp::Lt => Ok(Expression::Bool(l < r)),
                        CompOp::Le => Ok(Expression::Bool(l <= r)),
                        CompOp::Gt => Ok(Expression::Bool(l > r)),
                        CompOp::Ge => Ok(Expression::Bool(l >= r)),
                    },
                    (Expression::Bool(l), Expression::Bool(r), Bop::And) => {
                        Ok(Expression::Bool(l && r))
                    }
                    (Expression::Bool(l), Expression::Bool(r), Bop::Or) => {
                        Ok(Expression::Bool(l || r))
                    }
                    (Expression::Bool(l), Expression::Bool(r), Bop::Comp(CompOp::Eq)) => {
                        Ok(Expression::Bool(l == r))
                    }
                    (Expression::Bool(l), Expression::Bool(r), Bop::Comp(CompOp::Ne)) => {
                        Ok(Expression::Bool(l != r))
                    }
                    (x, y, z) => Err(anyhow!(
                        "type error in binary operation: {left:?} {op:?} {right:?}, got {x:?} {y:?} {z:?}"
                    )),
                }
            }
            Expression::UnOp { op, right } => {
                let val = right.evaluate(self, env)?;
                match (val, op) {
                    (Expression::Bool(b), Uop::Not) => Ok(Expression::Bool(!b)),
                    (Expression::Int(n), Uop::Neg) => Ok(Expression::Int(-n)),
                    (x, y) => Err(anyhow!(
                        "type error in unary operation: {op:?} {right:?}, got {y:?} {x:?}"
                    )),
                }
            }
            Expression::Int(n) => Ok(Expression::Int(*n)),
            Expression::Bool(b) => Ok(Expression::Bool(*b)),
            Expression::Unit => Ok(Expression::Unit),

            Expression::Var(name) => {
                if let Some(val) = env.borrow().get(name) {
                    Ok(val)
                } else {
                    Err(anyhow!("undefined variable: {:?}", name))
                }
            }

            Expression::Block { statements, expr } => {
                let local_env = Env::with_outer(env);
                let mut ret_expr = Expression::Unit;
                for stmt in statements {
                    match stmt {
                        Statement::Declaration { name, val, .. } => {
                            let evaluated_val = val.evaluate(self, &local_env)?;
                            local_env.borrow_mut().add_variable(*name, evaluated_val);
                        }
                        Statement::Assignment { name, val, .. } => {
                            let evaluated_val = val.evaluate(self, &local_env)?;
                            local_env.borrow_mut().assign(*name, evaluated_val)?;
                        }
                        Statement::Expr(e) => {
                            ret_expr = e.evaluate(self, &local_env)?;
                        }
                    }
                }
                if let Some(e) = expr {
                    e.evaluate(self, &local_env)
                } else {
                    Ok(ret_expr)
                }
            }
            Expression::Call { fn_name, args } => {
                let function = &self.functions[fn_name];

                if args.len() != function.parameters.len() {
                    return Err(anyhow!(
                        "function {:?} expects {} arguments, got {}",
                        function.name,
                        function.parameters.len(),
                        args.len()
                    ));
                }

                let local_env = Env::new();

                // evaluate arguments and bind to parameters
                for (param, arg) in function.parameters.iter().zip(args.iter()) {
                    let arg_val = arg.evaluate(self, env)?;
                    local_env.borrow_mut().add_variable(param.name, arg_val);
                }

                // evaluate function body
                function.body.evaluate(self, &local_env)
            }
            Expression::IntrinsicCall { fn_name, args } => match fn_name {
                Intrinsic::Println => {
                    for arg in args {
                        let val = arg.evaluate(self, env)?;
                        print!("{:?} ", val);
                    }
                    println!();
                    Ok(Expression::Unit)
                }
                Intrinsic::Print => {
                    for arg in args {
                        let val = arg.evaluate(self, env)?;
                        print!("{:?} ", val);
                    }
                    Ok(Expression::Unit)
                }
            },
        }
    }
}
