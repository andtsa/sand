//! an interpreter for the MIR

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::internal_bug;
use crate::ir_types::cfgmir::*;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::Bop;
use crate::lang::ops::CompOp;
use crate::lang::ops::Uop;

#[derive(Debug, Clone, PartialEq)]
pub enum MirValue {
    Int(i64),
    Bool(bool),
    Unit,
}

#[derive(Debug)]
pub enum MirInterpError {
    NoEntryPoint,
    UninitializedLocal(LocalId),
    DivisionByZero,
    Unreachable,
}

impl std::fmt::Display for MirInterpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MirInterpError::NoEntryPoint => write!(f, "no entry point found"),
            MirInterpError::UninitializedLocal(id) => {
                write!(f, "uninitialized local: {:?}", id)
            }
            MirInterpError::DivisionByZero => write!(f, "division by zero"),
            MirInterpError::Unreachable => write!(f, "reached unreachable terminator"),
        }
    }
}

impl MirProgram {
    pub fn interpret(&self, ctx: &CompileCtx) -> Result<MirValue, MirInterpError> {
        // find the main function
        let (main_ref, _main_fn) = self
            .functions
            .iter()
            .find(|(fr, _)| ctx.is_main(**fr))
            .ok_or(MirInterpError::NoEntryPoint)?;

        self.call_function(*main_ref, &[])
    }

    fn call_function(&self, fun: FunRef, args: &[MirValue]) -> Result<MirValue, MirInterpError> {
        let func = &self.functions[&fun];

        // initialise locals — all unset to start
        let mut locals: Vec<Option<MirValue>> = vec![None; func.locals.len()];

        // bind parameters
        for (param, val) in func.params.iter().zip(args.iter()) {
            locals[param.local.0] = Some(val.clone());
        }

        // execute blocks
        let mut current = func.entry;
        loop {
            let block = &func.blocks[current.0];

            for stmt in &block.statements {
                execute_statement(stmt, &mut locals, self)?;
            }

            match &block.terminator {
                Terminator::Goto { target } => {
                    current = *target;
                }
                Terminator::Branch {
                    cond,
                    then_bb,
                    else_bb,
                } => match eval_operand(cond, &locals)? {
                    MirValue::Bool(true) => current = *then_bb,
                    MirValue::Bool(false) => current = *else_bb,
                    v => internal_bug!("branch condition evaluated to non-bool: {:?}", v),
                },
                Terminator::Return { value: Some(op) } => {
                    return eval_operand(op, &locals);
                }
                Terminator::Return { value: None } => {
                    return Ok(MirValue::Unit);
                }
                Terminator::Unreachable => {
                    return Err(MirInterpError::Unreachable);
                }
            }
        }
    }
}

fn execute_statement(
    stmt: &Statement,
    locals: &mut Vec<Option<MirValue>>,
    prog: &MirProgram,
) -> Result<(), MirInterpError> {
    match stmt {
        Statement::Assign { dst, value, .. } => {
            let v = eval_rvalue(value, locals, prog)?;
            locals[dst.local.0] = Some(v);
            Ok(())
        }
        Statement::Eval { value, .. } => {
            eval_rvalue(value, locals, prog)?;
            Ok(())
        }
    }
}

fn eval_rvalue(
    rv: &RValue,
    locals: &mut [Option<MirValue>],
    prog: &MirProgram,
) -> Result<MirValue, MirInterpError> {
    match rv {
        RValue::Use(op) => eval_operand(op, locals),

        RValue::BinaryOp { op, left, right } => {
            let l = eval_operand(left, locals)?;
            let r = eval_operand(right, locals)?;
            eval_binop(*op, l, r)
        }

        RValue::UnaryOp { op, right } => {
            let v = eval_operand(right, locals)?;
            eval_unop(*op, v)
        }

        RValue::Call { fn_name, args } => {
            let arg_vals = args
                .iter()
                .map(|a| eval_operand(a, locals))
                .collect::<Result<Vec<_>, _>>()?;
            prog.call_function(*fn_name, &arg_vals)
        }

        RValue::IntrinsicCall { fn_name, args } => {
            let arg_vals = args
                .iter()
                .map(|a| eval_operand(a, locals))
                .collect::<Result<Vec<_>, _>>()?;
            eval_intrinsic(*fn_name, arg_vals)
        }
    }
}

fn eval_operand(op: &Operand, locals: &[Option<MirValue>]) -> Result<MirValue, MirInterpError> {
    match op {
        Operand::Const(c) => Ok(match c {
            Constant::Int(i) => MirValue::Int(*i),
            Constant::Bool(b) => MirValue::Bool(*b),
            Constant::Unit => MirValue::Unit,
        }),
        Operand::Copy(place) => locals[place.local.0]
            .clone()
            .ok_or(MirInterpError::UninitializedLocal(place.local)),
    }
}

fn eval_binop(op: Bop, l: MirValue, r: MirValue) -> Result<MirValue, MirInterpError> {
    match (op, l, r) {
        (Bop::Plus, MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(a + b)),
        (Bop::Minus, MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(a - b)),
        (Bop::Mult, MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(a * b)),
        (Bop::Div, MirValue::Int(a), MirValue::Int(b)) => {
            if b == 0 {
                Err(MirInterpError::DivisionByZero)
            } else {
                Ok(MirValue::Int(a / b))
            }
        }
        (Bop::Pow, MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(a.pow(b as u32))),
        (Bop::And, MirValue::Bool(a), MirValue::Bool(b)) => Ok(MirValue::Bool(a && b)),
        (Bop::Or, MirValue::Bool(a), MirValue::Bool(b)) => Ok(MirValue::Bool(a || b)),
        (Bop::Xor, MirValue::Bool(a), MirValue::Bool(b)) => Ok(MirValue::Bool(a ^ b)),
        (Bop::Comp(cop), MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Bool(match cop {
            CompOp::Eq => a == b,
            CompOp::Ne => a != b,
            CompOp::Lt => a < b,
            CompOp::Le => a <= b,
            CompOp::Gt => a > b,
            CompOp::Ge => a >= b,
        })),
        (Bop::Comp(CompOp::Eq), MirValue::Bool(a), MirValue::Bool(b)) => Ok(MirValue::Bool(a == b)),
        (Bop::Comp(CompOp::Ne), MirValue::Bool(a), MirValue::Bool(b)) => Ok(MirValue::Bool(a != b)),
        (op, l, r) => internal_bug!("type error in MIR binop: {:?} {:?} {:?}", l, op, r),
    }
}

fn eval_unop(op: Uop, v: MirValue) -> Result<MirValue, MirInterpError> {
    match (op, v) {
        (Uop::Neg, MirValue::Int(i)) => Ok(MirValue::Int(-i)),
        (Uop::Not, MirValue::Bool(b)) => Ok(MirValue::Bool(!b)),
        (op, v) => internal_bug!("type error in MIR unop: {:?} {:?}", op, v),
    }
}

fn eval_intrinsic(fn_name: Intrinsic, args: Vec<MirValue>) -> Result<MirValue, MirInterpError> {
    match fn_name {
        Intrinsic::Println => {
            for arg in &args {
                print!("{} ", fmt_value(arg));
            }
            println!();
            Ok(MirValue::Unit)
        }
        Intrinsic::Print => {
            for arg in &args {
                print!("{} ", fmt_value(arg));
            }
            Ok(MirValue::Unit)
        }
    }
}

fn fmt_value(v: &MirValue) -> String {
    match v {
        MirValue::Int(i) => i.to_string(),
        MirValue::Bool(b) => b.to_string(),
        MirValue::Unit => "()".to_string(),
    }
}
