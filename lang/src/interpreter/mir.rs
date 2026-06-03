//! an interpreter for the MIR

use crate::compiler::context::CompileCtx;
use crate::compiler::structure::FunRef;
use crate::internal_bug;
use crate::ir_types::mir::*;
use crate::lang::intrinsics::Intrinsic;
use crate::lang::ops::Bop;
use crate::lang::ops::CompOp;
use crate::lang::ops::Uop;
use crate::lang::types::EnumRef;

#[derive(Debug, Clone, PartialEq)]
pub enum MirValue {
    Int(i64),
    Bool(bool),
    Unit,
    EnumVariant {
        enum_ref: EnumRef,
        variant_idx: usize,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum MirInterpError {
    #[error("no entry point found")]
    NoEntryPoint,
    #[error("uninitialized local: {0:?}")]
    UninitializedLocal(LocalId),
    #[error("division by zero")]
    DivisionByZero,
    #[error("reached unreachable terminator")]
    Unreachable,
    #[error("runtime error: {0}")]
    Runtime(String),
}

impl MirProgram {
    pub fn interpret(&self, ctx: &CompileCtx) -> Result<MirValue, MirInterpError> {
        // find the main function
        let (main_ref, _main_fn) = self
            .functions
            .iter()
            .find(|(fr, _)| ctx.is_main(**fr))
            .ok_or(MirInterpError::NoEntryPoint)?;

        self.call_function(*main_ref, &[], ctx)
    }

    fn call_function(
        &self,
        fun: FunRef,
        args: &[MirValue],
        ctx: &CompileCtx,
    ) -> Result<MirValue, MirInterpError> {
        let func = &self.functions[&fun];

        // initialise locals: all unset to start
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
                execute_statement(stmt, &mut locals, self, ctx)?;
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
    locals: &mut [Option<MirValue>],
    prog: &MirProgram,
    ctx: &CompileCtx,
) -> Result<(), MirInterpError> {
    match stmt {
        Statement::Assign { dst, value, .. } => {
            let v = eval_rvalue(value, locals, prog, ctx)?;
            locals[dst.local.0] = Some(v);
            Ok(())
        }
        Statement::Eval { value, .. } => {
            eval_rvalue(value, locals, prog, ctx)?;
            Ok(())
        }
    }
}

fn eval_rvalue(
    rv: &RValue,
    locals: &mut [Option<MirValue>],
    prog: &MirProgram,
    ctx: &CompileCtx,
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
            prog.call_function(*fn_name, &arg_vals, ctx)
        }

        RValue::IntrinsicCall { fn_name, args } => {
            let arg_vals = args
                .iter()
                .map(|a| eval_operand(a, locals))
                .collect::<Result<Vec<_>, _>>()?;
            eval_intrinsic(*fn_name, arg_vals, ctx)
        }
    }
}

fn eval_operand(op: &Operand, locals: &[Option<MirValue>]) -> Result<MirValue, MirInterpError> {
    match op {
        Operand::Const(c) => Ok(match c {
            Constant::Int(i) => MirValue::Int(*i),
            Constant::Bool(b) => MirValue::Bool(*b),
            Constant::Unit => MirValue::Unit,
            Constant::EnumVariant {
                enum_ref,
                variant_idx,
            } => MirValue::EnumVariant {
                enum_ref: *enum_ref,
                variant_idx: *variant_idx,
            },
        }),
        Operand::Copy(place) => locals[place.local.0]
            .clone()
            .ok_or(MirInterpError::UninitializedLocal(place.local)),
    }
}

fn eval_binop(op: Bop, l: MirValue, r: MirValue) -> Result<MirValue, MirInterpError> {
    match (op, l, r) {
        (Bop::Plus, MirValue::Int(a), MirValue::Int(b)) => {
            Ok(MirValue::Int(a.overflowing_add(b).0))
        }
        (Bop::Minus, MirValue::Int(a), MirValue::Int(b)) => {
            Ok(MirValue::Int(a.overflowing_sub(b).0))
        }
        (Bop::Mult, MirValue::Int(a), MirValue::Int(b)) => {
            Ok(MirValue::Int(a.overflowing_mul(b).0))
        }
        (Bop::Div, MirValue::Int(a), MirValue::Int(b)) => {
            if b == 0 {
                Err(MirInterpError::DivisionByZero)
            } else {
                Ok(MirValue::Int(a / b))
            }
        }
        (Bop::Pow, MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(a.pow(b as u32))),
        (Bop::And, MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(a & b)),
        (Bop::Or, MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(a | b)),
        (Bop::Xor, MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(a ^ b)),
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
        (
            Bop::Comp(CompOp::Eq),
            MirValue::EnumVariant {
                enum_ref: er1,
                variant_idx: vi1,
            },
            MirValue::EnumVariant {
                enum_ref: er2,
                variant_idx: vi2,
            },
        ) => Ok(MirValue::Bool(er1 == er2 && vi1 == vi2)),
        (
            Bop::Comp(CompOp::Ne),
            MirValue::EnumVariant {
                enum_ref: er1,
                variant_idx: vi1,
            },
            MirValue::EnumVariant {
                enum_ref: er2,
                variant_idx: vi2,
            },
        ) => Ok(MirValue::Bool(er1 != er2 || vi1 != vi2)),
        (op, l, r) => internal_bug!("type error in MIR binop: {:?} {:?} {:?}", l, op, r),
    }
}

fn eval_unop(op: Uop, v: MirValue) -> Result<MirValue, MirInterpError> {
    match (op, v) {
        (Uop::Neg, MirValue::Int(i)) => Ok(MirValue::Int(-i)),
        (Uop::Not, MirValue::Bool(b)) => Ok(MirValue::Bool(!b)),
        (Uop::Not, MirValue::Int(i)) => Ok(MirValue::Int(!i)),
        (op, v) => internal_bug!("type error in MIR unop: {:?} {:?}", op, v),
    }
}

fn eval_intrinsic(
    fn_name: Intrinsic,
    args: Vec<MirValue>,
    ctx: &CompileCtx,
) -> Result<MirValue, MirInterpError> {
    match fn_name {
        Intrinsic::Println => {
            for arg in &args {
                print!("{} ", fmt_value(arg, ctx));
            }
            println!();
            Ok(MirValue::Unit)
        }
        Intrinsic::Print => {
            for arg in &args {
                print!("{} ", fmt_value(arg, ctx));
            }
            Ok(MirValue::Unit)
        }
        Intrinsic::Abs => {
            debug_assert_eq!(args.len(), 1, "Abs expects 1 arg");
            match args[0] {
                MirValue::Int(n) => Ok(MirValue::Int(n.abs())),
                ref v => Err(MirInterpError::Runtime(format!(
                    "__abs: expected Int, got {:?}",
                    v
                ))),
            }
        }
        Intrinsic::Min => {
            debug_assert_eq!(args.len(), 2, "Min expects 2 args");
            match (&args[0], &args[1]) {
                (MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(*a.min(b))),
                _ => Err(MirInterpError::Runtime("__min: expected (Int, Int)".into())),
            }
        }
        Intrinsic::Max => {
            debug_assert_eq!(args.len(), 2, "Max expects 2 args");
            match (&args[0], &args[1]) {
                (MirValue::Int(a), MirValue::Int(b)) => Ok(MirValue::Int(*a.max(b))),
                _ => Err(MirInterpError::Runtime("__max: expected (Int, Int)".into())),
            }
        }
        Intrinsic::ReadInt => {
            let mut line = String::new();
            std::io::stdin()
                .read_line(&mut line)
                .map_err(|e| MirInterpError::Runtime(format!("__read_int: io error: {e}")))?;
            let n = line
                .trim()
                .parse::<i64>()
                .map_err(|e| MirInterpError::Runtime(format!("__read_int: parse error: {e}")))?;
            Ok(MirValue::Int(n))
        }
        Intrinsic::Exit => {
            debug_assert_eq!(args.len(), 1, "Exit expects 1 arg");
            match args[0] {
                MirValue::Int(code) => std::process::exit(code as i32),
                ref v => Err(MirInterpError::Runtime(format!(
                    "__exit: expected Int, got {:?}",
                    v
                ))),
            }
        }
    }
}

fn fmt_value(v: &MirValue, ctx: &CompileCtx) -> String {
    match v {
        MirValue::Int(i) => i.to_string(),
        MirValue::Bool(b) => b.to_string(),
        MirValue::Unit => "()".to_string(),
        MirValue::EnumVariant {
            enum_ref,
            variant_idx,
        } => {
            let def = ctx.get_enum(*enum_ref);
            let name = &def.variants[*variant_idx];
            if def.is_anonymous {
                format!("#{name}")
            } else {
                name.clone()
            }
        }
    }
}
